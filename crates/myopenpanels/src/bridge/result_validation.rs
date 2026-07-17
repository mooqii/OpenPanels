fn validate_conversion_execution_result(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
) -> Result<Value, CliError> {
    let result = read_execution_result(workspace, "Document conversion")?;
    if result.get("schemaVersion").and_then(Value::as_u64) != Some(1)
        || result.get("outcome").and_then(Value::as_str) != Some("converted")
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Document conversion result must use schemaVersion 1 and outcome converted.",
        ));
    }
    if result
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Document conversion result summary cannot be empty.",
        ));
    }
    let document_id = task
        .pointer("/input/documentId")
        .and_then(Value::as_str)
        .or_else(|| task.get("documentId").and_then(Value::as_str))
        .unwrap_or("");
    if result.pointer("/output/documentId").and_then(Value::as_str) != Some(document_id)
        || result
            .pointer("/output/logicalPath")
            .and_then(Value::as_str)
            != Some("source.md")
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Document conversion result output does not match the source document.",
        ));
    }
    let staged = crate::content::staged_files_for_task(
        paths,
        task.get("id").and_then(Value::as_str).unwrap_or(""),
        crate::content::ResourceKind::WikiMarkdown,
    )?;
    let [(staged_document_id, logical_path, bytes, _)] = staged.as_slice() else {
        return Err(CliError::with_code(
            "invalid_output",
            "Document conversion must stage exactly one Source Markdown file.",
        ));
    };
    if staged_document_id != document_id || logical_path != "source.md" {
        return Err(CliError::with_code(
            "invalid_output",
            "Document conversion staged an unexpected document or logical path.",
        ));
    }
    let markdown = std::str::from_utf8(bytes).map_err(|_| {
        CliError::with_code(
            "invalid_output",
            "Converted Source Markdown must be valid UTF-8.",
        )
    })?;
    if markdown.trim().is_empty() {
        return Err(CliError::with_code(
            "invalid_output",
            "Converted Source Markdown cannot be empty.",
        ));
    }
    Ok(result)
}

fn validate_generation_execution_result(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
) -> Result<Value, CliError> {
    let result = read_execution_result(workspace, "Document generation")?;
    if result.get("schemaVersion").and_then(Value::as_u64) != Some(1)
        || result.get("outcome").and_then(Value::as_str) != Some("generated")
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Document generation result must use schemaVersion 1 and outcome generated.",
        ));
    }
    if result
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Document generation result summary cannot be empty.",
        ));
    }
    let task_id = task.get("id").and_then(Value::as_str).unwrap_or("");
    let document_id = task
        .pointer("/input/targetGeneratedDocumentId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let operation_id = result
        .pointer("/output/operationId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            CliError::with_code(
                "invalid_output",
                "Document generation result has no Operation id.",
            )
        })?;
    let logical_path = result
        .pointer("/output/logicalPath")
        .and_then(Value::as_str)
        .unwrap_or("");
    if result.pointer("/output/documentId").and_then(Value::as_str) != Some(document_id)
        || !matches!(logical_path, "content.md" | "content.txt")
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Document generation result output does not match the target document.",
        ));
    }

    let staged = crate::content::staged_files_for_task(
        paths,
        task_id,
        crate::content::ResourceKind::GeneratedDocument,
    )?;
    let [(staged_document_id, staged_path, bytes, metadata)] = staged.as_slice() else {
        return Err(CliError::with_code(
            "invalid_output",
            "Document generation must stage exactly one generated document.",
        ));
    };
    if staged_document_id != document_id || staged_path != logical_path {
        return Err(CliError::with_code(
            "invalid_output",
            "Document generation staged an unexpected document or logical path.",
        ));
    }
    if metadata.get("operationId").and_then(Value::as_str) != Some(operation_id) {
        return Err(CliError::with_code(
            "invalid_output",
            "Document generation result Operation does not match staged metadata.",
        ));
    }
    let generated = std::str::from_utf8(bytes).map_err(|_| {
        CliError::with_code("invalid_output", "Generated document must be valid UTF-8.")
    })?;
    if generated.trim().is_empty() {
        return Err(CliError::with_code(
            "invalid_output",
            "Generated document cannot be empty.",
        ));
    }

    let storage = crate::storage::Storage::open(paths)?;
    let active_operations = storage
        .list_agent_operations(None, None)?
        .into_iter()
        .filter(|operation| {
            operation.pointer("/input/taskId").and_then(Value::as_str) == Some(task_id)
                && matches!(
                    operation.get("status").and_then(Value::as_str),
                    Some("active" | "prepared")
                )
        })
        .collect::<Vec<_>>();
    let [operation] = active_operations.as_slice() else {
        return Err(CliError::with_code(
            "invalid_output",
            "Document generation must have exactly one active Attempt Operation.",
        ));
    };
    let expected_format = if logical_path == "content.txt" {
        "text"
    } else {
        "markdown"
    };
    if operation.get("id").and_then(Value::as_str) != Some(operation_id)
        || operation.get("status").and_then(Value::as_str) != Some("prepared")
        || operation
            .pointer("/target/documentId")
            .and_then(Value::as_str)
            != Some(document_id)
        || operation
            .pointer("/target/writingTaskId")
            .and_then(Value::as_str)
            != Some(task_id)
        || operation.pointer("/input/format").and_then(Value::as_str) != Some(expected_format)
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Document generation Operation does not match the declared output.",
        ));
    }
    Ok(result)
}

