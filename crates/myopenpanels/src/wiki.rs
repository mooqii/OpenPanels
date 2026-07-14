use crate::control::{now_iso, read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::{sanitize_path_part, MyOpenPanelsPaths};
use crate::storage::Storage;
use crate::trace::{self, TraceEventInput};
use crate::types::PanelKind;
use rand::Rng;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod state;

use state::*;

pub const WIKI_PANEL_SKILL_ID: &str = "wiki-panel";

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
    let wiki = get_wiki_bootstrap(paths)?;
    Ok(json!({
        "project": wiki.project,
        "panel": wiki.panel,
        "state": wiki.state,
    }))
}

pub fn read_agent_selection(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let wiki_panel = bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
        .ok_or_else(|| CliError::new("The current Project has no Wiki panel."))?;
    let wiki = get_wiki_target(paths, &bootstrap.project.id, &wiki_panel.panel.id)?;
    let storage = Storage::open(paths)?;
    let stored = storage
        .read_panel_selection(&wiki.project.id, &wiki.panel.id)?
        .unwrap_or_else(|| json!({}));
    let requested_document_ids = stored
        .get("selectedRawDocumentIds")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let documents = wiki
        .state
        .get("rawDocuments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let selected_documents = requested_document_ids
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|document_id| {
            let document = documents
                .iter()
                .find(|document| document.get("id").and_then(Value::as_str) == Some(document_id))?;
            let mut item = document.clone();
            if let Some(original_ref) = document.get("originalRef").and_then(Value::as_str) {
                if let Ok(path) = wiki_panel_path(
                    &storage.panel_dir(&wiki.project.id, &wiki.panel.id),
                    original_ref,
                ) {
                    item["originalFilePath"] = json!(path.display().to_string());
                }
            }
            Some(item)
        })
        .collect::<Vec<_>>();
    let selected_document_ids = selected_documents
        .iter()
        .filter_map(|document| document.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    let requested_generated_document_ids = stored
        .get("selectedGeneratedDocumentIds")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let generated_documents = wiki
        .state
        .get("generatedDocuments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let panel_dir = storage.panel_dir(&wiki.project.id, &wiki.panel.id);
    let selected_generated_documents = requested_generated_document_ids
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|document_id| {
            let document = generated_documents
                .iter()
                .find(|document| document.get("id").and_then(Value::as_str) == Some(document_id))?;
            let mut item = document.clone();
            if let Some(content_ref) = document.get("contentRef").and_then(Value::as_str) {
                if let Ok(path) = wiki_panel_path(&panel_dir, content_ref) {
                    item["contentFilePath"] = json!(path.display().to_string());
                }
            }
            Some(item)
        })
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
            WIKI_PANEL_SKILL_ID.to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ],
    )
    .expect("registered Wiki Panel Skill action");
    skill_action["loadWhen"] = json!("The user request requires Wiki query guidance.");
    Ok(json!({
        "selection": selection,
        "wiki": {
            "available": true,
            "selected": is_wiki_selected,
            "wikiSpaceId": wiki_space.id,
            "title": wiki_space.value.get("title").cloned().unwrap_or_else(|| json!("Wiki")),
            "pageCount": page_count,
            "querySkillId": WIKI_PANEL_SKILL_ID,
            "loadCommand": format!("myopenpanels agent skill read --skill-id {WIKI_PANEL_SKILL_ID} --format json"),
        },
        "nextActions": [skill_action],
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
    let wiki = get_wiki_bootstrap(paths)?;
    Ok(json!({
        "documents": wiki.state.get("generatedDocuments").cloned().unwrap_or_else(|| json!([]))
    }))
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
    let extension = Path::new(&safe_file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or(if format == "markdown" { "md" } else { "txt" });
    let content_ref = wiki_ref(&["generated", &document_id, &format!("content.{extension}")]);
    let content_path = wiki_panel_path(
        &storage.panel_dir(&wiki.project.id, &wiki.panel.id),
        &content_ref,
    )?;
    if let Some(parent) = content_path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(&content_path, content).map_err(to_cli_error)?;
    let now = now_iso();
    let document = json!({
        "id": document_id,
        "title": title.filter(|value| !value.trim().is_empty()).unwrap_or_else(|| title_from_file_name(&safe_file_name)),
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
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_target(paths, project_id, panel_id)?;
    let now = now_iso();
    if let Some(document_id) = document_id {
        let document = find_generated_document_mut(&mut wiki.state, document_id)?;
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
            if documents[index]["contentVersion"].as_u64().unwrap_or(0) == 0 {
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
    let wiki = get_wiki_bootstrap(paths)?;
    let storage = Storage::open(paths)?;
    let document = find_generated_document(&wiki.state, document_id)?.clone();
    let content_ref = document
        .get("contentRef")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CliError::new(format!(
                "Wiki generated document has no contentRef: {document_id}"
            ))
        })?;
    let content_path = wiki_panel_path(
        &storage.panel_dir(&wiki.project.id, &wiki.panel.id),
        content_ref,
    )?;
    let content = fs::read_to_string(&content_path).map_err(to_cli_error)?;
    Ok(json!({ "document": document, "content": content, "contentFilePath": content_path }))
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
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or(if format == "markdown" { "md" } else { "txt" });
    let content_ref = wiki_ref(&["generated", document_id, &format!("content.{extension}")]);
    let panel_dir = storage.panel_dir(&wiki.project.id, &wiki.panel.id);
    let content_path = wiki_panel_path(&panel_dir, &content_ref)?;
    if let Some(parent) = content_path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(&content_path, content).map_err(to_cli_error)?;
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

pub fn add_raw_document(
    paths: &MyOpenPanelsPaths,
    file_name: &str,
    title: Option<&str>,
    mime_type: Option<&str>,
    source: &str,
    wiki_space_id: Option<&str>,
    content: &[u8],
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let storage = Storage::open(paths)?;
    let now = now_iso();
    let wiki_space = resolve_wiki_space(&wiki.state, wiki_space_id)?;
    let safe_file_name = sanitize_path_part(file_name);
    let document_id = create_id("raw");
    let original_ref = wiki_ref(&["raw", &document_id, "original", &safe_file_name]);
    let markdown_ref = wiki_ref(&["raw", &document_id, "source.md"]);
    let panel_dir = storage.panel_dir(&wiki.project.id, &wiki.panel.id);
    let original_path = wiki_panel_path(&panel_dir, &original_ref)?;
    if let Some(parent) = original_path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(&original_path, content).map_err(to_cli_error)?;

    let is_text = is_plain_text_file(&safe_file_name, mime_type);
    let word_count = if is_text {
        std::str::from_utf8(content).ok().map(character_count)
    } else {
        None
    };
    if is_text {
        let markdown_path = wiki_panel_path(&panel_dir, &markdown_ref)?;
        if let Some(parent) = markdown_path.parent() {
            fs::create_dir_all(parent).map_err(to_cli_error)?;
        }
        fs::write(&markdown_path, content).map_err(to_cli_error)?;
    }

    let conversion_task = if is_text {
        None
    } else {
        Some(create_wiki_task(
            &wiki.state,
            &mut wiki.tasks,
            "convert_document_to_markdown",
            &document_id,
            Some(&document_id),
            Some(0),
            Some(wiki_space.id.as_str()),
        )?)
    };
    let ingest_task = if is_text {
        Some(create_wiki_task(
            &wiki.state,
            &mut wiki.tasks,
            "ingest_markdown_into_wiki",
            &document_id,
            Some(&document_id),
            Some(1),
            Some(wiki_space.id.as_str()),
        )?)
    } else {
        None
    };
    let mut ingestion = Map::new();
    ingestion.insert(
        wiki_space.id.clone(),
        if let Some(task) = &ingest_task {
            json!({
                "status": "queued",
                "taskId": task["id"],
                "markdownVersion": 1,
                "error": null,
                "updatedAt": task["updatedAt"],
            })
        } else {
            json!({
                "status": "not_started",
                "taskId": null,
                "markdownVersion": 0,
                "error": null,
                "updatedAt": now,
            })
        },
    );
    let document = json!({
        "id": document_id,
        "title": title.filter(|value| !value.trim().is_empty()).unwrap_or_else(|| title_from_file_name(&safe_file_name)),
        "originalFileName": safe_file_name,
        "mimeType": mime_type.unwrap_or_else(|| mime_type_for_file(file_name)),
        "sizeBytes": content.len(),
        "sha256": sha256_hex(content),
        "source": if source == "agent" { "agent" } else { "user" },
        "originalRef": original_ref,
        "markdownRef": if is_text { Value::String(markdown_ref.clone()) } else { Value::Null },
        "markdownVersion": if is_text { 1 } else { 0 },
        "wordCount": word_count,
        "conversion": {
            "status": if is_text { "not_required" } else { "queued" },
            "taskId": conversion_task.as_ref().map(|task| task["id"].clone()),
            "error": null,
            "updatedAt": now,
        },
        "ingestionByWikiSpace": Value::Object(ingestion),
        "createdAt": now,
        "updatedAt": now,
    });
    state_array_mut(&mut wiki.state, "rawDocuments")?.insert(0, document.clone());
    state_object_mut(&mut wiki.state)?
        .insert("activeRawDocumentId".to_owned(), document["id"].clone());
    let meta_path = wiki_panel_path(
        &panel_dir,
        &wiki_ref(&["raw", document["id"].as_str().unwrap_or("raw"), "meta.json"]),
    )?;
    if let Some(parent) = meta_path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(
        meta_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&document).map_err(to_cli_error)?
        ),
    )
    .map_err(to_cli_error)?;
    save_wiki_state(paths, &wiki)?;
    trace::record_simple(
        "task",
        "wiki",
        Some("document"),
        format!(
            "Imported {}",
            document["title"].as_str().unwrap_or("document")
        ),
        Some(format!(
            "Imported {}",
            document["title"].as_str().unwrap_or("document")
        )),
        Some(json!({ "document": document.clone() })),
    );
    Ok(json!({ "document": document, "state": wiki.state }))
}

pub struct WikiOriginalFile {
    pub document: Value,
    pub file_path: PathBuf,
    pub mime_type: String,
    pub size_bytes: u64,
}

pub fn raw_document_original(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
) -> Result<WikiOriginalFile, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    let document = find_document(&wiki.state, document_id)?.clone();
    let original_ref = document
        .get("originalRef")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CliError::new(format!(
                "Wiki raw document has no originalRef: {document_id}"
            ))
        })?;
    let storage = Storage::open(paths)?;
    let file_path = wiki_panel_path(
        &storage.panel_dir(&wiki.project.id, &wiki.panel.id),
        original_ref,
    )?;
    let metadata = fs::metadata(&file_path).map_err(to_cli_error)?;
    if !metadata.is_file() {
        return Err(CliError::new(format!(
            "Wiki raw document original is not a file: {document_id}"
        )));
    }
    let mime_type = document
        .get("mimeType")
        .and_then(Value::as_str)
        .unwrap_or_else(|| mime_type_for_file(file_path.to_string_lossy().as_ref()))
        .to_owned();
    Ok(WikiOriginalFile {
        document,
        file_path,
        mime_type,
        size_bytes: metadata.len(),
    })
}

pub fn reveal_raw_document_original(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
) -> Result<Value, CliError> {
    let original = raw_document_original(paths, document_id)?;
    let command = if cfg!(target_os = "macos") {
        Some((
            "open",
            vec!["-R".to_owned(), original.file_path.display().to_string()],
        ))
    } else if cfg!(target_os = "windows") {
        Some((
            "explorer.exe",
            vec![format!("/select,{}", original.file_path.display())],
        ))
    } else if cfg!(target_os = "linux") {
        original
            .file_path
            .parent()
            .map(|parent| ("xdg-open", vec![parent.display().to_string()]))
    } else {
        None
    };
    if let Some((program, args)) = command {
        let _ = Command::new(program).args(args).spawn();
    }
    Ok(json!({
        "document": original.document,
        "filePath": original.file_path,
        "revealed": true,
    }))
}

pub fn delete_raw_document(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
    wiki_space_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let storage = Storage::open(paths)?;
    let wiki_space = resolve_wiki_space(&wiki.state, wiki_space_id)?;
    let documents = state_array_mut(&mut wiki.state, "rawDocuments")?;
    let index = documents
        .iter()
        .position(|document| document.get("id").and_then(Value::as_str) == Some(document_id))
        .ok_or_else(|| CliError::new(format!("Wiki raw document not found: {document_id}")))?;
    let document = documents.remove(index);
    let now = now_iso();
    for task in &mut wiki.tasks {
        let matches_document = task.get("documentId").and_then(Value::as_str) == Some(document_id)
            || task.get("targetId").and_then(Value::as_str) == Some(document_id);
        if matches_document {
            task["status"] = json!("stale");
            task["error"] = json!("Source document deleted");
            task["updatedAt"] = json!(now);
        }
    }
    let task = create_wiki_task(
        &wiki.state,
        &mut wiki.tasks,
        "rebuild_wiki_index",
        "index.md",
        None,
        None,
        Some(wiki_space.id.as_str()),
    )?;
    if wiki
        .state
        .get("activeRawDocumentId")
        .and_then(Value::as_str)
        == Some(document_id)
    {
        let next_id = wiki
            .state
            .get("rawDocuments")
            .and_then(Value::as_array)
            .and_then(|documents| documents.first())
            .and_then(|document| document.get("id"))
            .cloned()
            .unwrap_or(Value::Null);
        state_object_mut(&mut wiki.state)?.insert("activeRawDocumentId".to_owned(), next_id);
    }
    let raw_dir = storage
        .panel_dir(&wiki.project.id, &wiki.panel.id)
        .join("raw")
        .join(sanitize_path_part(document_id));
    fs::remove_dir_all(raw_dir)
        .or_else(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(error)
            }
        })
        .map_err(to_cli_error)?;
    save_wiki_state(paths, &wiki)?;
    if let Some(mut selection) = storage.read_panel_selection(&wiki.project.id, &wiki.panel.id)? {
        if let Some(selected_ids) = selection
            .get_mut("selectedRawDocumentIds")
            .and_then(Value::as_array_mut)
        {
            selected_ids.retain(|value| value.as_str() != Some(document_id));
            selection["updatedAt"] = json!(now_iso());
            storage.write_panel_selection(&wiki.project.id, &wiki.panel.id, &selection)?;
        }
    }
    Ok(json!({ "document": document, "task": task, "state": wiki.state }))
}

