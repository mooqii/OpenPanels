use crate::control::{now_iso, read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::{sanitize_path_part, MyOpenPanelsPaths};
use crate::storage::Storage;
use crate::trace::{self, TraceEventInput};
use crate::types::PanelKind;
use base64::Engine;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod state;

use state::*;

fn character_count(content: &str) -> usize {
    content
        .chars()
        .filter(|character| !character.is_whitespace())
        .count()
}

#[cfg(test)]
mod character_count_tests {
    use super::character_count;

    #[test]
    fn counts_non_whitespace_unicode_characters() {
        assert_eq!(character_count("Hello 世界\n"), 7);
    }
}

pub fn wiki_context(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    reject_live_content_access_for_task()?;
    let wiki = get_wiki_bootstrap(paths)?;
    Ok(json!({
        "project": wiki.project,
        "panel": wiki.panel,
        "state": wiki.state,
    }))
}

pub fn read_agent_selection(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    reject_live_content_access_for_task()?;
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    if bootstrap.active_panel_kind != PanelKind::Wiki {
        return Err(CliError::with_code(
            "panel_kind_mismatch",
            "The current user selection belongs to the active panel, which is not Wiki.",
        ));
    }
    let wiki = get_wiki_target(paths, &bootstrap.project.id, &bootstrap.panel.id)?;
    let storage = Storage::open(paths)?;
    let stored = storage
        .read_panel_selection(&wiki.project.id, &wiki.panel.id)?
        .unwrap_or_else(|| json!({}));
    let requested_document_ids = stored
        .get("selectedRawDocumentIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let requested_generated_document_ids = stored
        .get("selectedGeneratedDocumentIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let (selected_documents, selected_generated_documents) = selected_document_context(
        paths,
        &wiki,
        &requested_document_ids,
        &requested_generated_document_ids,
    );
    let selected_document_ids = selected_documents
        .iter()
        .filter_map(|document| document.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    let selected_generated_document_ids = selected_generated_documents
        .iter()
        .filter_map(|document| document.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    let wiki_space = resolve_wiki_space(&wiki.state, None)?;
    let page_count = wiki_space
        .value
        .get("pageIndex")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let is_wiki_selected = stored
        .get("isWikiSelected")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let local_access = wiki_local_access(paths, &wiki, &wiki_space.id, is_wiki_selected);
    let selection = json!({
        "kind": "wiki",
        "projectId": wiki.project.id,
        "panelId": wiki.panel.id,
        "isExplicitSelection": is_wiki_selected || !selected_document_ids.is_empty() || !selected_generated_document_ids.is_empty(),
        "isWikiSelected": is_wiki_selected,
        "selectedRawDocumentIds": selected_document_ids,
        "selectedGeneratedDocumentIds": selected_generated_document_ids,
        "updatedAt": stored.get("updatedAt").cloned().unwrap_or(Value::Null),
    });
    let mut skill_action = crate::cli::registry::command_action(
        crate::cli::registry::CommandId::registered("agent.skill.read"),
        vec![
            "--skill-id".to_owned(),
            crate::agent::PANELS_SKILL_ID.to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ],
    )
    .expect("registered Wiki Panel Skill action");
    skill_action["condition"] = json!({
        "type": "agent-judgment",
        "description": "The user request requires Wiki query guidance."
    });
    Ok(json!({
        "selection": selection,
        "wiki": {
            "available": true,
            "selected": is_wiki_selected,
            "wikiSpaceId": wiki_space.id,
            "title": wiki_space.value.get("title").cloned().unwrap_or_else(|| json!("Wiki")),
            "pageCount": page_count,
            "querySkillId": crate::agent::PANELS_SKILL_ID,
            "localAccess": local_access,
        },
        "actions": { "required": [], "suggested": [skill_action] },
        "selectedRawDocuments": selected_documents,
        "selectedGeneratedDocuments": selected_generated_documents,
    }))
}

pub fn write_agent_selection(
    paths: &MyOpenPanelsPaths,
    is_wiki_selected: bool,
    selected_raw_document_ids: &[String],
    selected_generated_document_ids: &[String],
) -> Result<Value, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    let documents = wiki
        .state
        .get("rawDocuments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let known_ids = documents
        .iter()
        .filter_map(|document| document.get("id").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let selected_ids = selected_raw_document_ids
        .iter()
        .filter(|document_id| known_ids.contains(document_id.as_str()))
        .filter(|document_id| seen.insert((*document_id).clone()))
        .cloned()
        .collect::<Vec<_>>();
    let generated_documents = wiki
        .state
        .get("generatedDocuments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let known_generated_ids = generated_documents
        .iter()
        .filter_map(|document| document.get("id").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let mut seen_generated = BTreeSet::new();
    let selected_generated_ids = selected_generated_document_ids
        .iter()
        .filter(|document_id| known_generated_ids.contains(document_id.as_str()))
        .filter(|document_id| seen_generated.insert((*document_id).clone()))
        .cloned()
        .collect::<Vec<_>>();
    let selection = json!({
        "kind": "wiki",
        "projectId": wiki.project.id,
        "panelId": wiki.panel.id,
        "isWikiSelected": is_wiki_selected,
        "selectedRawDocumentIds": selected_ids,
        "selectedGeneratedDocumentIds": selected_generated_ids,
        "updatedAt": now_iso(),
    });
    Storage::open(paths)?.write_panel_selection(&wiki.project.id, &wiki.panel.id, &selection)?;
    read_agent_selection(paths)
}

pub fn list_generated_documents(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    reject_live_content_access_for_task()?;
    list_generated_documents_with_access(paths)
}

fn generated_document_format(
    file_name: &str,
    mime_type: Option<&str>,
) -> Result<(&'static str, &'static str), CliError> {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "md" | "markdown"
            if mime_type.is_none_or(|value| value == "text/markdown" || value == "text/plain") =>
        {
            Ok(("markdown", "text/markdown"))
        }
        "txt" if mime_type.is_none_or(|value| value == "text/plain") => Ok(("text", "text/plain")),
        _ => Err(CliError::with_code(
            "invalid_generated_document",
            "Generated documents must be UTF-8 .md, .markdown, or .txt files.",
        )),
    }
}

pub fn create_generated_document(
    paths: &MyOpenPanelsPaths,
    file_name: &str,
    title: Option<&str>,
    mime_type: Option<&str>,
    task_id: Option<&str>,
    thread_id: Option<&str>,
    content: &[u8],
) -> Result<Value, CliError> {
    let text = std::str::from_utf8(content).map_err(|_| {
        CliError::with_code(
            "invalid_generated_document",
            "Generated document content must be valid UTF-8.",
        )
    })?;
    let (format, normalized_mime_type) = generated_document_format(file_name, mime_type)?;
    let mut wiki = match task_id {
        Some(task_id) => get_wiki_task_target(paths, task_id)?,
        None => get_wiki_bootstrap(paths)?,
    };
    let storage = Storage::open(paths)?;
    let document_id = create_id("generated");
    let safe_file_name = sanitize_path_part(file_name);
    let extension = if format == "markdown" { "md" } else { "txt" };
    let content_ref = wiki_ref(&["generated", &document_id, &format!("content.{extension}")]);
    let content_path = wiki_panel_path(
        &storage.panel_dir(&wiki.project.id, &wiki.panel.id),
        &content_ref,
    )?;
    if let Some(parent) = content_path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(&content_path, content).map_err(to_cli_error)?;
    if task_id.is_none() {
        crate::content::commit_immediate_text(
            paths,
            &wiki.project.id,
            Some(&wiki.panel.id),
            crate::content::ResourceKind::GeneratedDocument,
            &document_id,
            &format!("content.{extension}"),
            content,
            &normalized_mime_type,
            true,
        )?;
    }
    let now = now_iso();
    let document = json!({
        "id": document_id,
        "title": title
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| title_from_file_name(file_name)),
        "originalFileName": safe_file_name,
        "format": format,
        "mimeType": normalized_mime_type,
        "contentRef": content_ref,
        "contentVersion": 1,
        "taskId": task_id,
        "threadId": thread_id,
        "publishHistory": [],
        "wordCount": character_count(text),
        "createdAt": now,
        "updatedAt": now,
    });
    state_array_mut(&mut wiki.state, "generatedDocuments")?.insert(0, document.clone());
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "document": document, "state": wiki.state }))
}

pub fn begin_generated_document_for_target(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: &str,
    operation_id: &str,
    title: &str,
    format: &str,
    document_id: Option<&str>,
    replace_placeholder_title: bool,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_target(paths, project_id, panel_id)?;
    let now = now_iso();
    if let Some(document_id) = document_id {
        let document = find_generated_document_mut(&mut wiki.state, document_id)?;
        if replace_placeholder_title && document["contentVersion"].as_u64().unwrap_or(0) == 0 {
            let title = title.trim();
            if !title.is_empty() {
                let extension = if format == "text" { "txt" } else { "md" };
                document["title"] = json!(title);
                document["originalFileName"] =
                    json!(format!("{}.{}", sanitize_path_part(title), extension));
            }
        }
        document["generation"] = json!({
            "operationId": operation_id,
            "status": "generating",
            "error": null,
        });
        document["updatedAt"] = json!(now);
        let version = document["contentVersion"].as_u64().unwrap_or(0);
        let document = document.clone();
        save_wiki_state(paths, &wiki)?;
        return Ok(json!({ "document": document, "baseContentVersion": version }));
    }
    let extension = if format == "text" { "txt" } else { "md" };
    let mime_type = if format == "text" {
        "text/plain"
    } else {
        "text/markdown"
    };
    let document_id = create_id("generated");
    let file_name = format!("{}.{}", sanitize_path_part(title), extension);
    let content_ref = wiki_ref(&["generated", &document_id, &format!("content.{extension}")]);
    let storage = Storage::open(paths)?;
    let content_path = wiki_panel_path(&storage.panel_dir(project_id, panel_id), &content_ref)?;
    if let Some(parent) = content_path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(&content_path, b"").map_err(to_cli_error)?;
    let document = json!({
        "id": document_id,
        "title": title,
        "originalFileName": file_name,
        "format": if format == "text" { "text" } else { "markdown" },
        "mimeType": mime_type,
        "contentRef": content_ref,
        "contentVersion": 0,
        "taskId": null,
        "threadId": null,
        "publishHistory": [],
        "wordCount": 0,
        "generation": { "operationId": operation_id, "status": "generating", "error": null },
        "createdAt": now,
        "updatedAt": now,
    });
    state_array_mut(&mut wiki.state, "generatedDocuments")?.insert(0, document.clone());
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "document": document, "baseContentVersion": 0 }))
}

