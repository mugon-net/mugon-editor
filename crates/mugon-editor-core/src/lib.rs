//! Data model and library access shared by every Mugon Editor component.
//!
//! On disk a library is a folder of packs, each with a `pack.json` that lists its assets.
//! Paths stored inside `pack.json` are relative to the pack folder and always use `/`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub mod config;
pub mod index;

pub const PACK_FILE: &str = "pack.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Bounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Variant {
    pub name: String,
    /// Texture path relative to the pack folder.
    pub texture: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    #[default]
    Static,
    Skinned,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Asset {
    /// `<pack id>/<name>`, unique in the library.
    pub id: String,
    /// Pack folder name; filled in when a library is loaded.
    #[serde(default)]
    pub pack: String,
    pub name: String,
    pub kind: AssetKind,
    pub category: String,
    pub tags: Vec<String>,
    /// glTF path relative to the pack folder.
    pub model: String,
    /// Default texture (relative to the pack folder).
    pub texture: Option<String>,
    pub variants: Vec<Variant>,
    pub bounds: Bounds,
    pub triangles: u32,
    pub animations: Vec<String>,
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Pack {
    /// Folder name, e.g. `Simple_Racer`.
    pub id: String,
    pub name: String,
    /// `Simple`, `Polygon`, ...
    pub family: String,
    pub source: String,
    pub textures: Vec<String>,
    pub assets: Vec<Asset>,
}

impl Pack {
    pub fn load(pack_dir: &Path) -> Result<Pack> {
        let file = pack_dir.join(PACK_FILE);
        let text = std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing {}", file.display()))
    }

    pub fn save(&self, pack_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(pack_dir)?;
        std::fs::write(pack_dir.join(PACK_FILE), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

/// A library folder on disk (outside the repo).
#[derive(Debug, Clone)]
pub struct Library {
    pub root: PathBuf,
}

impl Library {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn pack_dir(&self, pack_id: &str) -> PathBuf {
        self.root.join(pack_id)
    }

    /// Loads every pack that has a `pack.json`, sorted by id.
    pub fn load_packs(&self) -> Result<Vec<Pack>> {
        let mut packs = Vec::new();
        if !self.root.exists() {
            return Ok(packs);
        }
        for entry in std::fs::read_dir(&self.root)? {
            let path = entry?.path();
            if path.join(PACK_FILE).is_file() {
                let mut pack = Pack::load(&path)?;
                for a in &mut pack.assets {
                    a.pack = pack.id.clone();
                }
                packs.push(pack);
            }
        }
        packs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(packs)
    }
}

/// Converts a path to the `/`-separated form used in `pack.json`.
pub fn to_slash(path: &Path) -> String {
    path.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

/// Creates `<pack>/thumbnails/` with a `.gitignore` that excludes the generated PNGs, so a library that is
/// checked into git does not grow with previews. Safe to call repeatedly.
pub fn ensure_thumbnails_dir(pack_dir: &Path) -> Result<PathBuf> {
    let dir = pack_dir.join("thumbnails");
    std::fs::create_dir_all(&dir)?;
    let ignore = dir.join(".gitignore");
    if !ignore.is_file() {
        std::fs::write(&ignore, "# generated previews, re-create with `mugon-editor thumbs`
*.png
")?;
    }
    Ok(dir)
}
