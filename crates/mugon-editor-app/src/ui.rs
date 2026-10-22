//! egui panels: menu, asset palette, hierarchy, inspector, status bar.

use crate::actions::{scene_name, Ctx};
use crate::camera::FlyCam;
use crate::model::*;
use mugon_editor_core::index::Query;
use mugon_editor_core::Asset;
use mugon_editor_scene::components::{FieldType, FieldDef};
use mugon_editor_scene::export::ExportOptions;
use mugon_editor_scene::game::{scene_identifier, SCENE_EXTENSION};
use mugon_editor_scene::{ComponentInstance, MarkerShape, Project, SceneEntity};
use bevy::camera::Viewport;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContexts, EguiTextureHandle};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const THUMB: f32 = 64.0;

#[derive(Default)]
pub struct Palette {
    query: String,
    pack: String,
    category: String,
    key: String,
    ids: Vec<String>,
    hierarchy_query: String,
    tab: LeftTab,
    scenes: Vec<PathBuf>,
    scenes_scanned: f64,
    /// Scene switch waiting for the answer to the "unsaved changes" prompt.
    pending: Option<Pending>,
    /// Image / label shown under the cursor while dragging an item out of the palette.
    ghost: (Option<egui::TextureId>, String),
}

#[derive(Default, Clone, Copy, PartialEq)]
enum LeftTab {
    #[default]
    Assets,
    Scenes,
}

/// Replaces the open scene; asks to save first when there are unsaved changes.
#[derive(Clone)]
enum Pending {
    New,
    Open(PathBuf),
}

impl Pending {
    fn action(self) -> Action {
        match self {
            Pending::New => Action::New,
            Pending::Open(path) => Action::Open(path),
        }
    }
}

pub struct ExportDialog {
    open: bool,
    rs_path: String,
    fn_name: String,
    uses: String,
}

impl Default for ExportDialog {
    fn default() -> Self {
        Self { open: false, rs_path: "scene.rs".into(), fn_name: "scene".into(), uses: String::new() }
    }
}

#[derive(Default)]
pub struct Thumbs(HashMap<String, Option<egui::TextureId>>);

enum Action {
    New,
    Open(PathBuf),
    Save,
    SaveAs(PathBuf),
    Undo,
    Redo,
    Delete,
    Duplicate,
    Copy,
    Paste,
    AddEmpty,
    AddMarker(MarkerShape),
    Reparent(u64, Option<u64>),
    AddComponent(u64, String),
    RemoveComponent(u64, usize),
    Export,
    SetExportOnSave(bool),
    ToggleFavorite(String),
}