pub fn complete_generated_document_for_target(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: &str,
    document_id: &str,
    base_content_version: u64,
    file_name: &str,
    content: &[u8],
) -> Result<Value, CliError> {
    let text = std::str::from_utf8(content).map_err(|_| {
        CliError::with_code(
            "invalid_generated_document",
            "Generated document content must be valid UTF-8.",
        )
    })?;
    let (format, mime_type) = generated_document_format(file_name, None)?;
    let mut wiki = get_wiki_target(paths, project_id, panel_id)?;
    let current_version = find_generated_document(&wiki.state, document_id)?["contentVersion"]
        .as_u64()
        .unwrap_or(0);
    if current_version != base_content_version {
        return Err(CliError::with_code("content_conflict", format!("Generated document changed from version {base_content_version} to {current_version}")));
    }
    let old_ref = find_generated_document(&wiki.state, document_id)?["contentRef"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    let extension = Path::new(file_name)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or(if format == "markdown" { "md" } else { "txt" });
    let content_ref = wiki_ref(&["generated", document_id, &format!("content.{extension}")]);
    let storage = Storage::open(paths)?;
    let panel_dir = storage.panel_dir(project_id, panel_id);
    let content_path = wiki_panel_path(&panel_dir, &content_ref)?;
    if let Some(parent) = content_path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(&content_path, content).map_err(to_cli_error)?;
    crate::content::commit_immediate_text(
        paths,
        &wiki.project.id,
        Some(&wiki.panel.id),
        crate::content::ResourceKind::GeneratedDocument,
        document_id,
        &format!("content.{extension}"),
        content,
        &mime_type,
        true,
    )?;
    if !old_ref.is_empty() && old_ref != content_ref {
        let _ = fs::remove_file(wiki_panel_path(&panel_dir, &old_ref)?);
    }
    let document = find_generated_document_mut(&mut wiki.state, document_id)?;
    document["contentRef"] = json!(content_ref);
    document["contentVersion"] = json!(base_content_version + 1);
    document["format"] = json!(format);
    document["mimeType"] = json!(mime_type);
    document["originalFileName"] = json!(sanitize_path_part(file_name));
    document["wordCount"] = json!(character_count(text));
    document["generation"] = json!({ "status": "completed", "error": null });
    document["updatedAt"] = json!(now_iso());
    let document = document.clone();
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "document": document, "state": wiki.state }))
}

