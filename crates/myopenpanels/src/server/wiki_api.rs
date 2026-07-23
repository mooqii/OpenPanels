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
    selected_my_document_ids: Option<Vec<String>>,
}

pub(super) async fn api_wiki_set_selection(
    State(state): State<Arc<AppState>>,
    Json(body): Json<WikiSelectionBody>,
) -> Response {
    match wiki::write_agent_selection(
        &state.paths,
        body.selected_my_document_ids
            .as_deref()
            .unwrap_or(&[]),
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

fn my_document_error(error: CliError) -> Response {
    let status = match error.code() {
        Some("not_found") => StatusCode::NOT_FOUND,
        Some(
            "my_document_write_in_progress" | "my_document_write_not_failed" | "my_document_write_retry_unavailable",
        ) => StatusCode::CONFLICT,
        Some("invalid_my_document" | "invalid_raw_document" | "already_published") => {
            StatusCode::BAD_REQUEST
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    json_error(status, error.message())
}

pub(super) async fn api_my_documents(State(state): State<Arc<AppState>>) -> Response {
    match crate::my_document::list_my_documents(&state.paths) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => my_document_error(error),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MyDocumentBody {
    content: Option<String>,
    data_url: Option<String>,
    file_name: Option<String>,
    mime_type: Option<String>,
    task_id: Option<String>,
    thread_id: Option<String>,
    title: Option<String>,
}

pub(super) async fn api_import_my_document(
    State(state): State<Arc<AppState>>,
    Json(body): Json<MyDocumentBody>,
) -> Response {
    let file_name = body.file_name.as_deref().unwrap_or("document.md");
    let result = if let Some(data_url) = body.data_url.as_deref() {
        match data_url_to_buffer(data_url) {
            Ok(data) => crate::my_document::import_my_document(
                &state.paths,
                file_name,
                body.title.as_deref(),
                body.mime_type.as_deref().or(Some(data.mime_type.as_str())),
                &data.bytes,
            ),
            Err(error) => return json_error(StatusCode::BAD_REQUEST, error.message()),
        }
    } else {
        crate::my_document::create_my_document(
            &state.paths,
            file_name,
            body.title.as_deref(),
            body.mime_type.as_deref(),
            body.task_id.as_deref(),
            body.thread_id.as_deref(),
            body.content.unwrap_or_default().as_bytes(),
        )
    };
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => my_document_error(error),
    }
}

pub(super) async fn api_read_my_document(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
) -> Response {
    match crate::my_document::read_my_document(&state.paths, &document_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => my_document_error(error),
    }
}

pub(super) async fn api_my_document_original(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
) -> Response {
    match crate::my_document::my_document_import_original(&state.paths, &document_id) {
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
                .pointer("/importSource/fileName")
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
        Err(error) => my_document_error(error),
    }
}

pub(super) async fn api_reveal_my_document_original(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
) -> Response {
    match crate::my_document::reveal_my_document_import_original(&state.paths, &document_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => my_document_error(error),
    }
}

pub(super) async fn api_update_my_document(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
    Json(body): Json<MyDocumentBody>,
) -> Response {
    let result = if let Some(content) = body.content {
        let existing_file_name = crate::my_document::read_my_document(&state.paths, &document_id)
            .ok()
            .and_then(|payload| {
                payload["document"]["originalFileName"]
                    .as_str()
                    .map(str::to_owned)
            });
        crate::my_document::write_my_document(
            &state.paths,
            &document_id,
            body.file_name
                .as_deref()
                .or(existing_file_name.as_deref())
                .unwrap_or("document.md"),
            body.mime_type.as_deref(),
            content.as_bytes(),
        )
    } else if let Some(file_name) = body.file_name {
        crate::my_document::rename_my_document_file(&state.paths, &document_id, &file_name)
    } else if let Some(title) = body.title {
        crate::my_document::rename_my_document(&state.paths, &document_id, &title)
    } else {
        Err(CliError::with_code(
            "invalid_my_document",
            "My Document update requires content or title.",
        ))
    };
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => my_document_error(error),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UpdateRawDocumentBody {
    file_name: Option<String>,
    title: Option<String>,
}

pub(super) async fn api_wiki_rename_raw_document(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
    Json(body): Json<UpdateRawDocumentBody>,
) -> Response {
    let result = if let Some(title) = body.title {
        wiki::rename_raw_document_title(&state.paths, &document_id, &title)
    } else if let Some(file_name) = body.file_name {
        wiki::rename_raw_document(&state.paths, &document_id, &file_name)
    } else {
        Err(CliError::with_code(
            "invalid_raw_document",
            "Raw document update requires a title or file name.",
        ))
    };
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => my_document_error(error),
    }
}

pub(super) async fn api_delete_my_document(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
) -> Response {
    match crate::my_document::delete_my_document(&state.paths, &document_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => my_document_error(error),
    }
}

pub(super) async fn api_publish_my_document(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
    Json(body): Json<WikiSpaceQuery>,
) -> Response {
    match crate::my_document::publish_my_document(
        &state.paths,
        &document_id,
        body.wiki_space_id.as_deref(),
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => my_document_error(error),
    }
}

pub(super) async fn api_retry_my_document(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
) -> Response {
    match crate::operations::retry_my_document(&state.paths, &document_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => my_document_error(error),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MyDocumentOperationBody {
    title: String,
    document_format: Option<String>,
}

pub(super) async fn api_create_my_document(
    State(state): State<Arc<AppState>>,
    Json(body): Json<MyDocumentOperationBody>,
) -> Response {
    match crate::operations::begin_my_document(
        &state.paths,
        &body.title,
        body.document_format.as_deref().unwrap_or("markdown"),
        None,
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => my_document_error(error),
    }
}

pub(super) async fn api_revise_my_document(
    State(state): State<Arc<AppState>>,
    Path(document_id): Path<String>,
    Json(body): Json<MyDocumentOperationBody>,
) -> Response {
    match crate::operations::begin_my_document(
        &state.paths,
        &body.title,
        body.document_format.as_deref().unwrap_or("markdown"),
        Some(&document_id),
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => my_document_error(error),
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
    #[serde(default)]
    rebuild_confirmed: bool,
}

pub(super) async fn api_wiki_set_agent_skill(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AgentSkillBody>,
) -> Response {
    let Some(agent_skill_id) = body.agent_skill_id else {
        return json_error(StatusCode::BAD_REQUEST, "Missing agentSkillId");
    };
    match wiki::set_agent_skill(&state.paths, &agent_skill_id, body.rebuild_confirmed) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::BAD_REQUEST, error.message()),
    }
}

pub(super) async fn api_wiki_spaces(State(state): State<Arc<AppState>>) -> Response {
    match wiki::list_spaces(&state.paths) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

pub(super) async fn api_wiki_maintain_space(
    State(state): State<Arc<AppState>>,
    Path(wiki_space_id): Path<String>,
) -> Response {
    match wiki::maintain_wiki_space(&state.paths, Some(&wiki_space_id)) {
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

pub(super) async fn api_wiki_rename_page(
    State(state): State<Arc<AppState>>,
    Path((wiki_space_id, page_path)): Path<(String, String)>,
    Json(body): Json<WritePageBody>,
) -> Response {
    let Some(next_page_path) = body.page_path.as_deref() else {
        return json_error(StatusCode::BAD_REQUEST, "Missing pagePath");
    };
    match wiki::rename_page(&state.paths, &wiki_space_id, &page_path, next_page_path) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => my_document_error(error),
    }
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