#[allow(clippy::too_many_arguments)]
pub fn ui_system(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<UiState>,
    library: Res<Library>,
    mut editor: ResMut<EditorProject>,
    mut history: ResMut<History>,
    mut selection: ResMut<Selection>,
    mut clipboard: ResMut<SceneClipboard>,
    mut settings: ResMut<Settings>,
    asset_server: Res<AssetServer>,
    time: Res<Time>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut camera: Single<&mut Camera, With<FlyCam>>,
    mut palette: Local<Palette>,
    mut thumbs: Local<Thumbs>,
    mut export: Local<ExportDialog>,
    mut keep_alive: Local<Vec<Handle<Image>>>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let mut actions: Vec<Action> = Vec::new();
    let before = editor.project.clone();
    let mut switch: Option<Pending> = None;

    let mut root = egui::Ui::new(
        ctx.clone(),
        "viewport".into(),
        egui::UiBuilder::new().layer_id(egui::LayerId::background()).max_rect(ctx.viewport_rect()),
    );

    // ---- top bar
    let top = egui::Panel::top("top").show(&mut root, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() { switch = Some(Pending::New); ui.close(); }
                if ui.button("Open…").clicked() {
                    if let Some(p) = scene_dialog(&library, &editor).add_filter("Mugon scene", &["json"]).pick_file() { switch = Some(Pending::Open(p)); }
                    ui.close();
                }
                if ui.button("Save (Ctrl+S)").clicked() {
                    if editor.path.is_some() { actions.push(Action::Save) } else if let Some(p) = scene_dialog(&library, &editor).set_file_name(default_scene_file(&editor)).save_file() { actions.push(Action::SaveAs(p)) }
                    ui.close();
                }
                if ui.button("Save as…").clicked() {
                    if let Some(p) = scene_dialog(&library, &editor).set_file_name(default_scene_file(&editor)).save_file() { actions.push(Action::SaveAs(p)); }
                    ui.close();
                }
                ui.separator();
                let mut export_on_save = editor.project.export_on_save;
                if ui.checkbox(&mut export_on_save, "Export on save").on_hover_text("Also copies the used assets and writes the BSN file into the game folders").changed() {
                    actions.push(Action::SetExportOnSave(export_on_save));
                }
                if ui.button("Export BSN…").clicked() {
                    let name = editor.path.as_deref().map(scene_name).unwrap_or_else(|| editor.project.name.clone());
                    let ident = scene_identifier(&name);
                    export.rs_path = library.game.scenes_bsn_dir.join(format!("{ident}.rs")).to_string_lossy().to_string();
                    export.fn_name = ident;
                    export.uses = library.game.bsn_uses.join(", ");
                    export.open = true;
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.add_enabled(!history.undo.is_empty(), egui::Button::new("Undo (Ctrl+Z)")).clicked() { actions.push(Action::Undo); ui.close(); }
                if ui.add_enabled(!history.redo.is_empty(), egui::Button::new("Redo (Ctrl+Y)")).clicked() { actions.push(Action::Redo); ui.close(); }
                ui.separator();
                if ui.button("Copy (Ctrl+C)").clicked() { actions.push(Action::Copy); ui.close(); }
                if ui.button("Paste (Ctrl+V)").clicked() { actions.push(Action::Paste); ui.close(); }
                if ui.button("Duplicate (Ctrl+D)").clicked() { actions.push(Action::Duplicate); ui.close(); }
                if ui.button("Delete (Del)").clicked() { actions.push(Action::Delete); ui.close(); }
                if ui.button("Add empty entity").clicked() { actions.push(Action::AddEmpty); ui.close(); }
                ui.menu_button("Add marker", |ui| {
                    for shape in MarkerShape::ALL {
                        if ui.button(shape.variant()).clicked() { actions.push(Action::AddMarker(shape)); ui.close(); }
                    }
                });
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Shortcuts (F1)").clicked() { settings.help_open = !settings.help_open; ui.close(); }
            });
            ui.separator();
            ui.checkbox(&mut settings.grid_snap, "Grid");
            ui.add(egui::DragValue::new(&mut settings.grid_size).speed(0.05).range(0.05..=50.0).suffix(" m"));
            ui.checkbox(&mut settings.face_snap, "Snap to neighbours");
            ui.add(egui::DragValue::new(&mut settings.face_threshold).speed(0.01).range(0.01..=5.0).suffix(" m"));
            ui.label("rot");
            ui.add(egui::DragValue::new(&mut settings.angle_snap_deg).speed(1.0).range(1.0..=90.0).suffix("°"));
        });
    });

    // ---- bottom status bar
    let bottom = egui::Panel::bottom("status").show(&mut root, |ui| {
        ui.horizontal(|ui| {
            let name = editor.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "unsaved scene".into());
            let unsaved = if editor.is_dirty() { " *" } else { "" };
            ui.label(format!("{name}{unsaved}  ·  {} entities", editor.project.entities.len()));
            if let Some(a) = &settings.placing {
                ui.colored_label(egui::Color32::LIGHT_GREEN, format!("placing {a} (click in the viewport, Esc to stop)"));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(&settings.status));
        });
    });

    // ---- left: asset palette
    let mut ghost_start: Option<(Option<egui::TextureId>, String)> = None;
    let left = egui::Panel::left("palette").default_size(300.0).resizable(true).show(&mut root, |ui| {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut palette.tab, LeftTab::Assets, "Assets");
            ui.selectable_value(&mut palette.tab, LeftTab::Scenes, "Scenes");
        });
        ui.separator();
        if palette.tab == LeftTab::Scenes {
            scenes_tab(ui, &library, &editor, &mut palette, time.elapsed_secs_f64(), &mut switch);
        } else {
            ui.heading("Assets");
            ui.add(egui::TextEdit::singleline(&mut palette.query).hint_text("search…").desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                let packs = library.index.pack_facets();
                egui::ComboBox::from_id_salt("pack").selected_text(if palette.pack.is_empty() { "all packs" } else { &palette.pack }.to_string()).width(130.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut palette.pack, String::new(), "all packs");
                    for p in &packs { ui.selectable_value(&mut palette.pack, p.name.clone(), format!("{} ({})", p.name, p.count)); }
                });
                let cats = library.index.categories(Some(&palette.pack));
                egui::ComboBox::from_id_salt("cat").selected_text(if palette.category.is_empty() { "all categories" } else { &palette.category }.to_string()).width(130.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut palette.category, String::new(), "all categories");
                    for c in &cats { ui.selectable_value(&mut palette.category, c.name.clone(), format!("{} ({})", c.name, c.count)); }
                });
            });
            let key = format!("{}|{}|{}", palette.query, palette.pack, palette.category);
            if key != palette.key {
                let q = Query { q: palette.query.clone(), pack: Some(palette.pack.clone()), category: Some(palette.category.clone()), ..Default::default() };
                let hide_collision = palette.category != "Collision";
                palette.ids = library.index.search(&q).assets.iter().filter(|a| !(hide_collision && a.category == "Collision")).map(|a| a.id.clone()).collect();
                palette.key = key;
            }
            ui.horizontal(|ui| {
                ui.label("Markers:");
                for shape in MarkerShape::ALL {
                    let response = ui.add(egui::Button::new(shape.variant()).sense(egui::Sense::click_and_drag()));
                    if response.on_hover_text("Volume placeholder: drag into the viewport. Size = scale.").drag_started() {
                        settings.dragging = Some(Payload::Marker(shape));
                        ghost_start = Some((None, format!("{} marker", shape.variant())));
                    }
                }
            });
            let mut tiles = Tiles {
                library: &library,
                asset_server: &asset_server,
                contexts: &mut contexts,
                thumbs: &mut thumbs,
                keep_alive: &mut keep_alive,
                settings: &mut settings,
                favorites: &editor.project.favorites,
                actions: &mut actions,
                ghost_start: &mut ghost_start,
            };
            let query = palette.query.to_lowercase();
            let mut favorites: Vec<&Asset> = editor.project.favorites.iter().filter_map(|id| library.index.get(id)).filter(|asset| query.is_empty() || asset.name.to_lowercase().contains(&query) || asset.id.to_lowercase().contains(&query)).collect();
            favorites.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
            if !favorites.is_empty() {
                egui::CollapsingHeader::new(format!("★ Favorites ({})", favorites.len())).id_salt("favorites_header").default_open(true).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for asset in &favorites {
                            asset_tile(ui, &mut tiles, "favorites", &asset.id);
                        }
                    });
                });
            }
            ui.label(format!("{} assets", palette.ids.len()));
            let cell = THUMB + ui.spacing().item_spacing.x;
            let cols = ((ui.available_width() / cell).floor() as usize).max(1);
            let rows = palette.ids.len().div_ceil(cols);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, THUMB + ui.spacing().item_spacing.y, rows, |ui, range| {
                for row in range {
                    ui.horizontal(|ui| {
                        for id in palette.ids.iter().skip(row * cols).take(cols) {
                            asset_tile(ui, &mut tiles, "grid", id);
                        }
                    });
                }
            });
        }
    });

    if let Some(g) = ghost_start {
        palette.ghost = g;
    }

    // ---- right: hierarchy + inspector
    let right = egui::Panel::right("inspector").default_size(320.0).resizable(true).show(&mut root, |ui| {
        ui.heading("Hierarchy");
        ui.add(egui::TextEdit::singleline(&mut palette.hierarchy_query).hint_text("search name, model, component…").desired_width(f32::INFINITY));
        let visible = hierarchy_matches(&editor.project, &palette.hierarchy_query);
        egui::ScrollArea::vertical().id_salt("hier").max_height(ui.available_height() * 0.4).show(ui, |ui| {
            let roots: Vec<u64> = editor.project.children(None).map(|e| e.id).collect();
            for id in roots {
                hierarchy_node(ui, &editor.project, id, &mut selection, &ctx, visible.as_ref());
            }
            if visible.as_ref().is_some_and(HashSet::is_empty) {
                ui.weak("No matches.");
            }
        });
        ui.separator();
        ui.heading("Inspector");
        egui::ScrollArea::vertical().id_salt("insp").show(ui, |ui| {
            match selection.0.first().copied() {
                Some(id) if editor.project.get(id).is_some() => inspector(ui, &mut editor.project, id, &library, &mut actions),
                _ => { ui.label("Nothing selected."); }
            }
        });
    });

    // ---- floating export window
    if export.open {
        let mut open = true;
        egui::Window::new("Export BSN").open(&mut open).show(&ctx, |ui| {
            ui.label("Writes a Rust file with a bsn_list! scene. Asset copying happens with \"Export on save\".");
            egui::Grid::new("export").num_columns(2).show(ui, |ui| {
                ui.label("Rust file"); ui.text_edit_singleline(&mut export.rs_path); ui.end_row();
                ui.label("Function name"); ui.text_edit_singleline(&mut export.fn_name); ui.end_row();
                ui.label("use lines (comma sep.)"); ui.text_edit_singleline(&mut export.uses); ui.end_row();
            });
            if ui.button("Export").clicked() { actions.push(Action::Export); }
        });
        export.open = open;
    }

    if !ctx.egui_wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::F1)) {
        settings.help_open = !settings.help_open;
    }

    if let Some(target) = switch {
        if editor.is_dirty() {
            palette.pending = Some(target);
        } else {
            actions.push(target.action());
        }
    }
    if let Some(target) = palette.pending.clone() {
        let mut choice = None;
        egui::Window::new("Unsaved changes").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(&ctx, |ui| {
            ui.label("The scene has unsaved changes. Save them first?");
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() { choice = Some(true); }
                if ui.button("Don't save").clicked() { choice = Some(false); }
                if ui.button("Cancel").clicked() { palette.pending = None; }
            });
        });
        if let Some(save_first) = choice {
            let proceed = if !save_first {
                true
            } else if editor.path.is_some() {
                actions.push(Action::Save);
                true
            } else if let Some(path) = scene_dialog(&library, &editor).set_file_name(default_scene_file(&editor)).save_file() {
                actions.push(Action::SaveAs(path));
                true
            } else {
                false
            };
            if proceed {
                actions.push(target.action());
            }
            palette.pending = None;
        }
    }

    // ---- viewport rect from the panel sizes
    let scale = window.scale_factor();
    let (l, r, t, b) = (left.response.rect.width(), right.response.rect.width(), top.response.rect.height(), bottom.response.rect.height());
    let (w, h) = (window.width(), window.height());
    if w > l + r + 10.0 && h > t + b + 10.0 {
        camera.viewport = Some(Viewport {
            physical_position: UVec2::new((l * scale) as u32, (t * scale) as u32),
            physical_size: UVec2::new(((w - l - r) * scale) as u32, ((h - t - b) * scale) as u32),
            ..default()
        });
        ui_state.viewport = Rect::new(l, t, w - r, h - b);
        ui_state.ready = true;
    }
    // Unity-style tool buttons floating over the viewport's right edge
    egui::Area::new(egui::Id::new("tool_overlay"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-(r + 8.0), t + 8.0))
        .show(&ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                for (tool, icon, tip) in [(Tool::Translate, "✥", "Move (W)"), (Tool::Rotate, "⟳", "Rotate (E)"), (Tool::Scale, "⤢", "Scale (R)")] {
                    let b = egui::Button::new(egui::RichText::new(icon).size(20.0)).min_size(egui::vec2(32.0, 32.0)).selected(settings.tool == tool);
                    if ui.add(b).on_hover_text(tip).clicked() {
                        settings.tool = tool;
                    }
                }
            });
        });
    // collapsible shortcut hint in the viewport's bottom-left corner
    egui::Area::new(egui::Id::new("shortcut_hint"))
        .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(l + 8.0, -(b + 8.0)))
        .show(&ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                let header = egui::CollapsingHeader::new("Shortcuts (F1)").id_salt("shortcut_hint_header").open(Some(settings.help_open)).show(ui, |ui| {
                    shortcut_table(ui);
                });
                if header.header_response.clicked() {
                    settings.help_open = !settings.help_open;
                }
            });
        });
    // ---- drag from the palette: ghost under the cursor, drop into the viewport on release
    if settings.dragging.is_some() {
        let pos = ctx.input(|i| i.pointer.latest_pos());
        if let Some(pos) = pos {
            egui::Area::new(egui::Id::new("drag_ghost")).order(egui::Order::Tooltip).fixed_pos(pos + egui::vec2(12.0, 12.0)).interactable(false).show(&ctx, |ui| {
                match palette.ghost.0 {
                    Some(t) => { ui.add(egui::Image::new(egui::load::SizedTexture::new(t, [THUMB, THUMB])).tint(egui::Color32::from_white_alpha(200))); }
                    None => { ui.label(&palette.ghost.1); }
                }
            });
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            settings.dragging = None;
        } else if ctx.input(|i| i.pointer.any_released()) || !ctx.input(|i| i.pointer.any_down()) {
            let payload = settings.dragging.take();
            if let (Some(payload), Some(pos)) = (payload, pos) {
                let pos = Vec2::new(pos.x, pos.y);
                if ui_state.viewport.contains(pos) {
                    settings.drop = Some((payload, pos));
                }
            }
        }
    }
    if let Some(r) = ui_state.marquee {
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("marquee")));
        let rect = egui::Rect::from_min_max(egui::pos2(r.min.x, r.min.y), egui::pos2(r.max.x, r.max.y));
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(90, 160, 255, 40));
        painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 180, 255)), egui::StrokeKind::Inside);
    }
    ui_state.pointer_over_ui = ctx.is_pointer_over_egui();
    ui_state.wants_keyboard = ctx.egui_wants_keyboard_input();

    // ---- apply: inspector edits become one coalesced undo step, queued actions run afterwards
    let edited = editor.project != before;
    if edited {
        let now = time.elapsed_secs_f64();
        if now - history.last_ui_edit > 0.6 {
            history.push(before);
        }
        history.last_ui_edit = now;
        settings.rebuild = true;
    }
    let mut c = Ctx { editor: &mut editor, history: &mut history, selection: &mut selection, settings: &mut settings };
    for a in actions {
        match a {
            Action::New => c.new_project(),
            Action::Open(p) => c.open(&p),
            Action::Save => c.save(&library, None),
            Action::SaveAs(p) => c.save(&library, Some(p)),
            Action::SetExportOnSave(on) => c.editor.project.export_on_save = on,
            Action::ToggleFavorite(id) => c.editor.project.toggle_favorite(&id),
            Action::Undo => c.undo(),
            Action::Redo => c.redo(),
            Action::Delete => c.delete_selected(),
            Action::Duplicate => c.duplicate_selected(),
            Action::Copy => {
                if !c.selection.0.is_empty() {
                    clipboard.0 = c.editor.project.copy_subtrees(&c.selection.0);
                }
            }
            Action::Paste => {
                if !clipboard.0.is_empty() {
                    c.paste(&clipboard.0, [1.0, 0.0, 0.0]);
                }
            }
            Action::AddEmpty => c.add_empty(),
            Action::AddMarker(shape) => {
                c.place_marker(shape, [0.0, 1.0, 0.0], 2.0);
            }
            Action::Reparent(id, parent) => {
                c.checkpoint();
                if let Some(e) = c.editor.project.get_mut(id) { e.parent = parent; }
                c.settings.rebuild = true;
            }
            Action::AddComponent(id, name) => {
                if let Some(def) = library.registry.get(&name) {
                    c.checkpoint();
                    let values = def.fields.iter().map(|f| (f.name.clone(), f.default_value())).collect();
                    if let Some(e) = c.editor.project.get_mut(id) { e.components.push(ComponentInstance { name, values }); }
                }
            }
            Action::RemoveComponent(id, idx) => {
                c.checkpoint();
                if let Some(e) = c.editor.project.get_mut(id) { if idx < e.components.len() { e.components.remove(idx); } }
            }
            Action::Export => {
                let uses = export.uses.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                let opts = ExportOptions { fn_name: export.fn_name.clone(), asset_prefix: String::new(), uses };
                c.export(&library, &PathBuf::from(&export.rs_path), &opts);
            }
        }
    }
    Ok(())
}

