use base64::Engine as _;

pub(crate) struct FinalizeExecutionUnitRequest<'a> {
    pub task: &'a Value,
    pub workspace: &'a Path,
    pub handler_key: &'a str,
    pub execution_bundle_hash: &'a str,
    pub attempt_id: &'a str,
    pub execution_generation: i64,
    pub lease_token: &'a str,
    pub execution_token: &'a str,
}

#[derive(Clone, Copy)]
enum RuntimeFinalizationPhase {
    Validating,
    Applying,
    Committing,
    Completed,
    Failed,
}

impl RuntimeFinalizationPhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Validating => "validating",
            Self::Applying => "applying",
            Self::Committing => "committing",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

pub(crate) fn finalize_execution_unit(
    paths: &MyOpenPanelsPaths,
    request: FinalizeExecutionUnitRequest<'_>,
) -> Result<Value, CliError> {
    let task_id = request
        .task
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    record_finalization_phase(
        task_id,
        request.handler_key,
        request.attempt_id,
        request.execution_generation,
        RuntimeFinalizationPhase::Validating,
        None,
        None,
    );
    let prepared = match prepare_task_output_plan(
        paths,
        request.task,
        request.workspace,
        request.handler_key,
        request.execution_bundle_hash,
        request.attempt_id,
        request.execution_generation,
    ) {
        Ok(prepared) => prepared,
        Err(error) => {
            return finish_execution_unit_error(
                paths,
                task_id,
                request.lease_token,
                request.handler_key,
                RuntimeFinalizationPhase::Validating,
                None,
                error,
            )
        }
    };

    let current = tasks::inspect_task(paths, task_id)?;
    if current.pointer("/task/status").and_then(Value::as_str) == Some("succeeded") {
        let stored_hash = current
            .pointer("/task/result/runtimeFinalization/outputPlanHash")
            .and_then(Value::as_str);
        if stored_hash == Some(prepared.plan.content_hash.as_str()) {
            return Ok(json!({
                "taskId": task_id,
                "status": "succeeded",
                "replayed": true,
                "finalizationState": finalization_state(RuntimeFinalizationPhase::Completed, request.handler_key, stored_hash, None),
                "result": current.pointer("/task/result").cloned().unwrap_or(Value::Null),
                "lifecycle": current,
            }));
        }
        return Err(CliError::with_code(
            "execution_fenced",
            "The Task already completed with a different Task Output Plan.",
        ));
    }

    let live = tasks::verify_task_lease(paths, task_id, request.lease_token)?;
    let live_generation = live
        .get("executionGeneration")
        .and_then(Value::as_i64)
        .or_else(|| {
            live.pointer("/task/executionGeneration")
                .and_then(Value::as_i64)
        });
    if live_generation != Some(request.execution_generation)
    {
        record_finalization_phase(
            task_id,
            request.handler_key,
            request.attempt_id,
            request.execution_generation,
            RuntimeFinalizationPhase::Failed,
            Some(&prepared.plan.content_hash),
            Some("execution_fenced"),
        );
        return Err(CliError::with_code(
            "execution_fenced",
            "The Task Output Plan belongs to an older Task execution.",
        ));
    }

    record_finalization_phase(
        task_id,
        request.handler_key,
        request.attempt_id,
        request.execution_generation,
        RuntimeFinalizationPhase::Applying,
        Some(&prepared.plan.content_hash),
        None,
    );
    let applied = match apply_task_output_plan(paths, request.execution_token, &prepared.plan) {
        Ok(applied) => applied,
        Err(error) => {
            return finish_execution_unit_error(
                paths,
                task_id,
                request.lease_token,
                request.handler_key,
                RuntimeFinalizationPhase::Applying,
                Some(&prepared.plan.content_hash),
                error,
            )
        }
    };
    let runtime_finalization = json!({
        "phase": RuntimeFinalizationPhase::Completed.as_str(),
        "handlerKey": request.handler_key,
        "outputPlanHash": prepared.plan.content_hash,
        "artifacts": applied.get("artifacts").cloned().unwrap_or_else(|| json!([])),
    });
    let mut result = prepared.result;
    result["runtimeFinalization"] = runtime_finalization.clone();
    if result.get("outcome").and_then(Value::as_str) == Some("no_change") {
        result["bridgeValidated"] = json!(true);
    }
    record_finalization_phase(
        task_id,
        request.handler_key,
        request.attempt_id,
        request.execution_generation,
        RuntimeFinalizationPhase::Committing,
        Some(&prepared.plan.content_hash),
        None,
    );
    match tasks::complete_task(paths, task_id, request.lease_token, Some(result.clone())) {
        Ok(lifecycle) => {
            let committed_result = lifecycle
                .pointer("/task/result")
                .cloned()
                .unwrap_or(result);
            record_finalization_phase(
                task_id,
                request.handler_key,
                request.attempt_id,
                request.execution_generation,
                RuntimeFinalizationPhase::Completed,
                Some(&prepared.plan.content_hash),
                None,
            );
            Ok(json!({
                "taskId": task_id,
                "status": "succeeded",
                "finalizationState": finalization_state(RuntimeFinalizationPhase::Completed, request.handler_key, Some(&prepared.plan.content_hash), None),
                "result": committed_result,
                "runtimeFinalization": runtime_finalization,
                "lifecycle": lifecycle,
            }))
        }
        Err(error) => {
            finish_execution_unit_error(
                paths,
                task_id,
                request.lease_token,
                request.handler_key,
                RuntimeFinalizationPhase::Committing,
                Some(&prepared.plan.content_hash),
                error,
            )
        }
    }
}

