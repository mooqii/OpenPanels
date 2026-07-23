#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssetBody {
    data_url: String,
    file_name: Option<String>,
    mime_type: Option<String>,
    origin_panel_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicationAssetsQuery {
    project_id: String,
    scope: Option<String>,
}

async fn api_assets(State(state): State<Arc<AppState>>) -> Response {
    let result =
        ensure_project_bootstrap(&state.paths, BootstrapRequest::new()).and_then(|bootstrap| {
        let storage = Storage::open(&state.paths)?;
        Ok(json!({
            "projectId": bootstrap.project.id,
            "assets": storage.list_assets(&bootstrap.project.id)?,
        }))
        });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_cli_error(&error),
    }
}

async fn api_asset_content(
    State(state): State<Arc<AppState>>,
    Path(asset_id): Path<String>,
) -> Response {
    let result =
        ensure_project_bootstrap(&state.paths, BootstrapRequest::new()).and_then(|bootstrap| {
        Storage::open(&state.paths)?.read_asset_by_id(&bootstrap.project.id, &asset_id)
        });
    match result {
        Ok(bytes) => bytes_response(StatusCode::OK, bytes, "application/octet-stream"),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_publication_canvas_assets(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PublicationAssetsQuery>,
) -> Response {
    match publication::list_canvas_assets(
        &state.paths,
        &query.project_id,
        query.scope.as_deref().unwrap_or("current"),
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_publication_cover_skills(State(state): State<Arc<AppState>>) -> Response {
    match crate::publication::cover_skills(&state.paths) {
        Ok(skills) => json_response(StatusCode::OK, &json!({ "skills": skills })),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_publication_title_skills(State(state): State<Arc<AppState>>) -> Response {
    match crate::publication::title_skills(&state.paths) {
        Ok(skills) => json_response(StatusCode::OK, &json!({ "skills": skills })),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_publication_layout_skills(State(state): State<Arc<AppState>>) -> Response {
    match crate::publication::layout_skills(&state.paths) {
        Ok(skills) => json_response(StatusCode::OK, &json!({ "skills": skills })),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct PublicationCoverRequestBody {
    publication_id: String,
    skill_id: String,
    #[serde(default)]
    instruction: String,
    request_id: String,
}

async fn api_publication_cover_request(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PublicationCoverRequestBody>,
) -> Response {
    match crate::publication::create_cover_request(
        &state.paths,
        &body.publication_id,
        &body.skill_id,
        &body.instruction,
        &body.request_id,
    ) {
        Ok(payload) => json_response(StatusCode::CREATED, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct PublicationTitleRequestBody {
    publication_id: String,
    skill_id: String,
    #[serde(default)]
    instruction: String,
    request_id: String,
}

async fn api_publication_title_request(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PublicationTitleRequestBody>,
) -> Response {
    match crate::publication::create_title_request(
        &state.paths,
        &body.publication_id,
        &body.skill_id,
        &body.instruction,
        &body.request_id,
    ) {
        Ok(payload) => json_response(StatusCode::CREATED, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct PublicationLayoutRequestBody {
    publication_id: String,
    skill_id: String,
    #[serde(default)]
    instruction: String,
    request_id: String,
}

async fn api_publication_layout_request(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PublicationLayoutRequestBody>,
) -> Response {
    match crate::publication::create_layout_request(
        &state.paths,
        &body.publication_id,
        &body.skill_id,
        &body.instruction,
        &body.request_id,
    ) {
        Ok(payload) => json_response(StatusCode::CREATED, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportAssetBody {
    source_asset_ref: String,
    origin_panel_id: String,
}

async fn api_import_asset(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ImportAssetBody>,
) -> Response {
    let result =
        ensure_project_bootstrap(&state.paths, BootstrapRequest::new()).and_then(|bootstrap| {
            publication::import_canvas_asset(
                &state.paths,
                &bootstrap.project.id,
                &body.origin_panel_id,
                &body.source_asset_ref,
            )
        });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_write_asset(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AssetBody>,
) -> Response {
    let result =
        ensure_project_bootstrap(&state.paths, BootstrapRequest::new()).and_then(|bootstrap| {
        let storage = Storage::open(&state.paths)?;
        let project_id = bootstrap.project.id;
        let panel_id = body.origin_panel_id;
        if storage.read_panel(&project_id, &panel_id)?.is_none() {
            return Err(CliError::with_code(
                "target_not_found",
                format!("Origin panel not found: {panel_id}"),
            ));
        }
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
                "resourceId": written.resource_id,
                "fileName": written.file_name,
            },
            "mimeType": body.mime_type.unwrap_or(image.mime_type),
            "resourceId": written.resource_id,
            "src": format!("/api/assets/{}/content", written.resource_id),
        }))
    });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
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