/// Scene files of the game's scene folder; clicking one opens it (after the unsaved-changes prompt).
fn scenes_tab(ui: &mut egui::Ui, library: &Library, editor: &EditorProject, palette: &mut Palette, now: f64, switch: &mut Option<Pending>) {
    ui.heading("Scenes");
    if !library.game.present {
        ui.weak("No mugon.toml in the game folder.");
        return;
    }
    let dir = &library.game.scenes_json_dir;
    if now - palette.scenes_scanned > 1.0 {
        palette.scenes_scanned = now;
        let mut scenes: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.file_name().is_some_and(|name| name.to_string_lossy().ends_with(SCENE_EXTENSION)))
            .collect();
        scenes.sort();
        palette.scenes = scenes;
    }
    if !dir.is_dir() {
        ui.weak(format!("No scene folder yet ({}).", dir.display()));
        return;
    }
    ui.weak(dir.display().to_string());
    ui.label(format!("{} scenes", palette.scenes.len()));
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for path in &palette.scenes {
            let current = editor.path.as_deref() == Some(path.as_path());
            let mut label = scene_name(path);
            if current && editor.is_dirty() {
                label.push_str(" *");
            }
            if ui.selectable_label(current, label).clicked() && !current {
                *switch = Some(Pending::Open(path.clone()));
            }
        }
    });
}

