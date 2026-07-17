use super::*;
use std::env;

#[derive(Clone)]
pub(super) struct WikiBootstrapValue {
    pub(super) panel: crate::types::Panel,
    pub(super) project: crate::types::Project,
    pub(super) state: Value,
    pub(super) tasks: Vec<Value>,
}

#[derive(Clone)]
pub(super) struct WikiSpaceValue {
    pub(super) id: String,
    pub(super) value: Value,
}

pub(super) fn get_wiki_bootstrap(
    paths: &MyOpenPanelsPaths,
) -> Result<WikiBootstrapValue, CliError> {
    let mut request = BootstrapRequest::new();
    request.requested_panel_kind = Some(PanelKind::Wiki);
    let bootstrap = read_project_bootstrap(paths, request)?;
    ensure_default_wiki_files(
        paths,
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &bootstrap.state,
    )?;
    let state = bootstrap.state;
    let tasks = read_tasks(
        &Storage::open(paths)?,
        &bootstrap.project.id,
        &bootstrap.panel.id,
    )?;
    Ok(WikiBootstrapValue {
        project: bootstrap.project,
        panel: bootstrap.panel,
        state,
        tasks,
    })
}

pub(super) fn get_wiki_target(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: &str,
) -> Result<WikiBootstrapValue, CliError> {
    let storage = Storage::open(paths)?;
    let project = storage.read_project(project_id)?.ok_or_else(|| {
        CliError::with_code(
            "target_not_found",
            format!("Project not found: {project_id}"),
        )
    })?;
    let panel = storage.read_panel(project_id, panel_id)?.ok_or_else(|| {
        CliError::with_code(
            "target_not_found",
            format!("Wiki panel not found: {panel_id}"),
        )
    })?;
    if panel.kind != PanelKind::Wiki {
        return Err(CliError::with_code(
            "target_not_found",
            "Operation target is not a Wiki panel",
        ));
    }
    let state = storage
        .read_panel_state(project_id, panel_id)?
        .ok_or_else(|| {
            CliError::with_code(
                "target_not_found",
                format!("Wiki state not found: {panel_id}"),
            )
        })?;
    let tasks = read_tasks(&storage, project_id, panel_id)?;
    ensure_default_wiki_files(paths, project_id, panel_id, &state)?;
    Ok(WikiBootstrapValue {
        project,
        panel,
        state,
        tasks,
    })
}

pub(super) fn get_wiki_task_target(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
) -> Result<WikiBootstrapValue, CliError> {
    let storage = Storage::open(paths)?;
    let target = storage.task_panel_target(task_id)?.ok_or_else(|| {
        CliError::with_code(
            "task_not_found",
            format!("Project task not found: {task_id}"),
        )
    })?;
    get_wiki_target(paths, &target.0, &target.1)
}

pub(super) fn save_wiki_state(
    paths: &MyOpenPanelsPaths,
    wiki: &WikiBootstrapValue,
) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    storage.upsert_tasks(&wiki.project.id, &wiki.panel.id, "wiki", &wiki.tasks)?;
    let mut persisted_state = wiki.state.clone();
    persisted_state["schemaVersion"] = json!(4);
    storage.write_panel_state(&wiki.project.id, &wiki.panel.id, &persisted_state)?;
    Ok(())
}

