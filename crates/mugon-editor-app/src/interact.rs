//! Viewport interaction: gizmo setup, drop-to-place, click/marquee selection, copy/paste, snapping and shortcuts.

use crate::actions::Ctx;
use crate::camera::{cursor_in_viewport, FlyCam};
use crate::geom::*;
use crate::model::*;
use bevy::input::keyboard::Key;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use transform_gizmo_bevy::{EnumSet, GizmoMode, GizmoOptions, GizmoResult, GizmoTarget};

/// Pixels the mouse must travel with the button held before a click becomes a selection rectangle.
const MARQUEE_THRESHOLD: f32 = 4.0;
/// Markers start as 2 m volumes centred on their origin.
const MARKER_SIZE: f32 = 2.0;

pub fn configure_gizmo(settings: Res<Settings>, mut options: ResMut<GizmoOptions>) {
    options.gizmo_modes = match settings.tool {
        Tool::Translate => GizmoMode::all_translate(),
        Tool::Rotate => GizmoMode::all_rotate(),
        Tool::Scale => GizmoMode::all_scale(),
    };
    options.snapping = settings.grid_snap;
    options.snap_distance = settings.grid_size;
    options.snap_angle = settings.angle_snap_deg.to_radians();
    options.snap_scale = settings.scale_snap;
    options.hotkeys = None;
    let _: EnumSet<GizmoMode> = options.gizmo_modes;
}

fn world_aabb(bounds: &LocalBounds, gt: &GlobalTransform) -> Aabb {
    bounds.0.transformed(&gt.to_matrix())
}

type Pickable<'a> = (&'a EditorId, &'a LocalBounds, &'a GlobalTransform);

/// Everything `pick` needs besides the ray: mesh ray casting plus the lookups that map a hit mesh back to its entity.
#[derive(SystemParam)]
pub struct Picker<'w, 's> {
    ray_cast: MeshRayCast<'w, 's>,
    parents: Query<'w, 's, &'static ChildOf>,
    ids: Query<'w, 's, &'static EditorId>,
    outlines: Query<'w, 's, (), With<SelectionOutline>>,
    /// Entities with nothing to ray cast yet (scene still loading): picked by bounding box instead.
    unloaded: Query<'w, 's, Pickable<'static>, (Without<Mesh3d>, Without<Children>)>,
}

/// The editor entity a mesh belongs to: the mesh entity itself (markers) or its nearest `EditorId` ancestor (scene models).
fn owner(mut e: Entity, parents: &Query<&ChildOf>, ids: &Query<&EditorId>) -> Option<u64> {
    loop {
        if let Ok(id) = ids.get(e) {
            return Some(id.0);
        }
        e = parents.get(e).ok()?.parent();
    }
}

/// Nearest entity hit by the ray: (distance, id). Triangle-accurate for loaded meshes.

fn pick(origin: Vec3, dir: Vec3, picker: &mut Picker) -> Option<(f32, u64)> {
    let mut best: Option<(f32, u64)> = None;
    if let Ok(dir3) = Dir3::new(dir) {
        let Picker { ray_cast, parents, ids, outlines, .. } = &mut *picker;
        let filter = |e: Entity| !outlines.contains(e) && owner(e, parents, ids).is_some();
        let settings = MeshRayCastSettings::default().with_filter(&filter).always_early_exit();
        if let Some((e, hit)) = ray_cast.cast_ray(Ray3d::new(origin, dir3), &settings).first() {
            best = owner(*e, parents, ids).map(|id| (hit.distance, id));
        }
    }
    for (id, bounds, gt) in &picker.unloaded {
        if let Some(t) = ray_obb(origin, dir, &bounds.0, &gt.to_matrix()) {
            if best.is_none_or(|(bt, _)| t < bt) {
                best = Some((t, id.0));
            }
        }
    }
    best
}

