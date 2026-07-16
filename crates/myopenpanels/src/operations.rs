use crate::agent::{read_agent_skill, CANVAS_PANEL_SKILL_ID};
use crate::canvas::{
    insert_image_for_target, insert_placeholder_for_target, update_placeholder_for_target,
    InsertImageInput, InsertPlaceholderInput,
};
use crate::control::{now_iso, read_project_bootstrap, require_active_panel, BootstrapRequest};
use crate::error::CliError;
use crate::paths::{sanitize_path_part, MyOpenPanelsPaths};
use crate::selection::{read_selection, read_selection_asset_to_file};
use crate::storage::Storage;
use crate::types::PanelKind;
use crate::wiki;
use base64::Engine;
use rand::Rng;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

pub const OPERATION_PROTOCOL_VERSION: i64 = 2;
const TERMINAL_OPERATION_ARTIFACT_RETENTION_MINUTES: i64 = 30;

fn operation_id() -> String {
    let random: u128 = rand::rng().random();
    format!("operation:{random:032x}")
}

pub fn list(paths: &MyOpenPanelsPaths, status: Option<&str>) -> Result<Value, CliError> {
    let operations =
        Storage::open(paths)?.list_agent_operations(Some(&paths.context_id), status)?;
    Ok(json!({ "operations": operations }))
}

pub fn inspect(paths: &MyOpenPanelsPaths, id: &str) -> Result<Value, CliError> {
    Storage::open(paths)?
        .read_agent_operation(id)?
        .ok_or_else(|| {
            CliError::with_code(
                "operation_not_found",
                format!("Agent operation not found: {id}"),
            )
        })
}

fn active_operation(paths: &MyOpenPanelsPaths, id: &str, intent: &str) -> Result<Value, CliError> {
    let operation = inspect(paths, id)?;
    if operation["intent"].as_str() != Some(intent) {
        return Err(CliError::with_code(
            "operation_intent_mismatch",
            format!("Operation {id} is not {intent}"),
        ));
    }
    if operation["status"].as_str() != Some("active")
        && operation["status"].as_str() != Some("failed")
    {
        return Err(CliError::with_code(
            "operation_not_active",
            format!("Operation {id} is not active"),
        ));
    }
    Ok(operation)
}

fn save(paths: &MyOpenPanelsPaths, operation: &Value) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    storage.write_agent_operation(operation)?;
    cleanup_artifacts_with_storage(paths, &storage, chrono::Utc::now());
    Ok(())
}

/// Best-effort cleanup of files that can no longer be used by an Operation.
pub fn cleanup_operation_artifacts(paths: &MyOpenPanelsPaths) {
    let Ok(storage) = Storage::open(paths) else {
        return;
    };
    cleanup_artifacts_with_storage(paths, &storage, chrono::Utc::now());
}

fn cleanup_artifacts_with_storage(
    paths: &MyOpenPanelsPaths,
    storage: &Storage,
    now: chrono::DateTime<chrono::Utc>,
) {
    let cutoff = (now - chrono::Duration::minutes(TERMINAL_OPERATION_ARTIFACT_RETENTION_MINUTES))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let Ok(operation_ids) = storage.list_terminal_agent_operation_ids_before(&cutoff) else {
        return;
    };
    let operations_dir = paths.storage_dir.join("operations");
    for operation_id in operation_ids {
        let operation_dir = operations_dir.join(sanitize_path_part(&operation_id));
        let Ok(metadata) = fs::symlink_metadata(&operation_dir) else {
            continue;
        };
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            let _ = fs::remove_dir_all(operation_dir);
        }
    }
}

fn finish(operation: &mut Value, status: &str, result: Value, error: Value) {
    let now = now_iso();
    operation["status"] = json!(status);
    operation["result"] = result;
    operation["error"] = error;
    operation["updatedAt"] = json!(now);
    operation["completedAt"] = if status == "active" {
        Value::Null
    } else {
        json!(now)
    };
}

