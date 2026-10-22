//! Keeps the Bevy world and the project in sync.
//!
//! The project is authoritative for structure; while a gizmo is being dragged the Bevy `Transform` is
//! authoritative and flows back into the project.

use crate::geom::Aabb;
use crate::model::*;
use bevy::prelude::*;
use std::collections::HashMap;
use transform_gizmo_bevy::GizmoTarget;

/// Brings entities, hierarchy, models and transforms in line with the project (when `Settings::rebuild`).
pub fn reconcile(
    mut commands: Commands,
    mut settings: ResMut<Settings>,
    editor: Res<EditorProject>,
    library: Res<Library>,
    asset_server: Res<AssetServer>,
    marker_assets: Res<MarkerAssets>,
    existing: Query<(Entity, &EditorId, Option<&ModelAsset>, Option<&MarkerView>)>,
) {
    if !settings.rebuild {
        return;
    }
    settings.rebuild = false;
    let project = &editor.project;

    let mut by_id: HashMap<u64, (Entity, Option<ModelAsset>, Option<MarkerView>)> = HashMap::new();
    for (entity, id, model, marker) in &existing {
        if project.get(id.0).is_none() {
            commands.entity(entity).despawn();
        } else {
            by_id.insert(id.0, (entity, model.cloned(), marker.copied()));
        }
    }

    // spawn missing entities first so parents exist
    for e in &project.entities {
        if by_id.contains_key(&e.id) {
            continue;
        }
        let entity = commands.spawn((EditorId(e.id), Name::new(e.name.clone()), Transform::default(), Visibility::default())).id();
        by_id.insert(e.id, (entity, None, None));
    }

    for e in &project.entities {
        let (entity, current_model, current_marker) = by_id[&e.id].clone();
        let mut cmd = commands.entity(entity);
        cmd.insert((Name::new(e.name.clone()), xform_to_transform_component(&e.transform)));
        match e.parent.and_then(|p| by_id.get(&p)) {
            Some((parent, _, _)) => {
                cmd.insert(ChildOf(*parent));
            }
            None => {
                cmd.remove::<ChildOf>();
            }
        }
        let wanted = ModelAsset(e.asset.clone());
        if current_model.as_ref() != Some(&wanted) {
            cmd.remove::<(WorldAssetRoot, LocalBounds)>();
            if let Some(asset) = e.asset.as_ref().and_then(|id| library.index.get(id)) {
                let handle = asset_server.load(GltfAssetLabel::Scene(0).from_asset(format!("{}/{}", asset.pack, asset.model)));
                let b = &asset.bounds;
                cmd.insert((WorldAssetRoot(handle), LocalBounds(Aabb::new(Vec3::from(b.min), Vec3::from(b.max)))));
            }
            cmd.insert(wanted);
        }
        let wanted_marker = e.marker.map(MarkerView);
        if wanted_marker != current_marker {
            cmd.remove::<(Mesh3d, MeshMaterial3d<StandardMaterial>, MarkerView, MarkerColor, RayCastBackfaces)>();
            match wanted_marker {
                Some(view) => {
                    cmd.insert((
                        view,
                        Mesh3d(marker_assets.mesh(view.0)),
                        MeshMaterial3d(marker_assets.material.clone()),
                        // volumes stay clickable from inside
                        RayCastBackfaces,
                        LocalBounds(Aabb::new(Vec3::splat(-0.5), Vec3::splat(0.5))),
                    ));
                }
                None => {
                    cmd.remove::<LocalBounds>();
                }
            }
        }
    }
}

fn xform_to_transform_component(x: &mugon_editor_scene::Xform) -> Transform {
    xform_to_transform(x)
}

/// Gizmo targets follow the selection.
pub fn sync_gizmo_targets(
    mut commands: Commands,
    selection: Res<Selection>,
    entities: Query<(Entity, &EditorId, Has<GizmoTarget>)>,
) {
    for (entity, id, has) in &entities {
        let selected = selection.contains(id.0);
        if selected && !has {
            commands.entity(entity).insert(GizmoTarget::default());
        } else if !selected && has {
            commands.entity(entity).remove::<GizmoTarget>();
        }
    }
}

/// Copies transforms edited in the viewport into the project. The undo snapshot for a gizmo drag is taken
/// on the first frame the gizmo is active, while the project still holds the pre-drag transforms.
pub fn transforms_to_project(
    mut editor: ResMut<EditorProject>,
    mut history: ResMut<History>,
    mut was_active: Local<bool>,
    targets: Query<&GizmoTarget>,
    changed: Query<(&EditorId, &Transform), Changed<Transform>>,
) {
    let active = targets.iter().any(|t| t.is_active());
    if active && !*was_active {
        history.push(editor.project.clone());
    }
    *was_active = active;
    for (id, t) in &changed {
        let x = transform_to_xform(t);
        if let Some(e) = editor.project.get_mut(id.0) {
            if e.transform != x {
                e.transform = x;
            }
        }
    }
}