/// Places a palette item where the ray lands (ground or the first model hit), with grid / neighbour snapping.
fn place_payload(ctx: &mut Ctx, library: &Library, payload: &Payload, origin: Vec3, dir: Vec3, entities: &Query<Pickable>, picker: &mut Picker) {
    let ground = ray_ground(origin, dir, 0.0);
    let model_hit = pick(origin, dir, picker).map(|(t, _)| origin + dir * t);
    // prefer whichever surface is hit first
    let point = match (ground, model_hit) {
        (Some(g), Some(m)) => Some(if (g - origin).length() < (m - origin).length() { g } else { m }),
        (g, m) => g.or(m),
    };
    let Some(mut point) = point else { return };
    if ctx.settings.grid_snap {
        let g = ctx.settings.grid_size;
        point.x = (point.x / g).round() * g;
        point.z = (point.z / g).round() * g;
    }
    // local bounds of whatever is being placed, for neighbour snapping
    let local = match payload {
        Payload::Asset(asset_id) => {
            let Some(asset) = library.index.get(asset_id) else { return };
            let b = &asset.bounds;
            Aabb::new(Vec3::from(b.min), Vec3::from(b.max))
        }
        Payload::Marker(_) => {
            point.y += MARKER_SIZE / 2.0; // sit on the surface
            Aabb::new(Vec3::splat(-MARKER_SIZE / 2.0), Vec3::splat(MARKER_SIZE / 2.0))
        }
    };
    if ctx.settings.face_snap {
        let others: Vec<Aabb> = entities.iter().map(|(_, lb, gt)| world_aabb(lb, gt)).collect();
        point += snap_translation(&local.translated(point), &others, ctx.settings.face_threshold);
    }
    match payload {
        Payload::Asset(asset_id) => {
            let name = library.index.get(asset_id).map(|a| a.name.clone()).unwrap_or_default();
            ctx.place(asset_id, &name, point.to_array());
        }
        Payload::Marker(shape) => {
            ctx.place_marker(*shape, point.to_array(), MARKER_SIZE);
        }
    }
}

/// Left-button state between press and release in the viewport.
#[derive(Default)]
pub struct Press {
    start: Option<Vec2>,
    marquee: bool,
}