pub fn extract_raw_document_markdown(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
    wiki_space_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let _ = find_document(&wiki.state, document_id)?;
    let wiki_space = resolve_wiki_space(&wiki.state, wiki_space_id)?;
    let now = now_iso();
    for task in &mut wiki.tasks {
        let active_status = matches!(
            task.get("status").and_then(Value::as_str),
            Some("queued" | "claimed" | "running" | "failed")
        );
        if task.get("documentId").and_then(Value::as_str) == Some(document_id)
            && task.get("type").and_then(Value::as_str) == Some("convert_document_to_markdown")
            && active_status
        {
            task["status"] = json!("stale");
            task["error"] = json!("Superseded by a new extraction request");
            task["updatedAt"] = json!(now);
        }
    }
    let markdown_version = find_document(&wiki.state, document_id)?
        .get("markdownVersion")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let task = create_wiki_task(
        &wiki.state,
        &mut wiki.tasks,
        "convert_document_to_markdown",
        document_id,
        Some(document_id),
        Some(markdown_version),
        Some(wiki_space.id.as_str()),
    )?;
    let document = find_document_mut(&mut wiki.state, document_id)?;
    document["conversion"] = json!({
        "status": "queued",
        "taskId": task["id"],
        "error": null,
        "updatedAt": task["updatedAt"],
    });
    document["updatedAt"] = json!(now_iso());
    let document = document.clone();
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "document": document, "task": task, "state": wiki.state }))
}

