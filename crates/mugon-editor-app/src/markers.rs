//! Marker volumes: translucent placeholders with a wireframe outline.
//!
//! A marker is a unit shape (1 m box / sphere / cylinder, centred on its origin) scaled by the entity transform,
//! so the scale gizmo resizes it and the exported `Transform` describes the space it occupies.

use crate::model::*;
use mugon_editor_scene::MarkerShape;
use bevy::prelude::*;

pub fn setup_marker_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(MarkerAssets {
        boxm: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        sphere: meshes.add(Sphere::new(0.5)),
        cylinder: meshes.add(Cylinder::new(0.5, 1.0)),
        material: materials.add(StandardMaterial {
            base_color: Color::srgba(0.25, 0.8, 1.0, 0.3),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        }),
        tinted: Default::default(),
    });
}

pub const DEFAULT_COLOR: [f32; 3] = [0.4, 0.9, 1.0];
const SELECTED_OUTLINE: Color = Color::srgb(1.0, 0.75, 0.1);

fn fill_material(color: [f32; 3]) -> StandardMaterial {
    StandardMaterial { base_color: Color::srgba(color[0], color[1], color[2], 0.3), alpha_mode: AlphaMode::Blend, unlit: true, cull_mode: None, ..default() }
}

/// Tints each marker by the first of its components that defines a `marker_color`.
pub fn update_marker_colors(
    mut commands: Commands,
    editor: Res<EditorProject>,
    library: Res<Library>,
    mut assets: ResMut<MarkerAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    markers: Query<(Entity, &EditorId, &MeshMaterial3d<StandardMaterial>, Option<&MarkerColor>), With<MarkerView>>,
) {
    for (entity, id, material, current) in &markers {
        let Some(scene_entity) = editor.project.get(id.0) else { continue };
        let color = library.registry.marker_color(scene_entity.components.iter().map(|c| c.name.as_str())).unwrap_or(DEFAULT_COLOR);
        if current.is_some_and(|c| c.0 == color) {
            continue;
        }
        let key = color.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
        let handle = assets.tinted.entry(key).or_insert_with(|| materials.add(fill_material(color))).clone();
        let mut cmd = commands.entity(entity);
        cmd.insert(MarkerColor(color));
        if material.0 != handle {
            cmd.insert(MeshMaterial3d(handle));
        }
    }
}

/// Closed ring in the plane spanned by the unit axes `a` and `b`, offset by `offset`, mapped through the marker transform.
fn ring(gizmos: &mut Gizmos, outline: Color, gt: &GlobalTransform, a: Vec3, b: Vec3, offset: Vec3) {
    let points: Vec<Vec3> = (0..=48)
        .map(|i| {
            let t = i as f32 / 48.0 * std::f32::consts::TAU;
            gt.transform_point(offset + (a * t.cos() + b * t.sin()) * 0.5)
        })
        .collect();
    gizmos.linestrip(points, outline);
}

pub fn draw_marker_outlines(mut gizmos: Gizmos, selection: Res<Selection>, markers: Query<(&EditorId, &MarkerView, &GlobalTransform, Option<&MarkerColor>)>) {
    for (id, view, gt, color) in &markers {
        let outline = if selection.contains(id.0) {
            SELECTED_OUTLINE
        } else {
            let [r, g, b] = color.map_or(DEFAULT_COLOR, |c| c.0);
            Color::srgba(r, g, b, 0.9)
        };
        match view.0 {
            MarkerShape::Box => {
                gizmos.cube(*gt, outline);
            }
            MarkerShape::Sphere => {
                ring(&mut gizmos, outline, gt, Vec3::X, Vec3::Y, Vec3::ZERO);
                ring(&mut gizmos, outline, gt, Vec3::X, Vec3::Z, Vec3::ZERO);
                ring(&mut gizmos, outline, gt, Vec3::Y, Vec3::Z, Vec3::ZERO);
            }
            MarkerShape::Cylinder => {
                for y in [-0.5, 0.5] {
                    ring(&mut gizmos, outline, gt, Vec3::X, Vec3::Z, Vec3::Y * y);
                }
                for (x, z) in [(0.5, 0.0), (-0.5, 0.0), (0.0, 0.5), (0.0, -0.5)] {
                    gizmos.line(gt.transform_point(Vec3::new(x, -0.5, z)), gt.transform_point(Vec3::new(x, 0.5, z)), outline);
                }
            }
        }
    }
}
