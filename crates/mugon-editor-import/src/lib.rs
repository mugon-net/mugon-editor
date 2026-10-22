//! Importing Synty packs (`.unitypackage`) into a Mugon Editor library.
//!
//! Asset = FBX mesh. Category comes from the prefab folder that uses the mesh, the default texture and
//! the colour variants from the materials of those prefabs. Every FBX is converted with FBX2glTF and the
//! resulting glTF references the pack's shared texture instead of embedding one.

mod gltf_post;
pub mod naming;
pub mod unity;

use anyhow::{bail, Context, Result};
use mugon_editor_core::{to_slash, Asset, Library, Pack, Variant};
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct ImportOptions {
    pub fbx2gltf: PathBuf,
    pub temp_dir: PathBuf,
    pub keep_temp: bool,
}

#[derive(Default)]
struct MeshUsage {
    categories: Vec<String>,
    texture_guids: Vec<String>,
}

pub fn import_unitypackage(package: &Path, library: &Library, opts: &ImportOptions) -> Result<Pack> {
    let file_name = package.file_name().context("package file name")?.to_string_lossy().to_string();
    let (pack_id, family, name) = naming::pack_identity(&file_name);
    println!("[{pack_id}] extracting {file_name}");
    let work = opts.temp_dir.join(&pack_id);
    let _ = std::fs::remove_dir_all(&work);
    let pkg = unity::extract(package, &work.join("pkg"))?;

    let pack_dir = library.pack_dir(&pack_id);
    let _ = std::fs::remove_dir_all(pack_dir.join("models"));
    let _ = std::fs::remove_dir_all(pack_dir.join("textures"));
    std::fs::create_dir_all(pack_dir.join("models"))?;
    std::fs::create_dir_all(pack_dir.join("textures"))?;
    mugon_editor_core::ensure_thumbnails_dir(&pack_dir)?;

    // material guid -> texture guid
    let mut mat_texture: HashMap<String, String> = HashMap::new();
    for (guid, _) in pkg.guids_with_ext(".mat") {
        let text = std::fs::read_to_string(pkg.asset_file(guid)).unwrap_or_default();
        if let Some(t) = unity::material(&text).texture_guid {
            mat_texture.insert(guid.clone(), t);
        }
    }

    // mesh (fbx) guid -> usage by prefabs
    let mut usage: HashMap<String, MeshUsage> = HashMap::new();
    for (guid, path) in pkg.guids_with_ext(".prefab") {
        let text = std::fs::read_to_string(pkg.asset_file(guid)).unwrap_or_default();
        let refs = unity::prefab_refs(&text);
        let category = naming::category_from_prefab_path(path);
        for mesh in &refs.meshes {
            let u = usage.entry(mesh.clone()).or_default();
            if let Some(c) = &category {
                u.categories.push(c.clone());
            }
            for m in &refs.materials {
                if let Some(t) = mat_texture.get(m) {
                    u.texture_guids.push(t.clone());
                }
            }
        }
    }

    // copy the textures that are actually used by meshes
    let mut texture_paths: HashMap<String, String> = HashMap::new(); // texture guid -> "textures/X.png"
    let mut used_names: HashMap<String, String> = HashMap::new(); // file name -> guid
    let mut wanted: Vec<&String> = usage.values().flat_map(|u| u.texture_guids.iter()).collect();
    wanted.sort();
    wanted.dedup();
    for guid in wanted {
        let Some(src_path) = pkg.paths.get(guid) else { continue };
        let mut file = Path::new(src_path).file_name().unwrap().to_string_lossy().to_string();
        let needs_convert = !matches!(Path::new(&file).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(), Some("png" | "jpg" | "jpeg"));
        if needs_convert {
            file = format!("{}.png", Path::new(&file).file_stem().unwrap().to_string_lossy());
        }
        let file = match used_names.get(&file) {
            Some(other) if other != guid => {
                let (stem, ext) = file.rsplit_once('.').unwrap_or((&file, "png"));
                format!("{stem}_{}.{ext}", &guid[..6])
            }
            _ => file,
        };
        used_names.insert(file.clone(), guid.clone());
        let dest = pack_dir.join("textures").join(&file);
        if needs_convert {
            // browsers and the web game cannot read TGA etc.
            let format = image::ImageFormat::from_path(src_path).unwrap_or(image::ImageFormat::Tga);
            let decoded = std::fs::File::open(pkg.asset_file(guid))
                .map_err(image::ImageError::from)
                .and_then(|f| image::ImageReader::with_format(std::io::BufReader::new(f), format).decode());
            match decoded {
                Ok(img) => img.save_with_format(&dest, image::ImageFormat::Png)?,
                Err(e) => {
                    eprintln!("[{pack_id}] cannot convert texture {src_path}: {e}");
                    continue;
                }
            }
        } else {
            std::fs::copy(pkg.asset_file(guid), dest)?;
        }
        texture_paths.insert(guid.clone(), format!("textures/{file}"));
    }

    // the FBX files to convert, with unique names
    let mut fbx: Vec<(String, String)> = Vec::new(); // (guid, unique name)
    let mut names: HashMap<String, usize> = HashMap::new();
    let mut guids: Vec<(&String, &String)> = pkg.guids_with_ext(".fbx").collect();
    guids.sort_by(|a, b| a.1.cmp(b.1));
    for (guid, path) in guids {
        let stem = Path::new(path).file_stem().unwrap().to_string_lossy().to_string();
        let n = names.entry(stem.clone()).or_default();
        *n += 1;
        fbx.push((guid.clone(), if *n == 1 { stem } else { format!("{stem}_{}", &guid[..6]) }));
    }
    println!("[{pack_id}] converting {} meshes", fbx.len());

    let results: Vec<Result<Asset>> = fbx
        .par_iter()
        .map(|(guid, name)| {
            convert_one(&pkg, guid, name, &pack_id, &pack_dir, &work, usage.get(guid), &texture_paths, opts)
                .with_context(|| format!("{name}"))
        })
        .collect();

    let mut assets = Vec::new();
    let mut failed = 0;
    for r in results {
        match r {
            Ok(a) => assets.push(a),
            Err(e) => {
                failed += 1;
                eprintln!("[{pack_id}] skipped: {e:#}");
            }
        }
    }
    assets.sort_by(|a, b| a.id.cmp(&b.id));
    let mut textures: Vec<String> = texture_paths.values().cloned().collect();
    textures.sort();
    let pack = Pack { id: pack_id.clone(), name, family, source: file_name, textures, assets };
    pack.save(&pack_dir)?;
    if !opts.keep_temp {
        let _ = std::fs::remove_dir_all(&work);
    }
    println!("[{pack_id}] done: {} assets, {failed} failed", pack.assets.len());
    Ok(pack)
}

