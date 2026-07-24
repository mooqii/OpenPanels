fn build_conversion_output_plan(
    _paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
    _attempt_id: &str,
    _execution_generation: i64,
    _execution_unit: &Value,
) -> Result<TaskOutputPlanDraft, CliError> {
    let result = read_execution_result(workspace, "Document conversion")?;
    validate_result_keys(
        &result,
        &["outcome", "summary", "artifacts"],
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
    let (resource_kind, logical_path, document_kind) = conversion_output_target(task);
    Ok(TaskOutputPlanDraft {
        result,
        actions: vec![TaskOutputAction::StageText {
            resource_kind: resource_kind.to_owned(),
            resource_key: document_id.to_owned(),
            logical_path: logical_path.to_owned(),
            artifact,
            mime_type: "text/markdown".to_owned(),
            metadata: json!({ "documentId": document_id, "documentKind": document_kind }),
        }],
    })
}

fn build_my_document_write_output_plan(
    _paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
    _attempt_id: &str,
    _execution_generation: i64,
    _execution_unit: &Value,
) -> Result<TaskOutputPlanDraft, CliError> {
    let result = read_execution_result(workspace, "My Document write")?;
    validate_result_keys(
        &result,
        &["outcome", "summary", "title", "artifacts"],
        "My Document write",
    )?;
    require_outcome(&result, "written", "My Document write")?;
    let title = result
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CliError::with_code(
                "invalid_output",
                "My Document result title cannot be empty.",
            )
        })?
        .to_owned();
    let artifact = exactly_one_artifact(workspace, &result, "My Document write")?;
    if artifact.role != "my-document"
        || !matches!(
            artifact.relative_path.as_str(),
            "outputs/document.md" | "outputs/document.txt"
        )
        || artifact.logical_path.is_some()
    {
        return Err(CliError::with_code(
            "invalid_output",
            "My Document write must declare exactly outputs/document.md or outputs/document.txt.",
        ));
    }
    let document_id = task
        .pointer("/input/targetMyDocumentId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            CliError::with_code("invalid_output", "My Document target is missing.")
        })?;
    let document_format = if artifact.relative_path.ends_with(".txt") {
        "text"
    } else {
        "markdown"
    };
    let logical_path = if document_format == "text" {
        "content.txt"
    } else {
        "content.md"
    };
    let base_content_version = task
        .pointer("/input/targetContentVersion")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            CliError::with_code(
                "invalid_output",
                "My Document target version is missing.",
            )
        })?;
    let mode = task
        .pointer("/input/mode")
        .and_then(Value::as_str)
        .unwrap_or("create");
    Ok(TaskOutputPlanDraft {
        result,
        actions: vec![TaskOutputAction::StageText {
            resource_kind: crate::content::ResourceKind::MyDocument
                .as_str()
                .to_owned(),
            resource_key: document_id.to_owned(),
            logical_path: logical_path.to_owned(),
            artifact,
            mime_type: if document_format == "text" {
                "text/plain"
            } else {
                "text/markdown"
            }
            .to_owned(),
            metadata: json!({
                "documentId": document_id,
                "baseContentVersion": base_content_version,
                "mode": mode,
                "title": title,
                "fileName": logical_path,
            }),
        }],
    })
}

