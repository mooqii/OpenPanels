#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishingPreferencesBody {
    selected_publication_id: Option<String>,
    skill_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishingReleaseBody {
    publication_id: String,
    skill_id: String,
    request_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishingAttemptBody {
    acknowledged_unknown: Option<bool>,
    mode: String,
    request_id: String,
    skill_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct WechatConfigurationBody {
    app_id: String,
    app_secret: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct WechatDraftSubmissionBody {
    publication_id: String,
    request_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicationsBody {
    base_revision: i64,
    publications: Vec<Value>,
}

async fn api_publications(State(state): State<Arc<AppState>>) -> Response {
    let result =
        ensure_project_bootstrap(&state.paths, BootstrapRequest::new()).and_then(|bootstrap| {
        let storage = Storage::open(&state.paths)?;
        Ok(json!({
            "projectId": bootstrap.project.id,
            "publications": storage.list_publications(&bootstrap.project.id)?,
            "revision": storage.publication_revision(&bootstrap.project.id)?,
        }))
        });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_cli_error(&error),
    }
}

async fn api_update_publications(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PublicationsBody>,
) -> Response {
    let result =
        ensure_project_bootstrap(&state.paths, BootstrapRequest::new()).and_then(|bootstrap| {
            let storage = Storage::open(&state.paths)?;
            storage
                .write_publications_if_current(
                    &bootstrap.project.id,
                    &body.publications,
                    body.base_revision,
                )
                .map(|result| (bootstrap.project.id, result))
        });
    match result {
        Ok((project_id, Ok(revision))) => json_response(
            StatusCode::OK,
            &json!({ "projectId": project_id, "revision": revision }),
        ),
        Ok((_, Err(conflict))) => json_response(
            StatusCode::CONFLICT,
            &json!({
                "code": "revision_conflict",
                "baseRevision": conflict.base_revision,
                "currentRevision": conflict.current_revision,
            }),
        ),
        Err(error) => json_cli_error(&error),
    }
}

async fn api_releases(State(state): State<Arc<AppState>>) -> Response {
    let result =
        ensure_project_bootstrap(&state.paths, BootstrapRequest::new()).and_then(|bootstrap| {
        let storage = Storage::open(&state.paths)?;
        Ok(json!({
            "projectId": bootstrap.project.id,
            "releases": storage.list_releases(&bootstrap.project.id)?,
        }))
        });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_cli_error(&error),
    }
}

async fn api_wechat_configuration(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    if !trusted_studio_origin(&headers) {
        return json_error(StatusCode::FORBIDDEN, "Untrusted Studio origin.");
    }
    let paths = state.paths.clone();
    match tokio::task::spawn_blocking(move || crate::release::wechat_configuration_status(&paths))
        .await
    {
        Ok(Ok(payload)) => no_store_response(json_response(StatusCode::OK, &payload)),
        Ok(Err(error)) => no_store_response(json_cli_error(&error)),
        Err(error) => no_store_response(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &error.to_string(),
        )),
    }
}

async fn api_save_wechat_configuration(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<WechatConfigurationBody>,
) -> Response {
    if !trusted_studio_origin(&headers) {
        return json_error(StatusCode::FORBIDDEN, "Untrusted Studio origin.");
    }
    let paths = state.paths.clone();
    match tokio::task::spawn_blocking(move || {
        crate::release::validate_and_save_wechat_configuration(
            &paths,
            &body.app_id,
            body.app_secret.as_deref(),
        )
    })
    .await
    {
        Ok(Ok(payload)) => no_store_response(json_response(StatusCode::OK, &payload)),
        Ok(Err(error)) => no_store_response(json_cli_error(&error)),
        Err(error) => no_store_response(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &error.to_string(),
        )),
    }
}

async fn api_submit_wechat_draft(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<WechatDraftSubmissionBody>,
) -> Response {
    if !trusted_studio_origin(&headers) {
        return json_error(StatusCode::FORBIDDEN, "Untrusted Studio origin.");
    }
    let paths = state.paths.clone();
    match tokio::task::spawn_blocking(move || {
        crate::release::submit_wechat_draft_direct(
            &paths,
            &body.publication_id,
            &body.request_id,
        )
    })
    .await
    {
        Ok(Ok(payload)) => no_store_response(json_response(StatusCode::CREATED, &payload)),
        Ok(Err(error)) => no_store_response(json_cli_error(&error)),
        Err(error) => no_store_response(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &error.to_string(),
        )),
    }
}

fn trusted_studio_origin(headers: &HeaderMap) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let expected = format!("http://{host}");
    if let Some(origin) = headers.get(header::ORIGIN).and_then(|value| value.to_str().ok()) {
        return origin == expected;
    }
    headers
        .get(header::REFERER)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|referer| {
            referer == expected || referer.starts_with(&format!("{expected}/"))
        })
}

async fn api_publishing_preferences(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PublishingPreferencesBody>,
) -> Response {
    match crate::release::update_preferences(
        &state.paths,
        body.selected_publication_id.as_deref(),
        &body.skill_id,
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_cli_error(&error),
    }
}

async fn api_publishing_create_release(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PublishingReleaseBody>,
) -> Response {
    match crate::release::create_release(
        &state.paths,
        &body.publication_id,
        &body.skill_id,
        &body.request_id,
    ) {
        Ok(payload) => json_response(StatusCode::CREATED, &payload),
        Err(error) => json_cli_error(&error),
    }
}

async fn api_publishing_create_attempt(
    State(state): State<Arc<AppState>>,
    Path(release_id): Path<String>,
    Json(body): Json<PublishingAttemptBody>,
) -> Response {
    match crate::release::create_attempt(
        &state.paths,
        &release_id,
        &body.skill_id,
        &body.request_id,
        &body.mode,
        body.acknowledged_unknown.unwrap_or(false),
    ) {
        Ok(payload) => json_response(StatusCode::CREATED, &payload),
        Err(error) => json_cli_error(&error),
    }
}