pub fn begin_canvas(
    paths: &MyOpenPanelsPaths,
    width: Option<f64>,
    height: Option<f64>,
    use_selection: bool,
    text: Option<&str>,
) -> Result<Value, CliError> {
    let mut request = BootstrapRequest::new();
    request.requested_panel_kind = Some(PanelKind::Canvas);
    let bootstrap = read_project_bootstrap(paths, request)?;
    let id = operation_id();
    let mut selected_ids = Vec::new();
    let mut reference = Value::Null;
    if use_selection {
        require_active_panel(paths, PanelKind::Canvas, None)?;
        let selection = read_selection(paths, None, false)?;
        if !selection
            .selection
            .get("isExplicitSelection")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(CliError::with_code(
                "explicit_selection_required",
                "Select the intended Canvas item before starting a reference-based generation.",
            ));
        }
        selected_ids = selection
            .selection
            .get("selectedShapeIds")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        let operation_dir = paths
            .storage_dir
            .join("operations")
            .join(sanitize_path_part(&id));
        fs::create_dir_all(&operation_dir).map_err(to_cli_error)?;
        let output = operation_dir.join("reference.png");
        let exported = read_selection_asset_to_file(
            paths,
            None,
            output.to_str().unwrap_or("reference.png"),
            false,
        )?;
        reference = json!({
            "shapeIds": selected_ids,
            "assetRef": exported.asset_ref,
            "path": exported.output_path,
            "mimeType": exported.mime_type,
        });
    }
    let placeholder = insert_placeholder_for_target(
        paths,
        &bootstrap.project.id,
        &bootstrap.panel.id,
        InsertPlaceholderInput {
            anchor_shape_id: selected_ids.first().map(String::as_str),
            display_height: height,
            display_width: width,
            text,
        },
        false,
    )?;
    let now = now_iso();
    let panel_skill = read_agent_skill(paths, CANVAS_PANEL_SKILL_ID, None)?;
    let operation = json!({
        "id": id,
        "ownerContextId": paths.context_id,
        "intent": "canvas.image.generate",
        "status": "active",
        "projectId": bootstrap.project.id,
        "projectTitle": bootstrap.project.title,
        "panelId": bootstrap.panel.id,
        "panelTitle": bootstrap.panel.title,
        "panelKind": "canvas",
        "skillId": CANVAS_PANEL_SKILL_ID,
        "guideId": null,
        "protocolVersion": OPERATION_PROTOCOL_VERSION,
        "target": {
            "placeholderShapeId": placeholder.shape_id,
            "bounds": placeholder.bounds,
            "reference": reference,
        },
        "input": { "displayWidth": width, "displayHeight": height, "useSelection": use_selection, "workflowSkillId": null },
        "result": null,
        "error": null,
        "createdAt": now,
        "updatedAt": now,
        "completedAt": null,
    });
    save(paths, &operation)?;
    Ok(
        json!({ "operation": operation, "panelSkill": panel_skill, "nextAction": "Read the Canvas panel Skill and its relevant references, generate the bitmap, then run operation complete with the captured operation id." }),
    )
}

pub fn complete_canvas(
    paths: &MyOpenPanelsPaths,
    id: &str,
    image: &str,
    mut metadata: Value,
) -> Result<Value, CliError> {
    let mut operation = active_operation(paths, id, "canvas.image.generate")?;
    let prompt = metadata
        .pointer("/generateOptions/prompt")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if prompt.is_empty() {
        return Err(CliError::with_code(
            "generation_metadata_required",
            "Image generation metadata must include generateOptions.prompt.",
        ));
    }
    if !metadata.is_object() {
        return Err(CliError::with_code(
            "generation_metadata_required",
            "Image generation metadata must be a JSON object.",
        ));
    }
    metadata["generatedBy"] = metadata
        .get("generatedBy")
        .cloned()
        .unwrap_or_else(|| json!("agent"));
    if !operation
        .pointer("/target/reference")
        .is_none_or(Value::is_null)
    {
        let references = metadata
            .pointer_mut("/generateOptions/referenceImages")
            .and_then(Value::as_array_mut);
        if let Some(references) = references {
            if references.is_empty() {
                references.push(operation["target"]["reference"].clone());
            }
        } else if let Some(options) = metadata
            .get_mut("generateOptions")
            .and_then(Value::as_object_mut)
        {
            options.insert(
                "referenceImages".to_owned(),
                json!([operation["target"]["reference"].clone()]),
            );
        }
    }
    let project_id = operation["projectId"].as_str().unwrap_or_default();
    let panel_id = operation["panelId"].as_str().unwrap_or_default();
    let placeholder = operation
        .pointer("/target/placeholderShapeId")
        .and_then(Value::as_str);
    let inserted = match insert_image_for_target(
        paths,
        project_id,
        panel_id,
        InsertImageInput {
            anchor_shape_id: None,
            display_height: operation
                .pointer("/input/displayHeight")
                .and_then(Value::as_f64),
            display_width: operation
                .pointer("/input/displayWidth")
                .and_then(Value::as_f64),
            file_name: None,
            image_path: image,
            metadata: Some(metadata),
            placement: Some("auto"),
            replace_shape_id: placeholder,
        },
        false,
    ) {
        Ok(inserted) => inserted,
        Err(error) => {
            if error.code() == Some("target_not_found") {
                finish(
                    &mut operation,
                    "failed",
                    Value::Null,
                    json!({ "code": error.code(), "message": error.message() }),
                );
                save(paths, &operation)?;
            }
            return Err(error);
        }
    };
    let result = serde_json::to_value(&inserted).map_err(to_cli_error)?;
    finish(&mut operation, "completed", result.clone(), Value::Null);
    save(paths, &operation)?;
    Ok(json!({ "operation": operation, "image": result }))
}