fn read_tasks(storage: &Storage, project_id: &str, panel_id: &str) -> Result<Vec<Value>, CliError> {
    let tasks = storage
        .list_tasks(project_id)?
        .into_iter()
        .filter(|task| task.get("panelId").and_then(Value::as_str) == Some(panel_id))
        .map(|task| {
            let mut hydrated = task.as_object().cloned().unwrap_or_default();
            for field in ["input", "source"] {
                if let Some(values) = hydrated
                    .remove(field)
                    .and_then(|value| value.as_object().cloned())
                {
                    hydrated.extend(values);
                }
            }
            if let Some(lease) = hydrated
                .remove("lease")
                .and_then(|value| value.as_object().cloned())
            {
                hydrated.insert(
                    "leaseOwner".to_owned(),
                    lease.get("owner").cloned().unwrap_or(Value::Null),
                );
                hydrated.insert(
                    "leaseExpiresAt".to_owned(),
                    lease.get("expiresAt").cloned().unwrap_or(Value::Null),
                );
                hydrated.insert(
                    "lastHeartbeatAt".to_owned(),
                    lease.get("heartbeatAt").cloned().unwrap_or(Value::Null),
                );
            }
            Value::Object(hydrated)
        })
        .collect::<Vec<_>>();
    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{create_project, ensure_project_bootstrap};
    use crate::paths::resolve_myopenpanels_paths;

    #[test]
    fn new_wiki_does_not_create_an_index_page() {
        let temp = tempfile::tempdir().expect("temp");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");

        let bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let wiki = get_wiki_target(&paths, &bootstrap.project.id, &bootstrap.panel.id)
            .expect("wiki target");
        let panel_dir = Storage::open(&paths)
            .expect("storage")
            .panel_dir(&wiki.project.id, &wiki.panel.id);
        let wiki_space_id = wiki.state["activeWikiSpaceId"]
            .as_str()
            .expect("active Wiki space");
        let pages_dir = panel_dir
            .join("wikis")
            .join(sanitize_path_part(wiki_space_id))
            .join("pages");

        assert!(pages_dir.is_dir());
        assert!(!pages_dir.join("index.md").exists());
        assert_eq!(wiki.state["activeWikiPagePath"], Value::Null);
        assert_eq!(wiki.state["wikiSpaces"][0]["pageIndex"], json!([]));
    }

    #[test]
    fn typesetting_reads_wiki_documents_without_changing_focus() {
        let temp = tempfile::tempdir().expect("temp");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");
        let created = create_project(&paths, Some("Project")).expect("project");
        let raw = crate::wiki::add_raw_document(
            &paths,
            "raw.md",
            Some("Raw"),
            Some("text/markdown"),
            "user",
            None,
            b"# Raw content",
        )
        .expect("raw document");
        let generated = crate::wiki::create_generated_document(
            &paths,
            "generated.md",
            Some("Generated"),
            Some("text/markdown"),
            None,
            None,
            b"# Generated content",
        )
        .expect("generated document");
        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_project_id: Some(created.project.id.clone()),
                requested_panel_id: None,
                requested_panel_kind: Some(PanelKind::Typesetting),
            },
        )
        .expect("typesetting focus");

        let raw_id = raw["document"]["id"].as_str().expect("raw id");
        let generated_id = generated["document"]["id"].as_str().expect("generated id");
        assert_eq!(
            crate::wiki::read_markdown(&paths, raw_id).expect("read raw")["markdown"],
            "# Raw content"
        );
        assert_eq!(
            crate::wiki::read_generated_document(&paths, generated_id).expect("read generated")
                ["content"],
            "# Generated content"
        );
        assert_eq!(
            read_project_bootstrap(&paths, BootstrapRequest::new())
                .expect("focused bootstrap")
                .active_panel_kind,
            PanelKind::Typesetting
        );
    }
}

pub(super) fn ensure_default_wiki_files(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: &str,
    state: &Value,
) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    let panel_dir = storage.panel_dir(project_id, panel_id);
    for space in state
        .get("wikiSpaces")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(space_id) = space.get("id").and_then(Value::as_str) {
            let pages_dir = panel_dir
                .join("wikis")
                .join(sanitize_path_part(space_id))
                .join("pages");
            fs::create_dir_all(&pages_dir).map_err(to_cli_error)?;
        }
    }
    Ok(())
}