pub fn reindex_raw_document(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
    wiki_space_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let wiki_space = resolve_wiki_space(&wiki.state, wiki_space_id)?;
    let document = find_document(&wiki.state, document_id)?;
    if document
        .get("markdownRef")
        .and_then(Value::as_str)
        .is_none()
    {
        return Err(CliError::new(
            "Source Markdown is required before indexing.",
        ));
    }
    let markdown_version = document
        .get("markdownVersion")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let task = create_wiki_task(
        &wiki.state,
        &mut wiki.tasks,
        "ingest_markdown_into_wiki",
        document_id,
        Some(document_id),
        Some(markdown_version),
        Some(wiki_space.id.as_str()),
    )?;
    let document = find_document_mut(&mut wiki.state, document_id)?;
    let ingestion = document
        .as_object_mut()
        .ok_or_else(|| CliError::new("Wiki raw document is invalid."))?
        .entry("ingestionByWikiSpace")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| CliError::new("Wiki raw document ingestion state is invalid."))?;
    ingestion.insert(
        wiki_space.id.clone(),
        json!({
            "status": "queued",
            "taskId": task["id"],
            "markdownVersion": markdown_version,
            "error": null,
            "updatedAt": task["updatedAt"],
        }),
    );
    document["updatedAt"] = task["updatedAt"].clone();
    let document = document.clone();
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "document": document, "task": task, "state": wiki.state }))
}

