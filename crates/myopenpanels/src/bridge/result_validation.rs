fn build_conversion_output_plan(
    _paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
    _attempt_id: &str,
    _execution_generation: i64,
    _execution_unit: &Value,
) -> Result<TaskOutputPlanDraft, CliError> {
    let result = read_execution_result_v2(workspace, "Document conversion")?;
    validate_result_keys(
        &result,
        &["schemaVersion", "outcome", "summary", "artifacts"],
        "Document conversion",
    )?;
    require_outcome(&result, "converted", "Document conversion")?;
    let artifact = exactly_one_artifact(workspace, &result, "Document conversion")?;
    validate_fixed_artifact(
        &artifact,
        "source-markdown",
        "outputs/source.md",
        "Document conversion",
    )?;
    let document_id = task
        .pointer("/input/documentId")
        .and_then(Value::as_str)
        .or_else(|| task.get("documentId").and_then(Value::as_str))
        .ok_or_else(|| CliError::with_code("invalid_output", "Conversion target is missing."))?;
    Ok(TaskOutputPlanDraft {
        result,
        actions: vec![TaskOutputAction::StageText {
            resource_kind: crate::content::ResourceKind::WikiMarkdown
                .as_str()
                .to_owned(),
            resource_key: document_id.to_owned(),
            logical_path: "source.md".to_owned(),
            artifact,
            mime_type: "text/markdown".to_owned(),
            metadata: json!({ "documentId": document_id }),
        }],
    })
}

fn build_generation_output_plan(
    _paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
    _attempt_id: &str,
    _execution_generation: i64,
    _execution_unit: &Value,
) -> Result<TaskOutputPlanDraft, CliError> {
    let result = read_execution_result_v2(workspace, "Document generation")?;
    validate_result_keys(
        &result,
        &["schemaVersion", "outcome", "summary", "title", "artifacts"],
        "Document generation",
    )?;
    require_outcome(&result, "generated", "Document generation")?;
    let title = result
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CliError::with_code(
                "invalid_output",
                "Document generation result title cannot be empty.",
            )
        })?
        .to_owned();
    let artifact = exactly_one_artifact(workspace, &result, "Document generation")?;
    if artifact.role != "generated-document"
        || !matches!(
            artifact.relative_path.as_str(),
            "outputs/document.md" | "outputs/document.txt"
        )
        || artifact.logical_path.is_some()
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Document generation must declare exactly outputs/document.md or outputs/document.txt.",
        ));
    }
    let document_id = task
        .pointer("/input/targetGeneratedDocumentId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CliError::with_code("invalid_output", "Document generation target is missing.")
        })?;
    let document_format = if artifact.relative_path.ends_with(".txt") {
        "text"
    } else {
        "markdown"
    };
    Ok(TaskOutputPlanDraft {
        result,
        actions: vec![TaskOutputAction::PrepareGeneratedDocument {
            document_id: document_id.to_owned(),
            title,
            document_format: document_format.to_owned(),
            artifact,
        }],
    })
}