pub(super) fn create_wiki_task(
    state: &Value,
    tasks: &mut Vec<Value>,
    task_type: &str,
    target_id: &str,
    document_id: Option<&str>,
    markdown_version: Option<i64>,
    wiki_space_id: Option<&str>,
    mutation_key: Option<&str>,
) -> Result<Value, CliError> {
    let space = resolve_wiki_space(state, wiki_space_id)?;
    let agent_skill_id = selected_agent_skill_id(state).to_owned();
    let now = now_iso();
    let mutation_sequence = mutation_key.map(|key| {
        tasks
            .iter()
            .filter(|task| task.get("mutationKey").and_then(Value::as_str) == Some(key))
            .filter_map(|task| task.get("mutationSequence").and_then(Value::as_i64))
            .max()
            .unwrap_or(0)
            + 1
    });
    let task = json!({
        "id": create_id("task"),
        "type": task_type,
        "status": "queued",
        "targetId": target_id,
        "documentId": document_id,
        "wikiSpaceId": space.id,
        "agentSkillId": agent_skill_id,
        "markdownVersion": markdown_version,
        "mutationKey": mutation_key,
        "mutationSequence": mutation_sequence,
        "attempt": 0,
        "maxAttempts": 8,
        "leaseOwner": null,
        "leaseExpiresAt": null,
        "lastHeartbeatAt": null,
        "retryAfter": null,
        "error": null,
        "result": null,
        "createdAt": now,
        "updatedAt": now,
    });
    tasks.insert(0, task.clone());
    trace_task_event(
        "queued",
        &task,
        format!("Queued {}", task_type_label(task_type)),
        Some(format!("Queued {}", task_type_label(task_type))),
    );
    Ok(task)
}

pub(super) fn create_wiki_maintenance_task(
    state: &Value,
    tasks: &mut Vec<Value>,
    wiki_space_id: Option<&str>,
    mutation_key: &str,
    change_event: Value,
) -> Result<Value, CliError> {
    let space = resolve_wiki_space(state, wiki_space_id)?;
    if let Some(existing) = tasks.iter_mut().find(|task| {
        task.get("type").and_then(Value::as_str) == Some("maintain_wiki")
            && task.get("wikiSpaceId").and_then(Value::as_str) == Some(space.id.as_str())
            && matches!(
                task.get("status").and_then(Value::as_str),
                Some("waiting" | "queued" | "failed")
            )
    }) {
        append_unique_value(existing, "changeEvents", change_event.clone());
        existing["updatedAt"] = json!(now_iso());
        return Ok(existing.clone());
    }
    let mut task = create_wiki_task(
        state,
        tasks,
        "maintain_wiki",
        space.id.as_str(),
        None,
        None,
        Some(space.id.as_str()),
        Some(mutation_key),
    )?;
    task["changeEvents"] = json!([change_event.clone()]);
    task["idempotencyKey"] = json!(format!("maintain:{}:{}", space.id, task["id"]));
    if let Some(stored) = tasks
        .iter_mut()
        .find(|stored| stored.get("id") == task.get("id"))
    {
        *stored = task.clone();
    }
    Ok(task)
}

fn append_unique_value(task: &mut Value, field: &str, value: Value) {
    let values = task
        .as_object_mut()
        .expect("Wiki Task must be an object")
        .entry(field.to_owned())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("Wiki Task value collection must be an array");
    if !values.iter().any(|candidate| candidate == &value) {
        values.push(value);
    }
}

pub(super) fn wiki_mutation_key(project_id: &str, panel_id: &str, wiki_space_id: &str) -> String {
    format!("wiki:{project_id}:{panel_id}:{wiki_space_id}")
}

pub(super) fn create_ingestion_state(task: &Value, markdown_version: i64) -> Value {
    json!({
        "status": task.get("status").and_then(Value::as_str).unwrap_or("queued"),
        "taskId": task.get("id").cloned().unwrap_or(Value::Null),
        "markdownVersion": markdown_version,
        "error": null,
        "updatedAt": task.get("updatedAt").cloned().unwrap_or_else(|| json!(now_iso())),
    })
}

pub(super) fn task_mut<'a>(
    tasks: &'a mut [Value],
    task_id: &str,
) -> Result<&'a mut Value, CliError> {
    tasks
        .iter_mut()
        .find(|task| task.get("id").and_then(Value::as_str) == Some(task_id))
        .ok_or_else(|| CliError::new(format!("Wiki task not found: {task_id}")))
}

pub(super) fn task_value<'a>(tasks: &'a [Value], task_id: &str) -> Result<&'a Value, CliError> {
    tasks
        .iter()
        .find(|task| task.get("id").and_then(Value::as_str) == Some(task_id))
        .ok_or_else(|| CliError::new(format!("Wiki task not found: {task_id}")))
}

