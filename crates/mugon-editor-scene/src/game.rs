//! The `[editor]` table of a game's `mugon.toml`: component definitions and the scene/asset folders.

use crate::components::{ComponentDef, ComponentRegistry};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const MUGON_TOML: &str = "mugon.toml";
pub const SCENE_EXTENSION: &str = ".mugon-scene.json";
const DEFAULT_SCENES_JSON: &str = "scenes";
const DEFAULT_SCENES_BSN: &str = "src/scenes";
const DEFAULT_ASSETS: &str = "assets";

#[derive(Deserialize, Default)]
struct MugonToml {
    #[serde(default)]
    editor: EditorSection,
}

#[derive(Deserialize, Default)]
struct EditorSection {
    #[serde(default)]
    component: Vec<ComponentDef>,
    #[serde(default)]
    scenes: ScenesSection,
    #[serde(default)]
    assets: AssetsSection,
}

#[derive(Deserialize, Default)]
struct ScenesSection {
    #[serde(rename = "json-path")]
    json_path: Option<String>,
    #[serde(rename = "bsn-path")]
    bsn_path: Option<String>,
    #[serde(rename = "bsn-use", default)]
    bsn_use: Vec<String>,
}

#[derive(Deserialize, Default)]
struct AssetsSection {
    path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GameConfig {
    pub dir: PathBuf,
    /// Whether a `mugon.toml` was found in `dir`.
    pub present: bool,
    pub components: Vec<ComponentDef>,
    pub scenes_json_dir: PathBuf,
    pub scenes_bsn_dir: PathBuf,
    /// `use` paths written into exported BSN so the game's component types resolve.
    pub bsn_uses: Vec<String>,
    pub assets_dir: PathBuf,
}

impl GameConfig {
    /// Reads `<dir>/mugon.toml`. A missing file gives the defaults (no components); a malformed one is an error.
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(MUGON_TOML);
        if !path.is_file() {
            return Self::from_toml(dir, "", &path).map(|config| Self { present: false, ..config });
        }
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        Self::from_toml(dir, &text, &path)
    }

    pub fn from_toml(dir: &Path, text: &str, source: &Path) -> Result<Self> {
        let parsed: MugonToml = toml::from_str(text).with_context(|| format!("parsing {}", source.display()))?;
        let editor = parsed.editor;
        let mut seen = BTreeSet::new();
        for component in &editor.component {
            if !seen.insert(component.name.as_str()) {
                bail!("{}: [[editor.component]] '{}' is defined twice", source.display(), component.name);
            }
        }
        let resolve = |key: &str, value: Option<String>, default: &str| -> Result<PathBuf> {
            let value = value.unwrap_or_else(|| default.to_string());
            if value.trim().is_empty() {
                bail!("{}: {key} must not be empty", source.display());
            }
            let path = PathBuf::from(value);
            Ok(if path.is_absolute() { path } else { dir.join(path) })
        };
        Ok(Self {
            dir: dir.to_path_buf(),
            present: true,
            components: editor.component,
            scenes_json_dir: resolve("[editor.scenes] json-path", editor.scenes.json_path, DEFAULT_SCENES_JSON)?,
            bsn_uses: editor.scenes.bsn_use,
            scenes_bsn_dir: resolve("[editor.scenes] bsn-path", editor.scenes.bsn_path, DEFAULT_SCENES_BSN)?,
            assets_dir: resolve("[editor.assets] path", editor.assets.path, DEFAULT_ASSETS)?,
        })
    }

    pub fn registry(&self) -> ComponentRegistry {
        ComponentRegistry::from_defs(self.components.clone())
    }
}

/// `--game-dir` or the current directory.
pub fn game_dir(explicit: Option<PathBuf>) -> Result<PathBuf> {
    match explicit {
        Some(dir) => Ok(dir),
        None => std::env::current_dir().context("cannot determine the current directory"),
    }
}

/// Rust identifier for a scene name: lowercase snake_case, never empty, never starting with a digit.
pub fn scene_identifier(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let out = out.trim_matches('_').to_string();
    match out.chars().next() {
        None => "scene".into(),
        Some(first) if first.is_ascii_digit() => format!("scene_{out}"),
        Some(_) => out,
    }
}

/// The example `mugon.toml` shipped in `config/`, for tests of the exporter and the component code.
#[cfg(test)]
pub(crate) fn example_registry() -> ComponentRegistry {
    let text = include_str!("../../../config/mugon.example.toml");
    GameConfig::from_toml(Path::new("/game"), text, Path::new("mugon.example.toml")).unwrap().registry()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<GameConfig> {
        GameConfig::from_toml(Path::new("/game"), text, Path::new("/game/mugon.toml"))
    }

    #[test]
    fn defaults_without_editor_table() {
        let config = parse("id = \"g\"\nversion = \"0.1.0\"\n").unwrap();
        assert!(config.components.is_empty());
        assert_eq!(config.scenes_json_dir, Path::new("/game").join("scenes"));
        assert_eq!(config.scenes_bsn_dir, Path::new("/game").join("src/scenes"));
        assert_eq!(config.assets_dir, Path::new("/game").join("assets"));
    }

    #[test]
    fn missing_file_is_not_present() {
        let config = GameConfig::load(&std::env::temp_dir().join("mugon-editor-no-such-game")).unwrap();
        assert!(!config.present);
    }

    #[test]
    fn overrides_relative_and_absolute() {
        let abs = std::env::temp_dir().join("abs-assets");
        let text = format!(
            "[editor.scenes]\njson-path = \"levels\"\nbsn-path = \"src/gen\"\n[editor.assets]\npath = {:?}\n",
            abs.to_string_lossy()
        );
        let config = parse(&text).unwrap();
        assert_eq!(config.scenes_json_dir, Path::new("/game").join("levels"));
        assert_eq!(config.scenes_bsn_dir, Path::new("/game").join("src/gen"));
        assert_eq!(config.assets_dir, abs);
    }

    #[test]
    fn components_are_read_and_validated() {
        let config = parse("[[editor.component]]\nname = \"Health\"\nfields = [{ name = \"max\", type = \"f32\" }]\n").unwrap();
        assert!(config.registry().get("Health").is_some());
        assert!(parse("[[editor.component]]\nname = \"A\"\n[[editor.component]]\nname = \"A\"\n").is_err());
        assert!(parse("[[editor.component]]\nname = \"A\"\nfields = [{ name = \"x\", type = \"nope\" }]\n").is_err());
        assert!(parse("[editor.assets]\npath = \"\"\n").is_err());
    }

    #[test]
    fn scene_identifiers() {
        assert_eq!(scene_identifier("Level One!"), "level_one");
        assert_eq!(scene_identifier("1st"), "scene_1st");
        assert_eq!(scene_identifier("***"), "scene");
    }
}
