use crate::control::{
    create_project, ensure_project_bootstrap, now_iso, read_active_panel_value,
    read_active_session_id, write_active_session_id, BootstrapRequest,
};
use crate::error::CliError;
use crate::paths::OpenPanelsPaths;
use crate::storage::Storage;
use crate::types::PanelKind;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::Json;
use axum::Router;
use base64::Engine;
use include_dir::{include_dir, Dir};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

static STUDIO_DIST: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../apps/local-studio/dist");

#[derive(Clone)]
struct AppState {
    paths: OpenPanelsPaths,
    static_dir: Option<PathBuf>,
}

pub fn run_server(
    host: &str,
    port: u16,
    paths: OpenPanelsPaths,
    static_dir: Option<PathBuf>,
) -> Result<i32, CliError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(to_cli_error)?;
    runtime.block_on(async move { run_server_async(host, port, paths, static_dir).await })
}

async fn run_server_async(
    host: &str,
    port: u16,
    paths: OpenPanelsPaths,
    static_dir: Option<PathBuf>,
) -> Result<i32, CliError> {
    let std_listener = TcpListener::bind((host, port)).map_err(to_cli_error)?;
    std_listener.set_nonblocking(true).map_err(to_cli_error)?;
    let listener = tokio::net::TcpListener::from_std(std_listener).map_err(to_cli_error)?;
    axum::serve(listener, build_router(paths, static_dir))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(to_cli_error)?;
    Ok(0)
}

