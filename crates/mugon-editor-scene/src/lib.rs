//! Editor project model (the source of truth for a scene), component definitions and BSN export.
//!
//! Bevy-free so that the exporter, the bundler and their tests stay light.

pub mod components;
pub mod export;
pub mod game;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

pub const PROJECT_VERSION: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Xform {
    pub translation: [f32; 3],
    /// Quaternion (x, y, z, w).
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

impl Default for Xform {
    fn default() -> Self {
        Self { translation: [0.0; 3], rotation: [0.0, 0.0, 0.0, 1.0], scale: [1.0; 3] }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentInstance {
    pub name: String,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
}

/// Volume of a marker: a unit shape (1 m box / sphere of diameter 1 / cylinder of diameter 1 and height 1)
/// whose size is the entity's transform scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MarkerShape {
    #[default]
    Box,
    Sphere,
    Cylinder,
}

impl MarkerShape {
    pub const ALL: [MarkerShape; 3] = [MarkerShape::Box, MarkerShape::Sphere, MarkerShape::Cylinder];

    /// Variant name as written in the exported BSN (`MarkerShape::Box`).
    pub fn variant(self) -> &'static str {
        match self {
            MarkerShape::Box => "Box",
            MarkerShape::Sphere => "Sphere",
            MarkerShape::Cylinder => "Cylinder",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneEntity {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub parent: Option<u64>,
    /// Library asset id (`<pack>/<name>`), if the entity shows a model.
    #[serde(default)]
    pub asset: Option<String>,
    /// Set for marker entities (a volume placeholder instead of a model).
    #[serde(default)]
    pub marker: Option<MarkerShape>,
    #[serde(default)]
    pub transform: Xform,
    #[serde(default)]
    pub components: Vec<ComponentInstance>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub version: u32,
    pub name: String,
    pub next_id: u64,
    pub entities: Vec<SceneEntity>,
    /// Saving also copies the used assets into the game and writes the BSN file.
    #[serde(default)]
    pub export_on_save: bool,
    /// Library asset ids pinned in the editor's palette, sorted.
    #[serde(default)]
    pub favorites: Vec<String>,
}

impl Default for Project {
    fn default() -> Self {
        Self { version: PROJECT_VERSION, name: "scene".into(), next_id: 1, entities: Vec::new(), export_on_save: false, favorites: Vec::new() }
    }
}

impl Project {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn is_favorite(&self, asset_id: &str) -> bool {
        self.favorites.binary_search_by(|id| id.as_str().cmp(asset_id)).is_ok()
    }

    pub fn toggle_favorite(&mut self, asset_id: &str) {
        match self.favorites.binary_search_by(|id| id.as_str().cmp(asset_id)) {
            Ok(index) => {
                self.favorites.remove(index);
            }
            Err(index) => self.favorites.insert(index, asset_id.to_string()),
        }
    }

    pub fn add(&mut self, name: impl Into<String>, asset: Option<String>, transform: Xform, parent: Option<u64>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.entities.push(SceneEntity { id, name: name.into(), parent, asset, marker: None, transform, components: Vec::new() });
        id
    }

    pub fn add_marker(&mut self, name: impl Into<String>, shape: MarkerShape, transform: Xform, parent: Option<u64>) -> u64 {
        let id = self.add(name, None, transform, parent);
        if let Some(e) = self.get_mut(id) {
            e.marker = Some(shape);
        }
        id
    }

    pub fn get(&self, id: u64) -> Option<&SceneEntity> {
        self.entities.iter().find(|e| e.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut SceneEntity> {
        self.entities.iter_mut().find(|e| e.id == id)
    }

    pub fn children(&self, id: Option<u64>) -> impl Iterator<Item = &SceneEntity> {
        self.entities.iter().filter(move |e| e.parent == id)
    }

    /// Removes an entity and all its descendants.
    pub fn remove(&mut self, id: u64) {
        let mut doomed = vec![id];
        let mut i = 0;
        while i < doomed.len() {
            let cur = doomed[i];
            doomed.extend(self.entities.iter().filter(|e| e.parent == Some(cur)).map(|e| e.id));
            i += 1;
        }
        self.entities.retain(|e| !doomed.contains(&e.id));
    }

    /// Duplicates an entity with its subtree; returns the id of the copy of `id`.
    pub fn duplicate(&mut self, id: u64) -> Option<u64> {
        let root = self.get(id)?.clone();
        let new_root = self.clone_subtree(&root, root.parent);
        Some(new_root)
    }

    /// The subtrees rooted at `ids` (a root whose ancestor is also listed is skipped), parents before children.
    pub fn copy_subtrees(&self, ids: &[u64]) -> Vec<SceneEntity> {
        let is_under_listed = |mut cur: u64| {
            while let Some(parent) = self.get(cur).and_then(|e| e.parent) {
                if ids.contains(&parent) {
                    return true;
                }
                cur = parent;
            }
            false
        };
        let mut out: Vec<SceneEntity> = Vec::new();
        for id in ids.iter().filter(|id| !is_under_listed(**id)) {
            let Some(root) = self.get(*id) else { continue };
            let start = out.len();
            out.push(root.clone());
            let mut i = start;
            while i < out.len() {
                let cur = out[i].id;
                out.extend(self.children(Some(cur)).cloned());
                i += 1;
            }
        }
        out
    }

    /// Pastes entities from `copy_subtrees` with fresh ids. Roots (entities whose parent is not part of the
    /// copy) keep their parent if it still exists, otherwise become top-level, and are moved by `offset`.
    /// Returns the ids of the pasted roots.
    pub fn paste(&mut self, copied: &[SceneEntity], offset: [f32; 3]) -> Vec<u64> {
        let mut map: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
        let mut roots = Vec::new();
        for src in copied {
            let new_id = self.next_id;
            self.next_id += 1;
            map.insert(src.id, new_id);
            let mut e = SceneEntity { id: new_id, ..src.clone() };
            match src.parent.and_then(|p| map.get(&p).copied()) {
                Some(p) => e.parent = Some(p),
                None => {
                    e.parent = src.parent.filter(|p| self.get(*p).is_some());
                    for (t, o) in e.transform.translation.iter_mut().zip(offset) {
                        *t += o;
                    }
                    roots.push(new_id);
                }
            }
            self.entities.push(e);
        }
        roots
    }

    fn clone_subtree(&mut self, src: &SceneEntity, parent: Option<u64>) -> u64 {
        let new_id = self.next_id;
        self.next_id += 1;
        self.entities.push(SceneEntity { id: new_id, parent, ..src.clone() });
        let kids: Vec<SceneEntity> = self.children(Some(src.id)).cloned().collect();
        for k in kids {
            self.clone_subtree(&k, Some(new_id));
        }
        new_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_and_duplicate_subtrees() {
        let mut p = Project::default();
        let a = p.add("a", None, Xform::default(), None);
        let b = p.add("b", None, Xform::default(), Some(a));
        let _c = p.add("c", None, Xform::default(), Some(b));
        let copy = p.duplicate(a).unwrap();
        assert_eq!(p.entities.len(), 6);
        assert_eq!(p.children(Some(copy)).count(), 1);
        p.remove(a);
        assert_eq!(p.entities.len(), 3);
    }

    #[test]
    fn copy_paste_subtrees() {
        let mut p = Project::default();
        let a = p.add("a", None, Xform::default(), None);
        let b = p.add("b", None, Xform::default(), Some(a));
        let c = p.add("c", None, Xform::default(), None);
        // `b` is already covered by `a`
        let copied = p.copy_subtrees(&[a, b, c]);
        assert_eq!(copied.len(), 3);
        let roots = p.paste(&copied, [1.0, 0.0, 0.0]);
        assert_eq!(roots.len(), 2);
        assert_eq!(p.entities.len(), 6);
        assert_eq!(p.get(roots[0]).unwrap().transform.translation[0], 1.0);
        assert_eq!(p.children(Some(roots[0])).count(), 1);
        // the child keeps its local transform
        let kid = p.children(Some(roots[0])).next().unwrap();
        assert_eq!(kid.transform.translation[0], 0.0);
    }

    #[test]
    fn favorites_toggle_sorted_and_roundtrip() {
        let mut p = Project::default();
        p.toggle_favorite("B/Two");
        p.toggle_favorite("A/One");
        p.toggle_favorite("B/Two");
        p.toggle_favorite("C/Three");
        assert_eq!(p.favorites, ["A/One", "C/Three"]);
        assert!(p.is_favorite("A/One") && !p.is_favorite("B/Two"));
        let reloaded: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(reloaded.favorites, p.favorites);
        let old: Project = serde_json::from_str(r#"{"version":2,"name":"x","next_id":1,"entities":[]}"#).unwrap();
        assert!(old.favorites.is_empty());
    }

    #[test]
    fn save_load_roundtrip() {
        let mut p = Project::default();
        let id = p.add("barrel", Some("Pack/X".into()), Xform::default(), None);
        p.get_mut(id).unwrap().components.push(ComponentInstance { name: "Health".into(), values: BTreeMap::from([("max".into(), Value::from(50.0))]) });
        let dir = std::env::temp_dir().join("mugon-editor-scene-test.json");
        p.save(&dir).unwrap();
        assert_eq!(Project::load(&dir).unwrap(), p);
    }
}
