//! MCP server (stdio) that tells an agent what is in the Mugon Editor library.
//!
//! Read-only: search and inspect packs, categories and assets, and get absolute model paths.

use anyhow::Result;
use mugon_editor_core::index::{Index, Query};
use mugon_editor_core::{Asset, Library};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router, ServerHandler, ServiceExt,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Clone)]
pub struct MugonEditorMcp {
    index: Arc<Index>,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SearchParams {
    #[schemars(description = "Free text; every word must match the asset name, a tag, the category or the pack, e.g. 'wall stone'")]
    query: Option<String>,
    #[schemars(description = "Pack id, e.g. 'Simple_Racer' (see list_packs)")]
    pack: Option<String>,
    #[schemars(description = "Category (or category prefix), e.g. 'Props' or 'Vehicles/Presets' (see list_categories)")]
    category: Option<String>,
    #[schemars(description = "Exact tag, e.g. 'barrel'")]
    tag: Option<String>,
    #[schemars(description = "'static' or 'skinned'")]
    kind: Option<String>,
    #[schemars(description = "Max results, default 25, max 100")]
    limit: Option<usize>,
    #[schemars(description = "Skip this many results (paging)")]
    offset: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct IdParams {
    #[schemars(description = "Asset id as '<pack>/<name>', e.g. 'Simple_Racer/SR_Veh_DerbyCar_Black'")]
    id: String,
    #[schemars(description = "Max results (find_similar only), default 20")]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PackParams {
    #[schemars(description = "Restrict to one pack id")]
    pack: Option<String>,
}

fn size(a: &Asset) -> [f64; 3] {
    let r = |v: f32| (v as f64 * 100.0).round() / 100.0;
    [r(a.bounds.max[0] - a.bounds.min[0]), r(a.bounds.max[1] - a.bounds.min[1]), r(a.bounds.max[2] - a.bounds.min[2])]
}

fn text(v: Value) -> String {
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

impl MugonEditorMcp {
    pub fn new(library: &Library) -> Result<Self> {
        Ok(Self { index: Arc::new(Index::load(library)?), tool_router: Self::tool_router() })
    }

    fn brief(&self, a: &Asset) -> Value {
        json!({
            "id": a.id, "category": a.category, "tags": a.tags, "size_xyz_m": size(a), "triangles": a.triangles,
            "kind": a.kind, "variants": a.variants.len(), "animations": a.animations.len(),
        })
    }

    fn full(&self, a: &Asset) -> Value {
        let pack_dir = self.index.root.join(&a.pack);
        let abs = |rel: &str| pack_dir.join(rel).to_string_lossy().replace('\\', "/");
        json!({
            "id": a.id, "pack": a.pack, "name": a.name, "category": a.category, "tags": a.tags, "kind": a.kind,
            "size_xyz_m": size(a), "bounds": a.bounds, "triangles": a.triangles,
            "model_path": abs(&a.model),
            "model_path_in_pack": a.model,
            "texture": a.texture.as_deref().map(abs),
            "variants": a.variants.iter().map(|v| json!({ "name": v.name, "texture": abs(&v.texture) })).collect::<Vec<_>>(),
            "animations": a.animations,
            "thumbnail": a.thumbnail.as_deref().map(abs),
        })
    }
}

#[tool_router]
impl MugonEditorMcp {
    #[tool(description = "Overview of the whole asset library: packs and their categories with counts. Call this first.")]
    async fn get_library_summary(&self) -> String {
        self.index.summary()
    }

    #[tool(description = "List all asset packs with asset counts.")]
    async fn list_packs(&self) -> String {
        text(json!(self.index.packs.iter().map(|p| json!({ "id": p.id, "family": p.family, "name": p.name, "assets": p.assets.len() })).collect::<Vec<_>>()))
    }

    #[tool(description = "List categories (with asset counts), optionally for one pack.")]
    async fn list_categories(&self, Parameters(p): Parameters<PackParams>) -> String {
        text(json!(self.index.categories(p.pack.as_deref())))
    }

    #[tool(description = "List the most common tags (with counts), optionally for one pack.")]
    async fn list_tags(&self, Parameters(p): Parameters<PackParams>) -> String {
        let mut t = self.index.tags(p.pack.as_deref(), 3);
        t.truncate(150);
        text(json!(t))
    }

    #[tool(description = "Search assets by free text and filters. Returns brief records (id, category, tags, size in meters, triangle count). Use get_asset for file paths.")]
    async fn search_assets(&self, Parameters(p): Parameters<SearchParams>) -> String {
        let q = Query {
            q: p.query.unwrap_or_default(),
            pack: p.pack,
            category: p.category,
            tag: p.tag,
            kind: p.kind,
            offset: p.offset.unwrap_or(0),
            limit: Some(p.limit.unwrap_or(25).min(100)),
        };
        let r = self.index.search(&q);
        text(json!({ "total": r.total, "returned": r.assets.len(), "assets": r.assets.iter().map(|a| self.brief(a)).collect::<Vec<_>>() }))
    }

    #[tool(description = "Full details of one asset: absolute glTF path, texture and variants, bounds, animations, thumbnail.")]
    async fn get_asset(&self, Parameters(p): Parameters<IdParams>) -> String {
        match self.index.get(&p.id) {
            Some(a) => text(self.full(a)),
            None => format!("unknown asset '{}'. Use search_assets to find ids.", p.id),
        }
    }

    #[tool(description = "Find assets similar to the given one (same pack and category, overlapping tags, ordered by tag overlap).")]
    async fn find_similar(&self, Parameters(p): Parameters<IdParams>) -> String {
        let Some(base) = self.index.get(&p.id) else { return format!("unknown asset '{}'", p.id) };
        let mut scored: Vec<(usize, &Asset)> = self
            .index
            .iter()
            .filter(|a| a.id != base.id && a.pack == base.pack && a.category == base.category)
            .map(|a| (a.tags.iter().filter(|t| base.tags.contains(t)).count(), a))
            .filter(|(n, _)| *n > 0)
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.id.cmp(&b.1.id)));
        let out: Vec<Value> = scored.into_iter().take(p.limit.unwrap_or(20).min(100)).map(|(_, a)| self.brief(a)).collect();
        text(json!(out))
    }
}

#[tool_handler]
impl ServerHandler for MugonEditorMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Mugon Editor asset library: Synty low-poly 3D models as glTF (one shared texture per pack, units in meters). \
             Start with get_library_summary, then search_assets, then get_asset for file paths.",
        )
    }
}

/// Runs the MCP server on stdin/stdout until the client disconnects.
pub async fn serve_stdio(library: Library) -> Result<()> {
    let service = MugonEditorMcp::new(&library)?.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