pub fn finish_generated_document_operation(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: &str,
    document_id: &str,
    status: &str,
    message: Option<&str>,
) -> Result<(), CliError> {
    let mut wiki = get_wiki_target(paths, project_id, panel_id)?;
    if status == "cancelled" {
        let documents = state_array_mut(&mut wiki.state, "generatedDocuments")?;
        if let Some(index) = documents
            .iter()
            .position(|d| d["id"].as_str() == Some(document_id))
        {
            if documents[index]["contentVersion"].as_u64().unwrap_or(0) == 0
                && !documents[index]["taskId"].is_string()
            {
                documents.remove(index);
                let storage = Storage::open(paths)?;
                let _ = fs::remove_dir_all(
                    storage
                        .panel_dir(project_id, panel_id)
                        .join("generated")
                        .join(sanitize_path_part(document_id)),
                );
            } else {
                documents[index]
                    .as_object_mut()
                    .map(|object| object.remove("generation"));
                documents[index]["updatedAt"] = json!(now_iso());
            }
        }
    } else {
        let document = find_generated_document_mut(&mut wiki.state, document_id)?;
        let operation_id = document
            .pointer("/generation/operationId")
            .cloned()
            .unwrap_or(Value::Null);
        document["generation"] = json!({
            "operationId": operation_id,
            "status": status,
            "error": message,
        });
        document["updatedAt"] = json!(now_iso());
    }
    save_wiki_state(paths, &wiki)
}