fn build_refinement_output_plan(
    _paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
    _attempt_id: &str,
    _execution_generation: i64,
    _execution_unit: &Value,
) -> Result<TaskOutputPlanDraft, CliError> {
    let result = read_execution_result_v2(workspace, "Writing refinement")?;
    validate_result_keys(
        &result,
        &["schemaVersion", "outcome", "summary", "artifacts"],
        "Writing refinement",
    )?;
    require_outcome(&result, "refined", "Writing refinement")?;
    let artifact = exactly_one_artifact(workspace, &result, "Writing refinement")?;
    validate_fixed_artifact(
        &artifact,
        "writing-skill",
        "outputs/SKILL.md",
        "Writing refinement",
    )?;
    let skill_id = task
        .pointer("/input/skillId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliError::with_code("invalid_output", "Writing Skill target is missing."))?;
    let source = fs::read_to_string(&artifact.absolute_path).map_err(to_cli_error)?;
    crate::agent::validate_portable_writing_skill(&source, "outputs/SKILL.md", skill_id)
        .map_err(|error| CliError::with_code("invalid_output", error.message()))?;
    Ok(TaskOutputPlanDraft {
        result,
        actions: vec![TaskOutputAction::PrepareWritingSkill {
            skill_id: skill_id.to_owned(),
            artifact,
        }],
    })
}

fn build_typesetting_cover_output_plan(
    _paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
    _attempt_id: &str,
    _execution_generation: i64,
    _execution_unit: &Value,
) -> Result<TaskOutputPlanDraft, CliError> {
    let result = read_execution_result_v2(workspace, "Typesetting Cover")?;
    validate_result_keys(
        &result,
        &["schemaVersion", "outcome", "summary", "artifacts"],
        "Typesetting Cover",
    )?;
    require_outcome(&result, "generated", "Typesetting Cover")?;
    let artifact = exactly_one_binary_artifact(workspace, &result, "Typesetting Cover")?;
    validate_fixed_artifact(
        &artifact,
        "typesetting-cover",
        "outputs/cover.png",
        "Typesetting Cover",
    )?;
    let bytes = fs::read(&artifact.absolute_path).map_err(to_cli_error)?;
    let (width, height) = png_dimensions(&bytes).ok_or_else(|| {
        CliError::with_code(
            "invalid_output",
            "Typesetting Cover output must be a valid non-empty PNG bitmap.",
        )
    })?;
    let project_id = task
        .get("projectId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliError::with_code("invalid_output", "Cover project is missing."))?;
    let panel_id = task
        .get("panelId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliError::with_code("invalid_output", "Cover panel is missing."))?;
    let task_id = task
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliError::with_code("invalid_output", "Cover Task id is missing."))?;
    Ok(TaskOutputPlanDraft {
        result,
        actions: vec![TaskOutputAction::PrepareTypesettingCover {
            project_id: project_id.to_owned(),
            panel_id: panel_id.to_owned(),
            task_id: task_id.to_owned(),
            artifact,
            width,
            height,
        }],
    })
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 8 || bytes[..8] != [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a] {
        return None;
    }
    let mut offset = 8_usize;
    let mut dimensions = None;
    let mut saw_image_data = false;
    while offset < bytes.len() {
        let header_end = offset.checked_add(8)?;
        if header_end > bytes.len() {
            return None;
        }
        let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().ok()?) as usize;
        let chunk_type = &bytes[offset + 4..header_end];
        let data_end = header_end.checked_add(length)?;
        let chunk_end = data_end.checked_add(4)?;
        if chunk_end > bytes.len() {
            return None;
        }
        let stored_crc = u32::from_be_bytes(bytes[data_end..chunk_end].try_into().ok()?);
        if png_crc32(&bytes[offset + 4..data_end]) != stored_crc {
            return None;
        }
        match chunk_type {
            b"IHDR" => {
                if dimensions.is_some() || offset != 8 || length != 13 {
                    return None;
                }
                let data = &bytes[header_end..data_end];
                let width = u32::from_be_bytes(data[..4].try_into().ok()?);
                let height = u32::from_be_bytes(data[4..8].try_into().ok()?);
                if width == 0
                    || height == 0
                    || !valid_png_color_format(data[8], data[9])
                    || data[10] != 0
                    || data[11] != 0
                    || data[12] > 1
                {
                    return None;
                }
                dimensions = Some((width, height));
            }
            b"IDAT" => {
                if dimensions.is_none() || length == 0 {
                    return None;
                }
                saw_image_data = true;
            }
            b"IEND" => {
                return (length == 0 && saw_image_data && chunk_end == bytes.len())
                    .then_some(dimensions?)
            }
            _ => {}
        }
        offset = chunk_end;
    }
    None
}

fn valid_png_color_format(bit_depth: u8, color_type: u8) -> bool {
    match color_type {
        0 => matches!(bit_depth, 1 | 2 | 4 | 8 | 16),
        2 | 4 | 6 => matches!(bit_depth, 8 | 16),
        3 => matches!(bit_depth, 1 | 2 | 4 | 8),
        _ => false,
    }
}

fn png_crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn build_wiki_output_plan(
    _paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
    _attempt_id: &str,
    _execution_generation: i64,
    _execution_unit: &Value,
) -> Result<TaskOutputPlanDraft, CliError> {
    let result = read_execution_result_v2(workspace, "Wiki")?;
    validate_result_keys(
        &result,
        &[
            "schemaVersion",
            "outcome",
            "summary",
            "changedPaths",
            "artifacts",
        ],
        "Wiki",
    )?;
    let outcome = result.get("outcome").and_then(Value::as_str).unwrap_or("");
    if !matches!(outcome, "changed" | "no_change") {
        return Err(CliError::with_code(
            "invalid_output",
            "Wiki execution result outcome must be changed or no_change.",
        ));
    }
    let declared_values = result
        .get("changedPaths")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            CliError::with_code(
                "invalid_output",
                "Wiki execution result changedPaths must be an array.",
            )
        })?;
    let mut changed_paths = BTreeSet::new();
    for value in declared_values {
        let path = value.as_str().ok_or_else(|| {
            CliError::with_code(
                "invalid_output",
                "Wiki execution result paths must be strings.",
            )
        })?;
        validate_logical_output_path(path, "Wiki page")?;
        if !path.ends_with(".md") || !changed_paths.insert(path.to_owned()) {
            return Err(CliError::with_code(
                "invalid_output",
                "Wiki changedPaths must contain unique Markdown paths.",
            ));
        }
    }
    let artifacts = read_artifacts(workspace, &result, "Wiki")?;
    if artifacts.len() > crate::content::MAX_WIKI_FILES {
        return Err(CliError::with_code(
            "invalid_output",
            "Wiki execution result declares too many pages.",
        ));
    }
    let mut artifact_paths = BTreeSet::new();
    let mut artifacts_by_path = BTreeMap::new();
    for artifact in artifacts {
        let logical_path = artifact.logical_path.clone().ok_or_else(|| {
            CliError::with_code(
                "invalid_output",
                "Every Wiki page artifact requires logicalPath.",
            )
        })?;
        let expected_relative = format!("outputs/wiki/{logical_path}");
        if artifact.role != "wiki-page"
            || artifact.relative_path != expected_relative
            || !artifact_paths.insert(logical_path.clone())
        {
            return Err(CliError::with_code(
                "invalid_output",
                "Wiki artifacts must uniquely mirror outputs/wiki/<logicalPath>.",
            ));
        }
        artifacts_by_path.insert(logical_path, artifact);
    }
    if outcome == "no_change" && (!changed_paths.is_empty() || !artifacts_by_path.is_empty()) {
        return Err(CliError::with_code(
            "invalid_output",
            "Wiki no_change output cannot include changed paths or artifacts.",
        ));
    }
    if outcome == "changed"
        && (changed_paths.is_empty() || changed_paths != artifact_paths)
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Wiki changedPaths must exactly match the declared Wiki page artifacts.",
        ));
    }
    let wiki_space_id = task
        .get("wikiSpaceId")
        .and_then(Value::as_str)
        .or_else(|| task.pointer("/input/wikiSpaceId").and_then(Value::as_str))
        .or_else(|| task.pointer("/source/wikiSpaceId").and_then(Value::as_str))
        .unwrap_or("wiki:default")
        .to_owned();
    let actions = artifacts_by_path
        .into_iter()
        .map(|(logical_path, artifact)| TaskOutputAction::StageText {
            resource_kind: crate::content::ResourceKind::WikiSpace
                .as_str()
                .to_owned(),
            resource_key: wiki_space_id.clone(),
            logical_path,
            artifact,
            mime_type: "text/markdown".to_owned(),
            metadata: json!({ "wikiSpaceId": wiki_space_id }),
        })
        .collect();
    Ok(TaskOutputPlanDraft { result, actions })
}