/// File dialog that starts in the game's scene folder (created if missing) when the game has a mugon.toml.
fn scene_dialog(library: &Library, editor: &EditorProject) -> rfd::FileDialog {
    let dialog = rfd::FileDialog::new();
    if library.game.present {
        let dir = editor.path.as_deref().and_then(Path::parent).map(Path::to_path_buf).unwrap_or_else(|| library.game.scenes_json_dir.clone());
        if std::fs::create_dir_all(&dir).is_ok() {
            return dialog.set_directory(dir);
        }
    }
    dialog
}

fn default_scene_file(editor: &EditorProject) -> String {
    format!("{}{SCENE_EXTENSION}", scene_identifier(&editor.project.name))
}

struct Tiles<'a, 'w, 's> {
    library: &'a Library,
    asset_server: &'a AssetServer,
    contexts: &'a mut EguiContexts<'w, 's>,
    thumbs: &'a mut Thumbs,
    keep_alive: &'a mut Vec<Handle<Image>>,
    settings: &'a mut Settings,
    favorites: &'a [String],
    actions: &'a mut Vec<Action>,
    ghost_start: &'a mut Option<(Option<egui::TextureId>, String)>,
}

/// One asset thumbnail: drag it into the viewport, click the star in its corner to (un)favorite it.
/// `section` keeps the star widget ids of the favorites section and the grid apart.
fn asset_tile(ui: &mut egui::Ui, tiles: &mut Tiles, section: &str, id: &str) {
    let Some(asset) = tiles.library.index.get(id) else { return };
    let tex = tiles.thumbs.0.entry(id.to_string()).or_insert_with(|| {
        asset.thumbnail.as_ref().map(|thumbnail| {
            let handle: Handle<Image> = tiles.asset_server.load(format!("{}/{}", asset.pack, thumbnail));
            tiles.keep_alive.push(handle.clone());
            tiles.contexts.add_image(EguiTextureHandle::Strong(handle))
        })
    });
    let armed = tiles.settings.placing.as_deref() == Some(id);
    let response = match tex {
        Some(texture) => ui.add(egui::Button::image(egui::Image::new(egui::load::SizedTexture::new(*texture, [THUMB, THUMB]))).selected(armed).sense(egui::Sense::click_and_drag())),
        None => ui.add_sized([THUMB, THUMB], egui::Button::new(asset.name.chars().take(10).collect::<String>()).selected(armed).sense(egui::Sense::click_and_drag())),
    };
    let favorite = tiles.favorites.binary_search_by(|favorite| favorite.as_str().cmp(id)).is_ok();
    let star_rect = egui::Rect::from_min_size(response.rect.right_top() + egui::vec2(-20.0, 2.0), egui::vec2(18.0, 18.0));
    let star = ui.interact(star_rect, ui.id().with((section, id, "star")), egui::Sense::click());
    if favorite || response.hovered() || star.hovered() {
        let color = if favorite { egui::Color32::from_rgb(255, 200, 40) } else if star.hovered() { egui::Color32::WHITE } else { egui::Color32::from_gray(190) };
        ui.painter().text(star_rect.center(), egui::Align2::CENTER_CENTER, "★", egui::FontId::proportional(16.0), color);
    }
    if star.clicked() {
        tiles.actions.push(Action::ToggleFavorite(id.to_string()));
    }
    let response = response.on_hover_text(format!("{}\n{} · {} tris", asset.name, asset.category, asset.triangles));
    if response.drag_started() {
        tiles.settings.dragging = Some(Payload::Asset(id.to_string()));
        *tiles.ghost_start = Some((*tex, asset.name.clone()));
    }
}