pub fn claim_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let mut wiki = get_wiki_task_target(paths, task_id)?;
    let task = task_mut(&mut wiki.tasks, task_id)?;
    let status = task.get("status").and_then(Value::as_str).unwrap_or("");
    if status != "queued" && status != "failed" && status != "reserved" && !task_lease_expired(task)
    {
        return Err(CliError::new(format!(
            "Wiki task is not claimable: {task_id}"
        )));
    }
    let now = now_iso();
    let task_type = task.get("type").and_then(Value::as_str).unwrap_or("");
    task["status"] = json!(if task_type == "convert_document_to_markdown" {
        "running"
    } else {
        "claimed"
    });
    task["error"] = Value::Null;
    task["attempt"] = json!(task.get("attempt").and_then(Value::as_i64).unwrap_or(0) + 1);
    if task.get("maxAttempts").and_then(Value::as_i64).is_none() {
        task["maxAttempts"] = json!(3);
    }
    task["leaseOwner"] = Value::Null;
    task["leaseExpiresAt"] = json!(lease_expires_at(15));
    task["lastHeartbeatAt"] = json!(now);
    task["retryAfter"] = Value::Null;
    task["updatedAt"] = json!(now);
    let task_snapshot = task.clone();
    if task_snapshot.get("type").and_then(Value::as_str) == Some("convert_document_to_markdown") {
        if let Some(document_id) = task_snapshot.get("documentId").and_then(Value::as_str) {
            let document = find_document_mut(&mut wiki.state, document_id)?;
            document["conversion"]["status"] = json!("converting");
            document["conversion"]["updatedAt"] = json!(now);
            document["updatedAt"] = json!(now);
        }
    }
    if task_snapshot.get("type").and_then(Value::as_str) == Some("ingest_markdown_into_wiki") {
        if let Some(document_id) = task_snapshot.get("documentId").and_then(Value::as_str) {
            let wiki_space_id = task_snapshot
                .get("wikiSpaceId")
                .and_then(Value::as_str)
                .unwrap_or("wiki:default")
                .to_owned();
            let task_id = task_snapshot
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or(task_id)
                .to_owned();
            let markdown_version = task_snapshot
                .get("markdownVersion")
                .and_then(Value::as_i64)
                .or_else(|| {
                    find_document(&wiki.state, document_id)
                        .ok()
                        .and_then(|document| {
                            document.get("markdownVersion").and_then(Value::as_i64)
                        })
                })
                .unwrap_or(0);
            let document = find_document_mut(&mut wiki.state, document_id)?;
            let ingestion = document
                .as_object_mut()
                .ok_or_else(|| CliError::new("Wiki raw document is invalid."))?
                .entry("ingestionByWikiSpace")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(|| CliError::new("Wiki raw document ingestion state is invalid."))?;
            let mut current = ingestion
                .get(&wiki_space_id)
                .cloned()
                .unwrap_or_else(|| json!({}));
            current["status"] = json!("ingesting");
            current["taskId"] = json!(task_id);
            current["markdownVersion"] = json!(markdown_version);
            current["error"] = Value::Null;
            current["updatedAt"] = json!(now);
            ingestion.insert(wiki_space_id, current);
            document["updatedAt"] = json!(now);
        }
    }
    save_wiki_state(paths, &wiki)?;
    trace_task_event(
        "claimed",
        &task_snapshot,
        format!(
            "Agent claimed {}",
            task_type_label(
                task_snapshot
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            )
        ),
        Some(format!(
            "Started {}",
            task_type_label(
                task_snapshot
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            )
        )),
    );
    Ok(json!({ "task": task_snapshot, "state": wiki.state }))
}