fn build_xiaohongshu_publishing_output_plan(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
    _attempt_id: &str,
    _execution_generation: i64,
    _execution_unit: &Value,
) -> Result<TaskOutputPlanDraft, CliError> {
    build_publishing_output_plan(paths, task, workspace, "Xiaohongshu publishing", "xiaohongshu")
}

fn build_wechat_official_account_publishing_output_plan(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
    _attempt_id: &str,
    _execution_generation: i64,
    _execution_unit: &Value,
) -> Result<TaskOutputPlanDraft, CliError> {
    build_publishing_output_plan(
        paths,
        task,
        workspace,
        "WeChat Official Account publishing",
        "wechat_official_account",
    )
}

fn build_publishing_output_plan(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
    label: &str,
    platform: &str,
) -> Result<TaskOutputPlanDraft, CliError> {
    let result = read_execution_result_v2(workspace, label)?;
    validate_result_keys(
        &result,
        &[
            "schemaVersion",
            "outcome",
            "summary",
            "artifacts",
            "platform",
            "releaseId",
            "attemptId",
            "reasonCode",
            "remoteUrl",
            "publishedAt",
        ],
        label,
    )?;
    if !read_artifacts(workspace, &result, label)?.is_empty() {
        return Err(CliError::with_code(
            "invalid_output",
            "Publishing results cannot declare local output artifacts.",
        ));
    }
    let outcome = result.get("outcome").and_then(Value::as_str).unwrap_or("");
    if !matches!(
        outcome,
        "published" | "needs_user_action" | "not_published" | "unknown"
    ) || result.get("platform").and_then(Value::as_str) != Some(platform)
        || result.get("releaseId").and_then(Value::as_str)
            != task.pointer("/input/releaseId").and_then(Value::as_str)
        || result.get("attemptId").and_then(Value::as_str)
            != task.pointer("/input/attemptId").and_then(Value::as_str)
    {
        return Err(CliError::with_code(
            "invalid_output",
            format!("{label} result does not match its Task contract."),
        ));
    }
    if outcome == "published" {
        if result
            .get("publishedAt")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            return Err(CliError::with_code(
                "invalid_output",
                "A published result requires an observed publishedAt value.",
            ));
        }
    } else if result
        .get("reasonCode")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return Err(CliError::with_code(
            "invalid_output",
            "A non-published result requires a reasonCode.",
        ));
    }
    match result.get("remoteUrl") {
        Some(Value::Null) => {}
        Some(Value::String(remote_url)) => {
            let host = remote_url
                .strip_prefix("https://")
                .and_then(|rest| rest.split('/').next())
                .unwrap_or("")
                .split(':')
                .next()
                .unwrap_or("");
            let allowed = match platform {
                "xiaohongshu" => {
                    host == "xiaohongshu.com" || host.ends_with(".xiaohongshu.com")
                }
                "wechat_official_account" => host == "mp.weixin.qq.com",
                _ => false,
            };
            if !allowed {
                return Err(CliError::with_code(
                    "invalid_output",
                    format!("Publishing remoteUrl is not valid for {platform}."),
                ));
            }
        }
        _ => {
            return Err(CliError::with_code(
                "invalid_output",
                "Publishing remoteUrl must be a string or null.",
            ));
        }
    }
    if matches!(outcome, "published" | "unknown") {
        let project_id = task.get("projectId").and_then(Value::as_str).unwrap_or("");
        let panel_id = task.get("panelId").and_then(Value::as_str).unwrap_or("");
        let attempt_id = task
            .pointer("/input/attemptId")
            .and_then(Value::as_str)
            .unwrap_or("");
        let state = crate::publishing::normalize_state(
            crate::storage::Storage::open(paths)?
                .read_panel_state(project_id, panel_id)?
                .unwrap_or_else(crate::publishing::empty_state),
        );
        let committing = state
            .get("releases")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|release| {
                release
                    .get("attempts")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .find(|attempt| attempt.get("id").and_then(Value::as_str) == Some(attempt_id))
            .and_then(|attempt| attempt.get("phase"))
            .and_then(Value::as_str)
            == Some("committing");
        if !committing {
            return Err(CliError::with_code(
                "invalid_output",
                "Published or unknown outcomes require the committing checkpoint.",
            ));
        }
    }
    Ok(TaskOutputPlanDraft {
        result,
        actions: Vec::new(),
    })
}

