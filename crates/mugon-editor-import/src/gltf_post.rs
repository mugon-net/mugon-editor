//! Post-processing of FBX2glTF output: external shared texture, sane material defaults, stats.

use anyhow::{Context, Result};
use mugon_editor_core::{AssetKind, Bounds};
use serde_json::{json, Value};
use std::path::Path;

pub struct Stats {
    pub bounds: Bounds,
    pub triangles: u32,
    pub kind: AssetKind,
    pub animations: Vec<String>,
}

/// Rewrites the glTF at `path` in place. `texture_uri` is relative to the glTF file.
pub fn post_process(path: &Path, texture_uri: Option<&str>, scale: f32) -> Result<Stats> {
    let mut doc: Value = serde_json::from_str(&std::fs::read_to_string(path)?).context("parsing glTF")?;
    let mut stats = stats(&doc);
    if scale != 1.0 {
        apply_scale(&mut doc, scale);
        for k in 0..3 {
            stats.bounds.min[k] *= scale;
            stats.bounds.max[k] *= scale;
        }
    }

    // drop FBX2glTF's placeholder (data-URI) images
    let obj = doc.as_object_mut().context("glTF root")?;
    obj.remove("images");
    obj.remove("textures");
    obj.remove("samplers");
    if let Some(uri) = texture_uri {
        obj.insert("images".into(), json!([{ "uri": uri }]));
        obj.insert("samplers".into(), json!([{ "magFilter": 9728, "minFilter": 9728, "wrapS": 33071, "wrapT": 33071 }]));
        obj.insert("textures".into(), json!([{ "sampler": 0, "source": 0 }]));
    }
    if let Some(Value::Array(mats)) = obj.get_mut("materials") {
        for m in mats {
            let name = m.get("name").cloned().unwrap_or(json!("material"));
            let mut pbr = json!({ "metallicFactor": 0.0, "roughnessFactor": 1.0, "baseColorFactor": [1.0, 1.0, 1.0, 1.0] });
            if texture_uri.is_some() {
                pbr["baseColorTexture"] = json!({ "index": 0 });
            }
            *m = json!({ "name": name, "alphaMode": "OPAQUE", "pbrMetallicRoughness": pbr });
        }
    }
    std::fs::write(path, serde_json::to_string_pretty(&doc)?)?;
    Ok(stats)
}

fn stats(doc: &Value) -> Stats {
    let accessors = doc["accessors"].as_array().cloned().unwrap_or_default();
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    let mut triangles = 0u32;
    for mesh in doc["meshes"].as_array().into_iter().flatten() {
        for prim in mesh["primitives"].as_array().into_iter().flatten() {
            if let Some(a) = prim["attributes"]["POSITION"].as_u64().and_then(|i| accessors.get(i as usize)) {
                for k in 0..3 {
                    if let Some(v) = a["min"][k].as_f64() {
                        min[k] = min[k].min(v as f32);
                    }
                    if let Some(v) = a["max"][k].as_f64() {
                        max[k] = max[k].max(v as f32);
                    }
                }
            }
            if let Some(a) = prim["indices"].as_u64().and_then(|i| accessors.get(i as usize)) {
                triangles += a["count"].as_u64().unwrap_or(0) as u32 / 3;
            }
        }
    }
    if min[0] > max[0] {
        min = [0.0; 3];
        max = [0.0; 3];
    }
    let animations = doc["animations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["name"].as_str().map(str::to_string))
        .collect();
    let kind = if doc["skins"].as_array().is_some_and(|s| !s.is_empty()) { AssetKind::Skinned } else { AssetKind::Static };
    Stats { bounds: Bounds { min, max }, triangles, kind, animations }
}

/// Multiplies the scale of every root node of every scene (the importer's unit scale).
fn apply_scale(doc: &mut Value, scale: f32) {
    let roots: Vec<usize> = doc["scenes"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|s| s["nodes"].as_array().into_iter().flatten().filter_map(|n| n.as_u64().map(|n| n as usize)))
        .collect();
    for r in roots {
        let node = &mut doc["nodes"][r];
        let old: Vec<f64> = node["scale"].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
        let old = if old.len() == 3 { old } else { vec![1.0; 3] };
        node["scale"] = json!(old.iter().map(|v| v * scale as f64).collect::<Vec<_>>());
    }
}
