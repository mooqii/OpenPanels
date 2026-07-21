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
    match tokio::task::spawn_blocking(move || {
        model_gateway::cached_local_clis(&paths)?
            .map(Ok)
            .unwrap_or_else(|| model_gateway::scan_local_clis(&paths))
    })
    .await
    {
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WikiUpdateGroupDispatchBody {
    mutation_key: String,
    mode: String,
    model_gateway_connection_id: Option<String>,
}

async fn api_wiki_update_group_dispatch(
    State(state): State<Arc<AppState>>,
    Json(body): Json<WikiUpdateGroupDispatchBody>,
) -> Response {
    match tasks::set_wiki_update_group_dispatch(
        &state.paths,
        &body.mutation_key,
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

async fn api_task_delete(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Response {
    match tasks::delete_task(&state.paths, &task_id) {
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

async fn api_workflow_runs(State(state): State<Arc<AppState>>) -> Response {
    match tasks::list_workflow_runs(&state.paths) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_workflow_run(
    State(state): State<Arc<AppState>>,
    Path(workflow_run_id): Path<String>,
) -> Response {
    match tasks::read_workflow_run(&state.paths, &workflow_run_id) {
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
