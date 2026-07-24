use super::*;

pub fn hash_secret(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}

pub(crate) fn authorize(
    paths: &MyOpenPanelsPaths,
    token: &str,
) -> Result<ExecutionContext, CliError> {
    let storage = Storage::open(paths)?;
    let token_hash = hash_secret(token);
    let row = storage
        .connection()
        .query_row(
            r#"
            SELECT t.id, t.project_id, COALESCE(t.origin_panel_id, ''), t.handler_key,
                   t.execution_generation, t.input_json
            FROM tasks t
            WHERE t.execution_token_hash = ? AND t.status = 'running'
              AND (t.lease_expires_at IS NULL OR t.lease_expires_at > ?)
            "#,
            params![token_hash, now_iso()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()
        .map_err(to_cli_error)?
        .ok_or_else(|| {
            CliError::with_code("execution_fenced", "Execution token is invalid or expired.")
        })?;
    let route = crate::capabilities::task_route_for_handler(&row.3)?.ok_or_else(|| {
        CliError::with_code("task_handler_not_found", "Task handler is unavailable.")
    })?;
    Ok(ExecutionContext {
        task_id: row.0,
        project_id: row.1,
        panel_id: row.2,
        queue: route.queue.clone(),
        task_type: route.task_type.clone(),
        task_capability: route.capability.clone(),
        generation: row.4,
        input: serde_json::from_str(&row.5).unwrap_or_else(|_| json!({})),
    })
}

pub fn authorize_agent_broker_capability(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    capability: &str,
) -> Result<(), CliError> {
    let context = authorize(paths, execution_token)?;
    if crate::bridge::task_handler_allows_agent_broker_capability(
        &context.queue,
        &context.task_type,
        &context.task_capability,
        capability,
    ) {
        Ok(())
    } else {
        Err(CliError::with_code(
            "task_handler_command_not_allowed",
            format!("Task Handler does not allow Agent-side Broker capability {capability}."),
        ))
    }
}

pub fn create_execution_context_in_transaction(
    tx: &Transaction<'_>,
    task_id: &str,
    _attempt_id: &str,
    generation: i64,
    _expires_at: &str,
) -> Result<(String, String), CliError> {
    let token = format!(
        "execution:{:032x}{:032x}",
        rand::random::<u128>(),
        rand::random::<u128>()
    );
    let changed = tx
        .execute(
            "UPDATE tasks SET execution_token_hash = ? WHERE id = ? AND execution_generation = ? AND status = 'running'",
            params![hash_secret(&token), task_id, generation],
        )
        .map_err(to_cli_error)?;
    if changed != 1 {
        return Err(CliError::with_code(
            "execution_fenced",
            "The active Task execution no longer exists.",
        ));
    }
    Ok((token, format!("task:{task_id}:{generation}")))
}

pub fn abandon_task_staging_in_transaction(
    tx: &rusqlite::Connection,
    task_id: &str,
    _now: &str,
) -> Result<(), CliError> {
    tx.execute(
        "UPDATE tasks SET execution_token_hash = NULL WHERE id = ?",
        [task_id],
    )
    .map_err(to_cli_error)?;
    Ok(())
}

pub fn stage_file(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &StageFileRequest,
) -> Result<Value, CliError> {
    stage_file_internal(paths, execution_token, request, false)
}

pub(crate) fn stage_runtime_validated_file(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &StageFileRequest,
) -> Result<Value, CliError> {
    stage_file_internal(paths, execution_token, request, true)
}

pub(crate) fn stage_file_internal(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &StageFileRequest,
    domain_validated: bool,
) -> Result<Value, CliError> {
    let kind = ResourceKind::parse(&request.resource_kind)?;
    if kind == ResourceKind::Asset
        || (!domain_validated
            && matches!(kind, ResourceKind::MyDocument | ResourceKind::WritingSkill))
    {
        return Err(CliError::with_code(
            "invalid_broker_route",
            "This resource kind must use its domain preparation endpoint.",
        ));
    }
    validate_logical_path(&request.logical_path)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&request.content_base64)
        .map_err(|_| {
            CliError::with_code("invalid_output", "Staged content is not valid base64.")
        })?;
    if bytes.len() > MAX_TEXT_FILE_BYTES || std::str::from_utf8(&bytes).is_err() {
        return Err(CliError::with_code(
            "content_too_large",
            "Staged content must be bounded UTF-8 text.",
        ));
    }
    let context = authorize(paths, execution_token)?;
    let resource_dir = staging_resource_dir(paths, &context, kind, &request.resource_key);
    fs::create_dir_all(&resource_dir).map_err(to_cli_error)?;
    let metadata_path = resource_dir.join("resource.json");
    if !metadata_path.exists() {
        let active =
            read_authoritative_pointer(paths, &context.project_id, kind, &request.resource_key)?;
        write_json_atomic(
            &metadata_path,
            &StagedResource {
                project_id: context.project_id.clone(),
                panel_id: context.panel_id.clone(),
                resource_kind: kind.as_str().to_owned(),
                resource_key: request.resource_key.clone(),
                base_revision_id: active.as_ref().map(|value| value.revision_id.clone()),
                base_content_version: active.as_ref().map_or(0, |value| value.content_version),
                metadata: request.metadata.clone(),
            },
        )?;
    }
    let previous = read_staged_files(&resource_dir)?
        .into_iter()
        .find(|file| file.logical_path == request.logical_path)
        .map(|file| {
            let bytes = fs::read(staged_file_path(&resource_dir, &file)?).map_err(to_cli_error)?;
            Ok::<_, CliError>((file, bytes))
        })
        .transpose()?;
    let destination = write_staged_file(
        &resource_dir,
        &request.logical_path,
        &bytes,
        &request.mime_type,
        request.metadata.clone(),
    )?;
    let total = directory_size(&staging_task_dir(paths, &context))?;
    if total > MAX_STAGING_BYTES as u64 {
        if let Some((previous, bytes)) = previous {
            write_staged_file(
                &resource_dir,
                &previous.logical_path,
                &bytes,
                &previous.mime_type,
                previous.metadata,
            )?;
        } else {
            let _ = fs::remove_file(&destination);
            let mut staged = read_staged_files(&resource_dir)?;
            staged.retain(|file| file.logical_path != request.logical_path);
            write_json_atomic(&staged_files_manifest(&resource_dir), &staged)?;
        }
        return Err(CliError::with_code(
            "content_too_large",
            format!("An Attempt cannot stage more than {MAX_STAGING_BYTES} bytes."),
        ));
    }
    let content_hash = hash_bytes(&bytes);
    Ok(json!({
        "staged": true,
        "taskId": context.task_id,
        "executionGeneration": context.generation,
        "resourceKind": kind.as_str(),
        "resourceKey": request.resource_key,
        "logicalPath": request.logical_path,
        "contentHash": content_hash,
        "sizeBytes": bytes.len(),
    }))
}

pub fn read_file(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &ReadFileRequest,
) -> Result<Value, CliError> {
    let kind = ResourceKind::parse(&request.resource_kind)?;
    if kind == ResourceKind::Asset {
        return Err(CliError::with_code(
            "invalid_broker_route",
            "Assets must use their domain content endpoint.",
        ));
    }
    validate_logical_path(&request.logical_path)?;
    let context = authorize(paths, execution_token)?;
    let stage_dir = staging_resource_dir(paths, &context, kind, &request.resource_key);
    let staged = read_staged_files(&stage_dir)?
        .into_iter()
        .find(|file| file.logical_path == request.logical_path);
    let (bytes, mime_type) = if let Some(staged) = staged {
        (
            fs::read(staged_file_path(&stage_dir, &staged)?).map_err(to_cli_error)?,
            staged.mime_type,
        )
    } else {
        let snapshot = resource_snapshot_for_task(
            paths,
            &context.project_id,
            kind,
            &request.resource_key,
            &context.input,
        )?
        .ok_or_else(|| CliError::with_code("content_not_found", "Content resource not found."))?;
        let file = snapshot
            .files
            .into_iter()
            .find(|file| file.logical_path == request.logical_path)
            .ok_or_else(|| CliError::with_code("content_not_found", "Content file not found."))?;
        (file.bytes, file.mime_type)
    };
    Ok(json!({
        "resourceKind": kind.as_str(),
        "resourceKey": request.resource_key,
        "logicalPath": request.logical_path,
        "mimeType": mime_type,
        "contentBase64": base64::engine::general_purpose::STANDARD.encode(&bytes),
        "contentHash": hash_bytes(&bytes),
    }))
}

pub fn publishing_checkpoint(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &PublishingCheckpointRequest,
) -> Result<Value, CliError> {
    let context = authorize(paths, execution_token)?;
    if context.task_id != request.task_id
        || !crate::release::is_publishing_task_type(&context.task_type)
    {
        return Err(CliError::with_code(
            "execution_fenced",
            "Execution cannot checkpoint this Task.",
        ));
    }
    crate::release::checkpoint_attempt_for_broker(paths, &request.task_id, &request.phase)
}

pub fn read_task_context(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &TaskContextRequest,
) -> Result<Value, CliError> {
    let context = authorize(paths, execution_token)?;
    if context.task_id != request.task_id {
        return Err(CliError::with_code(
            "execution_fenced",
            "Execution token belongs to another Task.",
        ));
    }
    match request.context_kind.as_str() {
        "writing_request" => crate::writing::read_request(paths, &request.task_id),
        "writing_distillation" => crate::writing::read_distillation(paths, &request.task_id),
        _ => Err(CliError::with_code(
            "invalid_task_context",
            "Unsupported Task context request.",
        )),
    }
}

pub fn read_skill(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &SkillReadRequest,
) -> Result<Value, CliError> {
    let context = authorize(paths, execution_token)?;
    if context.task_id != request.task_id {
        return Err(CliError::with_code(
            "execution_fenced",
            "Execution token belongs to another Task.",
        ));
    }
    serde_json::to_value(crate::agent::read_agent_skill(
        paths,
        &request.skill_id,
        Some(&request.task_id),
    )?)
    .map_err(to_cli_error)
}

pub fn prepare_skill(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &PrepareSkillRequest,
) -> Result<Value, CliError> {
    let context = authorize(paths, execution_token)?;
    let expected_id = context
        .input
        .get("skillId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if request.skill_id != expected_id {
        return Err(CliError::with_code(
            "writing_skill_file_invalid",
            "Generated Writing Skill does not match the claimed Task.",
        ));
    }
    crate::agent::validate_portable_writing_skill(&request.source, "SKILL.md", expected_id)?;
    let mut manifest = request.manifest.clone();
    manifest["source"] = json!("custom");
    manifest["skillId"] = json!(expected_id);
    manifest["name"] = context
        .input
        .get("name")
        .cloned()
        .unwrap_or_else(|| json!(expected_id));
    manifest["binding"] = json!({ "moduleKinds": ["writing"] });
    manifest["originProjectId"] = json!(context.project_id);
    manifest["taskId"] = json!(context.task_id);
    let first = stage_file_internal(
        paths,
        execution_token,
        &StageFileRequest {
            resource_kind: ResourceKind::WritingSkill.as_str().to_owned(),
            resource_key: request.skill_id.clone(),
            logical_path: "SKILL.md".to_owned(),
            content_base64: base64::engine::general_purpose::STANDARD
                .encode(request.source.as_bytes()),
            mime_type: "text/markdown".to_owned(),
            metadata: manifest.clone(),
        },
        true,
    )?;
    stage_file_internal(
        paths,
        execution_token,
        &StageFileRequest {
            resource_kind: ResourceKind::WritingSkill.as_str().to_owned(),
            resource_key: request.skill_id.clone(),
            logical_path: "manifest.json".to_owned(),
            content_base64: base64::engine::general_purpose::STANDARD
                .encode(serde_json::to_vec_pretty(&manifest).map_err(to_cli_error)?),
            mime_type: "application/json".to_owned(),
            metadata: manifest,
        },
        true,
    )?;
    Ok(json!({ "skillId": request.skill_id, "staged": true, "contentHash": first["contentHash"] }))
}
