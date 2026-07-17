async fn api_model_gateway_settings(State(state): State<Arc<AppState>>) -> Response {
    match model_gateway::settings_payload(&state.paths) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_cli_error(&error),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct ModelGatewaySettingsBody {
    settings: model_gateway::ModelGatewaySettings,
}

async fn api_model_gateway_save_settings(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ModelGatewaySettingsBody>,
) -> Response {
    match model_gateway::write_settings(&state.paths, body.settings) {
        Ok(settings) => json_response(StatusCode::OK, &json!({ "settings": settings })),
        Err(error) => json_cli_error(&error),
    }
}

async fn api_model_gateway_local_clis(State(state): State<Arc<AppState>>) -> Response {
    let paths = state.paths.clone();
    match tokio::task::spawn_blocking(move || model_gateway::scan_local_clis(&paths)).await {
        Ok(Ok(payload)) => json_response(StatusCode::OK, &payload),
        Ok(Err(error)) => json_cli_error(&error),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn api_model_gateway_scan_local_clis(
    State(state): State<Arc<AppState>>,
    Json(body): Json<model_gateway::LocalCliScanRequest>,
) -> Response {
    let paths = state.paths.clone();
    match tokio::task::spawn_blocking(move || {
        model_gateway::scan_local_clis_with_overrides(&paths, body.executable_paths)
    })
    .await
    {
        Ok(Ok(payload)) => json_response(StatusCode::OK, &payload),
        Ok(Err(error)) => json_cli_error(&error),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn api_model_gateway_test_local_cli(
    State(state): State<Arc<AppState>>,
    Json(body): Json<model_gateway::ConnectionTestRequest>,
) -> Response {
    let paths = state.paths.clone();
    match tokio::task::spawn_blocking(move || model_gateway::test_local_cli(&paths, body)).await {
        Ok(Ok(payload)) => json_response(StatusCode::OK, &payload),
        Ok(Err(error)) => json_cli_error(&error),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
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
#[serde(deny_unknown_fields)]
struct AgentTargetRegistrationBody {
    name: String,
    host: Option<String>,
    transport: String,
    capabilities: Vec<String>,
    priority: Option<i64>,
    protocol_version: Option<i64>,
    max_concurrency: Option<i64>,
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
            capabilities: body.capabilities,
            priority: body.priority.unwrap_or(0),
            protocol_version: body
                .protocol_version
                .unwrap_or(crate::content::EXECUTION_PROTOCOL_VERSION),
            max_concurrency: body.max_concurrency.unwrap_or(1),
            model_gateway_connection_id: None,
        },
    ) {
        Ok(payload) => json_response(StatusCode::CREATED, &payload),
        Err(error) => json_cli_error(&error),
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
    let broker_url = request_broker_url(&state, &headers);
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
        Ok(mut payload) => {
            if payload.get("executionToken").is_some_and(Value::is_string) {
                payload["taskBrokerUrl"] = json!(broker_url);
            }
            json_response(StatusCode::OK, &payload)
        }
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
        Ok(mut payload) => {
            if payload.get("executionToken").is_some_and(Value::is_string) {
                payload["taskBrokerUrl"] = json!(request_broker_url(&state, &headers));
            }
            json_response(StatusCode::OK, &payload)
        }
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

fn request_broker_url(state: &AppState, headers: &HeaderMap) -> String {
    if matches!(state.host.as_str(), "127.0.0.1" | "localhost" | "::1") {
        return format!("http://127.0.0.1:{}", state.port);
    }
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.contains(['\r', '\n']))
        .unwrap_or("127.0.0.1");
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .filter(|value| matches!(*value, "http" | "https"))
        .unwrap_or("http");
    format!("{scheme}://{host}")
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskLeaseBody {
    lease_token: String,
    message: Option<String>,
    result: Option<Value>,
    retry_after: Option<String>,
    failure_class: Option<String>,
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
    let failure_class =
        match body.failure_class.as_deref() {
            Some(value) => match tasks::TaskFailureClass::parse(value) {
                Some(value) => value,
                None => return json_error(
                    StatusCode::BAD_REQUEST,
                    "failureClass must be retryable_channel, retryable_output, or terminal_task.",
                ),
            },
            None => tasks::TaskFailureClass::RetryableChannel,
        };
    match tasks::fail_task_with_class(
        &state.paths,
        &task_id,
        &body.lease_token,
        body.message.as_deref().unwrap_or("Task failed."),
        body.retry_after.as_deref(),
        failure_class,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskDispatchBody {
    mode: String,
    model_gateway_connection_id: Option<String>,
}

async fn api_task_dispatch(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    Json(body): Json<TaskDispatchBody>,
) -> Response {
    match tasks::set_task_dispatch(
        &state.paths,
        &task_id,
        &body.mode,
        body.model_gateway_connection_id.as_deref(),
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_cli_error(&error),
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

async fn api_task_archive(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Response {
    match tasks::archive_task(&state.paths, &task_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_task_events(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Response {
    match tasks::list_task_events(&state.paths, &task_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_task_attempts(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Response {
    match tasks::list_task_attempts(&state.paths, &task_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_workflows(State(state): State<Arc<AppState>>) -> Response {
    match tasks::list_workflows(&state.paths) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_workflow(
    State(state): State<Arc<AppState>>,
    Path(workflow_id): Path<String>,
) -> Response {
    match tasks::read_workflow(&state.paths, &workflow_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentRouteBody {
    capability: String,
    target_ids: Vec<String>,
}

async fn api_agent_routes(State(state): State<Arc<AppState>>) -> Response {
    match tasks::list_agent_routes(&state.paths) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_set_agent_route(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AgentRouteBody>,
) -> Response {
    match tasks::set_agent_route(&state.paths, &body.capability, &body.target_ids) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_remove_agent_route(
    State(state): State<Arc<AppState>>,
    Path(capability): Path<String>,
) -> Response {
    match tasks::remove_agent_route(&state.paths, &capability) {
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