pub fn complete_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    result: Option<Value>,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_task_target(paths, task_id)?;
    let now = now_iso();
    let task_snapshot = {
        let task = task_mut(&mut wiki.tasks, task_id)?;
        task["status"] = json!("succeeded");
        task["error"] = Value::Null;
        task["result"] = result.unwrap_or(Value::Null);
        task["leaseOwner"] = Value::Null;
        task["leaseExpiresAt"] = Value::Null;
        task["lastHeartbeatAt"] = Value::Null;
        task["retryAfter"] = Value::Null;
        task["updatedAt"] = json!(now);
        task.clone()
    };
    let mut follow_up_task_ids = Vec::new();
    if task_snapshot.get("type").and_then(Value::as_str) == Some("convert_document_to_markdown") {
        if let Some(document_id) = task_snapshot.get("documentId").and_then(Value::as_str) {
            let markdown_version = {
                let document = find_document_mut(&mut wiki.state, document_id)?;
                document["conversion"]["status"] = json!("ready");
                document["conversion"]["error"] = Value::Null;
                document["conversion"]["updatedAt"] = json!(now);
                document["updatedAt"] = json!(now);
                document
                    .get("markdownVersion")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
            };
            let mut ingest_task = create_wiki_task(
                &wiki.state,
                &mut wiki.tasks,
                "ingest_markdown_into_wiki",
                document_id,
                Some(document_id),
                Some(markdown_version),
                task_snapshot.get("wikiSpaceId").and_then(Value::as_str),
            )?;
            if let Some(agent_skill_id) = task_snapshot.get("agentSkillId").cloned() {
                ingest_task["agentSkillId"] = agent_skill_id.clone();
                let ingest_task_id = ingest_task
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                task_mut(&mut wiki.tasks, &ingest_task_id)?["agentSkillId"] = agent_skill_id;
            }
            follow_up_task_ids.push(
                ingest_task
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            );
            let wiki_space_id = ingest_task
                .get("wikiSpaceId")
                .and_then(Value::as_str)
                .unwrap_or("wiki:default")
                .to_owned();
            let document = find_document_mut(&mut wiki.state, document_id)?;
            let ingestion = document
                .as_object_mut()
                .ok_or_else(|| CliError::new("Wiki raw document is invalid."))?
                .entry("ingestionByWikiSpace")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(|| CliError::new("Wiki raw document ingestion state is invalid."))?;
            ingestion.insert(
                wiki_space_id,
                create_ingestion_state(&ingest_task, markdown_version),
            );
            document["updatedAt"] = json!(now);
        }
    }
    if task_snapshot.get("type").and_then(Value::as_str) == Some("ingest_markdown_into_wiki") {
        if let Some(document_id) = task_snapshot.get("documentId").and_then(Value::as_str) {
            let wiki_space_id = task_snapshot
                .get("wikiSpaceId")
                .and_then(Value::as_str)
                .unwrap_or("wiki:default")
                .to_owned();
            let document = find_document_mut(&mut wiki.state, document_id)?;
            let ingestion = document
                .get_mut("ingestionByWikiSpace")
                .and_then(Value::as_object_mut)
                .and_then(|ingestion| ingestion.get_mut(&wiki_space_id));
            if let Some(ingestion) = ingestion {
                ingestion["status"] = json!("ingested");
                ingestion["error"] = Value::Null;
                ingestion["updatedAt"] = json!(now);
            }
            document["updatedAt"] = json!(now);
        }
    }
    save_wiki_state(paths, &wiki)?;
    trace_task_event(
        "completed",
        &task_snapshot,
        format!(
            "Completed {}",
            task_type_label(
                task_snapshot
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            )
        ),
        Some(format!(
            "Completed {}",
            task_type_label(
                task_snapshot
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            )
        )),
    );
    Ok(json!({ "task": task_snapshot, "state": wiki.state }))
}

pub fn fail_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    message: &str,
) -> Result<Value, CliError> {
    fail_task_with_retry(paths, task_id, message, None)
}

pub fn fail_task_with_retry(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    message: &str,
    retry_after: Option<&str>,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_task_target(paths, task_id)?;
    let now = now_iso();
    let task_snapshot = {
        let task = task_mut(&mut wiki.tasks, task_id)?;
        task["status"] = json!("failed");
        task["error"] = json!(message);
        task["leaseOwner"] = Value::Null;
        task["leaseExpiresAt"] = Value::Null;
        task["lastHeartbeatAt"] = Value::Null;
        task["retryAfter"] = retry_after.map_or(Value::Null, Value::from);
        task["updatedAt"] = json!(now);
        task.clone()
    };
    if task_snapshot.get("type").and_then(Value::as_str) == Some("convert_document_to_markdown") {
        if let Some(document_id) = task_snapshot.get("documentId").and_then(Value::as_str) {
            let document = find_document_mut(&mut wiki.state, document_id)?;
            document["conversion"]["status"] = json!("failed");
            document["conversion"]["error"] = json!(message);
            document["conversion"]["updatedAt"] = json!(now);
            document["updatedAt"] = json!(now);
        }
    }
    if task_snapshot.get("type").and_then(Value::as_str) == Some("ingest_markdown_into_wiki") {
        if let Some(document_id) = task_snapshot.get("documentId").and_then(Value::as_str) {
            let wiki_space_id = task_snapshot
                .get("wikiSpaceId")
                .and_then(Value::as_str)
                .unwrap_or("wiki:default")
                .to_owned();
            let document = find_document_mut(&mut wiki.state, document_id)?;
            let ingestion = document
                .as_object_mut()
                .ok_or_else(|| CliError::new("Wiki raw document is invalid."))?
                .entry("ingestionByWikiSpace")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(|| CliError::new("Wiki raw document ingestion state is invalid."))?;
            let mut current = ingestion
                .get(&wiki_space_id)
                .cloned()
                .unwrap_or_else(|| create_ingestion_state(&task_snapshot, 0));
            current["status"] = json!("failed");
            current["error"] = json!(message);
            current["updatedAt"] = json!(now);
            ingestion.insert(wiki_space_id, current);
            document["updatedAt"] = json!(now);
        }
    }
    save_wiki_state(paths, &wiki)?;
    trace_task_event(
        "failed",
        &task_snapshot,
        format!("Task failed: {message}"),
        Some(format!("Task failed: {message}")),
    );
    Ok(json!({ "task": task_snapshot, "state": wiki.state }))
}

