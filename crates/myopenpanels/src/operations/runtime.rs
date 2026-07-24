use crate::agent::{read_agent_skill_for_panel, PANELS_SKILL_ID};
use crate::canvas::{InsertImageInput, InsertPlaceholderInput};
use crate::control::{now_iso, read_project_bootstrap, require_active_panel, BootstrapRequest};
use crate::error::CliError;
use crate::paths::{sanitize_path_part, MyOpenPanelsPaths};
use crate::selection::{read_selection, read_selection_asset_to_file};
use crate::storage::Storage;
use crate::types::PanelKind;
#[cfg(test)]
use crate::wiki;
use rusqlite::TransactionBehavior;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

const TERMINAL_OPERATION_ARTIFACT_RETENTION_MINUTES: i64 = 30;

fn operation_id() -> String {
    crate::ids::random_id("operation")
}

pub fn list(paths: &MyOpenPanelsPaths, status: Option<&str>) -> Result<Value, CliError> {
    let operations =
        Storage::open(paths)?.list_direct_operations(Some(&paths.context_id), status)?;
    Ok(json!({ "operations": operations }))
}

pub fn inspect(paths: &MyOpenPanelsPaths, id: &str) -> Result<Value, CliError> {
    Storage::open(paths)?
        .read_direct_operation(id)?
        .ok_or_else(|| {
            CliError::with_code(
                "operation_not_found",
                format!("Direct Operation not found: {id}"),
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
    if operation["status"].as_str() != Some("active") {
        return Err(CliError::with_code(
            "operation_not_active",
            format!("Operation {id} is not active"),
        ));
    }
    Ok(operation)
}

fn active_my_document_operation(
    paths: &MyOpenPanelsPaths,
    id: &str,
) -> Result<Value, CliError> {
    let operation = inspect(paths, id)?;
    if !matches!(
        operation["intent"].as_str(),
        Some("my-document.create" | "my-document.revise")
    ) {
        return Err(CliError::with_code(
            "operation_intent_mismatch",
            format!("Operation {id} is not a My Document operation"),
        ));
    }
    if operation["status"].as_str() != Some("active") {
        return Err(CliError::with_code(
            "operation_not_active",
            format!("Operation {id} is not active"),
        ));
    }
    Ok(operation)
}

fn save(paths: &MyOpenPanelsPaths, operation: &Value) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    storage.write_direct_operation(operation)?;
    cleanup_artifacts_with_storage(paths, &storage, chrono::Utc::now());
    Ok(())
}

fn commit_direct_panel_state(
    paths: &MyOpenPanelsPaths,
    operation: &mut Value,
    state: &Value,
    expected_panel_revision: i64,
    bind_resulting_revision: bool,
    removed_my_document_id: Option<&str>,
) -> Result<i64, CliError> {
    let project_id = operation["projectId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let panel_id = operation["panelId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let mut storage = Storage::open(paths)?;
    let tx = storage
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(to_cli_error)?;
    let revision = Storage::write_panel_state_if_revision_in_transaction(
        &tx,
        &project_id,
        &panel_id,
        expected_panel_revision,
        state,
    )?;
    if bind_resulting_revision {
        operation["baseRevision"] = json!(revision);
    }
    if let Some(document_id) = removed_my_document_id {
        Storage::delete_my_document_in_transaction(
            &tx,
            &project_id,
            &panel_id,
            document_id,
        )?;
    }
    Storage::write_direct_operation_in_transaction(&tx, operation)?;
    tx.commit().map_err(to_cli_error)?;
    Ok(revision)
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
    let Ok(terminal_operation_ids) = storage.list_terminal_direct_operation_ids_before(&cutoff)
    else {
        return;
    };
    let Ok(known_operation_ids) = storage.direct_operation_ids() else {
        return;
    };
    let terminal_operation_ids = terminal_operation_ids.into_iter().collect::<HashSet<_>>();
    let known_operation_ids = known_operation_ids.into_iter().collect::<HashSet<_>>();
    let operations_dir = paths.storage_dir.join("operations");
    let Ok(entries) = fs::read_dir(&operations_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let operation_dir = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&operation_dir) else {
            continue;
        };
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            continue;
        }
        let operation_id = entry.file_name().to_string_lossy().into_owned();
        let known = known_operation_ids
            .iter()
            .any(|id| sanitize_path_part(id) == operation_id);
        let expired = terminal_operation_ids
            .iter()
            .any(|id| sanitize_path_part(id) == operation_id);
        if !known || expired {
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
    let prepared = crate::canvas::prepare_placeholder_for_target(
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
    let panel_skill =
        read_agent_skill_for_panel(paths, PANELS_SKILL_ID, None, Some(PanelKind::Canvas))?;
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
        "targetId": prepared.shape_id,
        "baseRevision": 0,
        "skillId": PANELS_SKILL_ID,
        "guideId": null,
        "target": {
            "placeholderShapeId": prepared.shape_id,
            "bounds": prepared.bounds,
            "reference": reference,
        },
        "input": { "displayWidth": width, "displayHeight": height, "useSelection": use_selection },
        "result": null,
        "error": null,
        "createdAt": now,
        "updatedAt": now,
        "completedAt": null,
    });
    let mut operation = operation;
    commit_direct_panel_state(
        paths,
        &mut operation,
        &prepared.state,
        prepared.base_revision,
        true,
        None,
    )?;
    Ok(
        json!({ "operation": operation, "panelSkill": panel_skill, "nextAction": "Read the Panels Skill and returned Canvas references, generate the bitmap, then run operation complete with the captured operation id." }),
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
    let project_id = operation["projectId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let panel_id = operation["panelId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let placeholder = operation
        .pointer("/target/placeholderShapeId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if operation["targetId"].as_str() != Some(placeholder.as_str()) {
        return Err(CliError::with_code(
            "operation_target_mismatch",
            "The Direct Operation target binding is invalid.",
        ));
    }
    let expected_revision = operation["baseRevision"].as_i64().unwrap_or(-1);
    let mut prepared = match crate::canvas::prepare_image_for_target(
        paths,
        &project_id,
        &panel_id,
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
            replace_shape_id: Some(&placeholder),
        },
        false,
        Some(expected_revision),
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
    let prepared_asset_revision = prepared.asset.revision_dir.clone();
    let mut storage = Storage::open(paths)?;
    let committed = (|| {
        let tx = storage
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        Storage::write_prepared_asset_in_transaction(
            &tx,
            &project_id,
            &panel_id,
            &prepared.asset,
        )?;
        prepared.payload.revision = Storage::write_panel_state_if_revision_in_transaction(
            &tx,
            &project_id,
            &panel_id,
            expected_revision,
            &prepared.state,
        )?;
        let result = serde_json::to_value(&prepared.payload).map_err(to_cli_error)?;
        finish(&mut operation, "completed", result, Value::Null);
        Storage::write_direct_operation_in_transaction(&tx, &operation)?;
        tx.commit().map_err(to_cli_error)
    })();
    if let Err(error) = committed {
        let _ = fs::remove_dir_all(prepared_asset_revision);
        return Err(error);
    }
    let result = operation["result"].clone();
    cleanup_artifacts_with_storage(paths, &storage, chrono::Utc::now());
    Ok(json!({ "operation": operation, "image": result }))
}

pub fn finish_canvas(
    paths: &MyOpenPanelsPaths,
    id: &str,
    status: &str,
    message: Option<&str>,
) -> Result<Value, CliError> {
    if !matches!(status, "failed" | "cancelled") {
        return Err(CliError::with_code(
            "invalid_operation_status",
            "A Direct Operation can only be finished as failed or cancelled.",
        ));
    }
    let mut operation = active_operation(paths, id, "canvas.image.generate")?;
    let project_id = operation["projectId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let panel_id = operation["panelId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let placeholder = operation
        .pointer("/target/placeholderShapeId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if operation["targetId"].as_str() != Some(placeholder.as_str()) {
        return Err(CliError::with_code(
            "operation_target_mismatch",
            "The Direct Operation target binding is invalid.",
        ));
    }
    let prepared = crate::canvas::prepare_placeholder_update_for_target(
        paths,
        &project_id,
        &panel_id,
        &placeholder,
        message,
        true,
    )?;
    let expected_revision = operation["baseRevision"].as_i64().unwrap_or(-1);
    if prepared.base_revision != expected_revision {
        return Err(CliError::with_code(
            "content_conflict",
            format!(
                "Canvas changed from revision {expected_revision} to {}.",
                prepared.base_revision
            ),
        ));
    }
    finish(
        &mut operation,
        status,
        Value::Null,
        message
            .map(|m| json!({"message": m}))
            .unwrap_or(Value::Null),
    );
    commit_direct_panel_state(
        paths,
        &mut operation,
        &prepared.state,
        expected_revision,
        false,
        None,
    )?;
    Ok(operation)
}

pub fn begin_my_document(
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
    let prepared = crate::my_document::prepare_begin_my_document_for_target(
        paths,
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &id,
        title,
        format,
        document_id,
        false,
    )?;
    let document_id = prepared.document["id"].as_str().unwrap_or_default();
    let now = now_iso();
    let panel_skill =
        read_agent_skill_for_panel(paths, PANELS_SKILL_ID, None, Some(PanelKind::Wiki))?;
    let operation = json!({
        "id": id, "ownerContextId": paths.context_id,
        "intent": if is_update { "my-document.revise" } else { "my-document.create" },
        "status": "active",
        "projectId": bootstrap.project.id, "projectTitle": bootstrap.project.title,
        "panelId": bootstrap.panel.id, "panelTitle": bootstrap.panel.title, "panelKind": "wiki",
        "targetId": document_id, "baseRevision": prepared.base_content_version,
        "skillId": PANELS_SKILL_ID, "guideId": null,
        "target": { "documentId": document_id, "baseContentVersion": prepared.base_content_version },
        "input": { "title": title, "format": format, "mode": if is_update { "revise" } else { "create" } },
        "result": null, "error": null, "createdAt": now, "updatedAt": now, "completedAt": null,
    });
    let mut operation = operation;
    commit_direct_panel_state(
        paths,
        &mut operation,
        &prepared.state,
        prepared.base_panel_revision,
        false,
        None,
    )?;
    Ok(
        json!({ "operation": operation, "panelSkill": panel_skill, "document": prepared.document, "nextAction": "Read the Panels Skill and returned Wiki references, write the result file, then run operation complete with the captured operation id." }),
    )
}

pub fn complete_my_document(
    paths: &MyOpenPanelsPaths,
    id: &str,
    file: &str,
) -> Result<Value, CliError> {
    crate::content::require_broker_for_task_execution()?;
    let mut operation = active_my_document_operation(paths, id)?;
    let content = fs::read(file).map_err(to_cli_error)?;
    let file_name = Path::new(file)
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("document.md");
    let project_id = operation["projectId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let panel_id = operation["panelId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let document_id = operation
        .pointer("/target/documentId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let base_content_version = operation["baseRevision"].as_u64().unwrap_or(0);
    if operation["targetId"].as_str() != Some(document_id.as_str())
        || operation
            .pointer("/target/baseContentVersion")
            .and_then(Value::as_u64)
            != Some(base_content_version)
    {
        return Err(CliError::with_code(
            "operation_target_mismatch",
            "The Direct Operation target binding is invalid.",
        ));
    }
    let (_, mime_type, content_ref) =
        crate::my_document::my_document_content_descriptor(file_name)?;
    let prepared_content = crate::content::prepare_direct_text_content(
        paths,
        id,
        &project_id,
        &panel_id,
        crate::content::ResourceKind::MyDocument,
        &document_id,
        content_ref,
        &content,
        mime_type,
        base_content_version,
    )?;
    let committed_content_version = prepared_content.commit.content_version as u64;
    let prepared_document = match crate::my_document::prepare_complete_my_document_for_target(
        paths,
        &project_id,
        &panel_id,
        id,
        &document_id,
        base_content_version,
        file_name,
        content_ref,
        committed_content_version,
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
    let result = json!({
        "document": prepared_document.document.clone(),
        "contentCommits": [prepared_content.commit.clone()],
    });
    finish(&mut operation, "completed", result.clone(), Value::Null);
    let mut storage = Storage::open(paths)?;
    let tx = storage
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(to_cli_error)?;
    Storage::write_my_document_content_in_transaction(
        &tx,
        &project_id,
        base_content_version,
        &prepared_document.document,
    )?;
    Storage::write_content_commit_in_transaction(
        &tx,
        &project_id,
        &prepared_content.commit,
    )?;
    Storage::write_direct_operation_in_transaction(&tx, &operation)?;
    tx.commit().map_err(to_cli_error)?;
    crate::content::publish_prepared_direct_content(prepared_content)?;
    cleanup_artifacts_with_storage(paths, &storage, chrono::Utc::now());
    Ok(json!({ "operation": operation, "document": result["document"] }))
}

pub fn finish_my_document(
    paths: &MyOpenPanelsPaths,
    id: &str,
    status: &str,
    message: Option<&str>,
) -> Result<Value, CliError> {
    if !matches!(status, "failed" | "cancelled") {
        return Err(CliError::with_code(
            "invalid_operation_status",
            "A Direct Operation can only be finished as failed or cancelled.",
        ));
    }
    let mut operation = active_my_document_operation(paths, id)?;
    let project_id = operation["projectId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let panel_id = operation["panelId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let document_id = operation
        .pointer("/target/documentId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if operation["targetId"].as_str() != Some(document_id.as_str()) {
        return Err(CliError::with_code(
            "operation_target_mismatch",
            "The Direct Operation target binding is invalid.",
        ));
    }
    let prepared = crate::my_document::prepare_finish_my_document_operation(
        paths,
        &project_id,
        &panel_id,
        id,
        &document_id,
    )?;
    finish(
        &mut operation,
        status,
        Value::Null,
        message
            .map(|m| json!({"message": m}))
            .unwrap_or(Value::Null),
    );
    commit_direct_panel_state(
        paths,
        &mut operation,
        &prepared.state,
        prepared.base_panel_revision,
        false,
        prepared.document.is_null().then_some(document_id.as_str()),
    )?;
    Ok(operation)
}

pub fn complete(
    paths: &MyOpenPanelsPaths,
    id: &str,
    artifact_file: &str,
    metadata: Option<Value>,
) -> Result<Value, CliError> {
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
        Some("my-document.create" | "my-document.revise") => {
            if metadata.is_some() {
                return Err(CliError::with_code(
                    "invalid_argument",
                    "My Document completion does not accept --metadata-file.",
                ));
            }
            complete_my_document(paths, id, artifact_file)
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
        Some("my-document.create" | "my-document.revise") => {
            finish_my_document(paths, id, status, message)
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

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