pub fn finish_canvas(
    paths: &MyOpenPanelsPaths,
    id: &str,
    status: &str,
    message: Option<&str>,
) -> Result<Value, CliError> {
    let mut operation = active_operation(paths, id, "canvas.image.generate")?;
    let project_id = operation["projectId"].as_str().unwrap_or_default();
    let panel_id = operation["panelId"].as_str().unwrap_or_default();
    let placeholder = operation
        .pointer("/target/placeholderShapeId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    update_placeholder_for_target(
        paths,
        project_id,
        panel_id,
        placeholder,
        message,
        status == "cancelled",
    )?;
    finish(
        &mut operation,
        status,
        Value::Null,
        message
            .map(|m| json!({"message": m}))
            .unwrap_or(Value::Null),
    );
    save(paths, &operation)?;
    Ok(operation)
}

pub fn begin_wiki(
    paths: &MyOpenPanelsPaths,
    title: &str,
    format: &str,
    document_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut request = BootstrapRequest::new();
    request.requested_panel_kind = Some(PanelKind::Wiki);
    let bootstrap = read_project_bootstrap(paths, request)?;
    let id = operation_id();
    let is_update = document_id.is_some();
    let started = wiki::begin_generated_document_for_target(
        paths,
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &id,
        title,
        format,
        document_id,
        false,
    )?;
    let document_id = started["document"]["id"].as_str().unwrap_or_default();
    let now = now_iso();
    let panel_skill = read_agent_skill(paths, wiki::WIKI_PANEL_SKILL_ID, None)?;
    let operation = json!({
        "id": id, "ownerContextId": paths.context_id,
        "intent": "wiki.document.generate", "status": "active",
        "projectId": bootstrap.project.id, "projectTitle": bootstrap.project.title,
        "panelId": bootstrap.panel.id, "panelTitle": bootstrap.panel.title, "panelKind": "wiki",
        "skillId": wiki::WIKI_PANEL_SKILL_ID, "guideId": null, "protocolVersion": OPERATION_PROTOCOL_VERSION,
        "target": { "documentId": document_id, "baseContentVersion": started["baseContentVersion"] },
        "input": { "title": title, "format": format, "mode": if is_update { "update" } else { "create" } },
        "result": null, "error": null, "createdAt": now, "updatedAt": now, "completedAt": null,
    });
    save(paths, &operation)?;
    Ok(
        json!({ "operation": operation, "panelSkill": panel_skill, "document": started["document"], "nextAction": "Read the Wiki panel Skill and generated-document reference, write the result file, then run operation complete with the captured operation id." }),
    )
}

pub fn begin_writing(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    title: &str,
    format: &str,
) -> Result<Value, CliError> {
    if crate::content::broker_execution_available() {
        return crate::content::broker_begin_operation(&crate::content::BeginOperationRequest {
            task_id: task_id.to_owned(),
            title: title.to_owned(),
            document_format: format.to_owned(),
        });
    }
    crate::tasks::verify_task_write_access(paths, task_id)?;
    begin_writing_for_broker(paths, task_id, title, format)
}