fn apply_task_output_plan(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    plan: &TaskOutputPlan,
) -> Result<Value, CliError> {
    let mut artifacts = Vec::new();
    for action in &plan.actions {
        match action {
            TaskOutputAction::StageText {
                resource_kind,
                resource_key,
                logical_path,
                artifact,
                mime_type,
                metadata,
            } => {
                let bytes = read_planned_artifact(artifact)?;
                let mut metadata = metadata.clone();
                metadata["runtimeOutputPlanHash"] = json!(plan.content_hash);
                let request = crate::content::StageFileRequest {
                        resource_kind: resource_kind.clone(),
                        resource_key: resource_key.clone(),
                        logical_path: logical_path.clone(),
                        content_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
                        mime_type: mime_type.clone(),
                        metadata,
                    };
                if resource_kind == crate::content::ResourceKind::MyDocument.as_str() {
                    crate::content::stage_runtime_validated_file(
                        paths,
                        execution_token,
                        &request,
                    )?;
                } else {
                    crate::content::stage_file(paths, execution_token, &request)?;
                }
                artifacts.push(finalized_artifact(artifact, Some(logical_path)));
            }
            TaskOutputAction::PrepareWritingSkill { skill_id, artifact } => {
                let bytes = read_planned_artifact(artifact)?;
                let source = String::from_utf8(bytes).map_err(|_| {
                    CliError::with_code("invalid_output", "Writing Skill must be valid UTF-8.")
                })?;
                crate::content::prepare_skill(
                    paths,
                    execution_token,
                    &crate::content::PrepareSkillRequest {
                        skill_id: skill_id.clone(),
                        source,
                        manifest: json!({ "runtimeOutputPlanHash": plan.content_hash }),
                    },
                )?;
                artifacts.push(finalized_artifact(artifact, Some("SKILL.md")));
            }
            TaskOutputAction::PrepareTypesettingCover {
                project_id,
                panel_id,
                task_id,
                artifact,
                width,
                height,
            } => {
                let bytes = read_planned_artifact(artifact)?;
                let storage = crate::storage::Storage::open(paths)?;
                let logical_path = cover_artifact_file_name(artifact)?;
                let requested = format!("cover-tasks/{task_id}/{logical_path}");
                let written = storage.write_asset_from_buffer(
                    project_id,
                    panel_id,
                    &requested,
                    &bytes,
                    true,
                )?;
                artifacts.push(json!({
                    "role": artifact.role,
                    "logicalPath": logical_path,
                    "contentHash": artifact.content_hash,
                    "sizeBytes": artifact.size_bytes,
                    "assetRef": written.asset_ref,
                    "resourceId": written.resource_id,
                    "fileName": written.file_name,
                    "mimeType": "image/png",
                    "width": width,
                    "height": height,
                }));
            }
            TaskOutputAction::PreparePublicationTitles {
                project_id,
                panel_id,
                task_id,
                artifact,
                titles,
            } => {
                let bytes = read_planned_artifact(artifact)?;
                let storage = crate::storage::Storage::open(paths)?;
                let requested = format!("title-tasks/{task_id}/titles.json");
                let written = storage.write_asset_from_buffer(
                    project_id,
                    panel_id,
                    &requested,
                    &bytes,
                    true,
                )?;
                artifacts.push(json!({
                    "role": artifact.role,
                    "logicalPath": "titles.json",
                    "contentHash": artifact.content_hash,
                    "sizeBytes": artifact.size_bytes,
                    "assetRef": written.asset_ref,
                    "resourceId": written.resource_id,
                    "fileName": written.file_name,
                    "mimeType": "application/json",
                    "titles": titles,
                }));
            }
            TaskOutputAction::PrepareTypesettingLayout {
                project_id,
                panel_id,
                task_id,
                artifact,
                content,
            } => {
                let bytes = read_planned_artifact(artifact)?;
                let storage = crate::storage::Storage::open(paths)?;
                let requested = format!("layout-tasks/{task_id}/content.json");
                let written = storage.write_asset_from_buffer(
                    project_id,
                    panel_id,
                    &requested,
                    &bytes,
                    true,
                )?;
                artifacts.push(json!({
                    "role": artifact.role,
                    "logicalPath": "content.json",
                    "contentHash": artifact.content_hash,
                    "sizeBytes": artifact.size_bytes,
                    "assetRef": written.asset_ref,
                    "resourceId": written.resource_id,
                    "fileName": written.file_name,
                    "mimeType": "application/json",
                    "content": content,
                }));
            }
        }
    }
    Ok(json!({ "artifacts": artifacts }))
}