pub fn heartbeat_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_expires_at: &str,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_task_target(paths, task_id)?;
    let now = now_iso();
    let task_snapshot = {
        let task = task_mut(&mut wiki.tasks, task_id)?;
        if !matches!(
            task.get("status").and_then(Value::as_str),
            Some("running" | "claimed" | "converting" | "indexing")
        ) {
            return Err(CliError::new(format!(
                "Wiki task is not running: {task_id}"
            )));
        }
        task["leaseExpiresAt"] = json!(lease_expires_at);
        task["lastHeartbeatAt"] = json!(now);
        task["updatedAt"] = json!(now);
        task.clone()
    };
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "task": task_snapshot, "state": wiki.state }))
}

pub fn release_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let mut wiki = get_wiki_task_target(paths, task_id)?;
    let now = now_iso();
    let task_snapshot = {
        let task = task_mut(&mut wiki.tasks, task_id)?;
        task["status"] = json!("queued");
        task["error"] = Value::Null;
        task["leaseOwner"] = Value::Null;
        task["leaseExpiresAt"] = Value::Null;
        task["lastHeartbeatAt"] = Value::Null;
        task["retryAfter"] = Value::Null;
        task["updatedAt"] = json!(now);
        task.clone()
    };
    reset_document_task_state(&mut wiki.state, &task_snapshot, "queued", None, &now)?;
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "task": task_snapshot, "state": wiki.state }))
}

pub fn retry_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let mut wiki = get_wiki_task_target(paths, task_id)?;
    let now = now_iso();
    let task_snapshot = {
        let task = task_mut(&mut wiki.tasks, task_id)?;
        if !matches!(
            task.get("status").and_then(Value::as_str),
            Some("failed" | "queued")
        ) {
            return Err(CliError::new(format!(
                "Wiki task cannot be retried: {task_id}"
            )));
        }
        task["status"] = json!("queued");
        task["attempt"] = json!(0);
        task["error"] = Value::Null;
        task["result"] = Value::Null;
        task["leaseOwner"] = Value::Null;
        task["leaseExpiresAt"] = Value::Null;
        task["lastHeartbeatAt"] = Value::Null;
        task["retryAfter"] = Value::Null;
        task["updatedAt"] = json!(now);
        task.clone()
    };
    reset_document_task_state(&mut wiki.state, &task_snapshot, "queued", None, &now)?;
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "task": task_snapshot, "state": wiki.state }))
}

pub fn cancel_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let mut wiki = get_wiki_task_target(paths, task_id)?;
    let now = now_iso();
    let task_snapshot = {
        let task = task_mut(&mut wiki.tasks, task_id)?;
        if task.get("status").and_then(Value::as_str) == Some("succeeded") {
            return Err(CliError::new(format!(
                "Completed Wiki task cannot be cancelled: {task_id}"
            )));
        }
        task["status"] = json!("cancelled");
        task["error"] = Value::Null;
        task["leaseOwner"] = Value::Null;
        task["leaseExpiresAt"] = Value::Null;
        task["lastHeartbeatAt"] = Value::Null;
        task["retryAfter"] = Value::Null;
        task["updatedAt"] = json!(now);
        task.clone()
    };
    reset_document_task_state(&mut wiki.state, &task_snapshot, "cancelled", None, &now)?;
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "task": task_snapshot, "state": wiki.state }))
}

fn reset_document_task_state(
    state: &mut Value,
    task: &Value,
    status: &str,
    error: Option<&str>,
    now: &str,
) -> Result<(), CliError> {
    let Some(document_id) = task.get("documentId").and_then(Value::as_str) else {
        return Ok(());
    };
    if task.get("type").and_then(Value::as_str) == Some("convert_document_to_markdown") {
        let document = find_document_mut(state, document_id)?;
        document["conversion"]["status"] = json!(status);
        document["conversion"]["error"] = error.map_or(Value::Null, Value::from);
        document["conversion"]["updatedAt"] = json!(now);
        document["updatedAt"] = json!(now);
    } else if task.get("type").and_then(Value::as_str) == Some("ingest_markdown_into_wiki") {
        let wiki_space_id = task
            .get("wikiSpaceId")
            .and_then(Value::as_str)
            .unwrap_or("wiki:default")
            .to_owned();
        let document = find_document_mut(state, document_id)?;
        if let Some(ingestion) = document
            .get_mut("ingestionByWikiSpace")
            .and_then(Value::as_object_mut)
            .and_then(|ingestion| ingestion.get_mut(&wiki_space_id))
        {
            ingestion["status"] = json!(status);
            ingestion["error"] = error.map_or(Value::Null, Value::from);
            ingestion["updatedAt"] = json!(now);
        }
        document["updatedAt"] = json!(now);
    }
    Ok(())
}

fn lease_expires_at(minutes: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::minutes(minutes))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn task_lease_expired(task: &Value) -> bool {
    let Some(expires_at) = task.get("leaseExpiresAt").and_then(Value::as_str) else {
        return false;
    };
    chrono::DateTime::parse_from_rfc3339(expires_at)
        .map(|expires_at| expires_at.with_timezone(&chrono::Utc) <= chrono::Utc::now())
        .unwrap_or(false)
}