#[allow(clippy::too_many_arguments)]
fn convert_one(
    pkg: &unity::Extracted,
    guid: &str,
    name: &str,
    pack_id: &str,
    pack_dir: &Path,
    work: &Path,
    usage: Option<&MeshUsage>,
    texture_paths: &HashMap<String, String>,
    opts: &ImportOptions,
) -> Result<Asset> {
    let fbx_path = work.join("fbx").join(format!("{name}.fbx"));
    std::fs::create_dir_all(fbx_path.parent().unwrap())?;
    std::fs::copy(pkg.asset_file(guid), &fbx_path)?;
    let out_base = work.join("gltf").join(name);
    std::fs::create_dir_all(out_base.parent().unwrap())?;
    let output = Command::new(&opts.fbx2gltf)
        .arg("-i")
        .arg(&fbx_path)
        .arg("-o")
        .arg(&out_base)
        .output()
        .with_context(|| format!("running {}", opts.fbx2gltf.display()))?;
    let out_dir = work.join("gltf").join(format!("{name}_out"));
    let produced = out_dir.join(format!("{name}.gltf"));
    if !produced.is_file() {
        bail!("FBX2glTF produced no output: {}", String::from_utf8_lossy(&output.stderr).lines().last().unwrap_or(""));
    }

    // texture choice: most used is the default, the rest are variants
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for t in usage.iter().flat_map(|u| u.texture_guids.iter()) {
        if let Some(p) = texture_paths.get(t) {
            *counts.entry(p.as_str()).or_default() += 1;
        }
    }
    let mut ranked: Vec<(&str, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let default_texture = ranked.first().map(|(p, _)| p.to_string());

    let model_dir = pack_dir.join("models").join(name);
    std::fs::create_dir_all(&model_dir)?;
    let model_path = model_dir.join(format!("{name}.gltf"));
    for entry in std::fs::read_dir(&out_dir)? {
        let entry = entry?;
        if entry.path().is_file() {
            std::fs::copy(entry.path(), model_dir.join(entry.file_name()))?;
        }
    }
    let texture_uri = default_texture.as_ref().map(|t| format!("../../{t}"));
    let stats = gltf_post::post_process(&model_path, texture_uri.as_deref(), pkg.global_scale(guid))?;

    let category = if name.to_ascii_lowercase().contains("_collision") || name.to_ascii_lowercase().contains("_collider") {
        "Collision".to_string()
    } else {
        usage.and_then(|u| most_common(&u.categories)).unwrap_or_else(|| naming::category_from_name(name))
    };
    let prefix_len = if name.split('_').nth(1).is_some_and(|t| naming::category_from_name(name) != "Uncategorized" && t.len() <= 5) { 2 } else { 1 };
    let mut tags = naming::tags_from_name(name, prefix_len);
    for part in category.split('/') {
        let t = part.to_ascii_lowercase();
        if !tags.contains(&t) {
            tags.push(t);
        }
    }
    let variants = ranked
        .iter()
        .skip(1)
        .map(|(p, _)| Variant {
            name: Path::new(p).file_stem().unwrap().to_string_lossy().to_string(),
            texture: p.to_string(),
        })
        .collect();

    Ok(Asset {
        id: format!("{pack_id}/{name}"),
        pack: pack_id.to_string(),
        name: name.to_string(),
        kind: stats.kind,
        category,
        tags,
        model: to_slash(&Path::new("models").join(name).join(format!("{name}.gltf"))),
        texture: default_texture,
        variants,
        bounds: stats.bounds,
        triangles: stats.triangles,
        animations: stats.animations,
        thumbnail: None,
    })
}

fn most_common(items: &[String]) -> Option<String> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for i in items {
        *counts.entry(i).or_default() += 1;
    }
    counts.into_iter().max_by_key(|(_, n)| *n).map(|(s, _)| s.to_string())
}