fn read_execution_result_v2(workspace: &Path, label: &str) -> Result<Value, CliError> {
    let result_path = workspace.join(EXECUTION_RESULT_FILE);
    let raw = fs::read_to_string(&result_path).map_err(|_| {
        CliError::with_code(
            "invalid_output",
            format!("{label} Agent did not write execution-result.json."),
        )
    })?;
    let result: Value = serde_json::from_str(&raw).map_err(|_| {
        CliError::with_code(
            "invalid_output",
            format!("{label} execution-result.json is not valid JSON."),
        )
    })?;
    if result.get("schemaVersion").and_then(Value::as_u64)
        != Some(EXECUTION_RESULT_SCHEMA_VERSION as u64)
    {
        return Err(CliError::with_code(
            "invalid_output",
            format!(
                "{label} execution result schemaVersion must be {EXECUTION_RESULT_SCHEMA_VERSION}."
            ),
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
            format!("{label} execution result summary cannot be empty."),
        ));
    }
    Ok(result)
}

fn validate_result_keys(result: &Value, allowed: &[&str], label: &str) -> Result<(), CliError> {
    let object = result.as_object().ok_or_else(|| {
        CliError::with_code(
            "invalid_output",
            format!("{label} execution result must be a JSON object."),
        )
    })?;
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(CliError::with_code(
            "invalid_output",
            format!("{label} execution result contains unsupported field {key}."),
        ));
    }
    Ok(())
}

