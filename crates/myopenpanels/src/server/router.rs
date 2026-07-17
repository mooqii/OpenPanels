use crate::agent as agent_resources;
use crate::bridge;
use crate::content::{
    self, BeginOperationRequest, PrepareOperationRequest, PrepareSkillRequest, ReadFileRequest,
    SkillReadRequest, StageFileRequest, TaskContextRequest,
};
use crate::control::{
    create_project, delete_project, ensure_project_bootstrap, now_iso, open_runtime_panel,
    read_active_panel_value, read_focus_revision, rename_project, validate_typesetting_state,
    write_active_project_id, BootstrapRequest,
};
use crate::error::CliError;
use crate::model_gateway;
use crate::paths::MyOpenPanelsPaths;
use crate::storage::Storage;
use crate::studio::{
    acquire_studio_transition_lock, open_browser, spawn_studio_server_process,
    write_studio_session, StudioSession,
};
use crate::tasks;
use crate::trace::{self, TraceEventInput};
use crate::types::PanelKind;
use crate::typesetting;
use crate::update::{check_for_update, download_update, install_update};
use crate::wiki;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::Json;
use axum::Router;
use base64::Engine;
use include_dir::{include_dir, Dir};
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
    std::env::set_var(
        "MYOPENPANELS_TASK_BROKER_URL",
        format!("http://127.0.0.1:{port}"),
    );
    std::env::set_var("MYOPENPANELS_TRACE_URL", trace::trace_url_for_port(port));
    std::env::set_var("MYOPENPANELS_TRACE_AUDIENCE", build_info.channel);
    let std_listener = TcpListener::bind((host, port)).map_err(to_cli_error)?;
    std_listener.set_nonblocking(true).map_err(to_cli_error)?;
    let listener = tokio::net::TcpListener::from_std(std_listener).map_err(to_cli_error)?;
    tasks::recover_builtin_worker_tasks_after_restart(&paths)?;
    let worker = bridge::start_builtin_worker_loop(paths.clone());
    content::start_gc_loop(paths.clone());
    write_studio_session(&paths, &studio_session_for_server(host, port, &paths))?;
    let shutdown_worker = worker.clone();
    let serve_result = axum::serve(
        listener,
        build_router(host.to_owned(), port, paths, static_dir, build_info),
    )
    .with_graceful_shutdown(async move {
        wait_for_studio_shutdown_signal().await;
        shutdown_worker.request_shutdown();
    })
    .await;
    worker.shutdown_and_join();
    serve_result.map_err(to_cli_error)?;
    Ok(0)
}

