use super::*;

#[derive(Clone)]
pub(super) struct WikiBootstrapValue {
    pub(super) panel: crate::types::Panel,
    pub(super) session: crate::types::Session,
    pub(super) state: Value,
}

#[derive(Clone)]
pub(super) struct WikiSpaceValue {
    pub(super) id: String,
    pub(super) value: Value,
}

pub(super) fn get_wiki_bootstrap(paths: &MyOpenPanelsPaths) -> Result<WikiBootstrapValue, CliError> {
    let mut request = BootstrapRequest::new();
    request.requested_panel_kind = Some(PanelKind::Wiki);
    let bootstrap = read_project_bootstrap(paths, request)?;
    ensure_default_wiki_files(
        paths,
        &bootstrap.session.id,
        &bootstrap.panel.id,
        &bootstrap.state,
    )?;
    Ok(WikiBootstrapValue {
        session: bootstrap.session,
        panel: bootstrap.panel,
        state: bootstrap.state,
    })
}

pub(super) fn get_wiki_target(
    paths: &MyOpenPanelsPaths,
    session_id: &str,
    panel_id: &str,
) -> Result<WikiBootstrapValue, CliError> {
    let storage = Storage::open(paths)?;
    let session = storage.read_session(session_id)?.ok_or_else(|| {
        CliError::with_code(
            "target_not_found",
            format!("Project not found: {session_id}"),
        )
    })?;
    let panel = storage.read_panel(session_id, panel_id)?.ok_or_else(|| {
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
        .read_panel_state(session_id, panel_id)?
        .ok_or_else(|| {
            CliError::with_code(
                "target_not_found",
                format!("Wiki state not found: {panel_id}"),
            )
        })?;
    ensure_default_wiki_files(paths, session_id, panel_id, &state)?;
    Ok(WikiBootstrapValue {
        session,
        panel,
        state,
    })
}

pub(super) fn save_wiki_state(
    paths: &MyOpenPanelsPaths,
    wiki: &WikiBootstrapValue,
) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    storage.write_panel_state(&wiki.session.id, &wiki.panel.id, &wiki.state)?;
    storage.sync_wiki_tasks(&wiki.session.id, &wiki.panel.id, &wiki.state)?;
    storage.sync_project_tasks_from_panel(
        &wiki.session.id,
        &wiki.panel.id,
        wiki.panel.kind.as_str(),
        "wiki",
        &wiki.state,
    )
}

pub(super) fn ensure_default_wiki_files(
    paths: &MyOpenPanelsPaths,
    session_id: &str,
    panel_id: &str,
    state: &Value,
) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    let panel_dir = storage.panel_dir(session_id, panel_id);
    write_file_if_missing(
        &wiki_panel_path(&panel_dir, "rules/default/rules.md")?,
        default_rules_markdown(),
    )?;
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
            write_file_if_missing(&pages_dir.join("index.md"), "# Index\n\nNo pages yet.\n")?;
            write_file_if_missing(&pages_dir.join("log.md"), "# Log\n")?;
        }
    }
    Ok(())
}

pub(super) fn create_wiki_task(
    state: &mut Value,
    task_type: &str,
    target_id: &str,
    document_id: Option<&str>,
    markdown_version: Option<i64>,
    wiki_space_id: Option<&str>,
) -> Result<Value, CliError> {
    let space = resolve_wiki_space(state, wiki_space_id)?;
    let agent_skill_id = selected_agent_skill_id(state).to_owned();
    let now = now_iso();
    let task = json!({
        "id": create_id("task"),
        "type": task_type,
        "status": "queued",
        "targetId": target_id,
        "documentId": document_id,
        "wikiSpaceId": space.id,
        "ruleSetId": space.value.get("ruleSetId").cloned().unwrap_or(Value::Null),
        "ruleSetVersion": space.value.get("ruleSetVersion").cloned().unwrap_or(Value::Null),
        "agentSkillId": agent_skill_id,
        "markdownVersion": markdown_version,
        "claimedByProcessId": null,
        "attempt": 0,
        "maxAttempts": 3,
        "leaseOwner": null,
        "leaseExpiresAt": null,
        "lastHeartbeatAt": null,
        "retryAfter": null,
        "error": null,
        "result": null,
        "createdAt": now,
        "updatedAt": now,
    });
    state_array_mut(state, "tasks")?.insert(0, task.clone());
    trace_task_event(
        "queued",
        &task,
        format!("Queued {}", task_type_label(task_type)),
        Some(format!("Queued {}", task_type_label(task_type))),
    );
    Ok(task)
}

