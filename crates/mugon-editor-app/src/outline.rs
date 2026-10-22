//! Highlight around selected models: every mesh below a selected entity gets an inverted-hull child
//! (the mesh pushed outwards along its normals, back faces only, flat colour).

use crate::model::*;
use bevy::light::NotShadowCaster;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::render_resource::Face;
use std::collections::{HashMap, HashSet};

const COLOR: Color = Color::srgb(1.0, 0.75, 0.1);
/// Outline thickness in world units.
const WIDTH: f32 = 0.025;

#[derive(Default)]
pub struct OutlineCache {
    material: Option<Handle<StandardMaterial>>,
    /// Pushed-out copies by source mesh and quantised thickness (local units).
    hulls: HashMap<(AssetId<Mesh>, u32), Handle<Mesh>>,
    /// Source mesh entity -> its outline child.
    active: HashMap<Entity, Entity>,
}

fn expanded(mesh: &Mesh, amount: f32) -> Option<Mesh> {
    let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL)? {
        VertexAttributeValues::Float32x3(values) => values.clone(),
        _ => return None,
    };
    let mut hull = mesh.clone();
    if let Some(VertexAttributeValues::Float32x3(positions)) = hull.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
        for (position, normal) in positions.iter_mut().zip(&normals) {
            for axis in 0..3 {
                position[axis] += normal[axis] * amount;
            }
        }
    }
    Some(hull)
}

pub fn sync_selection_outlines(
    mut commands: Commands,
    selection: Res<Selection>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: Local<OutlineCache>,
    editor_entities: Query<(Entity, &EditorId)>,
    children: Query<&Children>,
    sources: Query<(&Mesh3d, &GlobalTransform), (Without<SelectionOutline>, Without<MarkerView>)>,
) {
    let mut wanted: HashSet<Entity> = HashSet::new();
    for (root, id) in &editor_entities {
        if !selection.contains(id.0) {
            continue;
        }
        for entity in std::iter::once(root).chain(children.iter_descendants(root)) {
            if sources.contains(entity) {
                wanted.insert(entity);
            }
        }
    }

    cache.active.retain(|source, outline| {
        let keep = wanted.contains(source);
        if !keep {
            commands.entity(*outline).try_despawn();
        }
        keep
    });

    let material = cache
        .material
        .get_or_insert_with(|| {
            materials.add(StandardMaterial { base_color: COLOR, unlit: true, cull_mode: Some(Face::Front), ..default() })
        })
        .clone();

    for source in wanted {
        if cache.active.contains_key(&source) {
            continue;
        }
        let Ok((mesh3d, gt)) = sources.get(source) else { continue };
        let max_scale = gt.compute_transform().scale.abs().max_element().max(1e-4);
        let amount = WIDTH / max_scale;
        let key = (mesh3d.0.id(), (amount * 10_000.0) as u32);
        let hull = match cache.hulls.get(&key) {
            Some(hull) => hull.clone(),
            None => {
                let Some(hull) = meshes.get(&mesh3d.0).and_then(|mesh| expanded(mesh, amount)) else { continue };
                let handle = meshes.add(hull);
                cache.hulls.insert(key, handle.clone());
                handle
            }
        };
        let outline = commands.spawn((SelectionOutline, Mesh3d(hull), MeshMaterial3d(material.clone()), Transform::default(), NotShadowCaster, ChildOf(source))).id();
        cache.active.insert(source, outline);
    }
}