pub fn remove_pending_writing_document(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: &str,
    document_id: &str,
) -> Result<(), CliError> {
    let mut wiki = get_wiki_target(paths, project_id, panel_id)?;
    let documents = state_array_mut(&mut wiki.state, "generatedDocuments")?;
    let Some(index) = documents
        .iter()
        .position(|document| document["id"].as_str() == Some(document_id))
    else {
        return Ok(());
    };
    if documents[index]["contentVersion"].as_u64().unwrap_or(0) > 0 {
        return Ok(());
    }
    documents.remove(index);
    save_wiki_state(paths, &wiki)?;
    let storage = Storage::open(paths)?;
    let _ = fs::remove_dir_all(
        storage
            .panel_dir(project_id, panel_id)
            .join("generated")
            .join(sanitize_path_part(document_id)),
    );
    Ok(())
}

pub fn recover_generated_document_for_target(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: &str,
    document_id: &str,
    operation_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_target(paths, project_id, panel_id)?;
    let document = find_generated_document_mut(&mut wiki.state, document_id)?;
    if document["contentVersion"].as_u64().unwrap_or(0) == 0 {
        return Err(CliError::with_code(
            "generation_retry_unavailable",
            "The failed document has no saved content to recover.",
        ));
    }
    document["generation"] = json!({
        "operationId": operation_id,
        "status": "completed",
        "error": null,
    });
    document["updatedAt"] = json!(now_iso());
    let document = document.clone();
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "document": document, "state": wiki.state }))
}

