use super::*;

pub(super) async fn api_wiki_context(State(state): State<Arc<AppState>>) -> Response {
    match wiki::wiki_context(&state.paths) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

pub(super) async fn api_wiki_selection(State(state): State<Arc<AppState>>) -> Response {
    match wiki::read_agent_selection(&state.paths) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WikiSelectionBody {
    is_wiki_selected: Option<bool>,
    selected_raw_document_ids: Option<Vec<String>>,
    selected_generated_document_ids: Option<Vec<String>>,
}

pub(super) async fn api_wiki_set_selection(
    State(state): State<Arc<AppState>>,
    Json(body): Json<WikiSelectionBody>,
) -> Response {
    match wiki::write_agent_selection(
        &state.paths,
        body.is_wiki_selected.unwrap_or(false),
        body.selected_raw_document_ids.as_deref().unwrap_or(&[]),
        body.selected_generated_document_ids
            .as_deref()
            .unwrap_or(&[]),
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

fn wiki_document_error(error: CliError) -> Response {
    let status = match error.code() {
        Some("not_found") => StatusCode::NOT_FOUND,
        Some("invalid_generated_document" | "already_published") => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    json_error(status, error.message())
}

pub(super) async fn api_wiki_generated_documents(State(state): State<Arc<AppState>>) -> Response {
    match wiki::list_generated_documents(&state.paths) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => wiki_document_error(error),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeneratedDocumentBody {
    content: Option<String>,
    file_name: Option<String>,
    mime_type: Option<String>,
    task_id: Option<String>,
    thread_id: Option<String>,
    title: Option<String>,
}

pub(super) async fn api_wiki_create_generated_document(
    State(state): State<Arc<AppState>>,
    Json(body): Json<GeneratedDocumentBody>,
) -> Response {
    let file_name = body.file_name.as_deref().unwrap_or("document.md");
    match wiki::create_generated_document(
        &state.paths,
        file_name,
        body.title.as_deref(),
        body.mime_type.as_deref(),
        body.task_id.as_deref(),
        body.thread_id.as_deref(),
        body.content.unwrap_or_default().as_bytes(),
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => wiki_document_error(error),
    }
}

pub(super) async fn api_wiki_read_generated_document(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
) -> Response {
    match wiki::read_generated_document(&state.paths, &document_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => wiki_document_error(error),
    }
}

pub(super) async fn api_wiki_update_generated_document(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
    Json(body): Json<GeneratedDocumentBody>,
) -> Response {
    let result = if let Some(content) = body.content {
        let existing_file_name = wiki::read_generated_document(&state.paths, &document_id)
            .ok()
            .and_then(|payload| {
                payload["document"]["originalFileName"]
                    .as_str()
                    .map(str::to_owned)
            });
        wiki::write_generated_document(
            &state.paths,
            &document_id,
            body.file_name
                .as_deref()
                .or(existing_file_name.as_deref())
                .unwrap_or("document.md"),
            body.mime_type.as_deref(),
            content.as_bytes(),
        )
    } else if let Some(title) = body.title {
        wiki::rename_generated_document(&state.paths, &document_id, &title)
    } else {
        Err(CliError::with_code(
            "invalid_generated_document",
            "Generated document update requires content or title.",
        ))
    };
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => wiki_document_error(error),
    }
}

pub(super) async fn api_wiki_delete_generated_document(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
) -> Response {
    match wiki::delete_generated_document(&state.paths, &document_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => wiki_document_error(error),
    }
}

pub(super) async fn api_wiki_publish_generated_document(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
    Json(body): Json<WikiSpaceQuery>,
) -> Response {
    match wiki::publish_generated_document(
        &state.paths,
        &document_id,
        body.wiki_space_id.as_deref(),
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => wiki_document_error(error),
    }
}

pub(super) async fn api_wiki_raw_documents(State(state): State<Arc<AppState>>) -> Response {
    match wiki::wiki_context(&state.paths) {
        Ok(payload) => json_response(
            StatusCode::OK,
            &json!({
                "documents": payload["state"]["rawDocuments"],
                "state": payload["state"],
            }),
        ),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AddRawDocumentBody {
    content: Option<String>,
    data_url: Option<String>,
    file_name: Option<String>,
    mime_type: Option<String>,
    source: Option<String>,
    title: Option<String>,
    wiki_space_id: Option<String>,
}

pub(super) async fn api_wiki_add_raw_document(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AddRawDocumentBody>,
) -> Response {
    let data = match body.data_url.as_deref() {
        Some(data_url) => match data_url_to_buffer(data_url) {
            Ok(data) => data,
            Err(error) => return json_error(StatusCode::BAD_REQUEST, error.message()),
        },
        None => DataUrlBytes {
            bytes: body.content.unwrap_or_default().into_bytes(),
            mime_type: body
                .mime_type
                .clone()
                .unwrap_or_else(|| "text/plain".to_owned()),
        },
    };
    let result = wiki::add_raw_document(
        &state.paths,
        body.file_name.as_deref().unwrap_or("document.md"),
        body.title.as_deref(),
        body.mime_type.as_deref().or(Some(data.mime_type.as_str())),
        body.source.as_deref().unwrap_or("user"),
        body.wiki_space_id.as_deref(),
        &data.bytes,
    );
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

pub(super) async fn api_wiki_read_markdown(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
) -> Response {
    match wiki::read_markdown(&state.paths, &document_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WriteMarkdownBody {
    content: Option<String>,
    task_id: Option<String>,
}

pub(super) async fn api_wiki_write_markdown(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
    Json(body): Json<WriteMarkdownBody>,
) -> Response {
    match wiki::write_markdown(
        &state.paths,
        &document_id,
        body.content.as_deref().unwrap_or(""),
        body.task_id.as_deref(),
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

pub(super) async fn api_wiki_raw_document_original(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
) -> Response {
    match wiki::raw_document_original(&state.paths, &document_id) {
        Ok(original) => {
            let bytes = match fs::read(&original.file_path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    return json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string())
                }
            };
            let mut response = bytes_response(StatusCode::OK, bytes, &original.mime_type);
            response.headers_mut().insert(
                header::CONTENT_LENGTH,
                HeaderValue::from_str(&original.size_bytes.to_string())
                    .unwrap_or_else(|_| HeaderValue::from_static("0")),
            );
            if let Some(file_name) = original
                .document
                .get("originalFileName")
                .and_then(Value::as_str)
            {
                if let Ok(value) = HeaderValue::from_str(&content_disposition_inline(file_name)) {
                    response
                        .headers_mut()
                        .insert(header::CONTENT_DISPOSITION, value);
                }
            }
            response
        }
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

pub(super) async fn api_wiki_reveal_raw_document(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
) -> Response {
    match wiki::reveal_raw_document_original(&state.paths, &document_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WikiSpaceQuery {
    wiki_space_id: Option<String>,
}

pub(super) async fn api_wiki_extract_raw_document(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
    Query(query): Query<WikiSpaceQuery>,
) -> Response {
    match wiki::extract_raw_document_markdown(
        &state.paths,
        &document_id,
        query.wiki_space_id.as_deref(),
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

pub(super) async fn api_wiki_reindex_raw_document(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
    Query(query): Query<WikiSpaceQuery>,
) -> Response {
    match wiki::reindex_raw_document(&state.paths, &document_id, query.wiki_space_id.as_deref()) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

pub(super) async fn api_wiki_delete_raw_document(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
    Query(query): Query<WikiSpaceQuery>,
) -> Response {
    match wiki::delete_raw_document(&state.paths, &document_id, query.wiki_space_id.as_deref()) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct TasksQuery {
    status: Option<String>,
}

pub(super) async fn api_wiki_tasks(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TasksQuery>,
) -> Response {
    match wiki::list_tasks(&state.paths, query.status.as_deref()) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

pub(super) async fn api_wiki_next_task(State(state): State<Arc<AppState>>) -> Response {
    match wiki::next_task(&state.paths) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ClaimTaskBody {
    agent_host: Option<String>,
    thread_id: Option<String>,
}

pub(super) async fn api_wiki_claim_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    Json(body): Json<ClaimTaskBody>,
) -> Response {
    match wiki::claim_task(
        &state.paths,
        &task_id,
        body.agent_host.as_deref(),
        body.thread_id.as_deref(),
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

pub(super) async fn api_wiki_complete_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    match wiki::complete_task(&state.paths, &task_id, body.get("result").cloned()) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

pub(super) async fn api_wiki_fail_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    match wiki::fail_task(
        &state.paths,
        &task_id,
        body.get("error")
            .and_then(Value::as_str)
            .unwrap_or("Wiki task failed"),
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

pub(super) async fn api_wiki_agent_targets(State(state): State<Arc<AppState>>) -> Response {
    match wiki::list_agent_targets(&state.paths) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AgentTargetBody {
    host: Option<String>,
    thread_id: Option<String>,
    wake_url: Option<String>,
}

pub(super) async fn api_wiki_register_agent_target(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AgentTargetBody>,
) -> Response {
    match wiki::register_agent_target(
        &state.paths,
        body.host.as_deref().unwrap_or("unknown"),
        body.thread_id.as_deref().unwrap_or(&state.paths.context_id),
        body.wake_url.as_deref(),
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

pub(super) async fn api_wiki_active_space(State(state): State<Arc<AppState>>) -> Response {
    match wiki::wiki_context(&state.paths) {
        Ok(payload) => {
            let active_id = payload["state"]["activeWikiSpaceId"].clone();
            let active_space = payload["state"]["wikiSpaces"]
                .as_array()
                .and_then(|spaces| spaces.iter().find(|space| space["id"] == active_id))
                .cloned();
            json_response(
                StatusCode::OK,
                &json!({ "wikiSpaceId": active_id, "wikiSpace": active_space }),
            )
        }
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ActiveSpaceBody {
    wiki_space_id: Option<String>,
}

pub(super) async fn api_wiki_set_active_space(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ActiveSpaceBody>,
) -> Response {
    let Some(wiki_space_id) = body.wiki_space_id else {
        return json_error(StatusCode::BAD_REQUEST, "Missing wikiSpaceId");
    };
    match wiki::set_active_space(&state.paths, &wiki_space_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

pub(super) async fn api_wiki_agent_skill(State(state): State<Arc<AppState>>) -> Response {
    match wiki::wiki_context(&state.paths) {
        Ok(payload) => json_response(
            StatusCode::OK,
            &json!({
                "agentSkillId": wiki::selected_agent_skill_id(&payload["state"]),
                "state": payload["state"],
            }),
        ),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AgentSkillBody {
    agent_skill_id: Option<String>,
}

pub(super) async fn api_wiki_set_agent_skill(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AgentSkillBody>,
) -> Response {
    let Some(agent_skill_id) = body.agent_skill_id else {
        return json_error(StatusCode::BAD_REQUEST, "Missing agentSkillId");
    };
    match wiki::set_agent_skill(&state.paths, &agent_skill_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

pub(super) async fn api_wiki_spaces(State(state): State<Arc<AppState>>) -> Response {
    match wiki::list_spaces(&state.paths) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

pub(super) async fn api_wiki_reindex_space(
    State(state): State<Arc<AppState>>,
    Path(wiki_space_id): Path<String>,
) -> Response {
    match wiki::reindex_wiki_space(&state.paths, Some(&wiki_space_id)) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

pub(super) async fn api_wiki_pages(
    State(state): State<Arc<AppState>>,
    Path(wiki_space_id): Path<String>,
) -> Response {
    match wiki::list_pages(&state.paths, &wiki_space_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WritePageBody {
    content: Option<String>,
    page_path: Option<String>,
    task_id: Option<String>,
    title: Option<String>,
}

pub(super) async fn api_wiki_write_page_at_collection(
    State(state): State<Arc<AppState>>,
    Path(wiki_space_id): Path<String>,
    Json(body): Json<WritePageBody>,
) -> Response {
    let Some(page_path) = body.page_path.clone() else {
        return json_error(StatusCode::BAD_REQUEST, "Missing pagePath");
    };
    write_page_response(state, &wiki_space_id, &page_path, body)
}

pub(super) async fn api_wiki_read_page(
    State(state): State<Arc<AppState>>,
    Path((wiki_space_id, page_path)): Path<(String, String)>,
) -> Response {
    match wiki::read_page(&state.paths, &wiki_space_id, &page_path) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

pub(super) async fn api_wiki_write_page(
    State(state): State<Arc<AppState>>,
    Path((wiki_space_id, page_path)): Path<(String, String)>,
    Json(body): Json<WritePageBody>,
) -> Response {
    write_page_response(state, &wiki_space_id, &page_path, body)
}

pub(super) fn write_page_response(
    state: Arc<AppState>,
    wiki_space_id: &str,
    page_path: &str,
    body: WritePageBody,
) -> Response {
    match wiki::write_page(
        &state.paths,
        wiki_space_id,
        page_path,
        body.content.as_deref().unwrap_or(""),
        body.title.as_deref(),
        body.task_id.as_deref(),
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}