const SHORTCUTS: &[(&str, &[(&str, &str)])] = &[
    ("Camera", &[
        ("Right mouse + W A S D", "fly (Q down, E up, Shift faster)"),
        ("Wheel while flying", "change fly speed"),
        ("Middle mouse drag", "pan"),
        ("Wheel", "dolly forward / back"),
        ("F", "focus selection"),
    ]),
    ("Selecting and placing", &[
        ("Drag thumbnail into the viewport", "place a model (Esc while dragging cancels); also works for Markers"),
        ("Click object / click empty space", "select / deselect"),
        ("Drag a rectangle", "select everything it touches"),
        ("Shift or Ctrl + click or drag", "add to / toggle in selection"),
        ("Ctrl+C / Ctrl+V", "copy / paste the selection (pasted under the cursor)"),
        ("Arrow keys, PageUp / PageDown", "nudge selection 0.1 m (grid size if Grid is on), hold to repeat; Shift = x10"),
    ]),
    ("Tools", &[
        ("W / E / R", "move / rotate / scale gizmo (not while flying with the right mouse button); buttons at the viewport's right edge"),
        ("Toolbar: Grid, Snap to neighbours, rot", "snapping (grid distance, face alignment, rotation step)"),
    ]),
    ("Editing", &[
        ("Ctrl+Z / Ctrl+Y (Ctrl+Shift+Z)", "undo / redo"),
        ("Ctrl+D", "duplicate selection"),
        ("Delete / Backspace", "delete selection"),
        ("Ctrl+S", "save scene"),
        ("F1", "show / hide this hint"),
    ]),
];