pub fn read_generated_document(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
) -> Result<Value, CliError> {
    if crate::content::broker_execution_available() {
        let read = |logical_path: &str| {
            crate::content::broker_read_file(&crate::content::ReadFileRequest {
                resource_kind: crate::content::ResourceKind::GeneratedDocument
                    .as_str()
                    .to_owned(),
                resource_key: document_id.to_owned(),
                logical_path: logical_path.to_owned(),
            })
        };
        let payload = match read("content.md") {
            Err(error) if error.code() == Some("content_not_found") => read("content.txt")?,
            result => result?,
        };
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload["contentBase64"].as_str().unwrap_or_default())
            .map_err(to_cli_error)?;
        let content = String::from_utf8(bytes).map_err(to_cli_error)?;
        return Ok(json!({
            "document": { "id": document_id },
            "content": content,
            "contentFilePath": null,
            "contentAccess": {
                "status": "ready",
                "localPath": null,
                "source": "task_broker",
            },
            "staged": true,
        }));
    }
    crate::content::require_broker_for_task_execution()?;
    let wiki = get_wiki_bootstrap(paths)?;
    let document = find_generated_document(&wiki.state, document_id)?.clone();
    let (content_path, content_access) = materialize_generated_content(paths, &wiki, &document);
    let content_path = content_path.ok_or_else(|| {
        CliError::with_code(
            content_access
                .get("errorCode")
                .and_then(Value::as_str)
                .unwrap_or("content_unavailable"),
            content_access
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Generated document content is not ready."),
        )
    })?;
    let content = fs::read_to_string(&content_path).map_err(to_cli_error)?;
    Ok(json!({
        "document": document,
        "content": content,
        "contentFilePath": content_path,
        "contentAccess": content_access,
    }))
}

pub fn write_generated_document(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
    file_name: &str,
    mime_type: Option<&str>,
    content: &[u8],
) -> Result<Value, CliError> {
    let text = std::str::from_utf8(content).map_err(|_| {
        CliError::with_code(
            "invalid_generated_document",
            "Generated document content must be valid UTF-8.",
        )
    })?;
    let (format, normalized_mime_type) = generated_document_format(file_name, mime_type)?;
    let mut wiki = get_wiki_bootstrap(paths)?;
    let storage = Storage::open(paths)?;
    let existing_document = find_generated_document(&wiki.state, document_id)?;
    let old_ref = existing_document["contentRef"]
        .as_str()
        .ok_or_else(|| CliError::new("Generated document contentRef is missing."))?
        .to_owned();
    let extension = if format == "markdown" { "md" } else { "txt" };
    let content_ref = wiki_ref(&["generated", document_id, &format!("content.{extension}")]);
    let panel_dir = storage.panel_dir(&wiki.project.id, &wiki.panel.id);
    let content_path = wiki_panel_path(&panel_dir, &content_ref)?;
    if let Some(parent) = content_path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(&content_path, content).map_err(to_cli_error)?;
    crate::content::commit_immediate_text(
        paths,
        &wiki.project.id,
        Some(&wiki.panel.id),
        crate::content::ResourceKind::GeneratedDocument,
        document_id,
        &format!("content.{extension}"),
        content,
        &normalized_mime_type,
        true,
    )?;
    if old_ref != content_ref {
        let old_path = wiki_panel_path(&panel_dir, &old_ref)?;
        let _ = fs::remove_file(old_path);
    }
    let now = now_iso();
    let document = find_generated_document_mut(&mut wiki.state, document_id)?;
    document["contentRef"] = json!(content_ref);
    document["contentVersion"] = json!(document["contentVersion"].as_u64().unwrap_or(0) + 1);
    document["format"] = json!(format);
    document["mimeType"] = json!(normalized_mime_type);
    document["originalFileName"] = json!(sanitize_path_part(file_name));
    document["wordCount"] = json!(character_count(text));
    if document
        .pointer("/generation/status")
        .and_then(Value::as_str)
        == Some("failed")
    {
        document["generation"] = json!({ "status": "completed", "error": null });
    }
    document["updatedAt"] = json!(now);
    let document = document.clone();
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "document": document, "state": wiki.state }))
}

pub fn write_generated_document_for_agent(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
    file_name: &str,
    mime_type: Option<&str>,
    content: &[u8],
) -> Result<Value, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    if find_generated_document(&wiki.state, document_id)?
        .pointer("/generation/status")
        .and_then(Value::as_str)
        == Some("generating")
    {
        return Err(CliError::with_code(
            "generation_in_progress",
            "Complete the active generation Operation instead of writing the document directly.",
        ));
    }
    write_generated_document(paths, document_id, file_name, mime_type, content)
}

