use crate::agent as agent_resources;
use crate::bridge;
use crate::control::{
    create_project, delete_project, ensure_project_bootstrap, now_iso, open_runtime_panel,
    read_active_panel_value, rename_project, write_active_project_id, BootstrapRequest,
};
use crate::error::CliError;
use crate::paths::MyOpenPanelsPaths;
use crate::storage::Storage;
use crate::studio::{
    acquire_studio_transition_lock, open_browser, spawn_studio_server_process,
    write_studio_session, StudioSession,
};
use crate::tasks;
use crate::trace::{self, TraceEventInput};
use crate::types::PanelKind;
use crate::update::{check_for_update, download_update, install_update};
use crate::wiki;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::Json;
use axum::Router;
use base64::Engine;
use include_dir::{include_dir, Dir};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;
use tower_http::cors::CorsLayer;

mod wiki_api;
use wiki_api::*;

static STUDIO_DIST: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../apps/studio/dist");
const PROJECT_EVENT_POLL_INTERVAL_MS: u64 = 350;

#[derive(Clone)]
struct AppState {
    build_info: StudioBuildInfo,
    host: String,
    paths: MyOpenPanelsPaths,
    port: u16,
    static_dir: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StudioBuildInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    build_time: Option<String>,
    channel: &'static str,
    label: String,
    version: &'static str,
}