fn require_outcome(result: &Value, expected: &str, label: &str) -> Result<(), CliError> {
    if result.get("outcome").and_then(Value::as_str) == Some(expected) {
        Ok(())
    } else {
        Err(CliError::with_code(
            "invalid_output",
            format!("{label} execution result outcome must be {expected}."),
        ))
    }
}

fn exactly_one_artifact(
    workspace: &Path,
    result: &Value,
    label: &str,
) -> Result<TaskOutputArtifact, CliError> {
    let artifacts = read_artifacts(workspace, result, label)?;
    let [artifact] = artifacts.as_slice() else {
        return Err(CliError::with_code(
            "invalid_output",
            format!("{label} must declare exactly one artifact."),
        ));
    };
    Ok(artifact.clone())
}

fn exactly_one_binary_artifact(
    workspace: &Path,
    result: &Value,
    label: &str,
) -> Result<TaskOutputArtifact, CliError> {
    let artifacts = read_artifacts_with_mode(workspace, result, label, false)?;
    let [artifact] = artifacts.as_slice() else {
        return Err(CliError::with_code(
            "invalid_output",
            format!("{label} must declare exactly one artifact."),
        ));
    };
    Ok(artifact.clone())
}

fn validate_fixed_artifact(
    artifact: &TaskOutputArtifact,
    role: &str,
    relative_path: &str,
    label: &str,
) -> Result<(), CliError> {
    if artifact.role == role
        && artifact.relative_path == relative_path
        && artifact.logical_path.is_none()
    {
        Ok(())
    } else {
        Err(CliError::with_code(
            "invalid_output",
            format!("{label} declared an unexpected artifact."),
        ))
    }
}

fn read_artifacts(
    workspace: &Path,
    result: &Value,
    label: &str,
) -> Result<Vec<TaskOutputArtifact>, CliError> {
    read_artifacts_with_mode(workspace, result, label, true)
}

fn read_artifacts_with_mode(
    workspace: &Path,
    result: &Value,
    label: &str,
    require_utf8: bool,
) -> Result<Vec<TaskOutputArtifact>, CliError> {
    let declared = result
        .get("artifacts")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            CliError::with_code(
                "invalid_output",
                format!("{label} execution result artifacts must be an array."),
            )
        })?;
    let mut artifacts = Vec::with_capacity(declared.len());
    let mut total_bytes = 0_u64;
    for value in declared {
        let object = value.as_object().ok_or_else(|| {
            CliError::with_code("invalid_output", "Execution artifacts must be objects.")
        })?;
        if let Some(key) = object
            .keys()
            .find(|key| !matches!(key.as_str(), "role" | "relativePath" | "logicalPath"))
        {
            return Err(CliError::with_code(
                "invalid_output",
                format!("Execution artifact contains unsupported field {key}."),
            ));
        }
        let role = object
            .get("role")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                CliError::with_code("invalid_output", "Execution artifact role is required.")
            })?;
        let relative_path = object
            .get("relativePath")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                CliError::with_code(
                    "invalid_output",
                    "Execution artifact relativePath is required.",
                )
            })?;
        let logical_path = object
            .get("logicalPath")
            .map(|value| {
                value
                    .as_str()
                    .filter(|path| !path.trim().is_empty())
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        CliError::with_code(
                            "invalid_output",
                            "Execution artifact logicalPath must be a non-empty string.",
                        )
                    })
            })
            .transpose()?;
        if let Some(path) = logical_path.as_deref() {
            validate_logical_output_path(path, "Execution artifact")?;
        }
        let absolute_path = validate_workspace_artifact(workspace, relative_path)?;
        let metadata = fs::metadata(&absolute_path).map_err(to_cli_error)?;
        if metadata.len() > crate::content::MAX_TEXT_FILE_BYTES as u64 {
            return Err(CliError::with_code(
                "content_too_large",
                format!(
                    "An execution artifact cannot exceed {} bytes.",
                    crate::content::MAX_TEXT_FILE_BYTES
                ),
            ));
        }
        total_bytes = total_bytes.saturating_add(metadata.len());
        if total_bytes > crate::content::MAX_STAGING_BYTES as u64 {
            return Err(CliError::with_code(
                "content_too_large",
                format!(
                    "Execution artifacts cannot exceed {} bytes in total.",
                    crate::content::MAX_STAGING_BYTES
                ),
            ));
        }
        let bytes = fs::read(&absolute_path).map_err(to_cli_error)?;
        if bytes.is_empty() {
            return Err(CliError::with_code(
                "invalid_output",
                "Execution artifacts cannot be empty.",
            ));
        }
        if require_utf8 {
            let text = std::str::from_utf8(&bytes).map_err(|_| {
                CliError::with_code("invalid_output", "Execution artifacts must be valid UTF-8.")
            })?;
            if text.trim().is_empty() {
                return Err(CliError::with_code(
                    "invalid_output",
                    "Execution artifacts cannot be empty.",
                ));
            }
        }
        artifacts.push(TaskOutputArtifact {
            role: role.to_owned(),
            relative_path: relative_path.to_owned(),
            absolute_path,
            logical_path,
            size_bytes: metadata.len(),
            content_hash: format!("sha256:{:x}", Sha256::digest(&bytes)),
        });
    }
    Ok(artifacts)
}

