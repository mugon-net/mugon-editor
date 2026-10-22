//! Compiles the generated BSN (`level.rs`) against real Bevy and spawns it.
//! Regenerate the golden file with `UPDATE_GOLDEN=1 cargo test -p mugon-editor-scene`.

use bevy::prelude::*;

#[derive(Component, Default, Clone)]
pub struct Health {
    pub max: f32,
}

#[derive(Component, Default, Clone)]
pub struct Spawner {
    pub kind: String,
    pub count: i32,
    pub offset: Vec3,
}

#[derive(Default, Clone, Debug, PartialEq)]
pub enum ColliderShape {
    #[default]
    Box,
    Sphere,
    Mesh,
}

#[derive(Component, Default, Clone)]
pub struct Collider {
    pub shape: ColliderShape,
}

#[derive(Component, Default, Clone)]
pub struct Pickup;

pub mod level;

#[cfg(test)]
mod tests {
    use super::*;
    use mugon_sdk_bevy::scene::{Marker, MarkerShape};

    #[test]
    fn generated_scene_spawns() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), bevy::scene::ScenePlugin));
        app.init_asset::<WorldAsset>();
        let world = app.world_mut();
        let roots = level::spawn(world).unwrap();
        assert_eq!(roots.len(), 2);
        let mut q = world.query::<(&Name, Has<Transform>, Has<Visibility>)>();
        for (name, has_transform, has_visibility) in q.iter(world) {
            assert!(has_transform && has_visibility, "{name} needs Transform and Visibility");
        }
        let mut q = world.query::<(&Name, Option<&Health>)>();
        let names: Vec<String> = q.iter(world).map(|(n, _)| n.to_string()).collect();
        assert!(names.contains(&"Crate_1".to_string()), "{names:?}");
        assert!(names.contains(&"Crate_1_2".to_string()), "{names:?}");
        let mut q = world.query::<(&Name, &Marker, &Transform)>();
        let zone = q.iter(world).find(|(n, _, _)| n.as_str() == "Spawn_Zone").expect("marker spawned");
        assert_eq!(zone.1.shape, MarkerShape::Sphere);
        assert_eq!(zone.2.scale, Vec3::splat(4.0));
        let mut q = world.query::<&Health>();
        assert_eq!(q.iter(world).next().map(|h| h.max), Some(40.0));
    }
}