fn shortcut_table(ui: &mut egui::Ui) {
    for (section, rows) in SHORTCUTS {
        ui.label(egui::RichText::new(*section).strong());
        egui::Grid::new(*section).num_columns(2).spacing([12.0, 2.0]).show(ui, |ui| {
            for (keys, what) in *rows {
                ui.label(egui::RichText::new(*keys).monospace());
                ui.label(*what);
                ui.end_row();
            }
        });
        ui.add_space(4.0);
    }
}

/// With a non-empty `query`: ids of entities that match it (name, model, marker shape, component) plus their ancestors.
fn hierarchy_matches(project: &Project, query: &str) -> Option<HashSet<u64>> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return None;
    }
    let mut visible = HashSet::new();
    for e in &project.entities {
        let hit = e.name.to_lowercase().contains(&query)
            || e.asset.as_ref().is_some_and(|asset| asset.to_lowercase().contains(&query))
            || e.marker.is_some_and(|shape| format!("{} marker", shape.variant()).to_lowercase().contains(&query))
            || e.components.iter().any(|component| component.name.to_lowercase().contains(&query));
        if hit {
            visible.insert(e.id);
            let mut parent = e.parent;
            while let Some(ancestor) = parent {
                if !visible.insert(ancestor) {
                    break;
                }
                parent = project.get(ancestor).and_then(|entity| entity.parent);
            }
        }
    }
    Some(visible)
}

