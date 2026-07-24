pub fn panel_selection(
    paths: &MyOpenPanelsPaths,
    bootstrap: &ProjectBootstrap,
) -> Result<Value, CliError> {
    crate::wiki::reject_live_content_access_for_task()?;
    writing_selection_value(paths, bootstrap)
}

pub fn complete_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let payload = read_writing_task(paths, task_id)?;
    if payload["task"]["type"].as_str() == Some("distill_writing_skill")
        && !crate::content::task_has_staged_resource(
            paths,
            task_id,
            crate::content::ResourceKind::WritingSkill,
        )?
        && !installed_project_skill_for_task(paths, &payload["task"])?
    {
        return Err(CliError::with_code(
            "writing_skill_not_installed",
            "Install the distilled custom Writing Skill before completing its Task.",
        ));
    }
    Ok(payload)
}

pub(crate) fn prepare_task_completion(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
) -> Result<Option<crate::tasks::PreparedPanelState>, CliError> {
    let payload = complete_task(paths, task_id)?;
    if payload["task"]["type"].as_str() != Some("write_my_document") {
        return Ok(None);
    }
    let staged = crate::content::staged_files_for_task(
        paths,
        task_id,
        crate::content::ResourceKind::MyDocument,
    )?;
    if staged.len() != 1 {
        return Err(CliError::with_code(
            "invalid_output",
            "Writing Task must stage exactly one My Document.",
        ));
    }
    let (document_id, logical_path, bytes, metadata) = &staged[0];
    let text = std::str::from_utf8(bytes).map_err(|_| {
        CliError::with_code("invalid_output", "My Document must be valid UTF-8.")
    })?;
    if text.trim().is_empty() {
        return Err(CliError::with_code(
            "invalid_output",
            "My Document cannot be empty.",
        ));
    }
    let project_id = payload["task"]["projectId"].as_str().unwrap_or_default();
    let wiki_panel_id = payload["task"]["source"]["wikiPanelId"]
        .as_str()
        .ok_or_else(|| {
            CliError::with_code(
                "writing_target_not_found",
                "Writing Task has no Wiki target.",
            )
        })?;
    let storage = Storage::open(paths)?;
    let (mut state, base_revision) = storage
        .read_panel_state_snapshot(project_id, wiki_panel_id)?
        .ok_or_else(|| CliError::with_code("target_not_found", "Wiki state not found."))?;
    let document = state
        .get_mut("myDocuments")
        .and_then(Value::as_array_mut)
        .and_then(|documents| {
            documents
                .iter_mut()
                .find(|document| document.get("id").and_then(Value::as_str) == Some(document_id))
        })
        .ok_or_else(|| {
            CliError::with_code("target_not_found", "My Document target was deleted.")
        })?;
    let base_version = metadata
        .get("baseContentVersion")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let current_version = document
        .get("contentVersion")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if current_version != base_version {
        return Err(CliError::with_code(
            "content_conflict",
            format!("My Document changed from version {base_version} to {current_version}"),
        ));
    }
    let format = if logical_path.ends_with(".txt") {
        "text"
    } else {
        "markdown"
    };
    let mime_type = if format == "text" {
        "text/plain"
    } else {
        "text/markdown"
    };
    let expected_version = payload["task"]
        .pointer("/input/targetContentVersion")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if base_version != expected_version {
        return Err(CliError::with_code(
            "invalid_output",
            "Staged My Document does not match the Task target version.",
        ));
    }
    if payload["task"].pointer("/input/mode").and_then(Value::as_str) == Some("create")
        && current_version == 0
    {
        if let Some(title) = metadata
            .get("title")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            document["title"] = json!(title);
        }
    }
    document["contentRef"] = json!(logical_path);
    document["contentVersion"] = json!(current_version + 1);
    document["format"] = json!(format);
    document["mimeType"] = json!(mime_type);
    document["originalFileName"] = metadata
        .get("fileName")
        .cloned()
        .unwrap_or_else(|| json!(logical_path));
    document["wordCount"] = json!(text
        .chars()
        .filter(|character| !character.is_whitespace())
        .count());
    document
        .as_object_mut()
        .map(|object| object.remove("writeOperation"));
    document["updatedAt"] = json!(now_iso());
    Ok(Some(crate::tasks::PreparedPanelState::new(
        wiki_panel_id,
        base_revision,
        state,
    )))
}

pub(crate) fn prepare_task_cancellation(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
) -> Result<Option<crate::tasks::PreparedPanelState>, CliError> {
    let payload = read_writing_task(paths, task_id)?;
    let task = &payload["task"];
    if task.get("type").and_then(Value::as_str) != Some("write_my_document")
        || task.pointer("/input/mode").and_then(Value::as_str) != Some("create")
    {
        return Ok(None);
    }
    let (Some(project_id), Some(panel_id), Some(document_id)) = (
        task.get("projectId").and_then(Value::as_str),
        task.pointer("/source/wikiPanelId").and_then(Value::as_str),
        task.pointer("/input/targetMyDocumentId")
            .and_then(Value::as_str),
    ) else {
        return Ok(None);
    };
    crate::wiki::prepare_pending_writing_document_removal(
        paths,
        project_id,
        panel_id,
        document_id,
    )
}

pub(crate) fn cleanup_uncommitted_writing_skill(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
) -> Result<(), CliError> {
    let payload = read_writing_task(paths, task_id)?;
    remove_uncommitted_project_skill(paths, &payload["task"])
}

fn remove_uncommitted_project_skill(
    paths: &MyOpenPanelsPaths,
    task: &Value,
) -> Result<(), CliError> {
    if task.get("type").and_then(Value::as_str) != Some("distill_writing_skill") {
        return Ok(());
    }
    let skill_id = task
        .pointer("/input/skillId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if skill_id.is_empty() {
        return Ok(());
    }
    let skill_dir = crate::agent::custom_agent_skills_dir(paths)
        .join(crate::paths::sanitize_path_part(skill_id));
    match fs::remove_dir_all(skill_dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(to_cli_error(error)),
    }
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
