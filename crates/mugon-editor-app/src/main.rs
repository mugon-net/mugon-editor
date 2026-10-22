//! Mugon Editor scene editor: place library models, edit them and export BSN.

mod actions;
mod camera;
mod geom;
mod interact;
mod markers;
mod model;
mod outline;
mod scene_sync;
mod ui;

use anyhow::Result;
use mugon_editor_core::index::Index;
use mugon_editor_core::Library as CoreLibrary;
use mugon_editor_scene::game::GameConfig;
use mugon_editor_scene::{Project, Xform};
use bevy::diagnostic::FrameCount;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::camera::visibility::RenderLayers;
use bevy::camera::CameraOutputMode;
use bevy::render::render_resource::BlendState;
use bevy_egui::{EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext};
use camera::FlyCam;
use clap::Parser;
use model::*;
use std::path::PathBuf;
use std::sync::Arc;
use transform_gizmo_bevy::{GizmoCamera, GizmoOptions, TransformGizmoPlugin};

#[derive(Parser)]
#[command(name = "mugon-editor-app", version)]
struct Args {
    /// Library folder (default: `[editor] library` in ~/.mugon/config.toml).
    #[arg(long, env = "MUGON_EDITOR_LIBRARY")]
    library: Option<PathBuf>,
    /// Game folder with the mugon.toml whose [editor] table configures components and folders (default: the current directory).
    #[arg(long)]
    game_dir: Option<PathBuf>,
    /// Scene file to open.
    #[arg(long)]
    scene: Option<PathBuf>,
    /// Asset to arm for placing (set by the browser's "Open in editor"); the next viewport click places it.
    #[arg(long)]
    asset: Option<String>,
    /// Exit after this many frames (smoke test).
    #[arg(long)]
    smoke: Option<u32>,
    /// Save a screenshot of the window after loading and exit.
    #[arg(long)]
    screenshot: Option<PathBuf>,
    /// Start with the shortcut hint expanded.
    #[arg(long)]
    show_help: bool,
    /// Fill a new scene with some assets from the library (for testing).
    #[arg(long)]
    demo: bool,
    /// Frame rate cap (0 = limited only by the display's vsync).
    #[arg(long, default_value_t = 60)]
    max_fps: u32,
}

#[derive(Resource)]
struct FrameLimit(Option<std::time::Duration>);

