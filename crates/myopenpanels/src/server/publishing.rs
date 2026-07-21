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

async fn api_publishing_preferences(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PublishingPreferencesBody>,
) -> Response {
    match crate::publishing::update_preferences(
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
    match crate::publishing::create_release(
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
    match crate::publishing::create_attempt(
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