pub(super) fn find_document<'a>(
    state: &'a Value,
    document_id: &str,
) -> Result<&'a Value, CliError> {
    state
        .get("rawDocuments")
        .and_then(Value::as_array)
        .and_then(|documents| {
            documents
                .iter()
                .find(|document| document.get("id").and_then(Value::as_str) == Some(document_id))
        })
        .ok_or_else(|| CliError::new(format!("Wiki raw document not found: {document_id}")))
}

pub(super) fn find_document_mut<'a>(
    state: &'a mut Value,
    document_id: &str,
) -> Result<&'a mut Value, CliError> {
    state_array_mut(state, "rawDocuments")?
        .iter_mut()
        .find(|document| document.get("id").and_then(Value::as_str) == Some(document_id))
        .ok_or_else(|| CliError::new(format!("Wiki raw document not found: {document_id}")))
}

pub(super) fn find_generated_document<'a>(
    state: &'a Value,
    document_id: &str,
) -> Result<&'a Value, CliError> {
    state
        .get("generatedDocuments")
        .and_then(Value::as_array)
        .and_then(|documents| {
            documents
                .iter()
                .find(|document| document.get("id").and_then(Value::as_str) == Some(document_id))
        })
        .ok_or_else(|| {
            CliError::with_code(
                "not_found",
                format!("Wiki generated document not found: {document_id}"),
            )
        })
}

pub(super) fn find_generated_document_mut<'a>(
    state: &'a mut Value,
    document_id: &str,
) -> Result<&'a mut Value, CliError> {
    state_array_mut(state, "generatedDocuments")?
        .iter_mut()
        .find(|document| document.get("id").and_then(Value::as_str) == Some(document_id))
        .ok_or_else(|| {
            CliError::with_code(
                "not_found",
                format!("Wiki generated document not found: {document_id}"),
            )
        })
}

pub(super) fn resolve_wiki_space(
    state: &Value,
    wiki_space_id: Option<&str>,
) -> Result<WikiSpaceValue, CliError> {
    let requested =
        wiki_space_id.or_else(|| state.get("activeWikiSpaceId").and_then(Value::as_str));
    let spaces = state
        .get("wikiSpaces")
        .and_then(Value::as_array)
        .ok_or_else(|| CliError::new("Wiki space not found"))?;
    let value = requested
        .and_then(|id| {
            spaces
                .iter()
                .find(|space| space.get("id").and_then(Value::as_str) == Some(id))
        })
        .or_else(|| spaces.first())
        .ok_or_else(|| CliError::new("Wiki space not found"))?
        .clone();
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::new("Wiki space not found"))?
        .to_owned();
    Ok(WikiSpaceValue { id, value })
}

pub(super) fn upsert_page_index(
    state: &mut Value,
    wiki_space_id: &str,
    page_path: &str,
    markdown: &str,
    title: Option<&str>,
    updated_at: &str,
) -> Result<(), CliError> {
    let spaces = state_array_mut(state, "wikiSpaces")?;
    let space = spaces
        .iter_mut()
        .find(|space| space.get("id").and_then(Value::as_str) == Some(wiki_space_id))
        .ok_or_else(|| CliError::new("Wiki space not found"))?;
    let item = json!({
        "path": page_path,
        "title": title
            .filter(|value| !value.trim().is_empty())
            .map(str::trim)
            .or_else(|| heading_title(markdown))
            .unwrap_or_else(|| title_from_file_name(page_path)),
        "type": "page",
        "summary": first_markdown_paragraph(markdown),
        "tags": [],
        "sourceDocumentIds": [],
        "updatedAt": updated_at,
        "wordCount": character_count(markdown),
    });
    let page_index = space
        .as_object_mut()
        .ok_or_else(|| CliError::new("Wiki space not found"))?
        .entry("pageIndex")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| CliError::new("Wiki page index is invalid."))?;
    if let Some(existing) = page_index
        .iter_mut()
        .find(|item| item.get("path").and_then(Value::as_str) == Some(page_path))
    {
        *existing = item;
    } else {
        page_index.push(item);
    }
    Ok(())
}