pub fn read_markdown(paths: &MyOpenPanelsPaths, document_id: &str) -> Result<Value, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    let document = find_document(&wiki.state, document_id)?.clone();
    let markdown = if let Some(markdown_ref) = document.get("markdownRef").and_then(Value::as_str) {
        let storage = Storage::open(paths)?;
        let path = wiki_panel_path(
            &storage.panel_dir(&wiki.project.id, &wiki.panel.id),
            markdown_ref,
        )?;
        fs::read_to_string(path).unwrap_or_default()
    } else {
        String::new()
    };
    Ok(json!({ "document": document, "markdown": markdown }))
}

pub fn write_markdown(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
    content: &str,
    task_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut wiki = match task_id {
        Some(task_id) => get_wiki_task_target(paths, task_id)?,
        None => get_wiki_bootstrap(paths)?,
    };
    let storage = Storage::open(paths)?;
    let now = now_iso();
    let parent_task = task_id
        .map(|id| task_value(&wiki.tasks, id).cloned())
        .transpose()?;
    let should_queue_ingest = parent_task
        .as_ref()
        .and_then(|task| task.get("type").and_then(Value::as_str))
        != Some("convert_document_to_markdown");
    let wiki_space_id = resolve_wiki_space(
        &wiki.state,
        parent_task
            .as_ref()
            .and_then(|task| task.get("wikiSpaceId").and_then(Value::as_str)),
    )?
    .id;
    let document = find_document_mut(&mut wiki.state, document_id)?;
    let markdown_ref = document
        .get("markdownRef")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| wiki_ref(&["raw", document_id, "source.md"]));
    let path = wiki_panel_path(
        &storage.panel_dir(&wiki.project.id, &wiki.panel.id),
        &markdown_ref,
    )?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(path, content).map_err(to_cli_error)?;
    let version = document
        .get("markdownVersion")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        + 1;
    document["markdownRef"] = json!(markdown_ref);
    document["markdownVersion"] = json!(version);
    document["wordCount"] = json!(character_count(content));
    document["updatedAt"] = json!(now);
    let conversion_status = if document
        .get("conversion")
        .and_then(|conversion| conversion.get("status"))
        .and_then(Value::as_str)
        == Some("not_required")
    {
        "not_required"
    } else {
        "ready"
    };
    document["conversion"]["status"] = json!(conversion_status);
    document["conversion"]["error"] = Value::Null;
    document["conversion"]["updatedAt"] = json!(now);
    let task = if should_queue_ingest {
        let task = create_wiki_task(
            &wiki.state,
            &mut wiki.tasks,
            "ingest_markdown_into_wiki",
            document_id,
            Some(document_id),
            Some(version),
            Some(&wiki_space_id),
        )?;
        let document = find_document_mut(&mut wiki.state, document_id)?;
        let ingestion = document
            .as_object_mut()
            .ok_or_else(|| CliError::new("Wiki raw document is invalid."))?
            .entry("ingestionByWikiSpace")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| CliError::new("Wiki raw document ingestion state is invalid."))?;
        ingestion.insert(
            wiki_space_id.clone(),
            create_ingestion_state(&task, version),
        );
        Some(task)
    } else {
        None
    };
    let rebuild_task = if should_queue_ingest {
        Some(create_wiki_rebuild_index_task(
            &wiki.state,
            &mut wiki.tasks,
            Some(&wiki_space_id),
        )?)
    } else {
        None
    };
    save_wiki_state(paths, &wiki)?;
    let document = find_document(&wiki.state, document_id)?.clone();
    Ok(
        json!({ "document": document, "rebuildTask": rebuild_task, "task": task, "state": wiki.state }),
    )
}

pub fn set_agent_skill(paths: &MyOpenPanelsPaths, skill_id: &str) -> Result<Value, CliError> {
    if !matches!(skill_id, "karpathy-llm-wiki" | "karpathy-llm-wiki-zh") {
        return Err(CliError::new(
            "Expected wiki agent skill to be one of: karpathy-llm-wiki, karpathy-llm-wiki-zh",
        ));
    }
    let mut wiki = get_wiki_bootstrap(paths)?;
    let state = state_object_mut(&mut wiki.state)?;
    state.insert("wikiAgentSkillId".to_owned(), json!(skill_id));
    state.insert("wikiAgentSkillConfigured".to_owned(), json!(true));
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "agentSkillId": skill_id, "state": wiki.state }))
}

pub fn selected_agent_skill_id(state: &Value) -> &str {
    match state.get("wikiAgentSkillId").and_then(Value::as_str) {
        Some("karpathy-llm-wiki-zh") => "karpathy-llm-wiki-zh",
        _ => "karpathy-llm-wiki",
    }
}

pub fn list_spaces(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    Ok(json!({
        "spaces": wiki.state.get("wikiSpaces").cloned().unwrap_or_else(|| json!([])),
        "state": wiki.state,
    }))
}

pub fn set_active_space(paths: &MyOpenPanelsPaths, wiki_space_id: &str) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    state_object_mut(&mut wiki.state)?.insert("activeWikiSpaceId".to_owned(), json!(space.id));
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "wikiSpace": space.value, "state": wiki.state }))
}

pub fn list_pages(paths: &MyOpenPanelsPaths, wiki_space_id: &str) -> Result<Value, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    Ok(json!({ "pages": space.value.get("pageIndex").cloned().unwrap_or_else(|| json!([])) }))
}

