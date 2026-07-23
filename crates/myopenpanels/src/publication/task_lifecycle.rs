fn read_publication_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let payload = crate::tasks::inspect_task(paths, task_id)?;
    let task = &payload["task"];
    if task.get("queue").and_then(Value::as_str) != Some("publication")
        || !task
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(is_publication_task_type)
    {
        return Err(CliError::with_code(
            "task_kind_mismatch",
            format!("Unsupported Typesetting Task: {task_id}"),
        ));
    }
    Ok(payload)
}

pub fn claim_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let mut payload = read_publication_task(paths, task_id)?;
    let attempt = payload["task"]["attempt"].as_i64().unwrap_or(0) + 1;
    payload["task"]["status"] = json!("running");
    payload["task"]["attempt"] = json!(attempt);
    Ok(payload)
}

pub fn heartbeat_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    read_publication_task(paths, task_id)
}

pub(crate) fn prepare_task_completion(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    result: Option<Value>,
) -> Result<Option<(String, Value)>, CliError> {
    let payload = read_publication_task(paths, task_id)?;
    let task = &payload["task"];
    if task.get("type").and_then(Value::as_str) == Some(TITLE_TASK_TYPE) {
        return prepare_title_completion(paths, task_id, task, result);
    }
    if task.get("type").and_then(Value::as_str) == Some(LAYOUT_TASK_TYPE) {
        return prepare_layout_completion(paths, task_id, task, result);
    }
    prepare_cover_completion(paths, task_id, task, result)
}

fn prepare_title_completion(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    task: &Value,
    result: Option<Value>,
) -> Result<Option<(String, Value)>, CliError> {
    let generated = result
        .as_ref()
        .and_then(|value| value.pointer("/runtimeFinalization/artifacts/0/titles"))
        .and_then(Value::as_array)
        .filter(|titles| !titles.is_empty())
        .ok_or_else(|| {
            CliError::with_code(
                "invalid_output",
                "Title Task has no finalized title candidates.",
            )
        })?;
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
        .pointer("/input/titleSkillId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let storage = Storage::open(paths)?;
    let mut state = storage
        .read_panel_state(project_id, panel_id)?
        .ok_or_else(|| CliError::with_code("target_not_found", "Typesetting state not found."))?;
    let publication = state
        .get_mut("publications")
        .and_then(Value::as_array_mut)
        .and_then(|publications| {
            publications.iter_mut().find(|publication| {
                publication.get("id").and_then(Value::as_str) == Some(publication_id)
            })
        })
        .ok_or_else(|| {
            CliError::with_code(
                "publication_not_found",
                format!("Typesetting publication not found: {publication_id}"),
            )
        })?;
    if !publication.get("titles").is_some_and(Value::is_array) {
        let title = publication
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let selected_id = publication
            .get("selectedTitleId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{publication_id}:title:primary"));
        publication["selectedTitleId"] = json!(selected_id);
        publication["titles"] = json!([{ "id": selected_id, "value": title }]);
    }
    let titles = publication
        .get_mut("titles")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| CliError::with_code("invalid_target", "Typesetting titles are invalid."))?;
    let mut values = titles
        .iter()
        .filter_map(|title| title.get("value").and_then(Value::as_str))
        .map(|title| title.trim().to_lowercase())
        .collect::<HashSet<_>>();
    let mut changed = false;
    for (index, value) in generated.iter().enumerate() {
        let Some(value) = value.as_str() else {
            continue;
        };
        let id = format!("{task_id}:title:{}", index + 1);
        if titles
            .iter()
            .any(|title| title.get("id").and_then(Value::as_str) == Some(&id))
            || !values.insert(value.trim().to_lowercase())
        {
            continue;
        }
        titles.push(json!({
            "id": id,
            "value": value.trim(),
            "source": {
                "kind": "generated",
                "taskId": task_id,
                "skillId": skill_id,
            },
        }));
        changed = true;
    }
    if changed {
        publication["updatedAt"] = json!(crate::control::now_iso());
    }
    Ok(Some((panel_id.to_owned(), state)))
}

fn prepare_cover_completion(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    task: &Value,
    result: Option<Value>,
) -> Result<Option<(String, Value)>, CliError> {
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
    let resource_id = artifact
        .get("resourceId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliError::with_code("invalid_output", "Cover resource id is missing."))?;
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
    let publication = state
        .get_mut("publications")
        .and_then(Value::as_array_mut)
        .and_then(|publications| {
            publications.iter_mut().find(|publication| {
                publication.get("id").and_then(Value::as_str) == Some(publication_id)
            })
        })
        .ok_or_else(|| {
            CliError::with_code(
                "publication_not_found",
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
            "resourceId": resource_id,
            "fileName": file_name,
            "mimeType": artifact.get("mimeType").and_then(Value::as_str).unwrap_or("image/png"),
            "src": format!("/api/assets/{resource_id}/content"),
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

fn prepare_layout_completion(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    task: &Value,
    result: Option<Value>,
) -> Result<Option<(String, Value)>, CliError> {
    let content = result
        .as_ref()
        .and_then(|value| value.pointer("/runtimeFinalization/artifacts/0/content"))
        .cloned()
        .ok_or_else(|| {
            CliError::with_code("invalid_output", "Layout Task has no finalized content.")
        })?;
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
    let expected_hash = task
        .pointer("/input/snapshot/contentHash")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let storage = Storage::open(paths)?;
    let mut state = storage
        .read_panel_state(project_id, panel_id)?
        .ok_or_else(|| CliError::with_code("target_not_found", "Typesetting state not found."))?;
    let publication = state
        .get_mut("publications")
        .and_then(Value::as_array_mut)
        .and_then(|publications| {
            publications.iter_mut().find(|publication| {
                publication.get("id").and_then(Value::as_str) == Some(publication_id)
            })
        })
        .ok_or_else(|| {
            CliError::with_code(
                "publication_not_found",
                format!("Typesetting publication not found: {publication_id}"),
            )
        })?;
    let current_content = publication.get("content").cloned().unwrap_or(Value::Null);
    if hash_json(&current_content)? != expected_hash {
        return Err(CliError::with_code(
            "content_conflict",
            format!("Publication content changed before Layout Task {task_id} completed."),
        ));
    }
    publication["content"] = content;
    publication["updatedAt"] = json!(crate::control::now_iso());
    Ok(Some((panel_id.to_owned(), state)))
}

pub fn fail_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    _message: &str,
) -> Result<Value, CliError> {
    read_publication_task(paths, task_id)
}

pub fn release_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    read_publication_task(paths, task_id)
}

pub fn retry_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    read_publication_task(paths, task_id)
}

pub fn cancel_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    read_publication_task(paths, task_id)
}