fn hierarchy_node(ui: &mut egui::Ui, project: &Project, id: u64, selection: &mut Selection, ctx: &egui::Context, visible: Option<&HashSet<u64>>) {
    if visible.is_some_and(|set| !set.contains(&id)) {
        return;
    }
    let Some(e) = project.get(id) else { return };
    let kids: Vec<u64> = project.children(Some(id)).filter(|k| visible.is_none_or(|set| set.contains(&k.id))).map(|k| k.id).collect();
    let label = match &e.asset {
        Some(a) if a.rsplit('/').next() != Some(e.name.as_str()) => format!("{}  ·  {}", e.name, a.rsplit('/').next().unwrap_or(a)),
        Some(_) => e.name.clone(),
        None if e.marker.is_some() => format!("{}  ·  {} marker", e.name, e.marker.map(|m| m.variant().to_lowercase()).unwrap_or_default()),
        None => e.name.clone(),
    };
    let selected = selection.contains(id);
    let additive = ctx.input(|i| i.modifiers.ctrl || i.modifiers.shift);
    let mut clicked = false;
    if kids.is_empty() {
        clicked = ui.selectable_label(selected, label).clicked();
    } else {
        let header = egui::CollapsingHeader::new(label).id_salt(id).default_open(true);
        let header = if visible.is_some() { header.open(Some(true)) } else { header };
        header.show(ui, |ui| {
            for k in kids { hierarchy_node(ui, project, k, selection, ctx, visible); }
        });
    }
    if clicked {
        if additive { selection.toggle(id) } else { selection.set(id) }
    }
}

fn is_descendant(project: &Project, id: u64, of: u64) -> bool {
    let mut cur = project.get(id).and_then(|e| e.parent);
    while let Some(p) = cur {
        if p == of { return true; }
        cur = project.get(p).and_then(|e| e.parent);
    }
    false
}

