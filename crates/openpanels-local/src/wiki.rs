use crate::control::{ensure_project_bootstrap, now_iso, BootstrapRequest};
use crate::error::CliError;
use crate::paths::{sanitize_path_part, OpenPanelsPaths};
use crate::storage::Storage;
use crate::trace::{self, TraceEventInput};
use crate::types::PanelKind;
use rand::Rng;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

mod agent;
mod state;

pub use agent::{list_agent_targets, register_agent_target};
use agent::{save_process, wake_queued_wiki_tasks};
use state::*;

pub fn wiki_context(paths: &OpenPanelsPaths) -> Result<Value, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    Ok(json!({
        "session": wiki.session,
        "panel": wiki.panel,
        "state": wiki.state,
    }))
}

pub fn add_raw_document(
    paths: &OpenPanelsPaths,
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
    let panel_dir = storage.panel_dir(&wiki.session.id, &wiki.panel.id);
    let original_path = wiki_panel_path(&panel_dir, &original_ref)?;
    if let Some(parent) = original_path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(&original_path, content).map_err(to_cli_error)?;

    let is_text = is_plain_text_file(&safe_file_name, mime_type);
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
            &mut wiki.state,
            "convert_document_to_markdown",
            &document_id,
            Some(&document_id),
            Some(0),
            Some(wiki_space.id.as_str()),
        )?)
    };
    let ingest_task = if is_text {
        Some(create_wiki_task(
            &mut wiki.state,
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
    wake_queued_wiki_tasks(paths, &wiki, None, false)?;
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
    paths: &OpenPanelsPaths,
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
        &storage.panel_dir(&wiki.session.id, &wiki.panel.id),
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
    paths: &OpenPanelsPaths,
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
    paths: &OpenPanelsPaths,
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
    for task in state_array_mut(&mut wiki.state, "tasks")? {
        let matches_document = task.get("documentId").and_then(Value::as_str) == Some(document_id)
            || task.get("targetId").and_then(Value::as_str) == Some(document_id);
        if matches_document {
            task["status"] = json!("stale");
            task["error"] = json!("Source document deleted");
            task["updatedAt"] = json!(now);
        }
    }
    let task = create_wiki_task(
        &mut wiki.state,
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
        .panel_dir(&wiki.session.id, &wiki.panel.id)
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
    wake_queued_wiki_tasks(paths, &wiki, None, false)?;
    Ok(json!({ "document": document, "task": task, "state": wiki.state }))
}

pub fn extract_raw_document_markdown(
    paths: &OpenPanelsPaths,
    document_id: &str,
    wiki_space_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let _ = find_document(&wiki.state, document_id)?;
    let wiki_space = resolve_wiki_space(&wiki.state, wiki_space_id)?;
    let now = now_iso();
    for task in state_array_mut(&mut wiki.state, "tasks")? {
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
        &mut wiki.state,
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
    wake_queued_wiki_tasks(paths, &wiki, None, false)?;
    Ok(json!({ "document": document, "task": task, "state": wiki.state }))
}

pub fn reindex_raw_document(
    paths: &OpenPanelsPaths,
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
        &mut wiki.state,
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
    wake_queued_wiki_tasks(paths, &wiki, None, false)?;
    Ok(json!({ "document": document, "task": task, "state": wiki.state }))
}

pub fn list_tasks(paths: &OpenPanelsPaths, status: Option<&str>) -> Result<Value, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    let tasks = wiki
        .state
        .get("tasks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|task| {
            status.is_none_or(|status| task.get("status").and_then(Value::as_str) == Some(status))
        })
        .collect::<Vec<_>>();
    Ok(json!({ "tasks": tasks }))
}

pub fn next_task(paths: &OpenPanelsPaths) -> Result<Value, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    let task = wiki
        .state
        .get("tasks")
        .and_then(Value::as_array)
        .and_then(|tasks| {
            tasks
                .iter()
                .find(|task| task.get("status").and_then(Value::as_str) == Some("queued"))
                .or_else(|| {
                    tasks
                        .iter()
                        .find(|task| task.get("status").and_then(Value::as_str) == Some("failed"))
                })
                .cloned()
        });
    Ok(json!({ "task": task }))
}

pub fn claim_task(
    paths: &OpenPanelsPaths,
    task_id: &str,
    agent_host: Option<&str>,
    thread_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let task = task_mut(&mut wiki.state, task_id)?;
    let status = task.get("status").and_then(Value::as_str).unwrap_or("");
    if status != "queued" && status != "failed" {
        return Err(CliError::new(format!(
            "Wiki task is not claimable: {task_id}"
        )));
    }
    let process_id = create_id("process");
    let now = now_iso();
    let task_type = task.get("type").and_then(Value::as_str).unwrap_or("");
    task["status"] = json!(if task_type == "convert_document_to_markdown" {
        "running"
    } else {
        "claimed"
    });
    task["claimedByProcessId"] = json!(process_id);
    task["updatedAt"] = json!(now);
    let task_snapshot = task.clone();
    let process = json!({
        "id": process_id,
        "agentHost": agent_host.unwrap_or("unknown"),
        "threadId": thread_id,
        "taskId": task_id,
        "wikiSpaceId": task_snapshot.get("wikiSpaceId").and_then(Value::as_str).unwrap_or("wiki:default"),
        "status": "running",
        "startedAt": now,
        "updatedAt": now,
    });
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
    state_array_mut(&mut wiki.state, "agentProcesses")?.insert(0, process.clone());
    save_process(paths, &wiki, &process)?;
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
    Ok(json!({ "process": process, "task": task_snapshot, "state": wiki.state }))
}

pub fn complete_task(
    paths: &OpenPanelsPaths,
    task_id: &str,
    result: Option<Value>,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let now = now_iso();
    let task_snapshot = {
        let task = task_mut(&mut wiki.state, task_id)?;
        task["status"] = json!("succeeded");
        task["error"] = Value::Null;
        task["result"] = result.unwrap_or(Value::Null);
        task["updatedAt"] = json!(now);
        task.clone()
    };
    let process_snapshot = task_snapshot
        .get("claimedByProcessId")
        .and_then(Value::as_str)
        .and_then(|process_id| process_mut(&mut wiki.state, process_id).ok())
        .map(|process| {
            process["status"] = json!("finished");
            process["updatedAt"] = json!(now);
            process.clone()
        });
    if let Some(process) = &process_snapshot {
        save_process(paths, &wiki, process)?;
    }

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
            let ingest_task = create_wiki_task(
                &mut wiki.state,
                "ingest_markdown_into_wiki",
                document_id,
                Some(document_id),
                Some(markdown_version),
                task_snapshot.get("wikiSpaceId").and_then(Value::as_str),
            )?;
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
    if !follow_up_task_ids.is_empty() {
        wake_queued_wiki_tasks(paths, &wiki, Some(&follow_up_task_ids), true)?;
    }
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

pub fn fail_task(paths: &OpenPanelsPaths, task_id: &str, message: &str) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let now = now_iso();
    let task_snapshot = {
        let task = task_mut(&mut wiki.state, task_id)?;
        task["status"] = json!("failed");
        task["error"] = json!(message);
        task["updatedAt"] = json!(now);
        task.clone()
    };
    let process_snapshot = task_snapshot
        .get("claimedByProcessId")
        .and_then(Value::as_str)
        .and_then(|process_id| process_mut(&mut wiki.state, process_id).ok())
        .map(|process| {
            process["status"] = json!("failed");
            process["updatedAt"] = json!(now);
            process.clone()
        });
    if let Some(process) = &process_snapshot {
        save_process(paths, &wiki, process)?;
    }
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

pub fn read_markdown(paths: &OpenPanelsPaths, document_id: &str) -> Result<Value, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    let document = find_document(&wiki.state, document_id)?.clone();
    let markdown = if let Some(markdown_ref) = document.get("markdownRef").and_then(Value::as_str) {
        let storage = Storage::open(paths)?;
        let path = wiki_panel_path(
            &storage.panel_dir(&wiki.session.id, &wiki.panel.id),
            markdown_ref,
        )?;
        fs::read_to_string(path).unwrap_or_default()
    } else {
        String::new()
    };
    Ok(json!({ "document": document, "markdown": markdown }))
}

pub fn write_markdown(
    paths: &OpenPanelsPaths,
    document_id: &str,
    content: &str,
    task_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let storage = Storage::open(paths)?;
    let now = now_iso();
    let parent_task = task_id
        .map(|id| task_value(&wiki.state, id).cloned())
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
        &storage.panel_dir(&wiki.session.id, &wiki.panel.id),
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
            &mut wiki.state,
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
            &mut wiki.state,
            Some(&wiki_space_id),
        )?)
    } else {
        None
    };
    save_wiki_state(paths, &wiki)?;
    if task.is_some() || rebuild_task.is_some() {
        wake_queued_wiki_tasks(paths, &wiki, None, false)?;
    }
    let document = find_document(&wiki.state, document_id)?.clone();
    Ok(
        json!({ "document": document, "rebuildTask": rebuild_task, "task": task, "state": wiki.state }),
    )
}

