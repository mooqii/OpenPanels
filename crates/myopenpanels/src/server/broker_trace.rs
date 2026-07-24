fn broker_token(headers: &HeaderMap) -> Result<&str, CliError> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            CliError::with_code(
                "execution_fenced",
                "A Task Broker execution token is required.",
            )
        })
}

fn broker_error(error: CliError) -> Response {
    let status = status_for_cli_error(&error);
    json_response(
        status,
        &json!({ "code": error.code().unwrap_or("broker_error"), "error": error.message() }),
    )
}

async fn api_broker_stage(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<StageFileRequest>,
) -> Response {
    let token = match broker_token(&headers) {
        Ok(token) => token,
        Err(error) => return broker_error(error),
    };
    if let Err(error) =
        content::authorize_agent_broker_capability(&state.paths, token, "content.write")
    {
        return broker_error(error);
    }
    match content::stage_file(&state.paths, token, &body) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => broker_error(error),
    }
}

async fn api_broker_read(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<ReadFileRequest>,
) -> Response {
    let token = match broker_token(&headers) {
        Ok(token) => token,
        Err(error) => return broker_error(error),
    };
    if let Err(error) =
        content::authorize_agent_broker_capability(&state.paths, token, "content.read")
    {
        return broker_error(error);
    }
    match content::read_file(&state.paths, token, &body) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => broker_error(error),
    }
}

async fn api_broker_prepare_skill(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<PrepareSkillRequest>,
) -> Response {
    let token = match broker_token(&headers) {
        Ok(token) => token,
        Err(error) => return broker_error(error),
    };
    if let Err(error) =
        content::authorize_agent_broker_capability(&state.paths, token, "skill.prepare")
    {
        return broker_error(error);
    }
    match content::prepare_skill(&state.paths, token, &body) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => broker_error(error),
    }
}

async fn api_broker_read_skill(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<SkillReadRequest>,
) -> Response {
    let token = match broker_token(&headers) {
        Ok(token) => token,
        Err(error) => return broker_error(error),
    };
    if let Err(error) =
        content::authorize_agent_broker_capability(&state.paths, token, "skill.read")
    {
        return broker_error(error);
    }
    match content::read_skill(&state.paths, token, &body) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => broker_error(error),
    }
}

async fn api_broker_task_context(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<TaskContextRequest>,
) -> Response {
    let token = match broker_token(&headers) {
        Ok(token) => token,
        Err(error) => return broker_error(error),
    };
    if let Err(error) =
        content::authorize_agent_broker_capability(&state.paths, token, "task-context.read")
    {
        return broker_error(error);
    }
    match content::read_task_context(&state.paths, token, &body) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => broker_error(error),
    }
}

async fn api_broker_publishing_checkpoint(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<PublishingCheckpointRequest>,
) -> Response {
    let token = match broker_token(&headers) {
        Ok(token) => token,
        Err(error) => return broker_error(error),
    };
    if let Err(error) = content::authorize_agent_broker_capability(
        &state.paths,
        token,
        "release.checkpoint",
    ) {
        return broker_error(error);
    }
    match content::publishing_checkpoint(&state.paths, token, &body) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => broker_error(error),
    }
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