fn read_planned_artifact(artifact: &TaskOutputArtifact) -> Result<Vec<u8>, CliError> {
    let bytes = fs::read(&artifact.absolute_path).map_err(to_cli_error)?;
    let hash = format!("sha256:{:x}", Sha256::digest(&bytes));
    if hash != artifact.content_hash || bytes.len() as u64 != artifact.size_bytes {
        return Err(CliError::with_code(
            "execution_fenced",
            "An execution artifact changed after Task Output Plan validation.",
        ));
    }
    Ok(bytes)
}

fn cover_artifact_file_name(artifact: &TaskOutputArtifact) -> Result<String, CliError> {
    Path::new(&artifact.relative_path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            CliError::with_code("invalid_output", "Cover artifact file name is missing.")
        })
}

fn finalized_artifact(artifact: &TaskOutputArtifact, logical_path: Option<&str>) -> Value {
    json!({
        "role": artifact.role,
        "logicalPath": logical_path.or(artifact.logical_path.as_deref()),
        "contentHash": artifact.content_hash,
        "sizeBytes": artifact.size_bytes,
    })
}

fn finish_execution_unit_error(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
    handler_key: &str,
    failed_at: RuntimeFinalizationPhase,
    output_plan_hash: Option<&str>,
    error: CliError,
) -> Result<Value, CliError> {
    record_finalization_phase(
        task_id,
        handler_key,
        "",
        0,
        RuntimeFinalizationPhase::Failed,
        output_plan_hash,
        error.code(),
    );
    if matches!(
        error.code(),
        Some("execution_fenced" | "lease_expired" | "task_not_claimed")
    ) {
        return Err(error);
    }
    if error.code() == Some("content_conflict") {
        let lifecycle = tasks::supersede_task_for_content_conflict(
            paths,
            task_id,
            lease_token,
            error.message(),
        )?;
        return Ok(json!({
            "taskId": task_id,
            "status": "superseded",
            "finalizationState": finalization_state(RuntimeFinalizationPhase::Failed, handler_key, output_plan_hash, Some(failed_at.as_str())),
            "error": { "code": "content_conflict", "message": error.message() },
            "lifecycle": lifecycle,
        }));
    }
    let output_error = matches!(
        error.code(),
        Some(
            "invalid_output"
                | "content_too_large"
                | "task_output_plan_conflict"
                | "writing_skill_file_invalid"
        )
    );
    let handler_error = matches!(
        error.code(),
        Some("task_handler_not_found" | "task_handler_mismatch")
    );
    let failure_class = if handler_error {
        tasks::TaskFailureClass::TerminalTask
    } else if output_error {
        tasks::TaskFailureClass::RetryableOutput
    } else {
        tasks::TaskFailureClass::RetryableChannel
    };
    let lifecycle = tasks::fail_task_with_class(
        paths,
        task_id,
        lease_token,
        error.message(),
        None,
        failure_class,
    )?;
    if output_error {
        tasks::mark_latest_attempt_invalid_output(paths, task_id, error.message())?;
    }
    Ok(json!({
        "taskId": task_id,
        "status": "failed",
        "finalizationState": finalization_state(RuntimeFinalizationPhase::Failed, handler_key, output_plan_hash, Some(failed_at.as_str())),
        "error": {
            "code": error.code().unwrap_or(if output_error { "invalid_output" } else { "runtime_finalization_failed" }),
            "message": error.message(),
        },
        "lifecycle": lifecycle,
    }))
}

fn finalization_state(
    phase: RuntimeFinalizationPhase,
    handler_key: &str,
    output_plan_hash: Option<&str>,
    failed_at: Option<&str>,
) -> Value {
    json!({
        "phase": phase.as_str(),
        "handlerKey": handler_key,
        "outputPlanHash": output_plan_hash,
        "failedAt": failed_at,
    })
}

fn record_finalization_phase(
    task_id: &str,
    handler_key: &str,
    attempt_id: &str,
    execution_generation: i64,
    phase: RuntimeFinalizationPhase,
    output_plan_hash: Option<&str>,
    error_code: Option<&str>,
) {
    crate::trace::record(crate::trace::TraceEventInput {
        audience: None,
        category: Some(if matches!(phase, RuntimeFinalizationPhase::Failed) {
            "error".to_owned()
        } else {
            "task".to_owned()
        }),
        detail: Some(json!({
            "phase": phase.as_str(),
            "handlerKey": handler_key,
            "attemptId": attempt_id,
            "executionGeneration": execution_generation,
            "outputPlanHash": output_plan_hash,
            "errorCode": error_code,
        })),
        direction: Some("runtime".to_owned()),
        release_summary: Some(format!("Task finalization: {}", phase.as_str())),
        run_id: None,
        source: Some("runtime-finalizer".to_owned()),
        summary: Some(format!("Task {task_id} finalization {}", phase.as_str())),
        task_id: Some(task_id.to_owned()),
    });
}
