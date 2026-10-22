//! Headless Bevy renderer that writes a PNG thumbnail for every asset of a library.

use anyhow::Result;
use mugon_editor_core::{Asset, Library, Pack};
use bevy::{
    app::SubApps,
    asset::RenderAssetUsages,
    camera::RenderTarget,
    gltf::GltfAssetLabel,
    image::Image,
    prelude::*,
    render::{
        render_resource::{Extent3d, PollType, TextureDimension, TextureFormat, TextureUsages},
        renderer::RenderDevice,
        view::screenshot::{save_to_disk, Screenshot},
        RenderPlugin,
    },
    window::ExitCondition,
    winit::WinitPlugin,
};
use clap::Parser;
use std::path::{Path, PathBuf};

#[derive(Parser)]
struct Args {
    /// Library folder (default: `[editor] library` in ~/.mugon/config.toml).
    #[arg(long, env = "MUGON_EDITOR_LIBRARY")]
    library: Option<PathBuf>,
    /// Only this pack.
    #[arg(long)]
    pack: Option<String>,
    /// Only assets whose name contains this text.
    #[arg(long)]
    filter: Option<String>,
    /// Re-render existing thumbnails.
    #[arg(long)]
    force: bool,
    #[arg(long, default_value_t = 256)]
    size: u32,
    /// Stop after this many assets (for testing).
    #[arg(long)]
    limit: Option<usize>,
}

struct Renderer {
    apps: SubApps,
    target: RenderTarget,
    camera: Entity,
}

impl Renderer {
    fn new(library_root: &Path, size: u32) -> Self {
        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin { primary_window: None, exit_condition: ExitCondition::DontExit, ..default() })
                .set(RenderPlugin { synchronous_pipeline_compilation: true, ..default() })
                .set(AssetPlugin { file_path: library_root.to_string_lossy().to_string(), ..default() })
                .disable::<WinitPlugin>(),
        );
        app.finish();
        app.cleanup();
        let mut apps = std::mem::take(app.sub_apps_mut());

        let mut image = Image::new_uninit(
            Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            TextureDimension::D2,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
        let world = apps.main.world_mut();
        let target: RenderTarget = world.resource_mut::<Assets<Image>>().add(image).into();
        let camera = world
            .spawn((
                Camera3d::default(),
                Camera { clear_color: ClearColorConfig::Custom(Color::srgba(0.0, 0.0, 0.0, 0.0)), ..default() },
                target.clone(),
                Transform::IDENTITY,
            ))
            .id();
        world.spawn((DirectionalLight { illuminance: 14_000.0, shadow_maps_enabled: false, ..default() }, Transform::from_xyz(3.0, 6.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y)));
        world.insert_resource(GlobalAmbientLight { color: Color::WHITE, brightness: 700.0, ..default() });
        Self { apps, target, camera }
    }

    fn update(&mut self) {
        self.apps.update();
        self.apps
            .main
            .world()
            .resource::<RenderDevice>()
            .wgpu_device()
            .poll(PollType::Wait { submission_index: None, timeout: None })
            .unwrap();
    }

    fn render(&mut self, asset: &Asset, out: &Path) -> bool {
        let world = self.apps.main.world_mut();
        let handle: Handle<WorldAsset> = world
            .resource::<AssetServer>()
            .load(GltfAssetLabel::Scene(0).from_asset(format!("{}/{}", asset.pack, asset.model)));
        let scene = world.spawn(WorldAssetRoot(handle.clone())).id();

        let min = Vec3::from(asset.bounds.min);
        let max = Vec3::from(asset.bounds.max);
        let center = (min + max) / 2.0;
        let radius = ((max - min).length() / 2.0).max(0.01);
        let fov = std::f32::consts::FRAC_PI_4;
        let dist = radius / (fov / 2.0).sin() * 1.05;
        let eye = center + Vec3::new(0.8, 0.55, 1.0).normalize() * dist;
        let mut camera = world.entity_mut(self.camera);
        camera.insert(Transform::from_translation(eye).looking_at(center, Vec3::Y));
        camera.insert(Projection::Perspective(PerspectiveProjection { fov, near: radius / 100.0, far: radius * 100.0, ..default() }));

        let mut ok = false;
        for _ in 0..600 {
            self.update();
            let world = self.apps.main.world();
            if world.resource::<AssetServer>().is_loaded_with_dependencies(&handle) {
                ok = true;
                break;
            }
        }
        if ok {
            // let the scene spawn and textures upload
            for _ in 0..4 {
                self.update();
            }
            let _ = std::fs::remove_file(out);
            let target = self.target.as_image().unwrap().clone();
            self.apps.main.world_mut().spawn(Screenshot::image(target)).observe(save_to_disk(out.to_path_buf()));
            for _ in 0..200 {
                self.update();
                if out.is_file() {
                    ok = true;
                    break;
                }
                ok = false;
            }
        }
        self.apps.main.world_mut().despawn(scene);
        self.update();
        ok
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let library_dir = mugon_editor_core::config::configured_library(args.library.clone())?
        .ok_or_else(|| anyhow::anyhow!("no library configured: pass --library, set MUGON_EDITOR_LIBRARY, or run `mugon-editor config --set-library <dir>`"))?;
    let library = Library::new(&library_dir);
    let mut packs: Vec<Pack> = library.load_packs()?;
    packs.retain(|p| args.pack.as_deref().is_none_or(|n| n == p.id));
    let mut renderer = Renderer::new(&library.root, args.size);
    let mut rendered = 0usize;

    'outer: for pack in &mut packs {
        let pack_dir = library.pack_dir(&pack.id);
        mugon_editor_core::ensure_thumbnails_dir(&pack_dir)?;
        let total = pack.assets.len();
        let mut failed = 0;
        for (i, asset) in pack.assets.iter_mut().enumerate() {
            if args.filter.as_deref().is_some_and(|f| !asset.name.contains(f)) {
                continue;
            }
            let rel = format!("thumbnails/{}.png", asset.name);
            let out = pack_dir.join(&rel);
            if out.is_file() && !args.force {
                asset.thumbnail = Some(rel);
                continue;
            }
            if renderer.render(asset, &out) {
                asset.thumbnail = Some(rel);
                rendered += 1;
            } else {
                failed += 1;
                eprintln!("[{}] failed: {}", pack.id, asset.name);
            }
            if i % 100 == 0 {
                println!("[{}] {i}/{total}", pack.id);
            }
            if args.limit.is_some_and(|l| rendered >= l) {
                save(pack, &pack_dir)?;
                break 'outer;
            }
        }
        save(pack, &pack_dir)?;
        println!("[{}] done ({failed} failed)", pack.id);
    }
    println!("rendered {rendered} thumbnails");
    Ok(())
}

fn save(pack: &Pack, dir: &Path) -> Result<()> {
    pack.save(dir)
}