fn validate_workspace_artifact(
    workspace: &Path,
    relative_path: &str,
) -> Result<PathBuf, CliError> {
    validate_logical_output_path(relative_path, "Execution artifact")?;
    if !relative_path.starts_with("outputs/") {
        return Err(CliError::with_code(
            "invalid_output",
            "Execution artifacts must be inside the outputs directory.",
        ));
    }
    let root = workspace.canonicalize().map_err(to_cli_error)?;
    let candidate = workspace.join(relative_path);
    let mut current = workspace.to_path_buf();
    for component in Path::new(relative_path).components() {
        let std::path::Component::Normal(component) = component else {
            return Err(CliError::with_code(
                "invalid_output",
                "Execution artifact path is unsafe.",
            ));
        };
        current.push(component);
        let metadata = fs::symlink_metadata(&current).map_err(|_| {
            CliError::with_code(
                "invalid_output",
                format!("Execution artifact does not exist: {relative_path}"),
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(CliError::with_code(
                "invalid_output",
                "Execution artifact paths cannot contain symlinks.",
            ));
        }
    }
    let canonical = candidate.canonicalize().map_err(to_cli_error)?;
    if !canonical.starts_with(root) || !canonical.is_file() {
        return Err(CliError::with_code(
            "invalid_output",
            "Execution artifact must be a regular workspace file.",
        ));
    }
    Ok(canonical)
}

fn validate_logical_output_path(path: &str, label: &str) -> Result<(), CliError> {
    if path.is_empty()
        || Path::new(path).is_absolute()
        || path.contains('\\')
        || Path::new(path)
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(CliError::with_code(
            "invalid_output",
            format!("{label} path is unsafe: {path}"),
        ));
    }
    Ok(())
}

#[cfg(test)]
fn validate_conversion_execution_result(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
) -> Result<Value, CliError> {
    let legacy = read_legacy_result(workspace, "Document conversion")?;
    let document_id = task
        .pointer("/input/documentId")
        .and_then(Value::as_str)
        .or_else(|| task.get("documentId").and_then(Value::as_str))
        .unwrap_or("");
    if legacy.pointer("/output/documentId").and_then(Value::as_str) != Some(document_id)
        || legacy.pointer("/output/logicalPath").and_then(Value::as_str) != Some("source.md")
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Legacy conversion result does not match its target.",
        ));
    }
    let staged = crate::content::staged_files_for_task(
        paths,
        task.get("id").and_then(Value::as_str).unwrap_or(""),
        crate::content::ResourceKind::WikiMarkdown,
    )?;
    let [(staged_id, logical_path, bytes, _)] = staged.as_slice() else {
        return Err(CliError::with_code(
            "invalid_output",
            "Legacy conversion result requires one staged file.",
        ));
    };
    if staged_id != document_id || logical_path != "source.md" {
        return Err(CliError::with_code(
            "invalid_output",
            "Legacy conversion staged an unexpected file.",
        ));
    }
    write_test_artifact(workspace, "outputs/source.md", bytes)?;
    let result = json!({
        "schemaVersion": 2,
        "outcome": "converted",
        "summary": legacy.get("summary"),
        "artifacts": [{ "role": "source-markdown", "relativePath": "outputs/source.md" }],
    });
    write_test_result(workspace, &result)?;
    Ok(build_conversion_output_plan(paths, task, workspace, "test", 0, &json!({}))?.result)
}

