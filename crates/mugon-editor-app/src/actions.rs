//! Editing operations shared by the UI and keyboard shortcuts.

use crate::model::*;
use mugon_editor_scene::export::{export_bsn, export_to_game, ExportOptions};
use mugon_editor_scene::game::SCENE_EXTENSION;
use mugon_editor_scene::{MarkerShape, Project, Xform};
use std::path::{Path, PathBuf};

pub struct Ctx<'a> {
    pub editor: &'a mut EditorProject,
    pub history: &'a mut History,
    pub selection: &'a mut Selection,
    pub settings: &'a mut Settings,
}

impl Ctx<'_> {
    /// Records the current project as an undo step. Call before mutating.
    pub fn checkpoint(&mut self) {
        let snapshot = self.editor.project.clone();
        self.history.push(snapshot);
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.history.undo.pop() {
            let current = std::mem::replace(&mut self.editor.project, prev);
            self.editor.project.export_on_save = current.export_on_save;
            self.editor.project.favorites = current.favorites.clone();
            self.history.redo.push(current);
            self.after_restore();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.history.redo.pop() {
            let current = std::mem::replace(&mut self.editor.project, next);
            self.editor.project.export_on_save = current.export_on_save;
            self.editor.project.favorites = current.favorites.clone();
            self.history.undo.push(current);
            self.after_restore();
        }
    }

    fn after_restore(&mut self) {
        let project = &self.editor.project;
        self.selection.0.retain(|id| project.get(*id).is_some());
        self.settings.rebuild = true;
    }

    pub fn delete_selected(&mut self) {
        if self.selection.0.is_empty() {
            return;
        }
        self.checkpoint();
        for id in std::mem::take(&mut self.selection.0) {
            self.editor.project.remove(id);
        }
        self.settings.rebuild = true;
    }

    pub fn duplicate_selected(&mut self) {
        if self.selection.0.is_empty() {
            return;
        }
        self.checkpoint();
        let mut copies = Vec::new();
        for id in self.selection.0.clone() {
            if let Some(copy) = self.editor.project.duplicate(id) {
                // offset so the copy is visible next to the original
                if let Some(e) = self.editor.project.get_mut(copy) {
                    e.transform.translation[0] += 1.0;
                }
                copies.push(copy);
            }
        }
        self.selection.0 = copies;
        self.settings.rebuild = true;
    }

    /// Moves the selection by `delta` (top-level selected entities only; their children follow).
    /// `checkpoint: false` continues the undo step of an earlier call (key repeat).
    pub fn nudge_selected(&mut self, delta: bevy::math::Vec3, checkpoint: bool) {
        let project = &self.editor.project;
        let ids: Vec<u64> = self
            .selection
            .0
            .iter()
            .copied()
            .filter(|id| {
                let mut cur = *id;
                while let Some(p) = project.get(cur).and_then(|e| e.parent) {
                    if self.selection.contains(p) {
                        return false;
                    }
                    cur = p;
                }
                true
            })
            .collect();
        if ids.is_empty() {
            return;
        }
        if checkpoint {
            self.checkpoint();
        }
        for id in ids {
            if let Some(e) = self.editor.project.get_mut(id) {
                for (t, d) in e.transform.translation.iter_mut().zip(delta.to_array()) {
                    *t += d;
                }
            }
        }
        self.settings.rebuild = true;
    }

    /// Pastes `copied` (from `Project::copy_subtrees`) moved by `offset` and selects the pasted roots.
    pub fn paste(&mut self, copied: &[mugon_editor_scene::SceneEntity], offset: [f32; 3]) {
        self.checkpoint();
        self.selection.0 = self.editor.project.paste(copied, offset);
        self.settings.rebuild = true;
    }

    pub fn add_empty(&mut self) {
        self.checkpoint();
        let id = self.editor.project.add("Empty", None, Xform::default(), None);
        self.selection.set(id);
        self.settings.rebuild = true;
    }

    pub fn place(&mut self, asset_id: &str, name: &str, at: [f32; 3]) -> u64 {
        self.checkpoint();
        let xform = Xform { translation: at, ..Default::default() };
        let id = self.editor.project.add(name, Some(asset_id.to_string()), xform, None);
        self.selection.set(id);
        self.settings.rebuild = true;
        id
    }

    /// Adds a marker volume (`size` meters, stored as the transform scale) centred at `at`.
    pub fn place_marker(&mut self, shape: MarkerShape, at: [f32; 3], size: f32) -> u64 {
        self.checkpoint();
        let xform = Xform { translation: at, scale: [size; 3], ..Default::default() };
        let id = self.editor.project.add_marker("Marker", shape, xform, None);
        self.selection.set(id);
        self.settings.rebuild = true;
        id
    }

    pub fn new_project(&mut self) {
        self.editor.project = Project::default();
        self.editor.path = None;
        self.editor.mark_saved();
        self.history.undo.clear();
        self.history.redo.clear();
        self.selection.0.clear();
        self.settings.rebuild = true;
        self.settings.status = "New scene".into();
    }

    pub fn open(&mut self, path: &Path) {
        match Project::load(path) {
            Ok(p) => {
                self.editor.project = p;
                self.editor.path = Some(path.to_path_buf());
                self.editor.mark_saved();
                self.history.undo.clear();
                self.history.redo.clear();
                self.selection.0.clear();
                self.settings.rebuild = true;
                self.settings.status = format!("Opened {}", path.display());
            }
            Err(e) => self.settings.status = format!("Open failed: {e:#}"),
        }
    }

    pub fn save(&mut self, library: &Library, path: Option<PathBuf>) {
        let Some(path) = path.or_else(|| self.editor.path.clone()) else {
            self.settings.status = "No file chosen".into();
            return;
        };
        if let Err(e) = self.editor.project.save(&path) {
            self.settings.status = format!("Save failed: {e:#}");
            return;
        }
        self.editor.mark_saved();
        self.settings.status = format!("Saved {}", path.display());
        if self.editor.project.export_on_save {
            let scene_name = scene_name(&path);
            let exported = export_to_game(&self.editor.project, &library.index, &library.registry, &library.game, &scene_name);
            match exported {
                Ok(out) => {
                    self.settings.status += &format!(", exported {} and copied {} asset files", out.bsn_path.display(), out.bundle.files.len());
                }
                Err(e) => self.settings.status += &format!(", but exporting failed: {e:#}"),
            }
        }
        self.editor.path = Some(path);
    }

    /// Writes the BSN Rust source to `rs_path`.
    pub fn export(&mut self, library: &Library, rs_path: &Path, opts: &ExportOptions) {
        let result = (|| -> anyhow::Result<String> {
            let source = export_bsn(&self.editor.project, &library.index, &library.registry, opts)?;
            if let Some(dir) = rs_path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(rs_path, source)?;
            Ok(format!("Exported {}", rs_path.display()))
        })();
        self.settings.status = match result {
            Ok(m) => m,
            Err(e) => format!("Export failed: {e:#}"),
        };
    }
}

