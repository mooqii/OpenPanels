async fn api_agent_skills(State(state): State<Arc<AppState>>) -> Response {
    match agent_resources::list_agent_skills(&state.paths) {
        Ok(skills) => json_response(StatusCode::OK, &json!({ "skills": skills })),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WritingSelectionBody {
    is_wiki_selected: bool,
    #[serde(default)]
    selected_my_document_ids: Vec<String>,
}

async fn api_writing_selection(State(state): State<Arc<AppState>>) -> Response {
    match crate::writing::read_selection(&state.paths) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_writing_set_selection(
    State(state): State<Arc<AppState>>,
    Json(body): Json<WritingSelectionBody>,
) -> Response {
    match crate::writing::write_selection(
        &state.paths,
        body.is_wiki_selected,
        &body.selected_my_document_ids,
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WritingDraftBody {
    create_draft: Option<String>,
    draft: String,
    mode: String,
    #[serde(default)]
    distillation_name: String,
    target_my_document_id: Option<String>,
    #[serde(default)]
    selected_create_writing_skill_ids: Vec<String>,
    selected_revision_writing_skill_id: Option<String>,
    selected_distillation_skill_id: Option<String>,
    revision_draft: Option<String>,
}

async fn api_writing_save_draft(
    State(state): State<Arc<AppState>>,
    Json(body): Json<WritingDraftBody>,
) -> Response {
    match crate::writing::save_draft_with_distillation_skill(
        &state.paths,
        &body.draft,
        &body.mode,
        &body.distillation_name,
        body.target_my_document_id.as_deref(),
        &body.selected_create_writing_skill_ids,
        body.selected_revision_writing_skill_id.as_deref(),
        body.selected_distillation_skill_id.as_deref(),
        body.create_draft.as_deref(),
        body.revision_draft.as_deref(),
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WritingRequestBody {
    instruction: String,
    mode: String,
    target_my_document_id: Option<String>,
    writing_skill_ids: Vec<String>,
}

async fn api_writing_skills(State(state): State<Arc<AppState>>) -> Response {
    match (
        crate::agent::list_writing_agent_skills(&state.paths),
        crate::agent::list_writing_distillation_agent_skills(&state.paths),
    ) {
        (Ok(skills), Ok(distillation_skills)) => json_response(
            StatusCode::OK,
            &json!({ "skills": skills, "distillationSkills": distillation_skills }),
        ),
        (Err(error), _) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
        (_, Err(error)) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_writing_skill_files(
    State(state): State<Arc<AppState>>,
    Path(skill_id): Path<String>,
) -> Response {
    match crate::writing::read_skill_files(&state.paths, &skill_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WritingSkillFileBody {
    path: String,
    content: String,
}

async fn api_write_writing_skill_file(
    State(state): State<Arc<AppState>>,
    Path(skill_id): Path<String>,
    Json(body): Json<WritingSkillFileBody>,
) -> Response {
    match crate::writing::write_custom_skill_file(
        &state.paths,
        &skill_id,
        &body.path,
        &body.content,
    ) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_delete_writing_skill(
    State(state): State<Arc<AppState>>,
    Path(skill_id): Path<String>,
) -> Response {
    match crate::writing::delete_custom_skill(&state.paths, &skill_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_writing_create_request(
    State(state): State<Arc<AppState>>,
    Json(body): Json<WritingRequestBody>,
) -> Response {
    match crate::writing::create_requests(
        &state.paths,
        &body.instruction,
        &body.mode,
        body.target_my_document_id.as_deref(),
        &body.writing_skill_ids,
    ) {
        Ok(payload) => json_response(StatusCode::CREATED, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WritingDistillationRequestBody {
    name: String,
    distiller_skill_id: Option<String>,
}

async fn api_writing_create_distillation_request(
    State(state): State<Arc<AppState>>,
    Json(body): Json<WritingDistillationRequestBody>,
) -> Response {
    match crate::writing::create_distillation_request_with_skill(
        &state.paths,
        &body.name,
        body.distiller_skill_id.as_deref(),
    ) {
        Ok(payload) => json_response(StatusCode::CREATED, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_next_task(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TasksQuery>,
) -> Response {
    match tasks::next_task(&state.paths, task_list_filter(&query)) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

fn task_list_filter(query: &TasksQuery) -> tasks::TaskListFilter<'_> {
    tasks::TaskListFilter {
        pending: query.pending.unwrap_or(false),
        queue: query.queue.as_deref(),
        status: query.status.as_deref(),
    }
}

async fn api_inspect_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Response {
    match tasks::inspect_task(&state.paths, &task_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::NOT_FOUND, error.message()),
    }
}