/// Drop-to-place, click selection (click empty space to deselect) and marquee selection.
#[allow(clippy::too_many_arguments)]
pub fn viewport_click(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<UiState>,
    library: Res<Library>,
    mut editor: ResMut<EditorProject>,
    mut history: ResMut<History>,
    mut selection: ResMut<Selection>,
    mut settings: ResMut<Settings>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<FlyCam>>,
    gizmo_targets: Query<&GizmoTarget>,
    entities: Query<Pickable>,
    mut picker: Picker,
    mut press: Local<Press>,
) {
    let (cam, cam_gt) = *camera;
    let gizmo_busy = gizmo_targets.iter().any(|t| t.is_focused() || t.is_active());
    let ray_at = |pos: Vec2| cam.viewport_to_world(cam_gt, pos).ok().map(|r| (r.origin, *r.direction));

    // an item dragged out of the palette was released over the viewport
    if let Some((payload, pos)) = settings.drop.take() {
        if let Some((origin, dir)) = ray_at(pos) {
            let mut ctx = Ctx { editor: &mut editor, history: &mut history, selection: &mut selection, settings: &mut settings };
            place_payload(&mut ctx, &library, &payload, origin, dir, &entities, &mut picker);
        }
        return;
    }

    let cursor = window.cursor_position();
    let additive = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ControlLeft);

    if mouse.just_pressed(MouseButton::Left) && cursor_in_viewport(&window, &ui) && !gizmo_busy {
        // `--asset` arms one model: the next click places it
        if let Some(asset_id) = settings.placing.clone() {
            if let Some((origin, dir)) = cursor.and_then(|c| ray_at(c)) {
                let mut ctx = Ctx { editor: &mut editor, history: &mut history, selection: &mut selection, settings: &mut settings };
                place_payload(&mut ctx, &library, &Payload::Asset(asset_id), origin, dir, &entities, &mut picker);
            }
            return;
        }
        *press = Press { start: cursor, marquee: false };
    }

    let Some(start) = press.start else { return };
    let Some(cursor) = cursor else {
        *press = Press::default();
        ui.marquee = None;
        return;
    };
    if mouse.pressed(MouseButton::Left) {
        if gizmo_busy {
            *press = Press::default();
            ui.marquee = None;
        } else if press.marquee || start.distance(cursor) > MARQUEE_THRESHOLD {
            press.marquee = true;
            ui.marquee = Some(Rect::from_corners(start, cursor));
        }
        return;
    }
    // button released (or lost): act on the gesture
    let was_marquee = press.marquee;
    *press = Press::default();
    ui.marquee = None;
    if gizmo_busy || !mouse.just_released(MouseButton::Left) {
        return;
    }

    if was_marquee {
        // everything whose projected bounding box overlaps the rectangle
        let rect = Rect::from_corners(start, cursor);
        if !additive {
            selection.0.clear();
        }
        for (id, lb, gt) in &entities {
            let m = gt.to_matrix();
            let b = &lb.0;
            let mut lo = Vec2::splat(f32::MAX);
            let mut hi = Vec2::splat(f32::MIN);
            for i in 0..8 {
                let c = Vec3::new(
                    if i & 1 == 0 { b.min.x } else { b.max.x },
                    if i & 2 == 0 { b.min.y } else { b.max.y },
                    if i & 4 == 0 { b.min.z } else { b.max.z },
                );
                if let Ok(p) = cam.world_to_viewport(cam_gt, m.transform_point3(c)) {
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
            }
            let overlaps = lo.x <= hi.x && lo.x <= rect.max.x && hi.x >= rect.min.x && lo.y <= rect.max.y && hi.y >= rect.min.y;
            if overlaps && !selection.contains(id.0) {
                selection.0.push(id.0);
            }
        }
    } else {
        let hit = ray_at(cursor).and_then(|(origin, dir)| pick(origin, dir, &mut picker)).map(|(_, id)| id);
        match hit {
            Some(id) if additive => selection.toggle(id),
            Some(id) => selection.set(id),
            None if !additive => selection.0.clear(),
            None => {}
        }
    }
}

/// Ctrl+C / Ctrl+V on the selection. Pasted top-level entities land under the cursor (on the ground).
#[allow(clippy::too_many_arguments)]
pub fn clipboard_shortcuts(
    keys: Res<ButtonInput<KeyCode>>,
    ui: Res<UiState>,
    mut clipboard: ResMut<SceneClipboard>,
    mut editor: ResMut<EditorProject>,
    mut history: ResMut<History>,
    mut selection: ResMut<Selection>,
    mut settings: ResMut<Settings>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<FlyCam>>,
) {
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if ui.wants_keyboard || !ctrl {
        return;
    }
    if keys.just_pressed(KeyCode::KeyC) && !selection.0.is_empty() {
        clipboard.0 = editor.project.copy_subtrees(&selection.0);
        settings.status = format!("Copied {} entities", clipboard.0.len());
    }
    if keys.just_pressed(KeyCode::KeyV) && !clipboard.0.is_empty() {
        let tops: Vec<Vec3> = clipboard.0.iter().filter(|e| e.parent.is_none()).map(|e| Vec3::from(e.transform.translation)).collect();
        let mut offset = Vec3::X; // fallback: next to the original
        if !tops.is_empty() && cursor_in_viewport(&window, &ui) {
            let (cam, cam_gt) = *camera;
            let hit = window
                .cursor_position()
                .and_then(|c| cam.viewport_to_world(cam_gt, c).ok())
                .and_then(|r| ray_ground(r.origin, *r.direction, 0.0));
            if let Some(hit) = hit {
                let centre = tops.iter().sum::<Vec3>() / tops.len() as f32;
                offset = Vec3::new(hit.x - centre.x, 0.0, hit.z - centre.z);
            }
        }
        let mut ctx = Ctx { editor: &mut editor, history: &mut history, selection: &mut selection, settings: &mut settings };
        ctx.paste(&clipboard.0, offset.to_array());
    }
}