pub fn search_pages(
    paths: &MyOpenPanelsPaths,
    wiki_space_id: &str,
    query: &str,
    limit: usize,
) -> Result<Value, CliError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(CliError::new("Wiki page search query cannot be empty."));
    }
    let wiki = get_wiki_bootstrap(paths)?;
    let storage = Storage::open(paths)?;
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    let panel_dir = storage.panel_dir(&wiki.project.id, &wiki.panel.id);
    let query_lower = query.to_lowercase();
    let terms = query_lower
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    let mut results = space
        .value
        .get("pageIndex")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|page| {
            let page_path = page.get("path").and_then(Value::as_str)?;
            let path = wiki_page_path(&panel_dir, &space.id, page_path).ok()?;
            let markdown = fs::read_to_string(path).ok()?;
            let title = page
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or(page_path);
            let summary = page.get("summary").and_then(Value::as_str).unwrap_or("");
            let searchable = format!("{title}\n{page_path}\n{summary}\n{markdown}").to_lowercase();
            let matched_terms = terms
                .iter()
                .filter(|term| searchable.contains(**term))
                .count();
            if !searchable.contains(&query_lower) && matched_terms == 0 {
                return None;
            }
            let title_lower = title.to_lowercase();
            let path_lower = page_path.to_lowercase();
            let score = usize::from(title_lower.contains(&query_lower)) * 8
                + usize::from(path_lower.contains(&query_lower)) * 5
                + usize::from(searchable.contains(&query_lower)) * 3
                + matched_terms;
            Some((
                score,
                page_path.to_owned(),
                json!({
                    "path": page_path,
                    "title": title,
                    "summary": summary,
                    "snippet": search_snippet(&markdown, &query_lower, &terms),
                    "score": score,
                }),
            ))
        })
        .collect::<Vec<_>>();
    results.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    let matches = results
        .into_iter()
        .take(limit.clamp(1, 100))
        .map(|(_, _, value)| value)
        .collect::<Vec<_>>();
    Ok(json!({
        "query": query,
        "wikiSpace": space.value,
        "matches": matches,
    }))
}

pub fn read_page(
    paths: &MyOpenPanelsPaths,
    wiki_space_id: &str,
    page_path: &str,
) -> Result<Value, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    let storage = Storage::open(paths)?;
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    let path = wiki_page_path(
        &storage.panel_dir(&wiki.project.id, &wiki.panel.id),
        &space.id,
        page_path,
    )?;
    let markdown = fs::read_to_string(path).map_err(to_cli_error)?;
    Ok(json!({ "pagePath": page_path, "wikiSpace": space.value, "markdown": markdown }))
}

fn search_snippet(markdown: &str, query: &str, terms: &[&str]) -> String {
    let lines = markdown
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let selected = lines
        .clone()
        .find(|line| {
            let lower = line.to_lowercase();
            lower.contains(query) || terms.iter().any(|term| lower.contains(*term))
        })
        .or_else(|| lines.into_iter().find(|line| !line.starts_with("---")))
        .unwrap_or("");
    selected.chars().take(320).collect()
}

pub fn write_page(
    paths: &MyOpenPanelsPaths,
    wiki_space_id: &str,
    page_path: &str,
    content: &str,
    title: Option<&str>,
    task_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut wiki = match task_id {
        Some(task_id) => get_wiki_task_target(paths, task_id)?,
        None => get_wiki_bootstrap(paths)?,
    };
    let storage = Storage::open(paths)?;
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    let path = wiki_page_path(
        &storage.panel_dir(&wiki.project.id, &wiki.panel.id),
        &space.id,
        page_path,
    )?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(path, content).map_err(to_cli_error)?;
    let now = now_iso();
    upsert_page_index(&mut wiki.state, &space.id, page_path, content, title, &now)?;
    update_wiki_space_timestamp(&mut wiki.state, &space.id, &now)?;
    state_object_mut(&mut wiki.state)?.insert("activeWikiSpaceId".to_owned(), json!(space.id));
    state_object_mut(&mut wiki.state)?.insert("activeWikiPagePath".to_owned(), json!(page_path));
    let task = if task_id.is_none() {
        Some(create_wiki_rebuild_index_task(
            &wiki.state,
            &mut wiki.tasks,
            Some(space.id.as_str()),
        )?)
    } else {
        None
    };
    save_wiki_state(paths, &wiki)?;
    trace::record_simple(
        "task",
        "wiki",
        Some("write"),
        format!("Wrote wiki page {page_path}"),
        Some(format!("Updated wiki page {page_path}")),
        Some(json!({
            "wikiSpaceId": space.id,
            "pagePath": page_path,
            "taskId": task_id,
        })),
    );
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    Ok(
        json!({ "pagePath": page_path, "task": task, "wikiSpace": space.value, "state": wiki.state }),
    )
}

pub fn reindex_wiki_space(
    paths: &MyOpenPanelsPaths,
    wiki_space_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let space = resolve_wiki_space(&wiki.state, wiki_space_id)?;
    let task =
        create_wiki_rebuild_index_task(&wiki.state, &mut wiki.tasks, Some(space.id.as_str()))?;
    save_wiki_state(paths, &wiki)?;
    let space = resolve_wiki_space(&wiki.state, wiki_space_id)?;
    Ok(json!({ "task": task, "state": wiki.state, "wikiSpace": space.value }))
}