pub(crate) fn begin_writing_for_broker(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    title: &str,
    format: &str,
) -> Result<Value, CliError> {
    let request = crate::writing::read_request(paths, task_id)?;
    let task = &request["task"];
    if !matches!(
        task.get("status").and_then(Value::as_str),
        Some("reserved" | "running" | "claimed")
    ) {
        return Err(CliError::with_code(
            "task_not_claimed",
            "Claim the writing task before beginning generation.",
        ));
    }
    let project_id = task["projectId"].as_str().unwrap_or_default();
    let wiki_panel_id = task["source"]["wikiPanelId"].as_str().unwrap_or_default();
    if project_id.is_empty() || wiki_panel_id.is_empty() {
        return Err(CliError::with_code(
            "writing_target_not_found",
            "The writing task has no captured Wiki target.",
        ));
    }
    let mode = task["input"]["mode"].as_str().unwrap_or("create");
    let document_id = task["input"]["targetGeneratedDocumentId"].as_str();
    let storage = Storage::open(paths)?;
    let project = storage.read_project(project_id)?.ok_or_else(|| {
        CliError::with_code(
            "target_not_found",
            format!("Project not found: {project_id}"),
        )
    })?;
    let panel = storage
        .read_panel(project_id, wiki_panel_id)?
        .ok_or_else(|| CliError::with_code("target_not_found", "Wiki panel not found."))?;
    if let Some(document_id) = document_id {
        let expected_version = task["input"]["targetContentVersion"]
            .as_u64()
            .ok_or_else(|| {
                CliError::with_code(
                    "writing_target_version_missing",
                    "The writing task has no captured target content version.",
                )
            })?;
        let wiki_state = storage
            .read_panel_state(project_id, wiki_panel_id)?
            .ok_or_else(|| CliError::with_code("target_not_found", "Wiki state not found."))?;
        let current_version = wiki_state
            .get("generatedDocuments")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|document| document.get("id").and_then(Value::as_str) == Some(document_id))
            .and_then(|document| document.get("contentVersion"))
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                CliError::with_code(
                    "writing_target_not_found",
                    format!("Generated document not found: {document_id}"),
                )
            })?;
        if current_version != expected_version {
            crate::tasks::supersede_task_for_content_conflict(paths, task_id, document_id)?;
            return Err(CliError::with_code(
                "content_conflict",
                format!(
                    "Generated document changed from version {expected_version} to {current_version}"
                ),
            ));
        }
    }
    let id = operation_id();
    let started = wiki::begin_generated_document_for_target(
        paths,
        project_id,
        wiki_panel_id,
        &id,
        title,
        format,
        document_id,
        mode == "create",
    )?;
    let generated_id = started["document"]["id"].as_str().unwrap_or_default();
    let now = now_iso();
    let panel_skill = read_agent_skill(paths, crate::writing::WRITING_PANEL_SKILL_ID, None)?;
    let operation = json!({
        "id": id,
        "ownerContextId": paths.context_id,
        "intent": "wiki.document.generate",
        "status": "active",
        "projectId": project.id,
        "projectTitle": project.title,
        "panelId": panel.id,
        "panelTitle": panel.title,
        "panelKind": "wiki",
        "skillId": crate::writing::WRITING_PANEL_SKILL_ID,
        "guideId": null,
        "protocolVersion": OPERATION_PROTOCOL_VERSION,
        "target": {
            "documentId": generated_id,
            "baseContentVersion": started["baseContentVersion"],
            "writingTaskId": task_id,
        },
        "input": { "title": title, "format": format, "mode": mode, "taskId": task_id },
        "result": null,
        "error": null,
        "createdAt": now,
        "updatedAt": now,
        "completedAt": null,
    });
    save(paths, &operation)?;
    Ok(json!({
        "operation": operation,
        "panelSkill": panel_skill,
        "document": started["document"],
        "nextAction": "Write the requested document, complete this Operation, then complete the writing Task.",
    }))
}