pub fn set_language(paths: &OpenPanelsPaths, language: &str) -> Result<Value, CliError> {
    if language != "en" && language != "zh-CN" {
        return Err(CliError::new("Expected language to be one of: en, zh-CN"));
    }
    let mut wiki = get_wiki_bootstrap(paths)?;
    state_object_mut(&mut wiki.state)?.insert("wikiLanguage".to_owned(), json!(language));
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "language": language, "state": wiki.state }))
}

pub fn list_spaces(paths: &OpenPanelsPaths) -> Result<Value, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    Ok(json!({
        "spaces": wiki.state.get("wikiSpaces").cloned().unwrap_or_else(|| json!([])),
        "state": wiki.state,
    }))
}

pub fn set_active_space(paths: &OpenPanelsPaths, wiki_space_id: &str) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    state_object_mut(&mut wiki.state)?.insert("activeWikiSpaceId".to_owned(), json!(space.id));
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "wikiSpace": space.value, "state": wiki.state }))
}

pub fn list_pages(paths: &OpenPanelsPaths, wiki_space_id: &str) -> Result<Value, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    Ok(json!({ "pages": space.value.get("pageIndex").cloned().unwrap_or_else(|| json!([])) }))
}

pub fn read_page(
    paths: &OpenPanelsPaths,
    wiki_space_id: &str,
    page_path: &str,
) -> Result<Value, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    let storage = Storage::open(paths)?;
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    let path = wiki_page_path(
        &storage.panel_dir(&wiki.session.id, &wiki.panel.id),
        &space.id,
        page_path,
    )?;
    let markdown = fs::read_to_string(path).map_err(to_cli_error)?;
    Ok(json!({ "pagePath": page_path, "wikiSpace": space.value, "markdown": markdown }))
}

pub fn write_page(
    paths: &OpenPanelsPaths,
    wiki_space_id: &str,
    page_path: &str,
    content: &str,
    title: Option<&str>,
    task_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let storage = Storage::open(paths)?;
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    let path = wiki_page_path(
        &storage.panel_dir(&wiki.session.id, &wiki.panel.id),
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
            &mut wiki.state,
            Some(space.id.as_str()),
        )?)
    } else {
        None
    };
    save_wiki_state(paths, &wiki)?;
    if task.is_some() {
        wake_queued_wiki_tasks(paths, &wiki, None, false)?;
    }
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
    paths: &OpenPanelsPaths,
    wiki_space_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let space = resolve_wiki_space(&wiki.state, wiki_space_id)?;
    let task = create_wiki_rebuild_index_task(&mut wiki.state, Some(space.id.as_str()))?;
    save_wiki_state(paths, &wiki)?;
    wake_queued_wiki_tasks(paths, &wiki, None, false)?;
    let space = resolve_wiki_space(&wiki.state, wiki_space_id)?;
    Ok(json!({ "task": task, "state": wiki.state, "wikiSpace": space.value }))
}
