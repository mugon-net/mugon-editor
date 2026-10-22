//! HTTP server: REST API over the library index, the library files and the embedded web UI.

use anyhow::Result;
use mugon_editor_core::index::{Index, Override, Query};
use mugon_editor_core::Library;
use axum::{
    extract::{Query as Q, State},
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tower_http::services::ServeDir;

#[derive(RustEmbed)]
#[folder = "ui"]
struct WebAssets;

#[derive(Clone)]
pub struct AppState {
    library: Library,
    index: Arc<RwLock<Arc<Index>>>,
    editor_command: Option<String>,
}

impl AppState {
    fn index(&self) -> Arc<Index> {
        self.index.read().unwrap().clone()
    }
    fn reload(&self) -> Result<()> {
        *self.index.write().unwrap() = Arc::new(Index::load(&self.library)?);
        Ok(())
    }
}

pub fn router(library: Library, editor_command: Option<String>) -> Result<Router> {
    let state = AppState { index: Arc::new(RwLock::new(Arc::new(Index::load(&library)?))), library: library.clone(), editor_command };
    Ok(Router::new()
        .route("/api/info", get(info))
        .route("/api/packs", get(packs))
        .route("/api/categories", get(categories))
        .route("/api/tags", get(tags))
        .route("/api/assets", get(assets))
        .route("/api/asset", get(asset))
        .route("/api/override", post(set_override))
        .route("/api/reload", post(reload))
        .route("/api/editor/open", post(open_editor))
        .nest_service("/lib", ServeDir::new(&library.root))
        .fallback(static_file)
        .with_state(state))
}

pub async fn serve(library: Library, addr: SocketAddr, editor_command: Option<String>) -> Result<()> {
    let app = router(library, editor_command)?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("Mugon Editor browser: http://{}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}

type ApiResult = Result<Json<Value>, (StatusCode, String)>;

fn err(code: StatusCode, msg: impl ToString) -> (StatusCode, String) {
    (code, msg.to_string())
}

async fn info(State(s): State<AppState>) -> Json<Value> {
    let i = s.index();
    Json(json!({
        "library": s.library.root,
        "packs": i.packs.len(),
        "assets": i.iter().count(),
        "editor": s.editor_command.is_some(),
    }))
}

async fn packs(State(s): State<AppState>) -> Json<Value> {
    Json(json!(s.index().pack_facets()))
}

#[derive(Deserialize, Default)]
struct FacetQuery {
    pack: Option<String>,
    min: Option<usize>,
}

async fn categories(State(s): State<AppState>, Q(q): Q<FacetQuery>) -> Json<Value> {
    Json(json!(s.index().categories(q.pack.as_deref())))
}

async fn tags(State(s): State<AppState>, Q(q): Q<FacetQuery>) -> Json<Value> {
    Json(json!(s.index().tags(q.pack.as_deref(), q.min.unwrap_or(1))))
}

async fn assets(State(s): State<AppState>, Q(mut q): Q<Query>) -> Json<Value> {
    q.limit = Some(q.limit.unwrap_or(200).min(2000));
    let index = s.index();
    Json(json!(index.search(&q)))
}

#[derive(Deserialize)]
struct IdQuery {
    id: String,
}

async fn asset(State(s): State<AppState>, Q(q): Q<IdQuery>) -> ApiResult {
    let index = s.index();
    match index.get(&q.id) {
        Some(a) => Ok(Json(json!(a))),
        None => Err(err(StatusCode::NOT_FOUND, format!("unknown asset {}", q.id))),
    }
}

#[derive(Deserialize)]
struct OverrideBody {
    id: String,
    #[serde(flatten)]
    value: Override,
}

async fn set_override(State(s): State<AppState>, Json(body): Json<OverrideBody>) -> ApiResult {
    let mut overrides = mugon_editor_core::index::Overrides::load(&s.library.root).map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    overrides.0.insert(body.id, body.value);
    overrides.save(&s.library.root).map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    s.reload().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({ "ok": true })))
}

async fn reload(State(s): State<AppState>) -> ApiResult {
    s.reload().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize, Default)]
struct OpenEditor {
    asset: Option<String>,
}

async fn open_editor(State(s): State<AppState>, Json(body): Json<OpenEditor>) -> ApiResult {
    let Some(cmd) = &s.editor_command else {
        return Err(err(StatusCode::NOT_IMPLEMENTED, "no editor configured (start the server with --editor <path>)"));
    };
    let mut command = tokio::process::Command::new(cmd);
    command.arg("--library").arg(&s.library.root);
    if let Some(a) = body.asset {
        command.arg("--asset").arg(a);
    }
    command.spawn().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("starting editor: {e}")))?;
    Ok(Json(json!({ "ok": true })))
}

async fn static_file(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match WebAssets::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref().to_string())], file.data.into_owned()).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