pub fn complete_wiki(paths: &MyOpenPanelsPaths, id: &str, file: &str) -> Result<Value, CliError> {
    if crate::content::broker_execution_available() {
        let content = fs::read(file).map_err(to_cli_error)?;
        let file_name = Path::new(file)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("document.md");
        return crate::content::broker_prepare_operation(
            &crate::content::PrepareOperationRequest {
                operation_id: id.to_owned(),
                file_name: file_name.to_owned(),
                content_base64: base64::engine::general_purpose::STANDARD.encode(content),
            },
        );
    }
    crate::content::require_broker_for_task_execution()?;
    let mut operation = active_operation(paths, id, "wiki.document.generate")?;
    let content = fs::read(file).map_err(to_cli_error)?;
    let file_name = Path::new(file)
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("document.md");
    let result = match wiki::complete_generated_document_for_target(
        paths,
        operation["projectId"].as_str().unwrap_or_default(),
        operation["panelId"].as_str().unwrap_or_default(),
        operation
            .pointer("/target/documentId")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        operation
            .pointer("/target/baseContentVersion")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        file_name,
        &content,
    ) {
        Ok(result) => result,
        Err(error) => {
            if error.code() == Some("target_not_found") {
                finish(
                    &mut operation,
                    "failed",
                    Value::Null,
                    json!({ "code": error.code(), "message": error.message() }),
                );
                save(paths, &operation)?;
            }
            return Err(error);
        }
    };
    finish(&mut operation, "completed", result.clone(), Value::Null);
    save(paths, &operation)?;
    Ok(json!({ "operation": operation, "document": result["document"] }))
}

pub fn finish_wiki(
    paths: &MyOpenPanelsPaths,
    id: &str,
    status: &str,
    message: Option<&str>,
) -> Result<Value, CliError> {
    let mut operation = active_operation(paths, id, "wiki.document.generate")?;
    wiki::finish_generated_document_operation(
        paths,
        operation["projectId"].as_str().unwrap_or_default(),
        operation["panelId"].as_str().unwrap_or_default(),
        operation
            .pointer("/target/documentId")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        status,
        message,
    )?;
    finish(
        &mut operation,
        status,
        Value::Null,
        message
            .map(|m| json!({"message": m}))
            .unwrap_or(Value::Null),
    );
    save(paths, &operation)?;
    Ok(operation)
}