pub(super) fn create_wiki_rebuild_index_task(
    state: &mut Value,
    wiki_space_id: Option<&str>,
) -> Result<Value, CliError> {
    create_wiki_task(
        state,
        "rebuild_wiki_index",
        "index.md",
        None,
        None,
        wiki_space_id,
    )
}

pub(super) fn create_ingestion_state(task: &Value, markdown_version: i64) -> Value {
    json!({
        "status": "queued",
        "taskId": task.get("id").cloned().unwrap_or(Value::Null),
        "markdownVersion": markdown_version,
        "error": null,
        "updatedAt": task.get("updatedAt").cloned().unwrap_or_else(|| json!(now_iso())),
    })
}

pub(super) fn task_mut<'a>(state: &'a mut Value, task_id: &str) -> Result<&'a mut Value, CliError> {
    state_array_mut(state, "tasks")?
        .iter_mut()
        .find(|task| task.get("id").and_then(Value::as_str) == Some(task_id))
        .ok_or_else(|| CliError::new(format!("Wiki task not found: {task_id}")))
}

pub(super) fn task_value<'a>(state: &'a Value, task_id: &str) -> Result<&'a Value, CliError> {
    state
        .get("tasks")
        .and_then(Value::as_array)
        .and_then(|tasks| {
            tasks
                .iter()
                .find(|task| task.get("id").and_then(Value::as_str) == Some(task_id))
        })
        .ok_or_else(|| CliError::new(format!("Wiki task not found: {task_id}")))
}

pub(super) fn process_mut<'a>(
    state: &'a mut Value,
    process_id: &str,
) -> Result<&'a mut Value, CliError> {
    state_array_mut(state, "agentProcesses")?
        .iter_mut()
        .find(|process| process.get("id").and_then(Value::as_str) == Some(process_id))
        .ok_or_else(|| CliError::new(format!("Wiki process not found: {process_id}")))
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
        "type": if page_path == "index.md" { "overview" } else { "page" },
        "summary": first_markdown_paragraph(markdown),
        "tags": [],
        "sourceDocumentIds": [],
        "updatedAt": updated_at,
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

pub(super) fn agent_targets_path(paths: &MyOpenPanelsPaths) -> PathBuf {
    paths.context_dir.join("agent-targets.json")
}

pub(super) fn read_agent_targets(paths: &MyOpenPanelsPaths) -> Result<Vec<Value>, CliError> {
    let path = agent_targets_path(paths);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path).map_err(to_cli_error)?;
    Ok(serde_json::from_str::<Value>(&raw)
        .ok()
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default())
}

pub(super) fn write_json(path: &Path, value: &Value) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(
        path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(value).map_err(to_cli_error)?
        ),
    )
    .map_err(to_cli_error)
}

pub(super) fn write_file_if_missing(path: &Path, content: &str) -> Result<(), CliError> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(path, content).map_err(to_cli_error)
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

pub(super) fn default_rules_markdown() -> &'static str {
    "# Default LLM Wiki Rules\n\n- Keep `index.md` as the primary agent-readable map.\n- Keep `log.md` as an append-only change log.\n- Preserve source document references in page frontmatter.\n"
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
        "rebuild_wiki_index" => "wiki index rebuild",
        _ => "wiki task",
    }
}

pub(super) fn create_id(prefix: &str) -> String {
    let random: u128 = rand::rng().random();
    format!("{prefix}:{random:032x}")
}

pub(super) fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