async fn wait_for_studio_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        if let Ok(mut terminate) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = terminate.recv() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
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
    let regular = Router::new()
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
        .route(
            "/api/tasks/wiki-update-groups/dispatch",
            put(api_wiki_update_group_dispatch),
        )
        .route("/api/tasks/{task_id}/dispatch", put(api_task_dispatch))
        .route("/api/tasks/{task_id}/cancel", post(api_task_cancel))
        .route("/api/tasks/{task_id}/archive", post(api_task_archive))
        .route("/api/tasks/{task_id}/events", get(api_task_events))
        .route("/api/tasks/{task_id}/attempts", get(api_task_attempts))
        .route("/api/workflows", get(api_workflows))
        .route("/api/workflows/{workflow_id}", get(api_workflow))
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
        .route(
            "/api/agent/routes",
            get(api_agent_routes).put(api_set_agent_route),
        )
        .route(
            "/api/agent/routes/{capability}",
            delete(api_remove_agent_route),
        )
        .route("/api/agent/skills", get(api_agent_skills))
        .route("/api/skills", get(api_skills))
        .route("/api/skills/import", post(api_import_skill))
        .route(
            "/api/skills/{skill_id}",
            get(api_skill_files).delete(api_delete_skill),
        )
        .route(
            "/api/skills/{skill_id}/file",
            axum::routing::put(api_write_skill_file),
        )
        .route("/api/device/skills", get(api_device_skills))
        .route("/api/device/skills/install", post(api_install_device_skill))
        .route(
            "/api/skills/{skill_id}/modules/{module_kind}",
            delete(api_remove_skill_module),
        )
        .route(
            "/api/skills/{skill_id}/source",
            put(api_replace_skill_source),
        )
        .route(
            "/api/skills/{skill_id}/mismatch-ignore",
            put(api_ignore_skill_mismatch),
        )
        .route("/api/agent/bridge/status", get(api_bridge_status))
        .route(
            "/api/model-gateway/settings",
            get(api_model_gateway_settings).put(api_model_gateway_save_settings),
        )
        .route(
            "/api/model-gateway/local-clis",
            get(api_model_gateway_local_clis).post(api_model_gateway_scan_local_clis),
        )
        .route(
            "/api/model-gateway/local-clis/test",
            post(api_model_gateway_test_local_cli),
        )
        .route(
            "/api/projects/{project_id}",
            patch(api_rename_project).delete(api_delete_project),
        )
        .route("/api/panels", post(api_open_panel))
        .route("/api/artifacts", post(api_insert_artifact))
        .route(
            "/api/typesetting/canvas-assets",
            get(api_typesetting_canvas_assets),
        )
        .route(
            "/api/writing/selection",
            get(api_writing_selection).put(api_writing_set_selection),
        )
        .route(
            "/api/writing/draft",
            axum::routing::put(api_writing_save_draft),
        )
        .route("/api/writing/skills", get(api_writing_skills))
        .route(
            "/api/writing/skills/{skill_id}",
            get(api_writing_skill_files).delete(api_delete_writing_skill),
        )
        .route(
            "/api/writing/skills/{skill_id}/file",
            axum::routing::put(api_write_writing_skill_file),
        )
        .route("/api/writing/requests", post(api_writing_create_request))
        .route(
            "/api/writing/refinement-requests",
            post(api_writing_create_refinement_request),
        )
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
            patch(api_wiki_rename_raw_document).delete(api_wiki_delete_raw_document),
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
            "/api/wiki/generated-documents/{document_id}/retry",
            post(api_wiki_retry_generated_document),
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
            "/api/wiki/spaces/{wiki_space_id}/maintain",
            post(api_wiki_maintain_space),
        )
        .route(
            "/api/wiki/spaces/{wiki_space_id}/pages",
            get(api_wiki_pages).post(api_wiki_write_page_at_collection),
        )
        .route(
            "/api/wiki/spaces/{wiki_space_id}/pages/{*page_path}",
            get(api_wiki_read_page)
                .patch(api_wiki_rename_page)
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
            "/api/projects/{project_id}/panels/{panel_id}/assets/import",
            post(api_import_typesetting_asset),
        )
        .route(
            "/api/projects/{project_id}/panels/{panel_id}/assets/{*asset_name}",
            get(api_read_panel_asset),
        )
        .route("/", get(index))
        .route("/{*path}", get(static_asset))
        .layer(cors);
    let broker = Router::new()
        .route("/api/task-broker/v3/stage", post(api_broker_stage))
        .route("/api/task-broker/v3/read", post(api_broker_read))
        .route(
            "/api/task-broker/v3/operations/begin",
            post(api_broker_begin_operation),
        )
        .route(
            "/api/task-broker/v3/operations/prepare",
            post(api_broker_prepare_operation),
        )
        .route(
            "/api/task-broker/v3/skills/prepare",
            post(api_broker_prepare_skill),
        )
        .route(
            "/api/task-broker/v3/skills/read",
            post(api_broker_read_skill),
        )
        .route(
            "/api/task-broker/v3/task-context",
            post(api_broker_task_context),
        );
    regular
        .merge(broker)
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        .layer(middleware::from_fn(trace_api_middleware))
        .with_state(Arc::new(AppState {
            build_info,
            host,
            paths,
            port,
            static_dir,
        }))
}
