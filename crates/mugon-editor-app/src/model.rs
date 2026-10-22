//! Editor resources and components.

use crate::geom::Aabb;
use mugon_editor_core::index::Index;
use mugon_editor_scene::components::ComponentRegistry;
use mugon_editor_scene::game::GameConfig;
use mugon_editor_scene::{MarkerShape, Project, Xform};
use bevy::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// The asset library the editor places models from.
#[derive(Resource)]
pub struct Library {
    pub index: Arc<Index>,
    pub registry: ComponentRegistry,
    pub game: GameConfig,
}

/// The scene being edited (source of truth) and where it is saved.
#[derive(Resource, Default)]
pub struct EditorProject {
    pub project: Project,
    pub path: Option<PathBuf>,
    /// The project as last loaded or saved, to tell whether there are unsaved changes.
    pub saved: Project,
}

impl EditorProject {
    pub fn is_dirty(&self) -> bool {
        self.project != self.saved
    }

    pub fn mark_saved(&mut self) {
        self.saved = self.project.clone();
    }
}

/// Undo/redo as whole-project snapshots (scenes are small).
#[derive(Resource, Default)]
pub struct History {
    pub undo: Vec<Project>,
    pub redo: Vec<Project>,
    /// Time of the last coalesced UI edit, so a drag counts as one undo step.
    pub last_ui_edit: f64,
}

impl History {
    pub fn push(&mut self, before: Project) {
        self.undo.push(before);
        self.redo.clear();
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
    }
}

#[derive(Resource, Default)]
pub struct Selection(pub Vec<u64>);

/// Entities copied with Ctrl+C (whole subtrees, original ids).
#[derive(Resource, Default)]
pub struct SceneClipboard(pub Vec<mugon_editor_scene::SceneEntity>);

/// Something dragged out of the palette to be dropped into the viewport.
#[derive(Clone, Debug, PartialEq)]
pub enum Payload {
    Asset(String),
    Marker(MarkerShape),
}

impl Selection {
    pub fn contains(&self, id: u64) -> bool {
        self.0.contains(&id)
    }
    pub fn set(&mut self, id: u64) {
        self.0 = vec![id];
    }
    pub fn toggle(&mut self, id: u64) {
        if let Some(i) = self.0.iter().position(|x| *x == id) {
            self.0.remove(i);
        } else {
            self.0.push(id);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tool {
    #[default]
    Translate,
    Rotate,
    Scale,
}

#[derive(Resource)]
pub struct Settings {
    pub tool: Tool,
    pub grid_snap: bool,
    pub grid_size: f32,
    pub angle_snap_deg: f32,
    pub scale_snap: f32,
    pub face_snap: bool,
    pub face_threshold: f32,
    /// Asset armed from the command line (`--asset`): clicking in the viewport places it.
    pub placing: Option<String>,
    /// Palette item currently being dragged.
    pub dragging: Option<Payload>,
    /// Palette item released over the viewport at this window position, waiting to be placed.
    pub drop: Option<(Payload, Vec2)>,
    /// Ask `scene_sync` to reconcile the Bevy world with the project.
    pub rebuild: bool,
    pub status: String,
    /// Shortcut hint expanded.
    pub help_open: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            tool: Tool::Translate,
            grid_snap: false,
            grid_size: 0.5,
            angle_snap_deg: 15.0,
            scale_snap: 0.1,
            face_snap: true,
            face_threshold: 0.25,
            placing: None,
            dragging: None,
            drop: None,
            rebuild: true,
            status: String::new(),
            help_open: false,
        }
    }
}

/// Per-frame UI state shared with the viewport systems.
#[derive(Resource, Default)]
pub struct UiState {
    pub pointer_over_ui: bool,
    pub wants_keyboard: bool,
    /// Camera viewport in logical window pixels (the free area between the panels).
    pub viewport: Rect,
    pub ready: bool,
    /// Selection rectangle being dragged in the viewport (window pixels).
    pub marquee: Option<Rect>,
}

/// Marks the Bevy entity that mirrors a project entity.
#[derive(Component)]
pub struct EditorId(pub u64);

/// Asset id currently shown by the entity (to detect changes).
#[derive(Component, Clone, PartialEq)]
pub struct ModelAsset(pub Option<String>);

/// The marker shape an entity currently shows.
#[derive(Component, Clone, Copy, PartialEq)]
pub struct MarkerView(pub MarkerShape);

/// Shared meshes and the translucent material for marker volumes.
#[derive(Resource)]
pub struct MarkerAssets {
    pub boxm: Handle<Mesh>,
    pub sphere: Handle<Mesh>,
    pub cylinder: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
    /// Translucent fill materials by colour (8 bit rgb).
    pub tinted: HashMap<[u8; 3], Handle<StandardMaterial>>,
}

/// Colour a marker is drawn in (rgb, 0..1), from the `marker_color` of its components.
#[derive(Component, Clone, Copy, PartialEq)]
pub struct MarkerColor(pub [f32; 3]);

/// Inverted-hull child that outlines a selected mesh.
#[derive(Component)]
pub struct SelectionOutline;

impl MarkerAssets {
    pub fn mesh(&self, shape: MarkerShape) -> Handle<Mesh> {
        match shape {
            MarkerShape::Box => self.boxm.clone(),
            MarkerShape::Sphere => self.sphere.clone(),
            MarkerShape::Cylinder => self.cylinder.clone(),
        }
    }
}

/// Local-space bounds of the entity's model.
#[derive(Component, Clone, Copy)]
pub struct LocalBounds(pub Aabb);

/// State while a gizmo translation drag is in progress (used for neighbour snapping).
#[derive(Resource, Default)]
pub struct DragState {
    pub active: bool,
    pub start_locals: HashMap<Entity, Vec3>,
    pub start_aabb: Option<Aabb>,
    pub others: Vec<Aabb>,
}

pub fn xform_to_transform(x: &Xform) -> Transform {
    Transform {
        translation: Vec3::from(x.translation),
        rotation: Quat::from_xyzw(x.rotation[0], x.rotation[1], x.rotation[2], x.rotation[3]),
        scale: Vec3::from(x.scale),
    }
}

pub fn transform_to_xform(t: &Transform) -> Xform {
    Xform { translation: t.translation.to_array(), rotation: t.rotation.to_array(), scale: t.scale.to_array() }
}
