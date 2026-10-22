//! Reading `.unitypackage` files (tar.gz of `<guid>/{asset,pathname}`) and the bits of Unity YAML we need.

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use regex::Regex;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

pub struct Extracted {
    pub dir: PathBuf,
    /// guid -> `Assets/...` pathname inside the project
    pub paths: HashMap<String, String>,
    /// guid -> `asset.meta` text
    pub metas: HashMap<String, String>,
}

impl Extracted {
    /// Unity's `globalScale` import setting of a model (1 if absent).
    pub fn global_scale(&self, guid: &str) -> f32 {
        self.metas
            .get(guid)
            .and_then(|m| m.lines().find_map(|l| l.trim().strip_prefix("globalScale:")))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(1.0)
    }

    pub fn asset_file(&self, guid: &str) -> PathBuf {
        self.dir.join(guid).join("asset")
    }
    pub fn guids_with_ext<'a>(&'a self, ext: &'a str) -> impl Iterator<Item = (&'a String, &'a String)> {
        self.paths.iter().filter(move |(_, p)| p.to_ascii_lowercase().ends_with(ext))
    }
}

/// Unpacks all `asset` payloads and `pathname`s into `dest`.
pub fn extract(package: &Path, dest: &Path) -> Result<Extracted> {
    std::fs::create_dir_all(dest)?;
    let file = std::fs::File::open(package).with_context(|| format!("opening {}", package.display()))?;
    let mut archive = tar::Archive::new(GzDecoder::new(file));
    let mut paths = HashMap::new();
    let mut metas = HashMap::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let mut parts = path.components();
        let (Some(guid), Some(name)) = (parts.next(), parts.next()) else { continue };
        let guid = guid.as_os_str().to_string_lossy().to_string();
        match name.as_os_str().to_str() {
            Some("pathname") => {
                let mut s = String::new();
                entry.read_to_string(&mut s)?;
                paths.insert(guid, s.lines().next().unwrap_or("").trim().to_string());
            }
            Some("asset.meta") => {
                let mut s = String::new();
                if entry.read_to_string(&mut s).is_ok() {
                    metas.insert(guid, s);
                }
            }
            Some("asset") => {
                let dir = dest.join(&guid);
                std::fs::create_dir_all(&dir)?;
                entry.unpack(dir.join("asset"))?;
            }
            _ => {}
        }
    }
    // directories have a pathname but no payload
    paths.retain(|g, _| dest.join(g).join("asset").is_file());
    Ok(Extracted { dir: dest.to_path_buf(), paths, metas })
}

/// One YAML document of a Unity text asset.
pub struct Doc<'a> {
    pub class_id: u32,
    pub body: &'a str,
}

pub fn documents(text: &str) -> Vec<Doc<'_>> {
    let mut docs = Vec::new();
    let mut start: Option<(u32, usize)> = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("--- !u!") {
            if let Some((class_id, s)) = start.take() {
                docs.push(Doc { class_id, body: &text[s..offset] });
            }
            let class_id = rest.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0);
            start = Some((class_id, offset + line.len()));
        }
        offset += line.len();
    }
    if let Some((class_id, s)) = start {
        docs.push(Doc { class_id, body: &text[s..] });
    }
    docs
}

fn guid_re() -> Regex {
    Regex::new(r"guid:\s*([0-9a-f]{32})").unwrap()
}

/// Mesh/material references of one prefab.
#[derive(Default, Debug)]
pub struct PrefabRefs {
    pub meshes: Vec<String>,
    pub materials: Vec<String>,
}

pub fn prefab_refs(text: &str) -> PrefabRefs {
    let guid = guid_re();
    let mut refs = PrefabRefs::default();
    for doc in documents(text) {
        if doc.class_id == 1001 {
            // PrefabInstance: source model plus material overrides
            let lines: Vec<&str> = doc.body.lines().collect();
            for (i, line) in lines.iter().enumerate() {
                let t = line.trim_start();
                if t.starts_with("m_SourcePrefab:") {
                    if let Some(c) = guid.captures(line) {
                        refs.meshes.push(c[1].to_string());
                    }
                }
                if t.contains("propertyPath: m_Materials.Array.data[") {
                    if let Some(c) = lines.iter().skip(i + 1).take(3).find(|l| l.contains("objectReference:")).and_then(|l| guid.captures(l)) {
                        refs.materials.push(c[1].to_string());
                    }
                }
            }
            continue;
        }
        // 33 MeshFilter, 23 MeshRenderer, 137 SkinnedMeshRenderer
        if !matches!(doc.class_id, 23 | 33 | 137) {
            continue;
        }
        let mut in_materials = false;
        for line in doc.body.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("m_Mesh:") {
                if let Some(c) = guid.captures(line) {
                    refs.meshes.push(c[1].to_string());
                }
            }
            if trimmed.starts_with("m_Materials:") {
                in_materials = true;
                continue;
            }
            if in_materials {
                if trimmed.starts_with('-') {
                    if let Some(c) = guid.captures(line) {
                        refs.materials.push(c[1].to_string());
                    }
                } else {
                    in_materials = false;
                }
            }
        }
    }
    refs
}

pub struct Material {
    pub name: String,
    pub texture_guid: Option<String>,
}

/// Texture property names that hold the base colour, most specific first.
const BASE_TEXTURE_PROPS: &[&str] =
    &["_maintex", "_basemap", "_basecolormap", "_albedo_map", "_albedomap", "_albedo", "_base_map", "_basetex", "_maintexture", "_diffuse"];

/// Reads the name and base colour texture of a `.mat` file.
pub fn material(text: &str) -> Material {
    let guid = guid_re();
    let mut name = String::new();
    let mut textures: Vec<(String, String)> = Vec::new(); // (property, guid)
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if let Some(n) = t.strip_prefix("m_Name:") {
            if name.is_empty() {
                name = n.trim().to_string();
            }
        }
        if let Some(prop) = t.strip_prefix("- ").and_then(|p| p.strip_suffix(':')) {
            if let Some(c) = lines.get(i + 1).filter(|l| l.contains("m_Texture:")).and_then(|l| guid.captures(l)) {
                textures.push((prop.to_ascii_lowercase(), c[1].to_string()));
            }
        }
    }
    let texture_guid = BASE_TEXTURE_PROPS
        .iter()
        .find_map(|p| textures.iter().find(|(prop, _)| prop == p))
        .map(|(_, g)| g.clone());
    Material { name, texture_guid }
}