/// True when the key printing `ch` was pressed this frame, whatever the keyboard layout
/// (physical `KeyCode`s would swap Z and Y on a QWERTZ keyboard).
fn typed(logical: &ButtonInput<Key>, ch: &str) -> bool {
    logical.get_just_pressed().any(|k| matches!(k, Key::Character(s) if s.eq_ignore_ascii_case(ch)))
}

/// Seconds a nudge key is held before it starts repeating, and the repeat interval.
const NUDGE_DELAY: f32 = 0.35;
const NUDGE_INTERVAL: f32 = 0.05;

pub fn shortcuts(
    time: Res<Time>,
    logical: Res<ButtonInput<Key>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    ui: Res<UiState>,
    library: Res<Library>,
    mut editor: ResMut<EditorProject>,
    mut history: ResMut<History>,
    mut selection: ResMut<Selection>,
    mut settings: ResMut<Settings>,
    mut cam: Single<(&mut Transform, &FlyCam)>,
    objects: Query<(&EditorId, &LocalBounds, &GlobalTransform)>,
    mut held: Local<[f32; 6]>,
) {
    if ui.wants_keyboard {
        return;
    }
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let mut ctx = Ctx { editor: &mut editor, history: &mut history, selection: &mut selection, settings: &mut settings };
    if ctrl {
        if typed(&logical, "z") {
            if keys.pressed(KeyCode::ShiftLeft) { ctx.redo() } else { ctx.undo() }
        }
        if typed(&logical, "y") {
            ctx.redo();
        }
        if keys.just_pressed(KeyCode::KeyD) {
            ctx.duplicate_selected();
        }
        if keys.just_pressed(KeyCode::KeyS) {
            ctx.save(&library, None);
        }
        return;
    }
    // Unity tool keys (W/E are the fly camera while the right mouse button is held)
    if !mouse.pressed(MouseButton::Right) {
        if keys.just_pressed(KeyCode::KeyW) { ctx.settings.tool = Tool::Translate; }
        if keys.just_pressed(KeyCode::KeyE) { ctx.settings.tool = Tool::Rotate; }
        if keys.just_pressed(KeyCode::KeyR) { ctx.settings.tool = Tool::Scale; }
    }
    if keys.just_pressed(KeyCode::Delete) || keys.just_pressed(KeyCode::Backspace) {
        ctx.delete_selected();
    }
    if keys.just_pressed(KeyCode::Escape) {
        ctx.settings.placing = None;
        ctx.settings.dragging = None;
    }
    // Arrow keys nudge the selection along the world axis nearest to the camera's view (PageUp/PageDown: up/down)
    if !mouse.pressed(MouseButton::Right) && !ctx.selection.0.is_empty() {
        let rot = cam.1.rotation();
        let axis = |v: Vec3| {
            let v = Vec3::new(v.x, 0.0, v.z);
            if v.x.abs() > v.z.abs() { Vec3::X * v.x.signum() } else { Vec3::Z * v.z.signum() }
        };
        let (fwd, right) = (axis(rot * Vec3::NEG_Z), axis(rot * Vec3::X));
        let nudges = [
            (KeyCode::ArrowUp, fwd),
            (KeyCode::ArrowDown, -fwd),
            (KeyCode::ArrowRight, right),
            (KeyCode::ArrowLeft, -right),
            (KeyCode::PageUp, Vec3::Y),
            (KeyCode::PageDown, Vec3::NEG_Y),
        ];
        let base = if ctx.settings.grid_snap { ctx.settings.grid_size } else { 0.1 };
        let step = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) { base * 10.0 } else { base };
        for (i, (key, dir)) in nudges.into_iter().enumerate() {
            if keys.just_pressed(key) {
                held[i] = -NUDGE_DELAY;
                ctx.nudge_selected(dir * step, true);
            } else if keys.pressed(key) {
                held[i] += time.delta_secs();
                // repeats are part of the undo step started by the first press
                while held[i] >= NUDGE_INTERVAL {
                    held[i] -= NUDGE_INTERVAL;
                    ctx.nudge_selected(dir * step, false);
                }
            }
        }
    }
    if keys.just_pressed(KeyCode::KeyF) {
        // focus the selection
        let mut bounds: Option<Aabb> = None;
        for (id, lb, gt) in &objects {
            if ctx.selection.contains(id.0) {
                let a = world_aabb(lb, gt);
                bounds = Some(bounds.map_or(a, |b| Aabb::new(b.min.min(a.min), b.max.max(a.max))));
            }
        }
        if let Some(b) = bounds {
            let (transform, fly) = &mut *cam;
            let dist = (b.size().length() * 1.2).max(1.5);
            transform.translation = b.center() - fly.rotation() * Vec3::NEG_Z * dist;
        }
    }
}