fn build_distillation_output_plan(
    _paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
    _attempt_id: &str,
    _execution_generation: i64,
    _execution_unit: &Value,
) -> Result<TaskOutputPlanDraft, CliError> {
    let result = read_execution_result(workspace, "Writing distillation")?;
    validate_result_keys(
        &result,
        &["outcome", "summary", "artifacts"],
        "Writing distillation",
    )?;
    require_outcome(&result, "distilled", "Writing distillation")?;
    let artifact = exactly_one_artifact(workspace, &result, "Writing distillation")?;
    validate_fixed_artifact(
        &artifact,
        "writing-skill",
        "outputs/SKILL.md",
        "Writing distillation",
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

fn build_publication_cover_output_plan(
    _paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
    _attempt_id: &str,
    _execution_generation: i64,
    _execution_unit: &Value,
) -> Result<TaskOutputPlanDraft, CliError> {
    let result = read_execution_result(workspace, "Publication Cover")?;
    validate_result_keys(
        &result,
        &["outcome", "summary", "artifacts"],
        "Publication Cover",
    )?;
    require_outcome(&result, "generated", "Publication Cover")?;
    let artifacts = publication_cover_artifacts(workspace, &result)?;
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
    let mut actions = Vec::with_capacity(artifacts.len());
    for artifact in artifacts {
        let bytes = fs::read(&artifact.absolute_path).map_err(to_cli_error)?;
        let (width, height) = png_dimensions(&bytes).ok_or_else(|| {
            CliError::with_code(
                "invalid_output",
                "Publication Cover output must contain only valid non-empty PNG bitmaps.",
            )
        })?;
        actions.push(TaskOutputAction::PrepareTypesettingCover {
            project_id: project_id.to_owned(),
            panel_id: panel_id.to_owned(),
            task_id: task_id.to_owned(),
            artifact,
            width,
            height,
        });
    }
    Ok(TaskOutputPlanDraft {
        result,
        actions,
    })
}

fn publication_cover_artifacts(
    workspace: &Path,
    result: &Value,
) -> Result<Vec<TaskOutputArtifact>, CliError> {
    let artifacts = read_artifacts_with_mode(workspace, result, "Publication Cover", false)?;
    if artifacts.is_empty() || artifacts.len() > MAX_PUBLICATION_COVER_ARTIFACTS {
        return Err(CliError::with_code(
            "invalid_output",
            format!(
                "Publication Cover must declare between 1 and {MAX_PUBLICATION_COVER_ARTIFACTS} image artifacts."
            ),
        ));
    }
    let mut seen = HashSet::new();
    for artifact in &artifacts {
        if artifact.role != "publication-cover"
            || artifact.logical_path.is_some()
            || !artifact.relative_path.ends_with(".png")
            || !seen.insert(artifact.relative_path.clone())
        {
            return Err(CliError::with_code(
                "invalid_output",
                "Publication Cover declared an unexpected image artifact.",
            ));
        }
    }
    Ok(artifacts)
}

fn build_publication_title_output_plan(
    _paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
    _attempt_id: &str,
    _execution_generation: i64,
    _execution_unit: &Value,
) -> Result<TaskOutputPlanDraft, CliError> {
    let result = read_execution_result(workspace, "Publication Title")?;
    validate_result_keys(
        &result,
        &["outcome", "summary", "artifacts"],
        "Publication Title",
    )?;
    require_outcome(&result, "generated", "Publication Title")?;
    let artifact = exactly_one_artifact(workspace, &result, "Publication Title")?;
    validate_fixed_artifact(
        &artifact,
        "publication-titles",
        "outputs/titles.json",
        "Publication Title",
    )?;
    let payload: Value = serde_json::from_slice(
        &fs::read(&artifact.absolute_path).map_err(to_cli_error)?,
    )
    .map_err(|_| {
        CliError::with_code(
            "invalid_output",
            "Publication Title output must be valid JSON.",
        )
    })?;
    validate_result_keys(&payload, &["titles"], "Publication Title artifact")?;
    let values = payload
        .get("titles")
        .and_then(Value::as_array)
        .filter(|titles| !titles.is_empty())
        .ok_or_else(|| {
            CliError::with_code(
                "invalid_output",
                "Publication Title output must contain one or more titles.",
            )
        })?;
    let existing = task
        .pointer("/input/snapshot/existingTitles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|title| title.trim().to_lowercase())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut titles = Vec::with_capacity(values.len());
    for value in values {
        let title = value.as_str().map(str::trim).filter(|title| {
            !title.is_empty()
                && title.chars().count() <= 200
                && !title.chars().any(char::is_control)
        });
        let Some(title) = title else {
            return Err(CliError::with_code(
                "invalid_output",
                "Every generated title must be a non-empty string of at most 200 characters.",
            ));
        };
        let normalized = title.to_lowercase();
        if existing.contains(&normalized) || !seen.insert(normalized) {
            return Err(CliError::with_code(
                "invalid_output",
                "Generated titles must be distinct and must not repeat an existing title.",
            ));
        }
        titles.push(title.to_owned());
    }
    let task_id = task
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliError::with_code("invalid_output", "Title Task id is missing."))?;
    let project_id = task
        .get("projectId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliError::with_code("invalid_target", "Title Project id is missing."))?;
    let panel_id = task
        .get("panelId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliError::with_code("invalid_target", "Title panel id is missing."))?;
    Ok(TaskOutputPlanDraft {
        result,
        actions: vec![TaskOutputAction::PreparePublicationTitles {
            project_id: project_id.to_owned(),
            panel_id: panel_id.to_owned(),
            task_id: task_id.to_owned(),
            artifact,
            titles,
        }],
    })
}

include!("publication_layout_output.rs");
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
    let result = read_execution_result(workspace, "Wiki")?;
    let is_ingestion = task.get("type").and_then(Value::as_str)
        == Some("ingest_markdown_into_wiki");
    validate_result_keys(
        &result,
        if is_ingestion {
            &[
                "outcome",
                "disposition",
                "reasonCode",
                "summary",
                "changedPaths",
                "artifacts",
            ]
        } else {
            &["outcome", "summary", "changedPaths", "artifacts"]
        },
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
    if is_ingestion {
        let disposition = result
            .get("disposition")
            .and_then(Value::as_str)
            .unwrap_or("");
        let valid_disposition = match outcome {
            "changed" => disposition == "included",
            "no_change" => matches!(disposition, "already_covered" | "excluded"),
            _ => false,
        };
        if !valid_disposition {
            return Err(CliError::with_code(
                "invalid_output",
                "Wiki ingestion disposition does not match its outcome.",
            ));
        }
        let reason_code = result.get("reasonCode");
        if disposition == "excluded" {
            let valid_reason = matches!(
                reason_code.and_then(Value::as_str),
                Some(
                    "not_relevant"
                        | "insufficient_content"
                        | "unsupported_by_wiki_skill"
                        | "policy_excluded"
                )
            );
            if !valid_reason {
                return Err(CliError::with_code(
                    "invalid_output",
                    "Filtered Wiki ingestion requires a supported reasonCode.",
                ));
            }
        } else if !reason_code.is_some_and(Value::is_null) {
            return Err(CliError::with_code(
                "invalid_output",
                "Only filtered Wiki ingestion may include a reasonCode.",
            ));
        }
    }
    let wiki_space_id = task
        .get("wikiSpaceId")
        .and_then(Value::as_str)
        .or_else(|| task.pointer("/input/wikiSpaceId").and_then(Value::as_str))
        .or_else(|| task.pointer("/source/wikiSpaceId").and_then(Value::as_str))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            CliError::with_code(
                "invalid_task_input",
                "Wiki Task has no target Wiki Space.",
            )
        })?
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
    let result = read_execution_result(workspace, label)?;
    validate_result_keys(
        &result,
        &[
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
        let state = crate::release::normalize_state(
            crate::storage::Storage::open(paths)?
                .read_panel_state(project_id, panel_id)?
                .unwrap_or_else(crate::release::empty_state),
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

fn read_execution_result(workspace: &Path, label: &str) -> Result<Value, CliError> {
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
fn write_test_result(workspace: &Path, result: &Value) -> Result<(), CliError> {
    fs::write(
        workspace.join(EXECUTION_RESULT_FILE),
        serde_json::to_vec(result).map_err(to_cli_error)?,
    )
    .map_err(to_cli_error)
}


#[cfg(test)]
include!("result_validation_tests.rs");
