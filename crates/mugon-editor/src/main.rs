use anyhow::{bail, Result};
use mugon_editor_core::Library;
use mugon_editor_import::ImportOptions;
use clap::{Args, CommandFactory, Parser, Subcommand};
use std::path::PathBuf;

/// Mugon Editor: asset library and scene editor. Without a subcommand it opens the scene editor.
#[derive(Parser)]
#[command(name = "mugon-editor", version)]
struct Cli {
    /// Library folder (default: `[editor] library` in ~/.mugon/config.toml, asked for on first start).
    #[arg(long, env = "MUGON_EDITOR_LIBRARY", global = true)]
    library: Option<PathBuf>,
    /// Game folder with the mugon.toml whose [editor] table configures components and folders (default: the current directory).
    #[arg(long, global = true)]
    game_dir: Option<PathBuf>,
    #[command(flatten)]
    editor: EditorArgs,
    #[command(subcommand)]
    command: Option<Command>,
}

/// Options of the scene editor, used when no subcommand is given.
#[derive(Args, Default, Debug, PartialEq)]
struct EditorArgs {
    /// Scene file to open.
    #[arg(long)]
    scene: Option<PathBuf>,
    /// Asset to arm for placing; the next viewport click places it.
    #[arg(long)]
    asset: Option<String>,
    /// Fill a new scene with some assets from the library (for testing).
    #[arg(long)]
    demo: bool,
    /// Start with the shortcut hint expanded.
    #[arg(long)]
    show_help: bool,
    /// Frame rate cap (0 = limited only by the display's vsync).
    #[arg(long)]
    max_fps: Option<u32>,
    /// Exit after this many frames (smoke test).
    #[arg(long)]
    smoke: Option<u32>,
    /// Save a screenshot of the window after loading and exit.
    #[arg(long)]
    screenshot: Option<PathBuf>,
}

impl EditorArgs {
    /// Arguments for the mugon-editor-app executable.
    fn to_app_args(&self, game_dir: Option<&PathBuf>) -> Vec<String> {
        let mut args = Vec::new();
        let mut option = |name: &str, value: Option<String>| {
            if let Some(value) = value {
                args.push(format!("--{name}"));
                args.push(value);
            }
        };
        option("game-dir", game_dir.map(|dir| dir.to_string_lossy().to_string()));
        option("scene", self.scene.as_ref().map(|path| path.to_string_lossy().to_string()));
        option("asset", self.asset.clone());
        option("max-fps", self.max_fps.map(|fps| fps.to_string()));
        option("smoke", self.smoke.map(|frames| frames.to_string()));
        option("screenshot", self.screenshot.as_ref().map(|path| path.to_string_lossy().to_string()));
        if self.demo {
            args.push("--demo".into());
        }
        if self.show_help {
            args.push("--show-help".into());
        }
        args
    }
}

