fn read_cover_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let payload = crate::tasks::inspect_task(paths, task_id)?;
    let task = &payload["task"];
    if task.get("queue").and_then(Value::as_str) != Some("typesetting")
        || task.get("type").and_then(Value::as_str) != Some(COVER_TASK_TYPE)
    {
        return Err(CliError::with_code(
            "task_kind_mismatch",
            format!("Unsupported Typesetting Cover Task: {task_id}"),
        ));
    }
    Ok(payload)
}

pub fn claim_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let mut payload = read_cover_task(paths, task_id)?;
    let attempt = payload["task"]["attempt"].as_i64().unwrap_or(0) + 1;
    payload["task"]["status"] = json!("running");
    payload["task"]["attempt"] = json!(attempt);
    Ok(payload)
}

pub fn heartbeat_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    read_cover_task(paths, task_id)
}

pub(crate) fn prepare_task_completion(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    result: Option<Value>,
) -> Result<Option<(String, Value)>, CliError> {
    let payload = read_cover_task(paths, task_id)?;
    let task = &payload["task"];
    let result = result.ok_or_else(|| {
        CliError::with_code("invalid_output", "Cover Task completed without a result.")
    })?;
    let artifact = result
        .pointer("/runtimeFinalization/artifacts/0")
        .ok_or_else(|| {
            CliError::with_code("invalid_output", "Cover Task has no finalized image artifact.")
        })?;
    let asset_ref = artifact
        .get("assetRef")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliError::with_code("invalid_output", "Cover asset reference is missing."))?;
    let file_name = artifact
        .get("fileName")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliError::with_code("invalid_output", "Cover file name is missing."))?;
    let project_id = task
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let panel_id = task
        .get("panelId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let publication_id = task
        .pointer("/input/publicationId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let skill_id = task
        .pointer("/input/coverSkillId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let storage = Storage::open(paths)?;
    let mut state = storage
        .read_panel_state(project_id, panel_id)?
        .ok_or_else(|| CliError::with_code("target_not_found", "Typesetting state not found."))?;
    let publications = state
        .get_mut("publications")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| CliError::with_code("invalid_target", "Typesetting publications are invalid."))?;
    let publication = publications
        .iter_mut()
        .find(|publication| publication.get("id").and_then(Value::as_str) == Some(publication_id))
        .ok_or_else(|| {
            CliError::with_code(
                "typesetting_publication_not_found",
                format!("Typesetting publication not found: {publication_id}"),
            )
        })?;
    let covers = publication
        .get_mut("covers")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| CliError::with_code("invalid_target", "Typesetting covers are invalid."))?;
    let exists = covers.iter().any(|cover| {
        cover.pointer("/source/taskId").and_then(Value::as_str) == Some(task_id)
    });
    if !exists {
        covers.push(json!({
            "assetRef": asset_ref,
            "fileName": file_name,
            "mimeType": artifact.get("mimeType").and_then(Value::as_str).unwrap_or("image/png"),
            "src": format!("/api/projects/{project_id}/panels/{panel_id}/assets/{file_name}"),
            "width": artifact.get("width").cloned().unwrap_or(Value::Null),
            "height": artifact.get("height").cloned().unwrap_or(Value::Null),
            "source": {
                "kind": "generated",
                "taskId": task_id,
                "skillId": skill_id,
            },
        }));
        publication["updatedAt"] = json!(crate::control::now_iso());
    }
    Ok(Some((panel_id.to_owned(), state)))
}

pub fn fail_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    _message: &str,
) -> Result<Value, CliError> {
    read_cover_task(paths, task_id)
}

pub fn release_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    read_cover_task(paths, task_id)
}

pub fn retry_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    read_cover_task(paths, task_id)
}

pub fn cancel_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    read_cover_task(paths, task_id)
}