pub fn retry_wiki_document(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
) -> Result<Value, CliError> {
    let context = wiki::wiki_context(paths)?;
    let project_id = context["project"]["id"].as_str().unwrap_or_default();
    let panel_id = context["panel"]["id"].as_str().unwrap_or_default();
    let generated = wiki::read_generated_document(paths, document_id)?;
    let document = &generated["document"];
    if document
        .pointer("/generation/status")
        .and_then(Value::as_str)
        != Some("failed")
    {
        return Err(CliError::with_code(
            "generation_not_failed",
            "Only a failed generated document can be retried.",
        ));
    }

    let operation_id = document
        .pointer("/generation/operationId")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let storage = Storage::open(paths)?;
    let mut operation = match operation_id.as_deref() {
        Some(operation_id) => storage.read_agent_operation(operation_id)?,
        None => None,
    };
    if operation.is_none() {
        operation = storage
            .list_agent_operations(None, None)?
            .into_iter()
            .find(|candidate| {
                candidate.get("intent").and_then(Value::as_str) == Some("wiki.document.generate")
                    && candidate.get("projectId").and_then(Value::as_str) == Some(project_id)
                    && candidate.get("panelId").and_then(Value::as_str) == Some(panel_id)
                    && candidate
                        .pointer("/target/documentId")
                        .and_then(Value::as_str)
                        == Some(document_id)
            });
    }

    if let Some(task_id) = operation
        .as_ref()
        .and_then(|value| value.pointer("/target/writingTaskId"))
        .and_then(Value::as_str)
        .map(str::to_owned)
    {
        if let Some(operation_id) = operation
            .as_ref()
            .and_then(|value| value.get("id"))
            .and_then(Value::as_str)
        {
            finish_wiki(
                paths,
                operation_id,
                "cancelled",
                Some("Writing generation retried."),
            )?;
        }
        let task = crate::tasks::retry_task(paths, &task_id)?;
        return Ok(json!({ "retryMode": "task", "task": task["task"] }));
    }

    let base_content_version = operation
        .as_ref()
        .and_then(|value| value.pointer("/target/baseContentVersion"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let current_content_version = document["contentVersion"].as_u64().unwrap_or(0);
    if current_content_version <= base_content_version {
        return Err(CliError::with_code(
            "generation_retry_unavailable",
            "This generation has no saved result to recover. Ask the Agent to generate it again.",
        ));
    }

    let recovered = wiki::recover_generated_document_for_target(
        paths,
        project_id,
        panel_id,
        document_id,
        operation
            .as_ref()
            .and_then(|value| value.get("id"))
            .and_then(Value::as_str),
    )?;
    if let Some(operation) = operation.as_mut() {
        finish(operation, "completed", recovered.clone(), Value::Null);
        save(paths, operation)?;
    }
    Ok(json!({
        "retryMode": "recovered",
        "document": recovered["document"],
    }))
}

pub fn complete(
    paths: &MyOpenPanelsPaths,
    id: &str,
    artifact_file: &str,
    metadata: Option<Value>,
) -> Result<Value, CliError> {
    // A v3 executor has no database access, so route the task-bound Writing
    // completion to the Broker before trying to inspect the Operation locally.
    if crate::content::broker_execution_available() {
        if metadata.is_some() {
            return Err(CliError::with_code(
                "invalid_argument",
                "Wiki generation completion does not accept --metadata-file.",
            ));
        }
        return complete_wiki(paths, id, artifact_file);
    }
    let operation = inspect(paths, id)?;
    match operation.get("intent").and_then(Value::as_str) {
        Some("canvas.image.generate") => {
            let metadata = metadata.ok_or_else(|| {
                CliError::with_code(
                    "generation_metadata_required",
                    "Canvas generation completion requires --metadata-file.",
                )
            })?;
            complete_canvas(paths, id, artifact_file, metadata)
        }
        Some("wiki.document.generate") => {
            if metadata.is_some() {
                return Err(CliError::with_code(
                    "invalid_argument",
                    "Wiki generation completion does not accept --metadata-file.",
                ));
            }
            complete_wiki(paths, id, artifact_file)
        }
        Some(intent) => Err(CliError::with_code(
            "operation_intent_mismatch",
            format!("Operation {id} has unsupported intent {intent}"),
        )),
        None => Err(CliError::with_code(
            "operation_invalid",
            format!("Operation {id} has no intent."),
        )),
    }
}

pub fn finish_any(
    paths: &MyOpenPanelsPaths,
    id: &str,
    status: &str,
    message: Option<&str>,
) -> Result<Value, CliError> {
    let operation = inspect(paths, id)?;
    match operation.get("intent").and_then(Value::as_str) {
        Some("canvas.image.generate") => finish_canvas(paths, id, status, message),
        Some("wiki.document.generate") => finish_wiki(paths, id, status, message),
        Some(intent) => Err(CliError::with_code(
            "operation_intent_mismatch",
            format!("Operation {id} has unsupported intent {intent}"),
        )),
        None => Err(CliError::with_code(
            "operation_invalid",
            format!("Operation {id} has no intent."),
        )),
    }
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{
        activate_project_panel, create_project, ensure_project_bootstrap, read_active_project_id,
    };
    use crate::paths::resolve_myopenpanels_paths;
    use base64::Engine;

    fn test_paths() -> (tempfile::TempDir, MyOpenPanelsPaths) {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project).expect("project");
        let paths = resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("operation-test"),
        )
        .expect("paths");
        (temp, paths)
    }

    fn operation_record(id: &str, status: &str, completed_at: Option<&str>) -> Value {
        json!({
            "id": id,
            "ownerContextId": "operation-test",
            "intent": "canvas.image.generate",
            "status": status,
            "projectId": "session:test",
            "panelId": "panel:test",
            "panelKind": "canvas",
            "guideId": null,
            "protocolVersion": OPERATION_PROTOCOL_VERSION,
            "target": {},
            "input": {},
            "result": null,
            "error": null,
            "createdAt": "2026-01-01T00:00:00.000Z",
            "updatedAt": completed_at.unwrap_or("2026-01-01T00:00:00.000Z"),
            "completedAt": completed_at,
        })
    }

    #[test]
    fn cleanup_removes_only_expired_terminal_operation_artifacts() {
        let (_temp, paths) = test_paths();
        let bootstrap = crate::control::ensure_project_bootstrap(
            &paths,
            crate::control::BootstrapRequest::new(),
        )
        .expect("bootstrap");
        let storage = Storage::open(&paths).expect("storage");
        let cases = [
            (
                "operation:old-completed",
                "completed",
                Some("2026-01-01T00:00:00.000Z"),
            ),
            (
                "operation:old-cancelled",
                "cancelled",
                Some("2026-01-02T00:00:00.000Z"),
            ),
            (
                "operation:recent-completed",
                "completed",
                Some("2026-01-15T23:30:00.001Z"),
            ),
            (
                "operation:retryable-failed",
                "failed",
                Some("2026-01-01T00:00:00.000Z"),
            ),
            ("operation:active", "active", None),
        ];
        for (id, status, completed_at) in cases {
            let mut operation = operation_record(id, status, completed_at);
            operation["projectId"] = json!(bootstrap.project.id);
            operation["panelId"] = json!(bootstrap.panel.id);
            operation["panelKind"] = json!(bootstrap.panel.kind);
            storage
                .write_agent_operation(&operation)
                .expect("operation");
            let operation_dir = paths
                .storage_dir
                .join("operations")
                .join(sanitize_path_part(id));
            fs::create_dir_all(&operation_dir).expect("operation dir");
            fs::write(operation_dir.join("reference.png"), b"reference").expect("reference");
        }

        cleanup_artifacts_with_storage(
            &paths,
            &storage,
            chrono::DateTime::parse_from_rfc3339("2026-01-16T00:00:00.000Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );

        let operation_dir = |id: &str| {
            paths
                .storage_dir
                .join("operations")
                .join(sanitize_path_part(id))
        };
        assert!(!operation_dir("operation:old-completed").exists());
        assert!(!operation_dir("operation:old-cancelled").exists());
        assert!(operation_dir("operation:recent-completed").exists());
        assert!(operation_dir("operation:retryable-failed").exists());
        assert!(operation_dir("operation:active").exists());
        assert!(storage
            .read_agent_operation("operation:old-completed")
            .expect("read operation")
            .is_some());
    }

    #[test]
    fn canvas_generation_completes_against_original_project_after_focus_switch() {
        let (_temp, paths) = test_paths();
        ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let mut request = BootstrapRequest::new();
        request.requested_panel_kind = Some(PanelKind::Canvas);
        let canvas = read_project_bootstrap(&paths, request).expect("canvas");
        let started = begin_canvas(&paths, Some(128.0), Some(128.0), false, None).expect("begin");
        assert_eq!(started["panelSkill"]["skill"]["id"], CANVAS_PANEL_SKILL_ID);
        assert_eq!(started["operation"]["skillId"], CANVAS_PANEL_SKILL_ID);
        assert_eq!(started["operation"]["protocolVersion"], 2);
        assert!(started["operation"]["guideId"].is_null());
        assert!(started["operation"]["input"]["workflowSkillId"].is_null());
        let operation_id = started["operation"]["id"].as_str().unwrap();
        let next_project = create_project(&paths, Some("Another")).expect("new project");
        let image = paths.storage_dir.join("operation-result.png");
        fs::write(&image, base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==").unwrap()).expect("image");
        let completed = complete_canvas(
            &paths,
            operation_id,
            image.to_str().unwrap(),
            json!({
                "generatedBy": "agent",
                "generateOptions": { "prompt": "test image", "referenceImages": [] }
            }),
        )
        .expect("complete");
        assert_eq!(completed["operation"]["projectId"], canvas.project.id);
        assert_eq!(
            read_active_project_id(&paths).unwrap(),
            Some(next_project.project.id)
        );
        let state = Storage::open(&paths)
            .unwrap()
            .read_panel_state(&canvas.project.id, &canvas.panel.id)
            .unwrap()
            .unwrap();
        let shape_id = completed["image"]["shapeId"].as_str().unwrap();
        assert_eq!(state["store"][shape_id]["type"], "image");
    }

    #[test]
    fn reference_generation_requires_explicit_selection() {
        let (_temp, paths) = test_paths();
        ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        activate_project_panel(&paths, PanelKind::Canvas).expect("activate canvas");
        let error = begin_canvas(&paths, None, None, true, None).expect_err("selection required");
        assert_eq!(error.code(), Some("explicit_selection_required"));
    }

    #[test]
    fn wiki_generation_completes_against_original_project_after_restart_or_switch() {
        let (_temp, paths) = test_paths();
        let wiki_bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let started = begin_wiki(&paths, "Report", "markdown", None).expect("begin");
        assert_eq!(
            started["panelSkill"]["skill"]["id"],
            wiki::WIKI_PANEL_SKILL_ID
        );
        assert_eq!(started["operation"]["skillId"], wiki::WIKI_PANEL_SKILL_ID);
        assert_eq!(started["operation"]["protocolVersion"], 2);
        assert!(started["operation"]["guideId"].is_null());
        let operation_id = started["operation"]["id"].as_str().unwrap().to_owned();
        create_project(&paths, Some("Another")).expect("switch project");
        let file = paths.storage_dir.join("report.md");
        fs::write(&file, "# Report\n\nDone.\n").expect("file");
        let completed =
            complete_wiki(&paths, &operation_id, file.to_str().unwrap()).expect("complete");
        assert_eq!(
            completed["operation"]["projectId"],
            wiki_bootstrap.project.id
        );
        assert_eq!(completed["document"]["contentVersion"], 1);
        assert_eq!(completed["document"]["generation"]["status"], "completed");
        assert_eq!(
            inspect(&paths, &operation_id).unwrap()["status"],
            "completed"
        );
    }

    #[test]
    fn wiki_generation_detects_concurrent_document_updates() {
        let (_temp, paths) = test_paths();
        ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let created = wiki::create_generated_document(
            &paths,
            "report.md",
            Some("Report"),
            None,
            None,
            None,
            b"# First",
        )
        .expect("document");
        let document_id = created["document"]["id"].as_str().unwrap();
        let started = begin_wiki(&paths, "Report", "markdown", Some(document_id)).expect("begin");
        let operation_id = started["operation"]["id"].as_str().unwrap();
        wiki::write_generated_document(&paths, document_id, "report.md", None, b"# User edit")
            .expect("concurrent update");
        let file = paths.storage_dir.join("agent-report.md");
        fs::write(&file, "# Agent edit").expect("file");
        let error = complete_wiki(&paths, operation_id, file.to_str().unwrap())
            .expect_err("content conflict");
        assert_eq!(error.code(), Some("content_conflict"));
        assert_eq!(inspect(&paths, operation_id).unwrap()["status"], "active");
    }

    #[test]
    fn generated_document_write_rejects_an_active_generation_target() {
        let (_temp, paths) = test_paths();
        ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let started = begin_wiki(&paths, "Report", "markdown", None).expect("begin");
        let document_id = started["document"]["id"].as_str().unwrap();

        let error = wiki::write_generated_document_for_agent(
            &paths,
            document_id,
            "report.md",
            None,
            b"# Written through the wrong path",
        )
        .expect_err("active generation should reject a direct write");

        assert_eq!(error.code(), Some("generation_in_progress"));
        assert_eq!(
            wiki::read_generated_document(&paths, document_id).unwrap()["document"]
                ["contentVersion"],
            0
        );
    }

    #[test]
    fn retry_recovers_a_failed_generation_whose_content_was_already_written() {
        let (_temp, paths) = test_paths();
        let bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let started = begin_wiki(&paths, "Report", "markdown", None).expect("begin");
        let operation_id = started["operation"]["id"].as_str().unwrap().to_owned();
        let document_id = started["document"]["id"].as_str().unwrap().to_owned();
        let generated = wiki::read_generated_document(&paths, &document_id).expect("document");
        fs::write(
            generated["contentFilePath"].as_str().unwrap(),
            "# Already written\n",
        )
        .expect("content");

        let storage = Storage::open(&paths).expect("storage");
        let mut state = storage
            .read_panel_state(&bootstrap.project.id, &bootstrap.panel.id)
            .expect("read state")
            .expect("wiki state");
        let document = state["generatedDocuments"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|document| document["id"].as_str() == Some(document_id.as_str()))
            .unwrap();
        document["contentVersion"] = json!(1);
        document["wordCount"] = json!(15);
        storage
            .write_panel_state(&bootstrap.project.id, &bootstrap.panel.id, &state)
            .expect("write state");
        finish_wiki(
            &paths,
            &operation_id,
            "failed",
            Some("Content version conflict"),
        )
        .expect("fail operation");

        let retried = retry_wiki_document(&paths, &document_id).expect("retry");

        assert_eq!(retried["retryMode"], "recovered");
        assert_eq!(retried["document"]["generation"]["status"], "completed");
        assert_eq!(retried["document"]["contentVersion"], 1);
        assert_eq!(
            inspect(&paths, &operation_id).unwrap()["status"],
            "completed"
        );
    }
}