pub fn run_server(
    host: &str,
    port: u16,
    paths: MyOpenPanelsPaths,
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
    paths: MyOpenPanelsPaths,
    static_dir: Option<PathBuf>,
) -> Result<i32, CliError> {
    let build_info = current_build_info();
    std::env::set_var("MYOPENPANELS_TRACE_URL", trace::trace_url_for_port(port));
    std::env::set_var("MYOPENPANELS_TRACE_AUDIENCE", build_info.channel);
    bridge::start_builtin_worker_loop(paths.clone());
    tasks::start_dispatcher_loop(paths.clone(), format!("http://127.0.0.1:{port}"));
    let std_listener = TcpListener::bind((host, port)).map_err(to_cli_error)?;
    std_listener.set_nonblocking(true).map_err(to_cli_error)?;
    let listener = tokio::net::TcpListener::from_std(std_listener).map_err(to_cli_error)?;
    write_studio_session(&paths, &studio_session_for_server(host, port, &paths))?;
    axum::serve(
        listener,
        build_router(host.to_owned(), port, paths, static_dir, build_info),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
    .map_err(to_cli_error)?;
    Ok(0)
}

fn studio_session_for_server(host: &str, port: u16, paths: &MyOpenPanelsPaths) -> StudioSession {
    let local_server_url = format!("http://127.0.0.1:{port}");
    StudioSession {
        system_browser_url: Some(local_server_url.clone()),
        host: Some(host.to_owned()),
        lan_server_urls: Some(Vec::new()),
        local_server_url: Some(local_server_url.clone()),
        log_path: paths.studio_dir.join("studio.log").display().to_string(),
        pid: std::process::id(),
        port,
        server_url: local_server_url,
        started_at: now_iso(),
        storage_dir: paths.storage_dir.display().to_string(),
    }
}

fn build_router(
    host: String,
    port: u16,
    paths: MyOpenPanelsPaths,
    static_dir: Option<PathBuf>,
    build_info: StudioBuildInfo,
) -> Router {
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
        .route("/api/health", get(api_health))
        .route("/api/bootstrap", get(api_bootstrap))
        .route("/api/studio/open-browser", post(api_studio_open_browser))
        .route("/api/projects", get(api_projects).post(api_projects_create))
        .route("/api/update/status", get(api_update_status))
        .route("/api/update/download", post(api_update_download))
        .route(
            "/api/update/install-restart",
            post(api_update_install_restart),
        )
        .route("/api/trace/snapshot", get(api_trace_snapshot))
        .route("/api/trace/stream", get(api_trace_stream))
        .route("/api/trace/events", post(api_trace_events))
        .route("/api/events", get(api_project_events))
        .route("/api/tasks", get(api_tasks))
        .route("/api/tasks/next", get(api_next_task))
        .route("/api/tasks/claim-next", post(api_claim_next_task))
        .route("/api/tasks/{task_id}", get(api_inspect_task))
        .route("/api/tasks/{task_id}/claim", post(api_claim_task))
        .route("/api/tasks/{task_id}/heartbeat", post(api_task_heartbeat))
        .route("/api/tasks/{task_id}/complete", post(api_task_complete))
        .route("/api/tasks/{task_id}/fail", post(api_task_fail))
        .route("/api/tasks/{task_id}/release", post(api_task_release))
        .route("/api/tasks/{task_id}/retry", post(api_task_retry))
        .route("/api/tasks/{task_id}/cancel", post(api_task_cancel))
        .route("/api/tasks/{task_id}/deliveries", get(api_task_deliveries))
        .route(
            "/api/agent/targets",
            get(api_agent_targets).post(api_register_agent_target),
        )
        .route(
            "/api/agent/targets/{target_id}",
            delete(api_remove_agent_target),
        )
        .route(
            "/api/agent/targets/{target_id}/heartbeat",
            post(api_agent_target_heartbeat),
        )
        .route("/api/agent/skills", get(api_agent_skills))
        .route("/api/agent/bridge/status", get(api_bridge_status))
        .route(
            "/api/projects/{project_id}",
            patch(api_rename_project).delete(api_delete_project),
        )
        .route("/api/panels", post(api_open_panel))
        .route("/api/artifacts", post(api_insert_artifact))
        .route("/api/wiki/context", get(api_wiki_context))
        .route(
            "/api/wiki/selection",
            get(api_wiki_selection).put(api_wiki_set_selection),
        )
        .route(
            "/api/wiki/raw-documents",
            get(api_wiki_raw_documents).post(api_wiki_add_raw_document),
        )
        .route(
            "/api/wiki/raw-documents/{document_id}/markdown",
            get(api_wiki_read_markdown).put(api_wiki_write_markdown),
        )
        .route(
            "/api/wiki/raw-documents/{document_id}/original",
            get(api_wiki_raw_document_original),
        )
        .route(
            "/api/wiki/raw-documents/{document_id}/reveal",
            post(api_wiki_reveal_raw_document),
        )
        .route(
            "/api/wiki/raw-documents/{document_id}/extract",
            post(api_wiki_extract_raw_document),
        )
        .route(
            "/api/wiki/raw-documents/{document_id}/reindex",
            post(api_wiki_reindex_raw_document),
        )
        .route(
            "/api/wiki/raw-documents/{document_id}",
            delete(api_wiki_delete_raw_document),
        )
        .route(
            "/api/wiki/generated-documents",
            get(api_wiki_generated_documents).post(api_wiki_create_generated_document),
        )
        .route(
            "/api/wiki/generated-documents/{document_id}",
            get(api_wiki_read_generated_document)
                .put(api_wiki_update_generated_document)
                .delete(api_wiki_delete_generated_document),
        )
        .route(
            "/api/wiki/generated-documents/{document_id}/publish",
            post(api_wiki_publish_generated_document),
        )
        .route(
            "/api/wiki/active-space",
            get(api_wiki_active_space).put(api_wiki_set_active_space),
        )
        .route(
            "/api/wiki/agent-skill",
            get(api_wiki_agent_skill)
                .put(api_wiki_set_agent_skill)
                .post(api_wiki_set_agent_skill),
        )
        .route("/api/wiki/spaces", get(api_wiki_spaces))
        .route(
            "/api/wiki/spaces/{wiki_space_id}/reindex",
            post(api_wiki_reindex_space),
        )
        .route(
            "/api/wiki/spaces/{wiki_space_id}/pages",
            get(api_wiki_pages).post(api_wiki_write_page_at_collection),
        )
        .route(
            "/api/wiki/spaces/{wiki_space_id}/pages/{*page_path}",
            get(api_wiki_read_page)
                .put(api_wiki_write_page)
                .post(api_wiki_write_page),
        )
        .route(
            "/api/active-project",
            get(api_active_project).put(api_set_active_project),
        )
        .route(
            "/api/active-panel",
            get(api_active_panel).put(api_set_active_panel),
        )
        .route(
            "/api/projects/{project_id}/panels/{panel_id}/state",
            get(api_get_panel_state).put(api_save_panel_state),
        )
        .route(
            "/api/projects/{project_id}/panels/{panel_id}/selection",
            get(api_get_panel_selection).put(api_save_panel_selection),
        )
        .route(
            "/api/projects/{project_id}/panels/{panel_id}/selection-materializations",
            get(api_get_selection_materialization),
        )
        .route(
            "/api/projects/{project_id}/panels/{panel_id}/selection-materializations/{request_id}",
            post(api_complete_selection_materialization),
        )
        .route(
            "/api/projects/{project_id}/panels/{panel_id}/assets",
            post(api_write_panel_asset),
        )
        .route(
            "/api/projects/{project_id}/panels/{panel_id}/assets/{*asset_name}",
            get(api_read_panel_asset),
        )
        .route("/", get(index))
        .route("/{*path}", get(static_asset))
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        .layer(cors)
        .layer(middleware::from_fn(trace_api_middleware))
        .with_state(Arc::new(AppState {
            build_info,
            host,
            paths,
            port,
            static_dir,
        }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BootstrapQuery {
    panel_id: Option<String>,
    panel_kind: Option<String>,
    project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TraceQuery {
    audience: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TasksQuery {
    pending: Option<bool>,
    queue: Option<String>,
    status: Option<String>,
}

async fn api_health(State(state): State<Arc<AppState>>) -> Response {
    no_store_response(json_response(
        StatusCode::OK,
        &json!({
            "ok": true,
            "version": state.build_info.version,
            "contextId": state.paths.context_id.clone(),
        }),
    ))
}

async fn api_bootstrap(
    State(state): State<Arc<AppState>>,
    Query(query): Query<BootstrapQuery>,
) -> Response {
    let request = BootstrapRequest {
        requested_panel_id: query.panel_id,
        requested_panel_kind: query.panel_kind.as_deref().and_then(PanelKind::parse),
        requested_project_id: query.project_id,
    };
    no_store_response(match ensure_project_bootstrap(&state.paths, request) {
        Ok(bootstrap) => json_response(StatusCode::OK, &with_build_info(bootstrap, &state)),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenBrowserBody {
    path: Option<String>,
}

async fn api_studio_open_browser(
    State(state): State<Arc<AppState>>,
    Json(body): Json<OpenBrowserBody>,
) -> Response {
    let path = safe_browser_path(body.path.as_deref());
    let url = format!("http://127.0.0.1:{}{path}", state.port);
    studio_open_browser_response(&url, open_browser)
}

fn studio_open_browser_response(
    url: &str,
    opener: impl FnOnce(&str) -> Result<(), CliError>,
) -> Response {
    match opener(url) {
        Ok(()) => json_response(
            StatusCode::OK,
            &json!({
                "ok": true,
                "opened": true,
                "openTarget": "system_browser",
                "url": url,
            }),
        ),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

fn safe_browser_path(path: Option<&str>) -> &str {
    path.filter(|value| {
        value.starts_with('/') && !value.starts_with("//") && !value.contains(['\r', '\n'])
    })
    .unwrap_or("/")
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
        Ok(bootstrap) => json_response(StatusCode::OK, &with_build_info(bootstrap, &state)),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_trace_snapshot(Query(query): Query<TraceQuery>) -> Response {
    let audience = query.audience.as_deref().unwrap_or("release");
    json_response(StatusCode::OK, &trace::snapshot(audience))
}

async fn api_trace_stream(Query(query): Query<TraceQuery>) -> impl IntoResponse {
    let audience = query.audience.unwrap_or_else(|| "release".to_owned());
    let stream = UnboundedReceiverStream::new(trace::subscribe()).filter_map(move |event| {
        let audience = audience.clone();
        trace::event_for_audience(&event, &audience).map(|event| {
            Ok::<Event, std::convert::Infallible>(
                Event::default()
                    .event("trace")
                    .data(serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_owned())),
            )
        })
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn api_trace_events(Json(body): Json<Value>) -> Response {
    let inputs = if let Some(events) = body.as_array() {
        events.clone()
    } else {
        vec![body]
    };
    let mut recorded = Vec::new();
    for value in inputs {
        let input = serde_json::from_value::<TraceEventInput>(value.clone()).unwrap_or_else(|_| {
            TraceEventInput {
                audience: None,
                category: Some("system".to_owned()),
                detail: Some(value),
                direction: Some("in".to_owned()),
                release_summary: None,
                run_id: None,
                source: Some("trace-client".to_owned()),
                summary: Some("Trace event".to_owned()),
                task_id: None,
            }
        });
        recorded.push(trace::record(input));
    }
    json_response(StatusCode::OK, &json!({ "events": recorded }))
}

async fn api_tasks(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TasksQuery>,
) -> Response {
    match tasks::list_tasks(&state.paths, task_list_filter(&query)) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_bridge_status(State(state): State<Arc<AppState>>) -> Response {
    match bridge::read_bridge_status(&state.paths) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_agent_targets(State(state): State<Arc<AppState>>) -> Response {
    match tasks::list_targets(&state.paths) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentTargetRegistrationBody {
    name: String,
    host: Option<String>,
    transport: String,
    endpoint: Option<String>,
    capabilities: Vec<String>,
    priority: Option<i64>,
}

async fn api_register_agent_target(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AgentTargetRegistrationBody>,
) -> Response {
    match tasks::register_target(
        &state.paths,
        tasks::TargetRegistration {
            name: &body.name,
            host: body.host.as_deref(),
            transport: &body.transport,
            endpoint: body.endpoint.as_deref(),
            capabilities: body.capabilities,
            priority: body.priority.unwrap_or(0),
        },
    ) {
        Ok(payload) => json_response(StatusCode::CREATED, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_agent_target_heartbeat(
    State(state): State<Arc<AppState>>,
    Path(target_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = verify_http_target(&state.paths, &target_id, &headers) {
        return json_error(status_for_cli_error(&error), error.message());
    }
    match tasks::heartbeat_target(&state.paths, &target_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_remove_agent_target(
    State(state): State<Arc<AppState>>,
    Path(target_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = verify_http_target(&state.paths, &target_id, &headers) {
        return json_error(status_for_cli_error(&error), error.message());
    }
    match tasks::remove_target(&state.paths, &target_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskClaimBody {
    target_id: String,
    capability: Option<String>,
    wait_ms: Option<u64>,
}

async fn api_claim_next_task(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<TaskClaimBody>,
) -> Response {
    if let Err(error) = verify_http_target(&state.paths, &body.target_id, &headers) {
        return json_error(status_for_cli_error(&error), error.message());
    }
    let paths = state.paths.clone();
    let target_id = body.target_id;
    let capability = body.capability;
    let wait_ms = body.wait_ms;
    let result = tokio::task::spawn_blocking(move || {
        tasks::claim_next(&paths, &target_id, capability.as_deref(), wait_ms)
    })
    .await
    .map_err(to_cli_error)
    .and_then(|result| result);
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_claim_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<TaskClaimBody>,
) -> Response {
    if let Err(error) = verify_http_target(&state.paths, &body.target_id, &headers) {
        return json_error(status_for_cli_error(&error), error.message());
    }
    match tasks::claim_task(&state.paths, &task_id, &body.target_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskLeaseBody {
    lease_token: String,
    message: Option<String>,
    result: Option<Value>,
    retry_after: Option<String>,
}

async fn api_task_heartbeat(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    Json(body): Json<TaskLeaseBody>,
) -> Response {
    match tasks::heartbeat_task(&state.paths, &task_id, &body.lease_token) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_task_complete(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    Json(body): Json<TaskLeaseBody>,
) -> Response {
    match tasks::complete_task(&state.paths, &task_id, &body.lease_token, body.result) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_task_fail(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    Json(body): Json<TaskLeaseBody>,
) -> Response {
    match tasks::fail_task(
        &state.paths,
        &task_id,
        &body.lease_token,
        body.message.as_deref().unwrap_or("Task failed."),
        body.retry_after.as_deref(),
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_task_release(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    Json(body): Json<TaskLeaseBody>,
) -> Response {
    match tasks::release_task(&state.paths, &task_id, &body.lease_token) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_task_retry(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Response {
    match tasks::retry_task(&state.paths, &task_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_task_cancel(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Response {
    match tasks::cancel_task(&state.paths, &task_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_task_deliveries(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Response {
    match tasks::list_deliveries(&state.paths, Some(&task_id)) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

fn verify_http_target(
    paths: &MyOpenPanelsPaths,
    target_id: &str,
    headers: &HeaderMap,
) -> Result<(), CliError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| {
            CliError::with_code("unauthorized_target", "Missing target bearer token.")
        })?;
    tasks::verify_target_token(paths, target_id, token)
}

async fn api_agent_skills(State(state): State<Arc<AppState>>) -> Response {
    match agent_resources::list_agent_skills(&state.paths) {
        Ok(skills) => json_response(StatusCode::OK, &json!({ "skills": skills })),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_next_task(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TasksQuery>,
) -> Response {
    match tasks::next_task(&state.paths, task_list_filter(&query)) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

fn task_list_filter(query: &TasksQuery) -> tasks::TaskListFilter<'_> {
    tasks::TaskListFilter {
        pending: query.pending.unwrap_or(false),
        queue: query.queue.as_deref(),
        status: query.status.as_deref(),
    }
}

async fn api_inspect_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Response {
    match tasks::inspect_task(&state.paths, &task_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::NOT_FOUND, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectEventsQuery {
    project_id: Option<String>,
}

async fn api_project_events(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ProjectEventsQuery>,
) -> impl IntoResponse {
    let paths = state.paths.clone();
    let project_id = query.project_id;
    let (sender, receiver) = mpsc::unbounded_channel::<Result<Event, std::convert::Infallible>>();

    tokio::spawn(async move {
        let mut last_seq = read_storage_change_seq(&paths).unwrap_or(0);
        let mut interval =
            tokio::time::interval(Duration::from_millis(PROJECT_EVENT_POLL_INTERVAL_MS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            let (next_seq, changes) =
                match read_change_scopes_after(&paths, last_seq, project_id.as_deref()) {
                    Ok(result) => result,
                    Err(error) => {
                        let payload = json!({
                            "kind": "error",
                            "message": error.message(),
                        });
                        if sender
                            .send(Ok(Event::default()
                                .event("error")
                                .data(payload.to_string())))
                            .is_err()
                        {
                            break;
                        }
                        continue;
                    }
                };
            if next_seq == last_seq {
                continue;
            }
            last_seq = next_seq;
            for change in changes {
                if sender
                    .send(Ok(Event::default().event("project").data(
                        serde_json::to_string(&change).unwrap_or_else(|_| "{}".to_owned()),
                    )))
                    .is_err()
                {
                    return;
                }
            }
        }
    });

    Sse::new(UnboundedReceiverStream::new(receiver)).keep_alive(KeepAlive::default())
}

fn read_storage_change_seq(paths: &MyOpenPanelsPaths) -> Result<i64, CliError> {
    Storage::open(paths).and_then(|storage| storage.read_change_seq())
}

fn read_change_scopes_after(
    paths: &MyOpenPanelsPaths,
    revision: i64,
    project_id: Option<&str>,
) -> Result<(i64, Vec<crate::storage::ChangeScope>), CliError> {
    Storage::open(paths).and_then(|storage| storage.read_changes_after(revision, project_id))
}

async fn api_projects(State(state): State<Arc<AppState>>) -> Response {
    match Storage::open(&state.paths).and_then(|storage| storage.list_projects()) {
        Ok(projects) => json_response(StatusCode::OK, &projects),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
struct RenameProjectBody {
    title: Option<String>,
}

async fn api_rename_project(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
    Json(body): Json<RenameProjectBody>,
) -> Response {
    match rename_project(&state.paths, &project_id, body.title.as_deref()) {
        Ok(project) => json_response(StatusCode::OK, &json!({ "project": project })),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_delete_project(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> Response {
    match delete_project(&state.paths, &project_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenPanelBody {
    initial_state: Option<Value>,
    kind: String,
    project_id: String,
    title: Option<String>,
}

async fn api_open_panel(
    State(state): State<Arc<AppState>>,
    Json(body): Json<OpenPanelBody>,
) -> Response {
    let Some(kind) = PanelKind::parse(&body.kind) else {
        return json_error(
            StatusCode::BAD_REQUEST,
            "Expected kind to be one of: wiki, canvas, image, diff, preview, files.",
        );
    };
    match open_runtime_panel(
        &state.paths,
        &body.project_id,
        kind,
        body.title.as_deref(),
        body.initial_state,
    ) {
        Ok(panel) => json_response(StatusCode::OK, &panel),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InsertArtifactBody {
    artifact: Value,
    panel_id: Option<String>,
    project_id: String,
}

async fn api_insert_artifact(
    State(state): State<Arc<AppState>>,
    Json(body): Json<InsertArtifactBody>,
) -> Response {
    let result = Storage::open(&state.paths).and_then(|storage| {
        let timestamp = now_iso();
        let mut artifact = body.artifact;
        let object = artifact
            .as_object_mut()
            .ok_or_else(|| CliError::new("Artifact must be a JSON object."))?;
        object
            .entry("id".to_owned())
            .or_insert_with(|| json!(create_id("artifact")));
        object
            .entry("createdAt".to_owned())
            .or_insert_with(|| json!(timestamp));
        if !object.contains_key("panelId") {
            if let Some(panel_id) = body.panel_id {
                object.insert("panelId".to_owned(), json!(panel_id));
            }
        }
        storage.write_artifact(&body.project_id, &artifact)?;
        Ok(artifact)
    });
    match result {
        Ok(artifact) => json_response(StatusCode::OK, &artifact),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateStatusQuery {
    force: Option<String>,
    refresh: Option<String>,
}

impl UpdateStatusQuery {
    fn bypass_cache(&self) -> bool {
        query_flag(self.refresh.as_deref()) || query_flag(self.force.as_deref())
    }
}

fn query_flag(value: Option<&str>) -> bool {
    value
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "" | "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

async fn api_update_status(Query(query): Query<UpdateStatusQuery>) -> Response {
    match check_for_update(env!("CARGO_PKG_VERSION"), !query.bypass_cache()) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_update_download() -> Response {
    match download_update(env!("CARGO_PKG_VERSION")) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_update_install_restart(State(state): State<Arc<AppState>>) -> Response {
    let install = match install_update(env!("CARGO_PKG_VERSION")) {
        Ok(payload) => payload,
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    };

    if !install.updated {
        return json_response(
            StatusCode::OK,
            &json!({
                "ok": true,
                "restarting": false,
                "update": install,
                "message": "MyOpenPanels is already up to date.",
            }),
        );
    }

    match spawn_restarted_studio(&state) {
        Ok(session) => {
            schedule_current_server_exit();
            json_response(
                StatusCode::OK,
                &json!({
                    "ok": true,
                    "restarting": true,
                    "session": session,
                    "update": install,
                    "message": "Restarting MyOpenPanels Studio.",
                }),
            )
        }
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

fn spawn_restarted_studio(state: &AppState) -> Result<StudioSession, CliError> {
    let _transition_lock = acquire_studio_transition_lock(&state.paths)?;
    let process = spawn_studio_server_process(
        &state.paths,
        state.port,
        &state.host,
        state.static_dir.as_ref(),
        Some(750),
    )?;

    let local_server_url = format!("http://127.0.0.1:{}", state.port);
    let session = StudioSession {
        system_browser_url: Some(local_server_url.clone()),
        host: Some(state.host.clone()),
        lan_server_urls: Some(Vec::new()),
        local_server_url: Some(local_server_url.clone()),
        log_path: process.log_path.display().to_string(),
        pid: process.pid,
        port: state.port,
        server_url: local_server_url,
        started_at: now_iso(),
        storage_dir: state.paths.storage_dir.display().to_string(),
    };
    write_studio_session(&state.paths, &session)?;
    Ok(session)
}

fn schedule_current_server_exit() {
    thread::spawn(|| {
        thread::sleep(Duration::from_millis(150));
        std::process::exit(0);
    });
}

async fn api_active_project(State(state): State<Arc<AppState>>) -> Response {
    match ensure_project_bootstrap(&state.paths, BootstrapRequest::new()) {
        Ok(bootstrap) => json_response(
            StatusCode::OK,
            &json!({ "projectId": bootstrap.project.id }),
        ),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActiveProjectBody {
    project_id: Option<String>,
}

async fn api_set_active_project(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ActiveProjectBody>,
) -> Response {
    let Some(project_id) = body.project_id.filter(|value| !value.trim().is_empty()) else {
        return json_error(StatusCode::BAD_REQUEST, "Missing projectId.");
    };
    let result = Storage::open(&state.paths).and_then(|storage| {
        if storage.read_project(&project_id)?.is_none() {
            return Err(CliError::new(format!(
                "MyOpenPanels project not found: {project_id}"
            )));
        }
        write_active_project_id(&state.paths, &project_id)?;
        ensure_project_bootstrap(
            &state.paths,
            BootstrapRequest {
                requested_project_id: Some(project_id.clone()),
                requested_panel_id: None,
                requested_panel_kind: None,
            },
        )?;
        Ok(json!({ "projectId": project_id }))
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
    project_id: Option<String>,
}

async fn api_set_active_panel(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ActivePanelBody>,
) -> Response {
    let result = ensure_project_bootstrap(
        &state.paths,
        BootstrapRequest {
            requested_project_id: body.project_id,
            requested_panel_id: body.panel_id,
            requested_panel_kind: body.kind.as_deref().and_then(PanelKind::parse),
        },
    )
    .map(|bootstrap| {
        json!({
            "activePanelId": bootstrap.active_panel_id,
            "activePanelKind": bootstrap.active_panel_kind,
            "panel": bootstrap.panel,
            "revision": bootstrap.revision,
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
    Path((project_id, panel_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Response {
    let base_revision = body.get("baseRevision").and_then(Value::as_i64);
    let panel_state = body.get("state").cloned().unwrap_or_else(|| body.clone());
    let result = Storage::open(&state.paths).and_then(|storage| {
        storage
            .write_panel_state_if_current(&project_id, &panel_id, &panel_state, base_revision)
            .map(|result| {
                result.map(|revision| {
                    json!({
                        "saved": true,
                        "projectId": project_id,
                        "panelId": panel_id,
                        "revision": revision,
                    })
                })
            })
    });
    match result {
        Ok(Ok(payload)) => {
            if let Err(error) = write_active_project_id(&state.paths, &project_id) {
                return json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message());
            }
            json_response(StatusCode::OK, &payload)
        }
        Ok(Err(conflict)) => json_response(
            StatusCode::CONFLICT,
            &json!({
                "saved": false,
                "error": "Canvas state is stale.",
                "baseRevision": conflict.base_revision,
                "currentRevision": conflict.current_revision,
                "projectId": project_id,
                "panelId": panel_id,
            }),
        ),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_get_panel_state(
    State(state): State<Arc<AppState>>,
    Path((project_id, panel_id)): Path<(String, String)>,
) -> Response {
    let result = Storage::open(&state.paths).and_then(|storage| {
        let panel = storage.read_panel(&project_id, &panel_id)?.ok_or_else(|| {
            CliError::with_code("panel_not_found", format!("Panel not found: {panel_id}"))
        })?;
        let state = storage
            .read_panel_state(&project_id, &panel_id)?
            .ok_or_else(|| {
                CliError::with_code(
                    "panel_state_not_found",
                    format!("Panel state not found: {panel_id}"),
                )
            })?;
        let revision = storage.read_panel_state_revision(&project_id, &panel_id)?;
        Ok(json!({ "panel": panel, "state": state, "revision": revision }))
    });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectionBody {
    selection: Option<Value>,
}

async fn api_get_panel_selection(
    State(state): State<Arc<AppState>>,
    Path((project_id, panel_id)): Path<(String, String)>,
) -> Response {
    let result = Storage::open(&state.paths).and_then(|storage| {
        let panel = storage.read_panel(&project_id, &panel_id)?.ok_or_else(|| {
            CliError::with_code("panel_not_found", format!("Panel not found: {panel_id}"))
        })?;
        let selection = storage.read_panel_selection(&project_id, &panel_id)?;
        let revision = storage.read_panel_selection_revision(&project_id, &panel_id)?;
        Ok(json!({ "panel": panel, "selection": selection, "revision": revision }))
    });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_save_panel_selection(
    State(state): State<Arc<AppState>>,
    Path((project_id, panel_id)): Path<(String, String)>,
    Json(body): Json<SelectionBody>,
) -> Response {
    let result = Storage::open(&state.paths).and_then(|storage| {
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
        let asset_ref = selected_shapes
            .iter()
            .find_map(|shape| shape.pointer("/asset/assetRef").and_then(Value::as_str))
            .or_else(|| {
                body.selection
                    .as_ref()
                    .and_then(|selection| selection.get("assetRef"))
                    .and_then(Value::as_str)
            })
            .map(str::to_owned);
        let selection = json!({
            "projectId": project_id,
            "panelId": panel_id,
            "selectedShapeIds": selected_shape_ids,
            "selectedShapes": selected_shapes,
            "assetRef": asset_ref,
            "updatedAt": now_iso(),
        });
        storage.write_panel_selection(&project_id, &panel_id, &selection)?;
        write_active_project_id(&state.paths, &project_id)?;
        Ok(json!({ "saved": true, "selection": selection }))
    });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_get_selection_materialization(
    State(state): State<Arc<AppState>>,
    Path((project_id, panel_id)): Path<(String, String)>,
) -> Response {
    match crate::selection::pending_materialization_request(&state.paths, &project_id, &panel_id) {
        Ok(request) => json_response(StatusCode::OK, &json!({ "request": request })),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectionMaterializationBody {
    image_data_url: String,
}

async fn api_complete_selection_materialization(
    State(state): State<Arc<AppState>>,
    Path((project_id, panel_id, request_id)): Path<(String, String, String)>,
    Json(body): Json<SelectionMaterializationBody>,
) -> Response {
    let result = data_url_to_buffer(&body.image_data_url).and_then(|image| {
        crate::selection::complete_materialization_request(
            &state.paths,
            &project_id,
            &panel_id,
            &request_id,
            &image.bytes,
        )
    });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
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
    Path((project_id, panel_id)): Path<(String, String)>,
    Json(body): Json<AssetBody>,
) -> Response {
    let result = Storage::open(&state.paths).and_then(|storage| {
        let image = data_url_to_buffer(&body.data_url)?;
        let requested_name = body.file_name.as_deref().unwrap_or("asset.png");
        let written = storage.write_asset_from_buffer(
            &project_id,
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
                "/api/projects/{}/panels/{}/assets/{}",
                project_id,
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
    Path((project_id, panel_id, asset_name)): Path<(String, String, String)>,
) -> Response {
    let asset_ref = format!("projects/{project_id}/panels/{panel_id}/assets/{asset_name}");
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
    let response = if let Some(static_dir) = &state.static_dir {
        if let Some(response) = serve_file_from_dir(static_dir, path) {
            response
        } else if let Some(file) = STUDIO_DIST.get_file(path) {
            bytes_response(StatusCode::OK, file.contents().to_vec(), mime_type(path))
        } else {
            return StatusCode::NOT_FOUND.into_response();
        }
    } else if let Some(file) = STUDIO_DIST.get_file(path) {
        bytes_response(StatusCode::OK, file.contents().to_vec(), mime_type(path))
    } else {
        return StatusCode::NOT_FOUND.into_response();
    };

    if path == "index.html" {
        cache_control_response(response, "no-cache, must-revalidate")
    } else if path.starts_with("assets/") {
        cache_control_response(response, "public, max-age=31536000, immutable")
    } else {
        cache_control_response(response, "no-cache")
    }
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

async fn trace_api_middleware(request: axum::http::Request<Body>, next: Next) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_owned();
    let should_trace =
        path.starts_with("/api/") && !path.starts_with("/api/trace/") && path != "/api/health";
    let started = Instant::now();
    let response = next.run(request).await;
    if should_trace {
        let status = response.status().as_u16();
        let elapsed_ms = started.elapsed().as_millis() as u64;
        trace::record_simple(
            "api",
            "rust-server",
            Some("request"),
            format!("{method} {path} -> {status}"),
            None,
            Some(json!({
                "method": method,
                "path": path,
                "status": status,
                "elapsedMs": elapsed_ms,
            })),
        );
    }
    response
}

fn with_build_info(payload: impl Serialize, state: &AppState) -> Value {
    let mut value = serde_json::to_value(payload).unwrap_or_else(|_| json!({}));
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "buildInfo".to_owned(),
            serde_json::to_value(&state.build_info).unwrap_or_else(|_| json!({})),
        );
        object.insert(
            "agentWorker".to_owned(),
            bridge::read_bridge_status(&state.paths)
                .unwrap_or_else(|_| json!({ "status": "idle" })),
        );
        if let Ok(operations) = Storage::open(&state.paths)
            .and_then(|storage| storage.list_agent_operations(Some(&state.paths.context_id), None))
        {
            object.insert("agentOperations".to_owned(), json!(operations));
        }
        if let Ok(task_payload) = tasks::list_tasks(&state.paths, tasks::TaskListFilter::default())
        {
            for key in [
                "tasks",
                "pendingCount",
                "readyCount",
                "blockedCount",
                "unhandledCount",
                "runningCount",
            ] {
                if let Some(task_value) = task_payload.get(key) {
                    object.insert(key.to_owned(), task_value.clone());
                }
            }
            if let Some(pending_count) = task_payload.get("pendingCount") {
                object.insert("pendingTaskCount".to_owned(), pending_count.clone());
            }
        }
    }
    value
}

fn current_build_info() -> StudioBuildInfo {
    let is_development = std::env::current_exe()
        .ok()
        .and_then(|path| path.to_str().map(str::to_owned))
        .map(|path| {
            path.contains("/target/debug/")
                || path.contains("\\target\\debug\\")
                || std::env::var("MYOPENPANELS_DEV").is_ok()
        })
        .unwrap_or_else(|| std::env::var("MYOPENPANELS_DEV").is_ok());
    if !is_development {
        return StudioBuildInfo {
            build_time: None,
            channel: "release",
            label: env!("CARGO_PKG_VERSION").to_owned(),
            version: env!("CARGO_PKG_VERSION"),
        };
    }

    StudioBuildInfo {
        build_time: development_build_time(),
        channel: "development",
        label: "dev".to_owned(),
        version: env!("CARGO_PKG_VERSION"),
    }
}

fn development_build_time() -> Option<String> {
    let modified = std::env::current_exe()
        .ok()
        .and_then(|path| fs::metadata(path).ok())
        .and_then(|metadata| metadata.modified().ok())?;
    let datetime: chrono::DateTime<chrono::Utc> = modified.into();
    Some(datetime.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

fn json_response(status: StatusCode, payload: &impl serde::Serialize) -> Response {
    match serde_json::to_vec(payload) {
        Ok(bytes) => bytes_response(status, bytes, "application/json"),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

fn no_store_response(response: Response) -> Response {
    cache_control_response(response, "no-store")
}

fn cache_control_response(mut response: Response, value: &'static str) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(value));
    response
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

fn status_for_cli_error(error: &CliError) -> StatusCode {
    match error.code() {
        Some(
            "no_current_project" | "project_not_found" | "task_not_found" | "target_not_found",
        ) => StatusCode::NOT_FOUND,
        Some("unauthorized_target" | "invalid_lease" | "lease_expired") => StatusCode::UNAUTHORIZED,
        Some("task_not_claimable") => StatusCode::CONFLICT,
        Some("invalid_target" | "invalid_retry_after") => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
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

fn content_disposition_inline(file_name: &str) -> String {
    let fallback = std::path::Path::new(file_name)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("document")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("inline; filename=\"{}\"", fallback.replace('"', "_"))
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

fn create_id(prefix: &str) -> String {
    let random: u128 = rand::rng().random();
    format!("{prefix}:{random:032x}")
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::resolve_myopenpanels_paths;

    #[test]
    fn update_status_query_can_bypass_cached_checks() {
        assert!(!UpdateStatusQuery::default().bypass_cache());
        assert!(UpdateStatusQuery {
            refresh: Some("1".to_owned()),
            ..UpdateStatusQuery::default()
        }
        .bypass_cache());
        assert!(UpdateStatusQuery {
            force: Some("true".to_owned()),
            ..UpdateStatusQuery::default()
        }
        .bypass_cache());
    }

    #[test]
    fn browser_path_only_accepts_local_relative_urls() {
        assert_eq!(
            safe_browser_path(Some("/wiki?tab=source#top")),
            "/wiki?tab=source#top"
        );
        assert_eq!(safe_browser_path(Some("https://example.com")), "/");
        assert_eq!(safe_browser_path(Some("//example.com")), "/");
        assert_eq!(safe_browser_path(Some("/\r\nexample.com")), "/");
    }

    #[tokio::test]
    async fn studio_browser_endpoint_reports_successful_system_open() {
        let url = "http://127.0.0.1:43217/wiki";
        let response = studio_open_browser_response(url, |opened_url| {
            assert_eq!(opened_url, url);
            Ok(())
        });

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        let payload = serde_json::from_slice::<Value>(&body).expect("json response");
        assert_eq!(payload["opened"], true);
        assert_eq!(payload["openTarget"], "system_browser");
        assert_eq!(payload["url"], url);
    }

    #[tokio::test]
    async fn studio_browser_endpoint_propagates_launcher_failure() {
        let url = "http://127.0.0.1:43217";
        let response = studio_open_browser_response(url, |_| {
            Err(CliError::with_recovery(
                "browser_open_failed",
                "browser launcher failed",
                true,
                format!("Open {url} manually."),
            ))
        });

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        let payload = serde_json::from_slice::<Value>(&body).expect("json response");
        assert_eq!(payload["error"], "browser launcher failed");
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");
        let state = Arc::new(AppState {
            build_info: current_build_info(),
            host: "127.0.0.1".to_owned(),
            paths,
            port: 0,
            static_dir: None,
        });

        let response = api_health(State(state.clone())).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );

        let response = api_bootstrap(
            State(state),
            Query(BootstrapQuery {
                panel_id: None,
                panel_kind: None,
                project_id: None,
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
    }

    #[test]
    fn studio_static_assets_use_release_cache_policy() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");
        let state = AppState {
            build_info: current_build_info(),
            host: "127.0.0.1".to_owned(),
            paths,
            port: 0,
            static_dir: None,
        };

        let index = serve_static_path(&state, "index.html");
        assert_eq!(
            index.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-cache, must-revalidate"
        );

        let asset = STUDIO_DIST
            .get_dir("assets")
            .and_then(|assets| assets.files().next())
            .expect("built studio asset");
        let asset_path = asset.path().to_str().expect("asset path");
        let response = serve_static_path(&state, asset_path);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "public, max-age=31536000, immutable"
        );
    }
}