#[cfg(test)]
fn validate_generation_execution_result(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
) -> Result<Value, CliError> {
    let legacy = read_legacy_result(workspace, "Document generation")?;
    let task_id = task.get("id").and_then(Value::as_str).unwrap_or("");
    let document_id = task
        .pointer("/input/targetGeneratedDocumentId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let operation_id = legacy
        .pointer("/output/operationId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let staged = crate::content::staged_files_for_task(
        paths,
        task_id,
        crate::content::ResourceKind::GeneratedDocument,
    )?;
    let [(staged_id, logical_path, bytes, metadata)] = staged.as_slice() else {
        return Err(CliError::with_code(
            "invalid_output",
            "Legacy generation result requires one staged document.",
        ));
    };
    if staged_id != document_id
        || metadata.get("operationId").and_then(Value::as_str) != Some(operation_id)
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Legacy generation result does not match its Operation.",
        ));
    }
    let relative = if logical_path == "content.txt" {
        "outputs/document.txt"
    } else {
        "outputs/document.md"
    };
    write_test_artifact(workspace, relative, bytes)?;
    let result = json!({
        "schemaVersion": 2,
        "outcome": "generated",
        "summary": legacy.get("summary"),
        "title": "Generated document",
        "artifacts": [{ "role": "generated-document", "relativePath": relative }],
    });
    write_test_result(workspace, &result)?;
    Ok(build_generation_output_plan(paths, task, workspace, "test", 0, &json!({}))?.result)
}

#[cfg(test)]
fn validate_refinement_execution_result(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
) -> Result<Value, CliError> {
    let legacy = read_legacy_result(workspace, "Writing refinement")?;
    let skill_id = task
        .pointer("/input/skillId")
        .and_then(Value::as_str)
        .unwrap_or("");
    if legacy.pointer("/output/skillId").and_then(Value::as_str) != Some(skill_id) {
        return Err(CliError::with_code(
            "invalid_output",
            "Legacy refinement result does not match its Skill.",
        ));
    }
    let staged = crate::content::staged_files_for_task(
        paths,
        task.get("id").and_then(Value::as_str).unwrap_or(""),
        crate::content::ResourceKind::WritingSkill,
    )?;
    let bytes = staged
        .iter()
        .find(|(id, path, _, _)| id == skill_id && path == "SKILL.md")
        .map(|(_, _, bytes, _)| bytes)
        .ok_or_else(|| {
            CliError::with_code("invalid_output", "Legacy refinement did not stage SKILL.md.")
        })?;
    write_test_artifact(workspace, "outputs/SKILL.md", bytes)?;
    let result = json!({
        "schemaVersion": 2,
        "outcome": "refined",
        "summary": legacy.get("summary"),
        "artifacts": [{ "role": "writing-skill", "relativePath": "outputs/SKILL.md" }],
    });
    write_test_result(workspace, &result)?;
    Ok(build_refinement_output_plan(paths, task, workspace, "test", 0, &json!({}))?.result)
}

#[cfg(test)]
fn validate_wiki_execution_result(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
) -> Result<Value, CliError> {
    let legacy = read_legacy_result(workspace, "Wiki")?;
    let changed_paths = legacy
        .get("changedPaths")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let staged = crate::content::staged_files_for_task(
        paths,
        task.get("id").and_then(Value::as_str).unwrap_or(""),
        crate::content::ResourceKind::WikiSpace,
    )?;
    let mut artifacts = Vec::new();
    for (_, logical_path, bytes, _) in staged {
        let relative = format!("outputs/wiki/{logical_path}");
        write_test_artifact(workspace, &relative, &bytes)?;
        artifacts.push(json!({
            "role": "wiki-page",
            "relativePath": relative,
            "logicalPath": logical_path,
        }));
    }
    let result = json!({
        "schemaVersion": 2,
        "outcome": legacy.get("outcome"),
        "summary": legacy.get("summary"),
        "changedPaths": changed_paths,
        "artifacts": artifacts,
    });
    write_test_result(workspace, &result)?;
    Ok(build_wiki_output_plan(paths, task, workspace, "test", 0, &json!({}))?.result)
}

