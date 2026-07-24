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

async fn api_task_delete(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Response {
    match tasks::delete_task(&state.paths, &task_id) {
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