/// Neighbour snapping while a translate drag is in progress.
///
/// The gizmo accumulates per-frame deltas on the current transform, which would make snapped positions
/// "sticky". Instead the position is recomputed every frame from the drag start plus the gizmo's total.
#[allow(clippy::too_many_arguments)]
pub fn face_snap_drag(
    settings: Res<Settings>,
    mut drag: ResMut<DragState>,
    mut targets: Query<(Entity, &GizmoTarget, &mut Transform, &GlobalTransform, Option<&LocalBounds>, Option<&ChildOf>), With<EditorId>>,
    others: Query<(Entity, &LocalBounds, &GlobalTransform), (With<EditorId>, Without<GizmoTarget>)>,
    parents: Query<&ChildOf>,
    globals: Query<&GlobalTransform>,
) {
    let active = targets.iter().any(|t| t.1.is_active());
    if !active || !settings.face_snap || settings.tool != Tool::Translate {
        *drag = DragState::default();
        return;
    }

    if !drag.active {
        drag.active = true;
        let selected: Vec<Entity> = targets.iter().map(|t| t.0).collect();
        let mut union: Option<Aabb> = None;
        for (e, _, t, gt, lb, _) in targets.iter() {
            drag.start_locals.insert(e, t.translation);
            let a = lb.map_or(Aabb::new(gt.translation(), gt.translation()), |b| world_aabb(b, gt));
            union = Some(union.map_or(a, |u| Aabb::new(u.min.min(a.min), u.max.max(a.max))));
        }
        drag.start_aabb = union;
        drag.others = others
            .iter()
            .filter(|(e, _, _)| {
                // exclude descendants of the dragged entities
                let mut cur = *e;
                while let Ok(p) = parents.get(cur) {
                    cur = p.parent();
                    if selected.contains(&cur) {
                        return false;
                    }
                }
                true
            })
            .map(|(_, lb, gt)| world_aabb(lb, gt))
            .collect();
    }

    let Some(start_aabb) = drag.start_aabb else { return };
    let total = targets.iter().find_map(|t| match t.1.latest_result() {
        Some(GizmoResult::Translation { total, .. }) => Some(Vec3::new(total.x as f32, total.y as f32, total.z as f32)),
        _ => None,
    });
    let Some(total) = total else { return };
    let snap = snap_translation(&start_aabb.translated(total), &drag.others, settings.face_threshold);
    let world_delta = total + snap;
    for (e, _, mut t, _, _, parent) in targets.iter_mut() {
        let Some(start) = drag.start_locals.get(&e) else { continue };
        let local_delta = match parent.and_then(|p| globals.get(p.parent()).ok()) {
            Some(pg) => pg.affine().inverse().transform_vector3a(world_delta.into()).into(),
            None => world_delta,
        };
        t.translation = *start + local_delta;
    }
}