#[derive(Subcommand)]
enum Command {
    /// Import one or more .unitypackage files (or folders containing them).
    Import {
        paths: Vec<PathBuf>,
        /// Path to the FBX2glTF executable.
        #[arg(long, env = "MUGON_EDITOR_FBX2GLTF")]
        fbx2gltf: Option<PathBuf>,
        /// Scratch directory for extraction.
        #[arg(long)]
        temp: Option<PathBuf>,
        #[arg(long)]
        keep_temp: bool,
        /// Re-import packs that are already in the library (default: skip them).
        #[arg(long)]
        force: bool,
    },
    /// List packs and asset counts.
    List,
    /// Render thumbnails (runs the mugon-editor-thumbs binary; extra args are passed through).
    Thumbs {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Export a scene file into the game: writes the BSN (Rust `bsn_list!` source) to the game's `bsn-path` and copies
    /// the used assets into its assets folder. With `--out` only the BSN file is written, there.
    Export {
        scene: PathBuf,
        /// Write only the BSN source, to this file.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Name of the generated function (default: derived from the scene file name; only with --out).
        #[arg(long)]
        fn_name: Option<String>,
    },
    /// Show or change the saved configuration.
    Config {
        /// Set the library folder.
        #[arg(long)]
        set_library: Option<PathBuf>,
    },
    /// Run the MCP server on stdio (for agents).
    Mcp,
    /// Start the asset browser web UI (and its REST API).
    Serve {
        #[arg(long, default_value = "127.0.0.1:7878")]
        addr: std::net::SocketAddr,
        /// Editor executable the browser can launch.
        #[arg(long, env = "MUGON_EDITOR_EDITOR")]
        editor: Option<String>,
    },
}

fn game_config(cli: &Cli) -> Result<mugon_editor_scene::game::GameConfig> {
    mugon_editor_scene::game::GameConfig::load(&mugon_editor_scene::game::game_dir(cli.game_dir.clone())?)
}

fn library(cli: &Cli) -> Result<Library> {
    Ok(Library::new(mugon_editor_core::config::resolve_library_interactive(cli.library.clone())?))
}

/// Runs a sibling executable (mugon-editor-thumbs, mugon-editor-app) with the resolved library.
fn run_sibling(name: &str, lib: &Library, args: &[String]) -> Result<()> {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    let exe = std::env::current_exe()?.with_file_name(file);
    if !exe.is_file() {
        bail!("{} not found next to the mugon-editor executable", exe.display());
    }
    let status = std::process::Command::new(exe).arg("--library").arg(&lib.root).args(args).status()?;
    if !status.success() {
        bail!("{name} failed: {status}");
    }
    Ok(())
}

/// FBX2glTF: explicit path, next to the executable (installed layout), or in `third_party/` (repo layout).
fn find_fbx2gltf(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    let name = if cfg!(windows) { "FBX2glTF.exe" } else { "FBX2glTF" };
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.to_path_buf());
            dirs.push(dir.join("third_party").join("FBX2glTF"));
            // cargo layout: <repo>/target/{debug,release}/mugon-editor
            dirs.push(dir.join("..").join("..").join("third_party").join("FBX2glTF"));
        }
    }
    dirs.push(PathBuf::from("third_party").join("FBX2glTF"));
    for d in &dirs {
        let candidate = d.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    bail!("FBX2glTF not found; pass --fbx2gltf or set MUGON_EDITOR_FBX2GLTF")
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let Some(command) = &cli.command else {
        let lib = library(&cli)?;
        println!("The Mugon Editor has started. The mugon-editor CLI can do more than open the editor:\n");
        Cli::command().print_help()?;
        println!();
        return run_sibling("mugon-editor-app", &lib, &cli.editor.to_app_args(cli.game_dir.as_ref()));
    };
    match command {
        Command::Import { paths, fbx2gltf, temp, keep_temp, force } => {
            let lib = library(&cli)?;
            let opts = ImportOptions {
                fbx2gltf: find_fbx2gltf(fbx2gltf.clone())?,
                temp_dir: temp.clone().unwrap_or_else(|| lib.root.join(".tmp")),
                keep_temp: *keep_temp,
            };
            let mut packages = Vec::new();
            for p in paths {
                if p.is_dir() {
                    for e in std::fs::read_dir(p)? {
                        let e = e?.path();
                        if e.extension().is_some_and(|x| x == "unitypackage") {
                            packages.push(e);
                        }
                    }
                } else {
                    packages.push(p.clone());
                }
            }
            packages.sort();
            let (mut imported, mut skipped) = (0, 0);
            for pkg in packages {
                let file_name = pkg.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let (pack_id, _, _) = mugon_editor_import::naming::pack_identity(&file_name);
                if !*force && lib.pack_dir(&pack_id).join(mugon_editor_core::PACK_FILE).is_file() {
                    println!("[{pack_id}] already imported, skipping (use --force to re-import)");
                    skipped += 1;
                    continue;
                }
                imported += 1;
                if let Err(e) = mugon_editor_import::import_unitypackage(&pkg, &lib, &opts) {
                    eprintln!("failed {}: {e:#}", pkg.display());
                }
            }
            let _ = std::fs::remove_dir_all(&opts.temp_dir);
            println!("{imported} imported, {skipped} skipped");
            if imported > 0 {
                println!("Next: `mugon-editor thumbs` to render previews, then `mugon-editor serve` to browse.");
            }
        }
        Command::List => {
            for pack in library(&cli)?.load_packs()? {
                println!("{:<32} {:>5} assets", pack.id, pack.assets.len());
            }
        }
        Command::Thumbs { args } => run_sibling("mugon-editor-thumbs", &library(&cli)?, args)?,
        Command::Config { set_library } => {
            let path = mugon_editor_core::config::config_path()?;
            if let Some(dir) = set_library {
                std::fs::create_dir_all(dir)?;
                mugon_editor_core::config::Config { library: Some(dir.clone()) }.save()?;
                println!("Library set to {} ({})", dir.display(), path.display());
            } else {
                let c = mugon_editor_core::config::Config::load()?;
                println!("config file: {}", path.display());
                match c.library {
                    Some(l) => println!("library:     {}", l.display()),
                    None => println!("library:     (not set; run any command interactively or `mugon-editor config --set-library <dir>`)"),
                }
            }
        }
        Command::Export { scene, out, fn_name } => {
            let lib = library(&cli)?;
            let game = game_config(&cli)?;
            let index = mugon_editor_core::index::Index::load(&lib)?;
            let project = mugon_editor_scene::Project::load(scene)?;
            let registry = game.registry();
            let scene_name = scene
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .map(|name| name.strip_suffix(mugon_editor_scene::game::SCENE_EXTENSION).unwrap_or(&name).to_string())
                .unwrap_or_default();
            match out {
                Some(out) => {
                    let fn_name = fn_name.clone().unwrap_or_else(|| mugon_editor_scene::game::scene_identifier(&scene_name));
                    let opts = mugon_editor_scene::export::ExportOptions { fn_name, asset_prefix: String::new(), uses: game.bsn_uses.clone() };
                    std::fs::write(out, mugon_editor_scene::export::export_bsn(&project, &index, &registry, &opts)?)?;
                    println!("wrote {}", out.display());
                }
                None => {
                    let done = mugon_editor_scene::export::export_to_game(&project, &index, &registry, &game, &scene_name)?;
                    println!("wrote {}", done.bsn_path.display());
                    println!("copied {} asset files ({:.1} MB) into {}", done.bundle.files.len(), done.bundle.total_bytes as f64 / 1e6, game.assets_dir.display());
                }
            }
        }
        Command::Mcp => mugon_editor_mcp::serve_stdio(library(&cli)?).await?,
        Command::Serve { addr, editor } => {
            // default to a mugon-editor-app binary next to this executable
            let editor = editor.clone().or_else(|| {
                let exe = std::env::current_exe().ok()?.with_file_name(if cfg!(windows) { "mugon-editor-app.exe" } else { "mugon-editor-app" });
                exe.is_file().then(|| exe.to_string_lossy().to_string())
            });
            mugon_editor_server::serve(library(&cli)?, *addr, editor).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_invocation_opens_the_editor() {
        let cli = Cli::try_parse_from(["mugon-editor"]).unwrap();
        assert!(cli.command.is_none());
        assert!(cli.editor.to_app_args(None).is_empty());
    }

    #[test]
    fn editor_flags_are_forwarded() {
        let cli = Cli::try_parse_from(["mugon-editor", "--scene", "a.mugon-scene.json", "--demo", "--max-fps", "30", "--game-dir", "g"]).unwrap();
        assert!(cli.command.is_none());
        let args = cli.editor.to_app_args(cli.game_dir.as_ref());
        assert_eq!(args, ["--game-dir", "g", "--scene", "a.mugon-scene.json", "--max-fps", "30", "--demo"]);
    }

    #[test]
    fn subcommands_still_parse() {
        let cli = Cli::try_parse_from(["mugon-editor", "list"]).unwrap();
        assert!(matches!(cli.command, Some(Command::List)));
    }
}