fn validate_refinement_execution_result(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
) -> Result<Value, CliError> {
    let result = read_execution_result(workspace, "Writing refinement")?;
    if result.get("schemaVersion").and_then(Value::as_u64) != Some(1)
        || result.get("outcome").and_then(Value::as_str) != Some("refined")
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Writing refinement result must use schemaVersion 1 and outcome refined.",
        ));
    }
    if result
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Writing refinement result summary cannot be empty.",
        ));
    }
    let task_id = task.get("id").and_then(Value::as_str).unwrap_or("");
    let skill_id = task
        .pointer("/input/skillId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let skill_title = task
        .pointer("/input/name")
        .and_then(Value::as_str)
        .unwrap_or("");
    if result.pointer("/output/skillId").and_then(Value::as_str) != Some(skill_id)
        || result
            .pointer("/output/logicalPath")
            .and_then(Value::as_str)
            != Some("SKILL.md")
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Writing refinement result does not match the requested Skill.",
        ));
    }

    let staged = crate::content::staged_files_for_task(
        paths,
        task_id,
        crate::content::ResourceKind::WritingSkill,
    )?;
    if staged.len() != 2
        || staged
            .iter()
            .any(|(resource_key, _, _, _)| resource_key != skill_id)
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Writing refinement must stage exactly one SKILL.md and manifest.json.",
        ));
    }
    let skill_bytes = staged
        .iter()
        .find(|(_, path, _, _)| path == "SKILL.md")
        .map(|(_, _, bytes, _)| bytes)
        .ok_or_else(|| {
            CliError::with_code(
                "invalid_output",
                "Writing refinement did not stage SKILL.md.",
            )
        })?;
    let manifest_bytes = staged
        .iter()
        .find(|(_, path, _, _)| path == "manifest.json")
        .map(|(_, _, bytes, _)| bytes)
        .ok_or_else(|| {
            CliError::with_code(
                "invalid_output",
                "Writing refinement did not stage manifest.json.",
            )
        })?;
    let skill_source = std::str::from_utf8(skill_bytes).map_err(|_| {
        CliError::with_code(
            "invalid_output",
            "Refined Writing Skill must be valid UTF-8.",
        )
    })?;
    let parsed = crate::agent::parse_skill(skill_source, "SKILL.md").map_err(|error| {
        CliError::with_code(
            "invalid_output",
            format!("Refined Writing Skill is invalid: {}", error.message()),
        )
    })?;
    if parsed.metadata.id != skill_id
        || parsed.metadata.title != skill_title
        || parsed.metadata.source != "custom"
        || parsed.metadata.applies_to != ["writing"]
        || parsed.metadata.task_types != ["generate_document"]
        || !parsed.metadata.requires_commands.is_empty()
        || parsed.metadata.description.trim().is_empty()
        || parsed.body.trim().is_empty()
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Refined Writing Skill metadata does not match the Task contract.",
        ));
    }
    let manifest: Value = serde_json::from_slice(manifest_bytes).map_err(|_| {
        CliError::with_code(
            "invalid_output",
            "Refined Writing Skill manifest is not valid JSON.",
        )
    })?;
    if manifest.get("schemaVersion").and_then(Value::as_u64) != Some(1)
        || manifest.get("source").and_then(Value::as_str) != Some("custom")
        || manifest.get("taskId").and_then(Value::as_str) != Some(task_id)
        || manifest.get("skillId").and_then(Value::as_str) != Some(skill_id)
        || manifest.get("title").and_then(Value::as_str) != Some(skill_title)
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Refined Writing Skill manifest does not match the Task contract.",
        ));
    }
    Ok(result)
}

fn read_execution_result(workspace: &Path, label: &str) -> Result<Value, CliError> {
    let result_path = workspace.join(EXECUTION_RESULT_FILE);
    let raw = fs::read_to_string(&result_path).map_err(|_| {
        CliError::with_code(
            "invalid_output",
            format!("{label} Agent did not write execution-result.json."),
        )
    })?;
    serde_json::from_str(&raw).map_err(|_| {
        CliError::with_code(
            "invalid_output",
            format!("{label} execution-result.json is not valid JSON."),
        )
    })
}

fn validate_wiki_execution_result(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
) -> Result<Value, CliError> {
    let mut result = read_execution_result(workspace, "Wiki")?;
    if result.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        return Err(CliError::with_code(
            "invalid_output",
            "Wiki execution result schemaVersion must be 1.",
        ));
    }
    let summary = result
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if summary.is_empty() {
        return Err(CliError::with_code(
            "invalid_output",
            "Wiki execution result summary cannot be empty.",
        ));
    }
    let declared = result
        .get("changedPaths")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            CliError::with_code(
                "invalid_output",
                "Wiki execution result changedPaths must be an array.",
            )
        })?
        .iter()
        .map(|path| {
            path.as_str().map(str::to_owned).ok_or_else(|| {
                CliError::with_code(
                    "invalid_output",
                    "Wiki execution result paths must be strings.",
                )
            })
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let task_id = task.get("id").and_then(Value::as_str).unwrap_or("");
    let staged = crate::content::staged_files_for_task(
        paths,
        task_id,
        crate::content::ResourceKind::WikiSpace,
    )?
    .into_iter()
    .map(|(_, path, _, _)| path)
    .collect::<BTreeSet<_>>();
    match result.get("outcome").and_then(Value::as_str) {
        Some("changed") if !staged.is_empty() && declared == staged => {}
        Some("changed") => {
            return Err(CliError::with_code(
                "invalid_output",
                "Wiki execution result paths do not match staged Wiki pages.",
            ));
        }
        Some("no_change") if staged.is_empty() && declared.is_empty() => {}
        Some("no_change") => {
            return Err(CliError::with_code(
                "invalid_output",
                "Wiki no_change output cannot include or stage changed pages.",
            ));
        }
        _ => {
            return Err(CliError::with_code(
                "invalid_output",
                "Wiki execution result outcome must be changed or no_change.",
            ));
        }
    }
    result["bridgeValidated"] = json!(true);
    Ok(result)
}