pub fn rename_generated_document_file(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
    file_name: &str,
) -> Result<Value, CliError> {
    let existing = read_generated_document(paths, document_id)?;
    let content = existing["content"]
        .as_str()
        .ok_or_else(|| CliError::new("Generated document content is invalid."))?;
    let mime_type = existing["document"]["mimeType"].as_str();
    write_generated_document(paths, document_id, file_name, mime_type, content.as_bytes())?;
    rename_generated_document(paths, document_id, &title_from_file_name(file_name))
}

pub fn rename_generated_document(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
    title: &str,
) -> Result<Value, CliError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(CliError::with_code(
            "invalid_generated_document",
            "Generated document title cannot be empty.",
        ));
    }
    let mut wiki = get_wiki_bootstrap(paths)?;
    let document = find_generated_document_mut(&mut wiki.state, document_id)?;
    document["title"] = json!(title);
    document["updatedAt"] = json!(now_iso());
    let document = document.clone();
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "document": document, "state": wiki.state }))
}

pub fn delete_generated_document(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let storage = Storage::open(paths)?;
    let documents = state_array_mut(&mut wiki.state, "generatedDocuments")?;
    let index = documents
        .iter()
        .position(|document| document["id"].as_str() == Some(document_id))
        .ok_or_else(|| {
            CliError::with_code(
                "not_found",
                format!("Wiki generated document not found: {document_id}"),
            )
        })?;
    let document = documents.remove(index);
    let generated_dir = storage
        .panel_dir(&wiki.project.id, &wiki.panel.id)
        .join("generated")
        .join(sanitize_path_part(document_id));
    crate::tasks::cancel_tasks_for_resource(
        paths,
        &wiki.project.id,
        "writing.targetDocument",
        document_id,
        "prerequisite_deleted",
    )?;
    crate::content::archive_resource(
        paths,
        Some(&wiki.project.id),
        crate::content::ResourceKind::GeneratedDocument,
        document_id,
    )?;
    if let Err(error) = fs::remove_dir_all(generated_dir) {
        if error.kind() != std::io::ErrorKind::NotFound {
            return Err(to_cli_error(error));
        }
    }
    save_wiki_state(paths, &wiki)?;
    if let Some(mut selection) = storage.read_panel_selection(&wiki.project.id, &wiki.panel.id)? {
        if let Some(selected_ids) = selection
            .get_mut("selectedGeneratedDocumentIds")
            .and_then(Value::as_array_mut)
        {
            selected_ids.retain(|value| value.as_str() != Some(document_id));
            selection["updatedAt"] = json!(now_iso());
            storage.write_panel_selection(&wiki.project.id, &wiki.panel.id, &selection)?;
        }
    }
    Ok(json!({ "document": document, "state": wiki.state }))
}

pub fn publish_generated_document(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
    wiki_space_id: Option<&str>,
) -> Result<Value, CliError> {
    let generated = read_generated_document(paths, document_id)?;
    let document = &generated["document"];
    let version = document["contentVersion"].as_u64().unwrap_or(1);
    let already_published = document["publishHistory"]
        .as_array()
        .is_some_and(|history| {
            history
                .iter()
                .any(|entry| entry["generatedVersion"].as_u64() == Some(version))
        });
    if already_published {
        return Err(CliError::with_code(
            "already_published",
            format!("Generated document version {version} is already published."),
        ));
    }
    let raw = add_raw_document(
        paths,
        document["originalFileName"]
            .as_str()
            .unwrap_or("document.md"),
        document["title"].as_str(),
        document["mimeType"].as_str(),
        "agent",
        wiki_space_id,
        generated["content"].as_str().unwrap_or("").as_bytes(),
    )?;
    let raw_document_id = raw["document"]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let mut wiki = get_wiki_bootstrap(paths)?;
    let generated_document = find_generated_document_mut(&mut wiki.state, document_id)?;
    state_array_mut(generated_document, "publishHistory")?.push(json!({
        "generatedVersion": version,
        "rawDocumentId": raw_document_id,
        "publishedAt": now_iso(),
    }));
    let generated_document = generated_document.clone();
    save_wiki_state(paths, &wiki)?;
    Ok(json!({
        "document": generated_document,
        "rawDocument": raw["document"],
        "state": wiki.state,
    }))
}