fn build_router(paths: OpenPanelsPaths, static_dir: Option<PathBuf>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(HeaderValue::from_static("*"))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(tower_http::cors::Any);
    Router::new()
        .route("/api/bootstrap", get(api_bootstrap))
        .route("/api/projects", post(api_projects_create))
        .route("/api/sessions", get(api_sessions))
        .route(
            "/api/active-session",
            get(api_active_session).put(api_set_active_session),
        )
        .route(
            "/api/active-panel",
            get(api_active_panel).put(api_set_active_panel),
        )
        .route(
            "/api/panels/{session_id}/{panel_id}/state",
            put(api_save_panel_state),
        )
        .route(
            "/api/panels/{session_id}/{panel_id}/selection",
            put(api_save_panel_selection),
        )
        .route(
            "/api/panels/{session_id}/{panel_id}/assets",
            post(api_write_panel_asset),
        )
        .route(
            "/api/panels/{session_id}/{panel_id}/assets/{*asset_name}",
            get(api_read_panel_asset),
        )
        .route("/", get(index))
        .route("/{*path}", get(static_asset))
        .layer(cors)
        .with_state(Arc::new(AppState { paths, static_dir }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BootstrapQuery {
    panel_id: Option<String>,
    panel_kind: Option<String>,
    session_id: Option<String>,
}

async fn api_bootstrap(
    State(state): State<Arc<AppState>>,
    Query(query): Query<BootstrapQuery>,
) -> Response {
    let request = BootstrapRequest {
        requested_panel_id: query.panel_id,
        requested_panel_kind: query.panel_kind.as_deref().and_then(PanelKind::parse),
        requested_session_id: query.session_id,
    };
    match ensure_project_bootstrap(&state.paths, request) {
        Ok(bootstrap) => json_response(StatusCode::OK, &bootstrap),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
struct CreateProjectBody {
    title: Option<String>,
}

async fn api_projects_create(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateProjectBody>,
) -> Response {
    match create_project(&state.paths, body.title.as_deref()) {
        Ok(bootstrap) => json_response(StatusCode::OK, &bootstrap),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_sessions(State(state): State<Arc<AppState>>) -> Response {
    match Storage::open(&state.paths).and_then(|storage| storage.list_sessions()) {
        Ok(sessions) => json_response(StatusCode::OK, &sessions),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_active_session(State(state): State<Arc<AppState>>) -> Response {
    match read_active_session_id(&state.paths) {
        Ok(session_id) => json_response(StatusCode::OK, &json!({ "sessionId": session_id })),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActiveSessionBody {
    session_id: Option<String>,
}

async fn api_set_active_session(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ActiveSessionBody>,
) -> Response {
    let Some(session_id) = body.session_id.filter(|value| !value.trim().is_empty()) else {
        return json_error(StatusCode::BAD_REQUEST, "Missing sessionId.");
    };
    let result = Storage::open(&state.paths).and_then(|storage| {
        if storage.read_session(&session_id)?.is_none() {
            return Err(CliError::new(format!(
                "OpenPanels session not found: {session_id}"
            )));
        }
        write_active_session_id(&state.paths, &session_id)?;
        ensure_project_bootstrap(
            &state.paths,
            BootstrapRequest {
                requested_session_id: Some(session_id.clone()),
                requested_panel_id: None,
                requested_panel_kind: None,
            },
        )?;
        Ok(json!({ "sessionId": session_id }))
    });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_active_panel(State(state): State<Arc<AppState>>) -> Response {
    let result = ensure_project_bootstrap(&state.paths, BootstrapRequest::new())
        .and_then(|_| read_active_panel_value(&state.paths));
    match result {
        Ok(active_panel) => json_response(StatusCode::OK, &json!({ "activePanel": active_panel })),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivePanelBody {
    kind: Option<String>,
    panel_id: Option<String>,
    session_id: Option<String>,
}

async fn api_set_active_panel(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ActivePanelBody>,
) -> Response {
    let result = ensure_project_bootstrap(
        &state.paths,
        BootstrapRequest {
            requested_session_id: body.session_id,
            requested_panel_id: body.panel_id,
            requested_panel_kind: body.kind.as_deref().and_then(PanelKind::parse),
        },
    )
    .map(|bootstrap| {
        json!({
            "activePanelId": bootstrap.active_panel_id,
            "activePanelKind": bootstrap.active_panel_kind,
            "panel": bootstrap.panel,
            "state": bootstrap.state,
        })
    });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_save_panel_state(
    State(state): State<Arc<AppState>>,
    Path((session_id, panel_id)): Path<(String, String)>,
    Json(panel_state): Json<Value>,
) -> Response {
    let result = Storage::open(&state.paths).and_then(|storage| {
        storage.write_panel_state(&session_id, &panel_id, &panel_state)?;
        write_active_session_id(&state.paths, &session_id)?;
        Ok(json!({ "saved": true, "sessionId": session_id, "panelId": panel_id }))
    });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectionBody {
    image_data_url: Option<String>,
    selection: Option<Value>,
}

async fn api_save_panel_selection(
    State(state): State<Arc<AppState>>,
    Path((session_id, panel_id)): Path<(String, String)>,
    Json(body): Json<SelectionBody>,
) -> Response {
    let result = Storage::open(&state.paths).and_then(|storage| {
        let mut asset_ref = body
            .selection
            .as_ref()
            .and_then(|selection| selection.get("assetRef"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let Some(data_url) = body
            .image_data_url
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            let image = data_url_to_buffer(data_url)?;
            let written = storage.write_asset_from_buffer(
                &session_id,
                &panel_id,
                "__selection/current.png",
                &image.bytes,
                true,
            )?;
            asset_ref = Some(written.asset_ref);
        }
        let selected_shape_ids = body
            .selection
            .as_ref()
            .and_then(|selection| selection.get("selectedShapeIds"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let selected_shapes = body
            .selection
            .as_ref()
            .and_then(|selection| selection.get("selectedShapes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let selection = json!({
            "sessionId": session_id,
            "panelId": panel_id,
            "selectedShapeIds": selected_shape_ids,
            "selectedShapes": selected_shapes,
            "assetRef": asset_ref,
            "updatedAt": now_iso(),
        });
        storage.write_panel_selection(&session_id, &panel_id, &selection)?;
        write_active_session_id(&state.paths, &session_id)?;
        Ok(json!({ "saved": true, "selection": selection }))
    });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssetBody {
    data_url: String,
    file_name: Option<String>,
    mime_type: Option<String>,
}

async fn api_write_panel_asset(
    State(state): State<Arc<AppState>>,
    Path((session_id, panel_id)): Path<(String, String)>,
    Json(body): Json<AssetBody>,
) -> Response {
    let result = Storage::open(&state.paths).and_then(|storage| {
        let image = data_url_to_buffer(&body.data_url)?;
        let requested_name = body.file_name.as_deref().unwrap_or("asset.png");
        let written = storage.write_asset_from_buffer(
            &session_id,
            &panel_id,
            requested_name,
            &image.bytes,
            false,
        )?;
        Ok(json!({
            "assetRef": written.asset_ref,
            "fileName": written.file_name,
            "filePath": written.file_path,
            "meta": {
                "assetRef": written.asset_ref,
                "fileName": written.file_name,
            },
            "mimeType": body.mime_type.unwrap_or(image.mime_type),
            "src": format!(
                "/api/panels/{}/{}/assets/{}",
                session_id,
                panel_id,
                written.file_name
            ),
        }))
    });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_read_panel_asset(
    State(state): State<Arc<AppState>>,
    Path((session_id, panel_id, asset_name)): Path<(String, String, String)>,
) -> Response {
    let asset_ref = format!("sessions/{session_id}/panels/{panel_id}/assets/{asset_name}");
    match Storage::open(&state.paths).and_then(|storage| storage.read_asset(&asset_ref)) {
        Ok(bytes) => bytes_response(StatusCode::OK, bytes, mime_type(&asset_name)),
        Err(error) => json_error(StatusCode::NOT_FOUND, error.message()),
    }
}

async fn index(State(state): State<Arc<AppState>>) -> Response {
    serve_static_path(&state, "index.html")
}

async fn static_asset(State(state): State<Arc<AppState>>, Path(path): Path<String>) -> Response {
    if path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let response = serve_static_path(&state, path.trim_start_matches('/'));
    if response.status() == StatusCode::NOT_FOUND {
        serve_static_path(&state, "index.html")
    } else {
        response
    }
}

fn serve_static_path(state: &AppState, path: &str) -> Response {
    if let Some(static_dir) = &state.static_dir {
        if let Some(response) = serve_file_from_dir(static_dir, path) {
            return response;
        }
    }
    if let Some(file) = STUDIO_DIST.get_file(path) {
        return bytes_response(StatusCode::OK, file.contents().to_vec(), mime_type(path));
    }
    StatusCode::NOT_FOUND.into_response()
}

fn serve_file_from_dir(static_dir: &std::path::Path, path: &str) -> Option<Response> {
    let candidate = static_dir.join(path);
    if !candidate.starts_with(static_dir) || !candidate.is_file() {
        return None;
    }
    Some(bytes_response(
        StatusCode::OK,
        fs::read(candidate).ok()?,
        mime_type(path),
    ))
}

fn json_response(status: StatusCode, payload: &impl serde::Serialize) -> Response {
    match serde_json::to_vec(payload) {
        Ok(bytes) => bytes_response(status, bytes, "application/json"),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

fn json_error(status: StatusCode, message: &str) -> Response {
    bytes_response(
        status,
        serde_json::json!({ "error": message })
            .to_string()
            .into_bytes(),
        "application/json",
    )
}

fn bytes_response(status: StatusCode, bytes: Vec<u8>, content_type: &str) -> Response {
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    response
}

fn mime_type(path: &str) -> &'static str {
    mime_guess::from_path(path)
        .first_raw()
        .unwrap_or("application/octet-stream")
}

struct DataUrlBytes {
    bytes: Vec<u8>,
    mime_type: String,
}

fn data_url_to_buffer(data_url: &str) -> Result<DataUrlBytes, CliError> {
    let Some(rest) = data_url.strip_prefix("data:") else {
        return Err(CliError::new("Expected a data URL"));
    };
    let Some((meta, data)) = rest.split_once(',') else {
        return Err(CliError::new("Expected a data URL"));
    };
    let is_base64 = meta.split(';').any(|part| part == "base64");
    let mime_type = meta
        .split(';')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("application/octet-stream")
        .to_owned();
    let bytes = if is_base64 {
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(to_cli_error)?
    } else {
        percent_decode(data)?
    };
    Ok(DataUrlBytes { bytes, mime_type })
}

fn percent_decode(value: &str) -> Result<Vec<u8>, CliError> {
    let mut bytes = Vec::new();
    let raw = value.as_bytes();
    let mut index = 0;
    while index < raw.len() {
        if raw[index] == b'%' && index + 2 < raw.len() {
            let hex = std::str::from_utf8(&raw[index + 1..index + 3]).map_err(to_cli_error)?;
            let byte = u8::from_str_radix(hex, 16).map_err(to_cli_error)?;
            bytes.push(byte);
            index += 3;
        } else {
            bytes.push(raw[index]);
            index += 1;
        }
    }
    Ok(bytes)
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