fn inspector(ui: &mut egui::Ui, project: &mut Project, id: u64, library: &Library, actions: &mut Vec<Action>) {
    let candidates: Vec<(u64, String)> =
        project.entities.iter().filter(|e| e.id != id && !is_descendant(project, e.id, id)).map(|e| (e.id, e.name.clone())).collect();
    let Some(e) = project.get_mut(id) else { return };
    ui.horizontal(|ui| { ui.label("Name"); ui.text_edit_singleline(&mut e.name); });
    if let Some(a) = &e.asset { ui.label(format!("Model: {a}")); }
    if let Some(shape) = &mut e.marker {
        ui.horizontal(|ui| {
            ui.label("Marker shape");
            for s in MarkerShape::ALL {
                ui.selectable_value(shape, s, s.variant());
            }
        });
        ui.weak("The volume is the unit shape scaled by Scale below (use the Scale tool).");
    }

    let current_parent = e.parent;
    let parent_name = current_parent.and_then(|p| candidates.iter().find(|c| c.0 == p).map(|c| c.1.clone())).unwrap_or_else(|| "(none)".into());
    egui::ComboBox::from_label("Parent").selected_text(parent_name).show_ui(ui, |ui| {
        if ui.selectable_label(current_parent.is_none(), "(none)").clicked() && current_parent.is_some() { actions.push(Action::Reparent(id, None)); }
        for (cid, cname) in &candidates {
            if ui.selectable_label(current_parent == Some(*cid), format!("{cname} #{cid}")).clicked() && current_parent != Some(*cid) { actions.push(Action::Reparent(id, Some(*cid))); }
        }
    });

    ui.separator();
    ui.label("Transform");
    vec3_row(ui, "Position", &mut e.transform.translation, 0.05);
    let q = Quat::from_xyzw(e.transform.rotation[0], e.transform.rotation[1], e.transform.rotation[2], e.transform.rotation[3]);
    let (y, x, z) = q.to_euler(EulerRot::YXZ);
    let mut euler = [x.to_degrees(), y.to_degrees(), z.to_degrees()];
    let before = euler;
    vec3_row(ui, "Rotation °", &mut euler, 0.5);
    if euler != before {
        let q = Quat::from_euler(EulerRot::YXZ, euler[1].to_radians(), euler[0].to_radians(), euler[2].to_radians());
        e.transform.rotation = q.to_array();
    }
    vec3_row(ui, "Scale", &mut e.transform.scale, 0.01);

    ui.separator();
    ui.label("Components");
    let mut remove = None;
    for (idx, c) in e.components.iter_mut().enumerate() {
        let Some(def) = library.registry.get(&c.name) else {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("{} (undefined)", c.name));
            continue;
        };
        egui::CollapsingHeader::new(&c.name).id_salt((id, idx)).default_open(true).show(ui, |ui| {
            for f in &def.fields {
                let mut v = c.values.get(&f.name).cloned().unwrap_or_else(|| f.default_value());
                if field_editor(ui, f, &mut v) {
                    c.values.insert(f.name.clone(), v);
                }
            }
            if ui.small_button("remove").clicked() { remove = Some(idx); }
        });
    }
    if let Some(i) = remove { actions.push(Action::RemoveComponent(id, i)); }
    egui::ComboBox::from_label("Add component").selected_text("choose…").show_ui(ui, |ui| {
        for def in &library.registry.components {
            if ui.selectable_label(false, &def.name).clicked() { actions.push(Action::AddComponent(id, def.name.clone())); }
        }
    });
    if library.registry.components.is_empty() {
        ui.weak("No components defined ([[editor.component]] in mugon.toml).");
    }
}

fn vec3_row(ui: &mut egui::Ui, label: &str, v: &mut [f32; 3], speed: f64) {
    ui.horizontal(|ui| {
        ui.label(label);
        for x in v.iter_mut() { ui.add(egui::DragValue::new(x).speed(speed)); }
    });
}

/// Edits one component field; returns true if the value changed.
fn field_editor(ui: &mut egui::Ui, f: &FieldDef, v: &mut Value) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(&f.name);
        match f.ty {
            FieldType::Bool => {
                let mut b = v.as_bool().unwrap_or(false);
                if ui.checkbox(&mut b, "").changed() { *v = json!(b); changed = true; }
            }
            FieldType::I32 => {
                let mut n = v.as_i64().unwrap_or(0);
                if ui.add(egui::DragValue::new(&mut n)).changed() { *v = json!(n); changed = true; }
            }
            FieldType::F32 => {
                let mut n = v.as_f64().unwrap_or(0.0);
                if ui.add(egui::DragValue::new(&mut n).speed(0.05)).changed() { *v = json!(n); changed = true; }
            }
            FieldType::String | FieldType::Asset => {
                let mut s = v.as_str().unwrap_or("").to_string();
                if ui.text_edit_singleline(&mut s).changed() { *v = json!(s); changed = true; }
            }
            FieldType::Vec2 | FieldType::Vec3 => {
                let n = if f.ty == FieldType::Vec2 { 2 } else { 3 };
                let mut a: Vec<f64> = (0..n).map(|i| v.get(i).and_then(Value::as_f64).unwrap_or(0.0)).collect();
                for x in a.iter_mut() { if ui.add(egui::DragValue::new(x).speed(0.05)).changed() { changed = true; } }
                if changed { *v = json!(a); }
            }
            FieldType::Color => {
                let mut c: [f32; 4] = std::array::from_fn(|i| v.get(i).and_then(Value::as_f64).unwrap_or(1.0) as f32);
                if ui.color_edit_button_rgba_unmultiplied(&mut c).changed() { *v = json!(c); changed = true; }
            }
            FieldType::Enum => {
                let mut s = v.as_str().unwrap_or("").to_string();
                egui::ComboBox::from_id_salt(&f.name).selected_text(s.clone()).show_ui(ui, |ui| {
                    for o in &f.options {
                        if ui.selectable_value(&mut s, o.clone(), o).changed() { *v = json!(s.clone()); changed = true; }
                    }
                });
            }
        }
    });
    changed
}

#[allow(dead_code)]
fn _unused(_: &SceneEntity) {}