/// Sleeps at the end of the frame so the editor does not render faster than the cap (vsync alone follows the monitor).
fn frame_limit(limit: Res<FrameLimit>, mut last: Local<Option<std::time::Instant>>) {
    if let (Some(target), Some(t)) = (limit.0, *last) {
        if let Some(rest) = target.checked_sub(t.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    *last = Some(std::time::Instant::now());
}

#[derive(Resource)]
struct Launch {
    smoke: Option<u32>,
    screenshot: Option<PathBuf>,
    shot_taken_at: Option<u32>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let library_dir = mugon_editor_core::config::configured_library(args.library.clone())?
        .ok_or_else(|| anyhow::anyhow!("no library configured: pass --library, set MUGON_EDITOR_LIBRARY, or run `mugon-editor config --set-library <dir>`"))?;
    let library = CoreLibrary::new(&library_dir);
    let index = Arc::new(Index::load(&library)?);
    let game = GameConfig::load(&mugon_editor_scene::game::game_dir(args.game_dir.clone())?)?;
    let registry = game.registry();

    let mut editor = EditorProject::default();
    if let Some(p) = &args.scene {
        editor.project = Project::load(p)?;
        editor.path = Some(p.clone());
    } else if args.demo {
        editor.project = demo_project(&index);
    }
    editor.mark_saved();
    let mut settings = Settings::default();
    settings.help_open = args.show_help;
    settings.placing = args.asset.clone().filter(|a| index.get(a).is_some());

    App::new()
        .add_plugins(DefaultPlugins.set(AssetPlugin { file_path: library_dir.to_string_lossy().to_string(), ..default() }).set(WindowPlugin {
            primary_window: Some(Window { title: "Mugon Editor".into(), resolution: (1600, 900).into(), ..default() }),
            ..default()
        }))
        .add_plugins((EguiPlugin::default(), TransformGizmoPlugin))
        .insert_resource(ClearColor(Color::srgb(0.09, 0.1, 0.12)))
        .insert_resource(GlobalAmbientLight { color: Color::WHITE, brightness: 600.0, ..default() })
        .insert_resource(Library { index, registry, game })
        .insert_resource(editor)
        .insert_resource(settings)
        .insert_resource(FrameLimit((args.max_fps > 0).then(|| std::time::Duration::from_secs_f64(1.0 / args.max_fps as f64))))
        .add_systems(Last, frame_limit)
        .insert_resource(Launch { smoke: args.smoke, screenshot: args.screenshot.clone(), shot_taken_at: None })
        .init_resource::<History>()
        .init_resource::<Selection>()
        .init_resource::<SceneClipboard>()
        .init_resource::<UiState>()
        .init_resource::<DragState>()
        .add_systems(Startup, (setup, markers::setup_marker_assets, set_window_icon))
        .add_systems(EguiPrimaryContextPass, ui::ui_system)
        .add_systems(
            Update,
            (
                camera::camera_controls,
                interact::configure_gizmo,
                interact::viewport_click,
                interact::shortcuts,
                interact::clipboard_shortcuts,
                scene_sync::reconcile,
                scene_sync::sync_gizmo_targets,
                camera::draw_grid,
                markers::draw_marker_outlines,
                markers::update_marker_colors,
                outline::sync_selection_outlines,
                lifecycle,
            ),
        )
        .add_systems(PostUpdate, (scene_sync::transforms_to_project, interact::face_snap_drag.after(TransformSystems::Propagate)).chain())
        .run();
    Ok(())
}

fn demo_project(index: &Index) -> Project {
    let mut p = Project::default();
    let mut x = 0.0;
    for a in index.iter().filter(|a| a.category == "Props" && a.texture.is_some()).take(8) {
        let width = (a.bounds.max[0] - a.bounds.min[0]).max(0.5);
        p.add(a.name.clone(), Some(a.id.clone()), Xform { translation: [x, 0.0, 0.0], ..Default::default() }, None);
        x += width + 0.5;
    }
    p.add_marker("Trigger Zone", mugon_editor_scene::MarkerShape::Box, Xform { translation: [2.0, 1.0, 3.0], scale: [4.0, 2.0, 3.0], ..Default::default() }, None);
    p.add_marker("Spawn Radius", mugon_editor_scene::MarkerShape::Sphere, Xform { translation: [8.0, 1.5, 3.0], scale: [3.0, 3.0, 3.0], ..Default::default() }, None);
    p.add_marker("Pillar Zone", mugon_editor_scene::MarkerShape::Cylinder, Xform { translation: [-3.0, 1.0, 3.0], scale: [2.0, 2.0, 2.0], ..Default::default() }, None);
    p
}

fn setup(mut commands: Commands, mut egui_settings: ResMut<EguiGlobalSettings>) {
    // egui gets its own full-window camera; the 3D camera's viewport shrinks to the free area between panels.
    egui_settings.auto_create_primary_context = false;
    commands.spawn((
        PrimaryEguiContext,
        Camera2d,
        RenderLayers::none(),
        Camera {
            order: 1,
            output_mode: CameraOutputMode::Write { blend_state: Some(BlendState::ALPHA_BLENDING), clear_color: ClearColorConfig::None },
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
    ));
    commands.spawn((
        Camera3d::default(),
        Msaa::Sample4,
        Transform::from_xyz(4.0, 3.0, 8.0),
        FlyCam { yaw: 0.0, pitch: -0.25, speed: 6.0 },
        GizmoCamera,
    ));
    commands.spawn((
        DirectionalLight { illuminance: 12_000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(8.0, 14.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Title-bar icon (the executable's own icon is embedded by build.rs).
/// `NonSendMarker` pins the system to the main thread: bevy_winit keeps its windows in a thread-local.
fn set_window_icon(window: Single<Entity, With<bevy::window::PrimaryWindow>>, _main_thread: bevy::ecs::system::NonSendMarker) {
    const SIZE: u32 = 64;
    let Ok(icon) = winit::window::Icon::from_rgba(include_bytes!("../assets/icon-64.rgba").to_vec(), SIZE, SIZE) else { return };
    bevy::winit::WINIT_WINDOWS.with_borrow(|windows| {
        if let Some(window) = windows.get_window(*window) {
            window.set_window_icon(Some(icon));
        }
    });
}

/// `--smoke` / `--screenshot` handling.
fn lifecycle(mut commands: Commands, frames: Res<FrameCount>, mut launch: ResMut<Launch>, mut exit: MessageWriter<AppExit>, ui: Res<UiState>) {
    let f = frames.0;
    if let Some(n) = launch.smoke {
        if f >= n {
            exit.write(AppExit::Success);
        }
    }
    if let Some(path) = launch.screenshot.clone() {
        if launch.shot_taken_at.is_none() && f >= 240 && ui.ready {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
            launch.shot_taken_at = Some(f);
        }
        if launch.shot_taken_at.is_some_and(|t| f >= t + 30) {
            exit.write(AppExit::Success);
        }
    }
}

#[allow(dead_code)]
fn _opts(_: GizmoOptions) {}