pub(super) fn update_wiki_space_timestamp(
    state: &mut Value,
    wiki_space_id: &str,
    updated_at: &str,
) -> Result<(), CliError> {
    let spaces = state_array_mut(state, "wikiSpaces")?;
    let space = spaces
        .iter_mut()
        .find(|space| space.get("id").and_then(Value::as_str) == Some(wiki_space_id))
        .ok_or_else(|| CliError::new("Wiki space not found"))?;
    space["updatedAt"] = json!(updated_at);
    Ok(())
}

pub(super) fn state_object_mut(state: &mut Value) -> Result<&mut Map<String, Value>, CliError> {
    state
        .as_object_mut()
        .ok_or_else(|| CliError::new("Wiki state is invalid."))
}

pub(super) fn state_array_mut<'a>(
    state: &'a mut Value,
    key: &str,
) -> Result<&'a mut Vec<Value>, CliError> {
    state_object_mut(state)?
        .entry(key)
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| CliError::new(format!("Wiki state field is not an array: {key}")))
}

pub(super) fn wiki_ref(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| sanitize_path_part(part))
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn wiki_panel_path(panel_dir: &Path, relative_ref: &str) -> Result<PathBuf, CliError> {
    let mut path = panel_dir.to_path_buf();
    for part in relative_ref.split('/') {
        path.push(sanitize_path_part(part));
    }
    if !path.starts_with(panel_dir) {
        return Err(CliError::new("Resolved wiki path escapes panel directory."));
    }
    Ok(path)
}

pub(super) fn wiki_page_path(
    panel_dir: &Path,
    wiki_space_id: &str,
    page_path: &str,
) -> Result<PathBuf, CliError> {
    let pages_dir = panel_dir
        .join("wikis")
        .join(sanitize_path_part(wiki_space_id))
        .join("pages");
    let mut path = pages_dir.clone();
    for part in page_path.split('/') {
        path.push(sanitize_path_part(part));
    }
    if !path.starts_with(&pages_dir) {
        return Err(CliError::new(
            "Resolved wiki page path escapes pages directory.",
        ));
    }
    Ok(path)
}

pub(super) fn is_plain_text_file(file_name: &str, mime_type: Option<&str>) -> bool {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "md" | "markdown" | "mdx" | "txt" | "text" | "json" | "csv"
    ) || mime_type.is_some_and(|value| value.starts_with("text/") || value.contains("markdown"))
}

pub(super) fn mime_type_for_file(file_name: &str) -> &'static str {
    mime_guess::from_path(file_name)
        .first_raw()
        .unwrap_or("application/octet-stream")
}

pub(super) fn title_from_file_name(file_name: &str) -> &str {
    Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(file_name)
}

pub(super) fn heading_title(markdown: &str) -> Option<&str> {
    markdown
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
}

pub(super) fn first_markdown_paragraph(markdown: &str) -> String {
    markdown
        .split("\n\n")
        .map(str::trim)
        .find(|part| !part.is_empty() && !part.starts_with("---") && !part.starts_with('#'))
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(180)
        .collect()
}

pub(super) fn sha256_hex(content: &[u8]) -> String {
    format!("{:x}", Sha256::digest(content))
}

pub(super) fn trace_task_event(
    action: &str,
    task: &Value,
    summary: impl Into<String>,
    release_summary: Option<String>,
) {
    trace::record(TraceEventInput {
        audience: None,
        category: Some("task".to_owned()),
        detail: Some(json!({
            "action": action,
            "task": task,
        })),
        direction: Some("task".to_owned()),
        release_summary,
        run_id: env::var("MYOPENPANELS_TRACE_RUN_ID").ok(),
        source: Some("wiki".to_owned()),
        summary: Some(summary.into()),
        task_id: task.get("id").and_then(Value::as_str).map(str::to_owned),
    });
}

pub(super) fn task_type_label(task_type: &str) -> &'static str {
    match task_type {
        "convert_document_to_markdown" => "document conversion",
        "ingest_markdown_into_wiki" => "wiki indexing",
        "maintain_wiki" => "wiki maintenance",
        _ => "wiki task",
    }
}

pub(super) fn create_id(prefix: &str) -> String {
    crate::ids::random_id(prefix)
}

pub(super) fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
