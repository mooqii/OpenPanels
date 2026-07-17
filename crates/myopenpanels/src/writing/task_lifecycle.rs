pub fn panel_selection(
    paths: &MyOpenPanelsPaths,
    bootstrap: &ProjectBootstrap,
) -> Result<Value, CliError> {
    writing_selection_value(paths, bootstrap)
}

pub fn claim_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let mut payload = read_writing_task(paths, task_id)?;
    let attempt = payload["task"]["attempt"].as_i64().unwrap_or(0) + 1;
    payload["task"]["status"] = json!("running");
    payload["task"]["attempt"] = json!(attempt);
    Ok(payload)
}

pub fn heartbeat_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    read_writing_task(paths, task_id)
}

pub fn complete_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let payload = read_writing_task(paths, task_id)?;
    if payload["task"]["type"].as_str() == Some("refine_writing_skill")
        && !crate::content::task_has_staged_resource(
            paths,
            task_id,
            crate::content::ResourceKind::WritingSkill,
        )?
        && !installed_project_skill_for_task(paths, &payload["task"])?
    {
        return Err(CliError::with_code(
            "writing_skill_not_installed",
            "Install the refined custom Writing Skill before completing its Task.",
        ));
    }
    if payload["task"]["type"].as_str() == Some("generate_document") {
        let operations = task_operations(paths, task_id)?;
        if operations
            .iter()
            .any(|operation| operation.get("status").and_then(Value::as_str) == Some("active"))
        {
            return Err(CliError::with_code(
                "writing_operation_active",
                "Complete the Writing generation Operation before completing its Task.",
            ));
        }
        let completed = operations.iter().rev().find(|operation| {
            matches!(
                operation.get("status").and_then(Value::as_str),
                Some("prepared" | "completed")
            ) && operation
                .pointer("/target/documentId")
                .and_then(Value::as_str)
                .is_some_and(|document_id| {
                    operation.get("status").and_then(Value::as_str) == Some("prepared")
                        || crate::wiki::read_generated_document(paths, document_id).is_ok()
                })
        });
        if completed.is_none() {
            return Err(CliError::with_code(
                "invalid_output",
                "Writing Task completed without a successful generation Operation and target document.",
            ));
        }
    }
    Ok(payload)
}

pub(crate) fn prepare_task_completion(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
) -> Result<Option<(String, Value)>, CliError> {
    let payload = complete_task(paths, task_id)?;
    if payload["task"]["type"].as_str() != Some("generate_document") {
        return Ok(None);
    }
    let staged = crate::content::staged_files_for_task(
        paths,
        task_id,
        crate::content::ResourceKind::GeneratedDocument,
    )?;
    if staged.len() != 1 {
        return Err(CliError::with_code(
            "invalid_output",
            "Writing Task must stage exactly one generated document.",
        ));
    }
    let (document_id, logical_path, bytes, metadata) = &staged[0];
    let text = std::str::from_utf8(bytes).map_err(|_| {
        CliError::with_code("invalid_output", "Generated document must be valid UTF-8.")
    })?;
    if text.trim().is_empty() {
        return Err(CliError::with_code(
            "invalid_output",
            "Generated document cannot be empty.",
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
    let mut state = storage
        .read_panel_state(project_id, wiki_panel_id)?
        .ok_or_else(|| CliError::with_code("target_not_found", "Wiki state not found."))?;
    let document = state
        .get_mut("generatedDocuments")
        .and_then(Value::as_array_mut)
        .and_then(|documents| {
            documents
                .iter_mut()
                .find(|document| document.get("id").and_then(Value::as_str) == Some(document_id))
        })
        .ok_or_else(|| {
            CliError::with_code("target_not_found", "Generated document target was deleted.")
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
            format!("Generated document changed from version {base_version} to {current_version}"),
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
    document["contentRef"] = json!(format!("generated/{document_id}/{logical_path}"));
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
    document["generation"] = json!({ "status": "completed", "error": null });
    document["updatedAt"] = json!(now_iso());
    Ok(Some((wiki_panel_id.to_owned(), state)))
}

pub fn fail_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    message: &str,
) -> Result<Value, CliError> {
    let payload = read_writing_task(paths, task_id)?;
    finish_task_operations(paths, task_id, "failed", message)?;
    remove_uncommitted_project_skill(paths, &payload["task"])?;
    Ok(payload)
}

pub fn release_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let payload = read_writing_task(paths, task_id)?;
    finish_task_operations(paths, task_id, "cancelled", "Writing task released.")?;
    remove_uncommitted_project_skill(paths, &payload["task"])?;
    Ok(payload)
}

pub fn retry_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let payload = read_writing_task(paths, task_id)?;
    finish_task_operations(paths, task_id, "cancelled", "Writing task retried.")?;
    remove_uncommitted_project_skill(paths, &payload["task"])?;
    Ok(payload)
}

pub fn cancel_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let payload = read_writing_task(paths, task_id)?;
    finish_task_operations(paths, task_id, "cancelled", "Writing task cancelled.")?;
    let task = &payload["task"];
    if task.get("type").and_then(Value::as_str) == Some("generate_document")
        && task.pointer("/input/mode").and_then(Value::as_str) == Some("create")
    {
        if let (Some(project_id), Some(panel_id), Some(document_id)) = (
            task.get("projectId").and_then(Value::as_str),
            task.pointer("/source/wikiPanelId").and_then(Value::as_str),
            task.pointer("/input/targetGeneratedDocumentId")
                .and_then(Value::as_str),
        ) {
            crate::wiki::remove_pending_writing_document(paths, project_id, panel_id, document_id)?;
        }
    }
    remove_uncommitted_project_skill(paths, &payload["task"])?;
    Ok(payload)
}

fn active_task_operations<'a>(
    paths: &MyOpenPanelsPaths,
    task_id: &'a str,
) -> Result<impl Iterator<Item = Value> + 'a, CliError> {
    Ok(task_operations(paths, task_id)?
        .into_iter()
        .filter(|operation| operation.get("status").and_then(Value::as_str) == Some("active")))
}

fn task_operations(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Vec<Value>, CliError> {
    Ok(Storage::open(paths)?
        .list_agent_operations(None, None)?
        .into_iter()
        .filter(|operation| {
            operation
                .pointer("/target/writingTaskId")
                .and_then(Value::as_str)
                == Some(task_id)
        })
        .collect())
}

fn finish_task_operations(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    status: &str,
    message: &str,
) -> Result<(), CliError> {
    for operation in active_task_operations(paths, task_id)? {
        if let Some(operation_id) = operation.get("id").and_then(Value::as_str) {
            crate::operations::finish_any(paths, operation_id, status, Some(message))?;
        }
    }
    Ok(())
}

fn remove_uncommitted_project_skill(
    paths: &MyOpenPanelsPaths,
    task: &Value,
) -> Result<(), CliError> {
    if task.get("type").and_then(Value::as_str) != Some("refine_writing_skill") {
        return Ok(());
    }
    let skill_id = task
        .pointer("/input/skillId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if skill_id.is_empty() {
        return Ok(());
    }
    let skill_dir = crate::agent::custom_writing_skills_dir(paths)
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
