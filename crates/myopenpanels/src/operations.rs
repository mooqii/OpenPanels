use crate::agent::{read_agent_skill, CANVAS_PANEL_SKILL_ID};
use crate::canvas::{
    insert_image_for_target, insert_placeholder_for_target, update_placeholder_for_target,
    InsertImageInput, InsertPlaceholderInput,
};
use crate::control::{now_iso, read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::{sanitize_path_part, MyOpenPanelsPaths};
use crate::selection::{read_selection, read_selection_asset_to_file};
use crate::storage::Storage;
use crate::types::PanelKind;
use crate::wiki;
use rand::Rng;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

pub const AGENT_PROTOCOL_VERSION: i64 = 2;

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
    Storage::open(paths)?.write_agent_operation(operation)
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
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    if bootstrap.active_panel_kind != PanelKind::Canvas {
        return Err(CliError::with_code("panel_changed", "The current panel is not Canvas. Refresh agent bootstrap or switch to Canvas before starting generation."));
    }
    let id = operation_id();
    let mut selected_ids = Vec::new();
    let mut reference = Value::Null;
    if use_selection {
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
        &bootstrap.session.id,
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
        "sessionId": bootstrap.session.id,
        "projectTitle": bootstrap.session.title,
        "panelId": bootstrap.panel.id,
        "panelTitle": bootstrap.panel.title,
        "panelKind": "canvas",
        "skillId": CANVAS_PANEL_SKILL_ID,
        "guideId": null,
        "protocolVersion": AGENT_PROTOCOL_VERSION,
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
        json!({ "operation": operation, "panelSkill": panel_skill, "nextAction": "Read the Canvas panel Skill and its relevant references, generate the bitmap, then run canvas generation complete." }),
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
    let session_id = operation["sessionId"].as_str().unwrap_or_default();
    let panel_id = operation["panelId"].as_str().unwrap_or_default();
    let placeholder = operation
        .pointer("/target/placeholderShapeId")
        .and_then(Value::as_str);
    let inserted = match insert_image_for_target(
        paths,
        session_id,
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
    let session_id = operation["sessionId"].as_str().unwrap_or_default();
    let panel_id = operation["panelId"].as_str().unwrap_or_default();
    let placeholder = operation
        .pointer("/target/placeholderShapeId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    update_placeholder_for_target(
        paths,
        session_id,
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
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    if bootstrap.active_panel_kind != PanelKind::Wiki {
        return Err(CliError::with_code("panel_changed", "The current panel is not Wiki. Refresh agent bootstrap or switch to Wiki before starting generation."));
    }
    let id = operation_id();
    let is_update = document_id.is_some();
    let started = wiki::begin_generated_document_for_target(
        paths,
        &bootstrap.session.id,
        &bootstrap.panel.id,
        &id,
        title,
        format,
        document_id,
    )?;
    let document_id = started["document"]["id"].as_str().unwrap_or_default();
    let now = now_iso();
    let panel_skill = read_agent_skill(paths, wiki::WIKI_PANEL_SKILL_ID, None)?;
    let operation = json!({
        "id": id, "ownerContextId": paths.context_id,
        "intent": "wiki.document.generate", "status": "active",
        "sessionId": bootstrap.session.id, "projectTitle": bootstrap.session.title,
        "panelId": bootstrap.panel.id, "panelTitle": bootstrap.panel.title, "panelKind": "wiki",
        "skillId": wiki::WIKI_PANEL_SKILL_ID, "guideId": null, "protocolVersion": AGENT_PROTOCOL_VERSION,
        "target": { "documentId": document_id, "baseContentVersion": started["baseContentVersion"] },
        "input": { "title": title, "format": format, "mode": if is_update { "update" } else { "create" } },
        "result": null, "error": null, "createdAt": now, "updatedAt": now, "completedAt": null,
    });
    save(paths, &operation)?;
    Ok(
        json!({ "operation": operation, "panelSkill": panel_skill, "document": started["document"], "nextAction": "Read the Wiki panel Skill and generated-documents reference, write the result file, then run wiki generation complete." }),
    )
}

pub fn complete_wiki(paths: &MyOpenPanelsPaths, id: &str, file: &str) -> Result<Value, CliError> {
    let mut operation = active_operation(paths, id, "wiki.document.generate")?;
    let content = fs::read(file).map_err(to_cli_error)?;
    let file_name = Path::new(file)
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("document.md");
    let result = match wiki::complete_generated_document_for_target(
        paths,
        operation["sessionId"].as_str().unwrap_or_default(),
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
        operation["sessionId"].as_str().unwrap_or_default(),
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

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{create_project, ensure_project_bootstrap, read_active_session_id};
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
        assert_eq!(completed["operation"]["sessionId"], canvas.session.id);
        assert_eq!(
            read_active_session_id(&paths).unwrap(),
            Some(next_project.session.id)
        );
        let state = Storage::open(&paths)
            .unwrap()
            .read_panel_state(&canvas.session.id, &canvas.panel.id)
            .unwrap()
            .unwrap();
        let shape_id = completed["image"]["shapeId"].as_str().unwrap();
        assert_eq!(state["store"][shape_id]["type"], "image");
    }

    #[test]
    fn reference_generation_requires_explicit_selection() {
        let (_temp, paths) = test_paths();
        ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let mut request = BootstrapRequest::new();
        request.requested_panel_kind = Some(PanelKind::Canvas);
        read_project_bootstrap(&paths, request).expect("canvas");
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
        assert!(started["operation"]["guideId"].is_null());
        let operation_id = started["operation"]["id"].as_str().unwrap().to_owned();
        create_project(&paths, Some("Another")).expect("switch project");
        let file = paths.storage_dir.join("report.md");
        fs::write(&file, "# Report\n\nDone.\n").expect("file");
        let completed =
            complete_wiki(&paths, &operation_id, file.to_str().unwrap()).expect("complete");
        assert_eq!(
            completed["operation"]["sessionId"],
            wiki_bootstrap.session.id
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
}