/// Scene name for a scene file: the file name without `.mugon-scene.json`.
pub fn scene_name(path: &Path) -> String {
    let file = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    match file.strip_suffix(SCENE_EXTENSION) {
        Some(name) => name.to_string(),
        None => path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_names_from_files() {
        assert_eq!(scene_name(Path::new("a/level_one.mugon-scene.json")), "level_one");
        assert_eq!(scene_name(Path::new("a/other.json")), "other");
    }

    fn with_ctx(f: impl FnOnce(&mut Ctx)) -> (EditorProject, History, Selection) {
        let (mut editor, mut history, mut selection, mut settings) = (EditorProject::default(), History::default(), Selection::default(), Settings::default());
        f(&mut Ctx { editor: &mut editor, history: &mut history, selection: &mut selection, settings: &mut settings });
        (editor, history, selection)
    }

    #[test]
    fn place_undo_redo_delete() {
        let (editor, history, selection) = with_ctx(|c| {
            let id = c.place("P/Crate", "Crate", [1.0, 0.0, 0.0]);
            assert_eq!(c.selection.0, vec![id]);
            c.undo();
            assert!(c.editor.project.entities.is_empty());
            c.redo();
            assert_eq!(c.editor.project.entities.len(), 1);
            c.selection.set(id); // undo clears the selection of removed entities
            c.duplicate_selected();
            assert_eq!(c.editor.project.entities.len(), 2);
            c.delete_selected();
            assert_eq!(c.editor.project.entities.len(), 1);
            c.undo();
            assert_eq!(c.editor.project.entities.len(), 2);
        });
        assert_eq!(editor.project.entities.len(), 2);
        assert!(!history.undo.is_empty());
        assert!(selection.0.is_empty());
    }
}