#[cfg(test)]
fn read_legacy_result(workspace: &Path, label: &str) -> Result<Value, CliError> {
    let raw = fs::read_to_string(workspace.join(EXECUTION_RESULT_FILE)).map_err(|_| {
        CliError::with_code("invalid_output", format!("{label} result is missing."))
    })?;
    serde_json::from_str(&raw)
        .map_err(|_| CliError::with_code("invalid_output", format!("{label} result is invalid.")))
}

#[cfg(test)]
fn write_test_artifact(workspace: &Path, relative: &str, bytes: &[u8]) -> Result<(), CliError> {
    let path = workspace.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(path, bytes).map_err(to_cli_error)
}

#[cfg(test)]
fn write_test_result(workspace: &Path, result: &Value) -> Result<(), CliError> {
    fs::write(
        workspace.join(EXECUTION_RESULT_FILE),
        serde_json::to_vec(result).map_err(to_cli_error)?,
    )
    .map_err(to_cli_error)
}

#[cfg(test)]
mod typesetting_cover_png_tests {
    use super::*;

    fn append_chunk(png: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        png.extend_from_slice(chunk_type);
        png.extend_from_slice(data);
        let start = png.len() - data.len() - chunk_type.len();
        let crc = png_crc32(&png[start..]);
        png.extend_from_slice(&crc.to_be_bytes());
    }

    fn structurally_valid_png() -> Vec<u8> {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        append_chunk(
            &mut png,
            b"IHDR",
            &[0, 0, 0, 4, 0, 0, 0, 3, 8, 6, 0, 0, 0],
        );
        append_chunk(&mut png, b"IDAT", &[0x78, 0x9c, 0x03, 0x00]);
        append_chunk(&mut png, b"IEND", &[]);
        png
    }

    #[test]
    fn png_validation_requires_complete_crc_checked_structure() {
        let png = structurally_valid_png();
        assert_eq!(png_dimensions(&png), Some((4, 3)));
        assert_eq!(png_dimensions(&png[..24]), None);

        let mut forged = png;
        forged[20] ^= 1;
        assert_eq!(png_dimensions(&forged), None);
    }

    #[test]
    fn cover_output_plan_accepts_one_png_and_rejects_unsafe_or_invalid_outputs() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(workspace.join("outputs")).expect("workspace");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("cover-output-test"),
        )
        .expect("paths");
        let task = json!({
            "id": "task:cover",
            "projectId": "project:cover",
            "panelId": "panel:typesetting"
        });
        let write_result = |relative_path: &str| {
            write_test_result(
                &workspace,
                &json!({
                    "schemaVersion": 2,
                    "outcome": "generated",
                    "summary": "cover",
                    "artifacts": [{
                        "role": "typesetting-cover",
                        "relativePath": relative_path
                    }]
                }),
            )
            .expect("execution result");
        };

        fs::write(workspace.join("outputs/cover.png"), structurally_valid_png())
            .expect("valid png");
        write_result("outputs/cover.png");
        let plan = build_typesetting_cover_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .expect("valid output");
        assert_eq!(plan.actions.len(), 1);

        write_result("../cover.png");
        assert!(build_typesetting_cover_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .is_err());

        write_result("outputs/cover.png");
        fs::write(workspace.join("outputs/cover.png"), []).expect("empty png");
        assert!(build_typesetting_cover_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .is_err());
        fs::write(
            workspace.join("outputs/cover.png"),
            [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
        )
        .expect("forged png");
        assert!(build_typesetting_cover_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .is_err());

        let oversized = fs::File::create(workspace.join("outputs/cover.png"))
            .expect("oversized output");
        oversized
            .set_len(crate::content::MAX_TEXT_FILE_BYTES as u64 + 1)
            .expect("sparse file");
        let error = build_typesetting_cover_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .expect_err("oversized output must fail");
        assert_eq!(error.code(), Some("content_too_large"));
    }
}
