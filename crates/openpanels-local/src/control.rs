use crate::error::CliError;
use crate::paths::{sanitize_path_part, OpenPanelsPaths};
use crate::storage::Storage;
use crate::tasks::{annotate_tasks, pending_task_count};
use crate::types::{Panel, PanelKind, ProjectBootstrap, ProjectPanelSnapshot, Session};
use rand::Rng;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

const DEFAULT_PANEL_KINDS: &[PanelKind] = &[PanelKind::Wiki, PanelKind::Canvas];
const DEFAULT_ACTIVE_PANEL_KIND: PanelKind = PanelKind::Wiki;
const DEFAULT_WIKI_RULE_SET_ID: &str = "rule-set:default";
const DEFAULT_WIKI_SPACE_ID: &str = "wiki:default";

pub struct BootstrapRequest {
    pub requested_panel_id: Option<String>,
    pub requested_panel_kind: Option<PanelKind>,
    pub requested_session_id: Option<String>,
}

impl BootstrapRequest {
    pub fn new() -> Self {
        Self {
            requested_panel_id: None,
            requested_panel_kind: None,
            requested_session_id: None,
        }
    }
}

pub fn ensure_project_bootstrap(
    paths: &OpenPanelsPaths,
    request: BootstrapRequest,
) -> Result<ProjectBootstrap, CliError> {
    let storage = Storage::open(paths)?;
    let sessions = storage.list_sessions()?;
    let active_session_id = read_active_session(paths)?;
    let mut session = if let Some(session) =
        requested_or_active_session(&storage, &request, active_session_id.as_deref())?
    {
        session
    } else {
        create_session(&storage, next_project_title(&sessions))?
    };

    for kind in DEFAULT_PANEL_KINDS {
        session = ensure_panel_for_session(&storage, &session, *kind)?;
    }

    let panels = read_panel_snapshots(&storage, &session)?;
    for snapshot in &panels {
        if snapshot.panel.kind == PanelKind::Wiki {
            storage.sync_project_tasks_from_panel(
                &session.id,
                &snapshot.panel.id,
                snapshot.panel.kind.as_str(),
                "wiki",
                &snapshot.state,
            )?;
        }
    }
    let tasks = annotate_tasks(storage.list_project_tasks(&session.id)?);
    let pending_task_count = pending_task_count(&tasks);
    let active_panel = read_active_panel(paths)?;
    let preferred_kind = request
        .requested_panel_kind
        .or_else(|| {
            active_panel.as_ref().and_then(|active| {
                if active.session_id.as_deref() == Some(&session.id) {
                    active.kind
                } else {
                    None
                }
            })
        })
        .or_else(|| active_panel.as_ref().and_then(|active| active.kind))
        .unwrap_or(DEFAULT_ACTIVE_PANEL_KIND);

    let snapshot = request
        .requested_panel_id
        .as_deref()
        .and_then(|panel_id| panels.iter().find(|item| item.panel.id == panel_id))
        .or_else(|| panels.iter().find(|item| item.panel.kind == preferred_kind))
        .or_else(|| {
            panels
                .iter()
                .find(|item| item.panel.kind == DEFAULT_ACTIVE_PANEL_KIND)
        })
        .or_else(|| panels.first())
        .ok_or_else(|| CliError::new(format!("OpenPanels project has no panels: {}", session.id)))?
        .clone();

    write_active_session(paths, &session.id)?;
    write_active_panel(paths, &snapshot.panel)?;

    Ok(ProjectBootstrap {
        active_panel_id: snapshot.panel.id.clone(),
        active_panel_kind: snapshot.panel.kind,
        context_dir: paths.context_dir.display().to_string(),
        context_id: paths.context_id.clone(),
        context_id_source: paths.context_id_source.clone(),
        panel: snapshot.panel.clone(),
        panel_dir: storage
            .panel_dir(&session.id, &snapshot.panel.id)
            .display()
            .to_string(),
        panels,
        pending_task_count,
        revision: snapshot.revision,
        session,
        sessions: storage.list_sessions()?,
        state: snapshot.state.clone(),
        storage_dir: paths.storage_dir.display().to_string(),
        tasks,
    })
}

pub fn read_project_bootstrap(
    paths: &OpenPanelsPaths,
    request: BootstrapRequest,
) -> Result<ProjectBootstrap, CliError> {
    let storage = Storage::open(paths)?;
    let sessions = storage.list_sessions()?;
    let active_session_id = read_active_session(paths)?;
    let session =
        requested_or_active_session(&storage, &request, active_session_id.as_deref())?
            .ok_or_else(|| {
                CliError::with_code(
                    "no_current_project",
                    "No current MyOpenPanels project is available. Create a project explicitly with `openpanels-local project create`.",
                )
            })?;

    let panels = read_panel_snapshots(&storage, &session)?;
    for snapshot in &panels {
        if snapshot.panel.kind == PanelKind::Wiki {
            storage.sync_project_tasks_from_panel(
                &session.id,
                &snapshot.panel.id,
                snapshot.panel.kind.as_str(),
                "wiki",
                &snapshot.state,
            )?;
        }
    }
    let tasks = annotate_tasks(storage.list_project_tasks(&session.id)?);
    let pending_task_count = pending_task_count(&tasks);
    let active_panel = read_active_panel(paths)?;
    let preferred_kind = request
        .requested_panel_kind
        .or_else(|| {
            active_panel.as_ref().and_then(|active| {
                if active.session_id.as_deref() == Some(&session.id) {
                    active.kind
                } else {
                    None
                }
            })
        })
        .or_else(|| active_panel.as_ref().and_then(|active| active.kind));

    let snapshot = request
        .requested_panel_id
        .as_deref()
        .and_then(|panel_id| panels.iter().find(|item| item.panel.id == panel_id))
        .or_else(|| {
            preferred_kind.and_then(|kind| panels.iter().find(|item| item.panel.kind == kind))
        })
        .or_else(|| {
            panels
                .iter()
                .find(|item| item.panel.kind == DEFAULT_ACTIVE_PANEL_KIND)
        })
        .or_else(|| panels.first())
        .ok_or_else(|| CliError::new(format!("OpenPanels project has no panels: {}", session.id)))?
        .clone();

    write_active_session(paths, &session.id)?;
    write_active_panel(paths, &snapshot.panel)?;

    Ok(ProjectBootstrap {
        active_panel_id: snapshot.panel.id.clone(),
        active_panel_kind: snapshot.panel.kind,
        context_dir: paths.context_dir.display().to_string(),
        context_id: paths.context_id.clone(),
        context_id_source: paths.context_id_source.clone(),
        panel: snapshot.panel.clone(),
        panel_dir: storage
            .panel_dir(&session.id, &snapshot.panel.id)
            .display()
            .to_string(),
        panels,
        pending_task_count,
        revision: snapshot.revision,
        session,
        sessions,
        state: snapshot.state.clone(),
        storage_dir: paths.storage_dir.display().to_string(),
        tasks,
    })
}

pub fn create_project(
    paths: &OpenPanelsPaths,
    title: Option<&str>,
) -> Result<ProjectBootstrap, CliError> {
    let storage = Storage::open(paths)?;
    let sessions = storage.list_sessions()?;
    let session_title = title
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_owned())
        .unwrap_or_else(|| next_project_title(&sessions));
    let session = create_session(&storage, session_title)?;
    write_active_session_id(paths, &session.id)?;
    ensure_project_bootstrap(
        paths,
        BootstrapRequest {
            requested_session_id: Some(session.id),
            requested_panel_id: None,
            requested_panel_kind: None,
        },
    )
}

pub fn create_runtime_session(
    paths: &OpenPanelsPaths,
    title: Option<&str>,
) -> Result<Session, CliError> {
    let storage = Storage::open(paths)?;
    let session = create_session(
        &storage,
        title
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().to_owned())
            .unwrap_or_else(|| "OpenPanels Session".to_owned()),
    )?;
    write_active_session_id(paths, &session.id)?;
    Ok(session)
}

pub fn open_runtime_panel(
    paths: &OpenPanelsPaths,
    session_id: &str,
    kind: PanelKind,
    title: Option<&str>,
    initial_state: Option<Value>,
) -> Result<Panel, CliError> {
    let storage = Storage::open(paths)?;
    let Some(session) = storage.read_session(session_id)? else {
        return Err(CliError::new(format!(
            "OpenPanels session not found: {session_id}"
        )));
    };
    let timestamp = now_iso();
    let mut panel = Panel {
        id: create_openpanels_id("panel"),
        session_id: session.id.clone(),
        kind,
        title: title
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().to_owned())
            .unwrap_or_else(|| initial_panel_title(kind).to_owned()),
        created_at: timestamp.clone(),
        updated_at: timestamp.clone(),
        state_ref: None,
    };
    panel.state_ref = Some(format!(
        "sqlite:panel-states/{}/{}",
        panel.session_id, panel.id
    ));
    storage.write_panel(&panel)?;
    storage.write_panel_state(
        &session.id,
        &panel.id,
        &initial_state.unwrap_or_else(|| initial_panel_state(kind)),
    )?;
    let mut next_session = session;
    next_session.updated_at = timestamp;
    next_session.panel_ids.push(panel.id.clone());
    storage.write_session(&next_session)?;
    Ok(panel)
}

pub fn rename_session(
    paths: &OpenPanelsPaths,
    session_id: &str,
    title: Option<&str>,
) -> Result<Session, CliError> {
    let storage = Storage::open(paths)?;
    let Some(mut session) = storage.read_session(session_id)? else {
        return Err(CliError::new(format!(
            "OpenPanels session not found: {session_id}"
        )));
    };
    let Some(title) = title.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(CliError::new("Project title is required"));
    };
    session.title = title.to_owned();
    session.updated_at = now_iso();
    storage.write_session(&session)?;
    Ok(session)
}

pub fn delete_session(paths: &OpenPanelsPaths, session_id: &str) -> Result<Value, CliError> {
    let storage = Storage::open(paths)?;
    let sessions = storage.list_sessions()?;
    if sessions.len() <= 1 {
        return Err(CliError::new("At least one project must remain"));
    }
    if !sessions.iter().any(|session| session.id == session_id) {
        return Err(CliError::new(format!(
            "OpenPanels session not found: {session_id}"
        )));
    }
    storage.delete_session(session_id)?;
    let remaining_sessions = storage.list_sessions()?;
    let current_active_session_id = read_active_session(paths)?;
    let next_active_session = remaining_sessions
        .iter()
        .find(|session| Some(session.id.as_str()) == current_active_session_id.as_deref())
        .or_else(|| remaining_sessions.first())
        .ok_or_else(|| CliError::new("At least one project must remain"))?;
    write_active_session(paths, &next_active_session.id)?;
    Ok(json!({
        "activeSessionId": next_active_session.id,
        "deletedSessionId": session_id,
        "sessions": remaining_sessions,
    }))
}

pub fn read_active_session_id(paths: &OpenPanelsPaths) -> Result<Option<String>, CliError> {
    read_active_session(paths)
}

pub fn write_active_session_id(paths: &OpenPanelsPaths, session_id: &str) -> Result<(), CliError> {
    write_active_session(paths, session_id)
}

pub fn read_active_panel_value(paths: &OpenPanelsPaths) -> Result<Option<Value>, CliError> {
    read_json_object_or_null(&paths.context_dir.join("active-panel.json"))
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn requested_or_active_session(
    storage: &Storage,
    request: &BootstrapRequest,
    active_session_id: Option<&str>,
) -> Result<Option<Session>, CliError> {
    if let Some(session_id) = request.requested_session_id.as_deref() {
        if let Some(session) = storage.read_session(session_id)? {
            return Ok(Some(session));
        }
    }
    if let Some(session_id) = active_session_id {
        if let Some(session) = storage.read_session(session_id)? {
            return Ok(Some(session));
        }
    }
    Ok(None)
}

fn create_session(storage: &Storage, title: String) -> Result<Session, CliError> {
    let timestamp = now_iso();
    let session = Session {
        id: create_openpanels_id("session"),
        title,
        created_at: timestamp.clone(),
        updated_at: timestamp,
        panel_ids: Vec::new(),
    };
    storage.write_session(&session)?;
    Ok(session)
}

fn ensure_panel_for_session(
    storage: &Storage,
    session: &Session,
    kind: PanelKind,
) -> Result<Session, CliError> {
    for panel_id in &session.panel_ids {
        if storage
            .read_panel(&session.id, panel_id)?
            .is_some_and(|panel| panel.kind == kind)
        {
            return Ok(session.clone());
        }
    }

    let timestamp = now_iso();
    let mut panel = Panel {
        id: create_openpanels_id("panel"),
        session_id: session.id.clone(),
        kind,
        title: initial_panel_title(kind).to_owned(),
        created_at: timestamp.clone(),
        updated_at: timestamp.clone(),
        state_ref: None,
    };
    panel.state_ref = Some(format!(
        "sqlite:panel-states/{}/{}",
        panel.session_id, panel.id
    ));
    storage.write_panel(&panel)?;
    storage.write_panel_state(&session.id, &panel.id, &initial_panel_state(kind))?;

    let mut next_session = session.clone();
    next_session.updated_at = timestamp;
    next_session.panel_ids.push(panel.id);
    storage.write_session(&next_session)?;
    Ok(next_session)
}

fn read_panel_snapshots(
    storage: &Storage,
    session: &Session,
) -> Result<Vec<ProjectPanelSnapshot>, CliError> {
    let mut snapshots = Vec::new();
    for panel_id in &session.panel_ids {
        let Some(panel) = storage.read_panel(&session.id, panel_id)? else {
            continue;
        };
        let raw_state = storage.read_panel_state(&session.id, &panel.id)?;
        let migrated_state = migrate_panel_state(storage, session, &panel, raw_state)?;
        let revision = if migrated_state.changed {
            storage.write_panel_state(&session.id, &panel.id, &migrated_state.state)?
        } else {
            storage.read_panel_state_revision(&session.id, &panel.id)?
        };
        snapshots.push(ProjectPanelSnapshot {
            state: migrated_state.state,
            revision,
            panel,
        });
    }
    snapshots.sort_by_key(|snapshot| panel_sort_index(snapshot.panel.kind));
    Ok(snapshots)
}

struct PanelStateMigrationResult {
    state: Value,
    changed: bool,
}

fn migrate_panel_state(
    storage: &Storage,
    session: &Session,
    panel: &Panel,
    state: Option<Value>,
) -> Result<PanelStateMigrationResult, CliError> {
    match panel.kind {
        PanelKind::Canvas => migrate_canvas_state(state),
        PanelKind::Wiki => migrate_wiki_state(storage, session, panel, state),
        _ => {
            let changed = state.is_none();
            Ok(PanelStateMigrationResult {
                state: state.unwrap_or_else(|| json!({})),
                changed,
            })
        }
    }
}

fn migrate_canvas_state(state: Option<Value>) -> Result<PanelStateMigrationResult, CliError> {
    let Some(state) = state else {
        return Ok(PanelStateMigrationResult {
            state: empty_canvas_snapshot(),
            changed: true,
        });
    };
    match state
        .get("schema")
        .and_then(|schema| schema.get("schemaVersion"))
        .and_then(Value::as_i64)
    {
        Some(1) => Ok(PanelStateMigrationResult {
            state,
            changed: false,
        }),
        Some(version) => Err(CliError::new(format!(
            "Unsupported future canvas panel state schemaVersion: {version}"
        ))),
        None => Err(CliError::new(
            "Malformed canvas panel state: missing schema.schemaVersion.",
        )),
    }
}

fn migrate_wiki_state(
    storage: &Storage,
    session: &Session,
    panel: &Panel,
    state: Option<Value>,
) -> Result<PanelStateMigrationResult, CliError> {
    let Some(state) = state else {
        return Ok(PanelStateMigrationResult {
            state: empty_wiki_state(),
            changed: true,
        });
    };
    match state.get("schemaVersion").and_then(Value::as_i64) {
        Some(2) => {
            if is_wiki_state_v2(&state) {
                Ok(PanelStateMigrationResult {
                    state,
                    changed: false,
                })
            } else {
                Err(CliError::new(
                    "Malformed wiki panel state: schemaVersion 2 is missing required arrays.",
                ))
            }
        }
        Some(1) => Ok(PanelStateMigrationResult {
            state: migrate_wiki_state_v1_to_v2(storage, session, panel, &state)?,
            changed: true,
        }),
        Some(version) => Err(CliError::new(format!(
            "Unsupported future wiki panel state schemaVersion: {version}"
        ))),
        None => Err(CliError::new(
            "Malformed wiki panel state: missing schemaVersion.",
        )),
    }
}

fn is_wiki_state_v2(state: &Value) -> bool {
    state.get("rawDocuments").is_some_and(Value::is_array)
        && state.get("ruleSets").is_some_and(Value::is_array)
        && state.get("wikiSpaces").is_some_and(Value::is_array)
        && state.get("tasks").is_some_and(Value::is_array)
}

fn migrate_wiki_state_v1_to_v2(
    storage: &Storage,
    session: &Session,
    panel: &Panel,
    state: &Value,
) -> Result<Value, CliError> {
    let pages = state
        .get("pages")
        .and_then(Value::as_array)
        .ok_or_else(|| CliError::new("Malformed wiki v1 state: pages must be an array."))?;
    let active_page_id = state.get("activePageId").and_then(Value::as_str);
    let panel_dir = storage.panel_dir(&session.id, &panel.id);
    let mut migrated = empty_wiki_state();
    let mut page_index = Vec::new();
    let mut first_page_path = None;
    let mut active_page_path = None;

    for page in pages {
        let page_object = page
            .as_object()
            .ok_or_else(|| CliError::new("Malformed wiki v1 state: page must be an object."))?;
        let page_path = page_object
            .get("path")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| CliError::new("Malformed wiki v1 state: page.path is required."))?;
        let title = page_object
            .get("title")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::trim)
            .unwrap_or_else(|| wiki_title_from_path(page_path));
        let markdown = page_object
            .get("markdown")
            .and_then(Value::as_str)
            .unwrap_or("");
        let updated_at = page_object
            .get("updatedAt")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(now_iso);

        let page_file = wiki_page_file_path(&panel_dir, page_path)?;
        if let Some(parent) = page_file.parent() {
            fs::create_dir_all(parent).map_err(to_cli_error)?;
        }
        fs::write(&page_file, markdown).map_err(to_cli_error)?;

        if first_page_path.is_none() {
            first_page_path = Some(page_path.to_owned());
        }
        if active_page_path.is_none()
            && active_page_id.is_some()
            && page_object.get("id").and_then(Value::as_str) == active_page_id
        {
            active_page_path = Some(page_path.to_owned());
        }
        page_index.push(json!({
            "path": page_path,
            "title": title,
            "type": if page_path == "index.md" { "overview" } else { "page" },
            "summary": first_markdown_paragraph(markdown),
            "tags": [],
            "sourceDocumentIds": [],
            "updatedAt": updated_at,
        }));
    }

    let active_path = active_page_path
        .or(first_page_path)
        .unwrap_or_else(|| "index.md".to_owned());
    if let Some(space) = migrated
        .get_mut("wikiSpaces")
        .and_then(Value::as_array_mut)
        .and_then(|spaces| spaces.first_mut())
        .and_then(Value::as_object_mut)
    {
        space.insert("pageIndex".to_owned(), Value::Array(page_index));
    }
    if let Some(object) = migrated.as_object_mut() {
        object.insert("activeWikiPagePath".to_owned(), json!(active_path));
    }
    Ok(migrated)
}

fn wiki_page_file_path(panel_dir: &Path, page_path: &str) -> Result<std::path::PathBuf, CliError> {
    let pages_dir = panel_dir
        .join("wikis")
        .join(sanitize_path_part(DEFAULT_WIKI_SPACE_ID))
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

fn wiki_title_from_path(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .and_then(|file_name| file_name.strip_suffix(".md").or(Some(file_name)))
        .filter(|value| !value.is_empty())
        .unwrap_or("Untitled")
}

fn first_markdown_paragraph(markdown: &str) -> String {
    markdown
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .unwrap_or("")
        .chars()
        .take(240)
        .collect()
}

fn initial_panel_state(kind: PanelKind) -> Value {
    match kind {
        PanelKind::Canvas => empty_canvas_snapshot(),
        PanelKind::Wiki => empty_wiki_state(),
        _ => json!({}),
    }
}

fn empty_canvas_snapshot() -> Value {
    json!({
        "schema": {
            "schemaVersion": 1,
            "recordVersions": { "page": 1, "shape": 1, "asset": 1 },
        },
        "camera": { "x": 0, "y": 0, "zoom": 1 },
        "currentPageId": "page:main",
        "openedGroupId": null,
        "selectedShapeIds": [],
        "store": {
            "page:main": {
                "id": "page:main",
                "typeName": "page",
                "name": "Page 1",
                "index": 1,
            },
        },
    })
}

fn empty_wiki_state() -> Value {
    let now = now_iso();
    json!({
        "schemaVersion": 2,
        "rawDocuments": [],
        "ruleSets": [{
            "id": DEFAULT_WIKI_RULE_SET_ID,
            "title": "Default LLM Wiki",
            "description": "Default agent-friendly structured wiki rules.",
            "builtIn": true,
            "version": 1,
            "rulesRef": "rules/default/rules.md",
            "createdAt": now,
            "updatedAt": now,
        }],
        "wikiSpaces": [{
            "id": DEFAULT_WIKI_SPACE_ID,
            "title": "Default Wiki",
            "ruleSetId": DEFAULT_WIKI_RULE_SET_ID,
            "ruleSetVersion": 1,
            "rootRef": "wikis/wiki:default",
            "pageIndex": [],
            "createdAt": now,
            "updatedAt": now,
        }],
        "activeRawDocumentId": null,
        "activeWikiSpaceId": DEFAULT_WIKI_SPACE_ID,
        "activeWikiPagePath": "index.md",
        "agentProcesses": [],
        "tasks": [],
        "wikiLanguage": null,
    })
}

fn initial_panel_title(kind: PanelKind) -> &'static str {
    match kind {
        PanelKind::Wiki => "文档库",
        PanelKind::Canvas => "Design canvas",
        PanelKind::Image => "Images",
        PanelKind::Diff => "Diff",
        PanelKind::Preview => "Preview",
        PanelKind::Files => "Files",
    }
}

fn next_project_title(sessions: &[Session]) -> String {
    let max_project_number = sessions
        .iter()
        .filter_map(|session| session.title.strip_prefix("Project "))
        .filter_map(|number| number.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("Project {}", max_project_number + 1)
}

fn panel_sort_index(kind: PanelKind) -> usize {
    DEFAULT_PANEL_KINDS
        .iter()
        .position(|candidate| *candidate == kind)
        .unwrap_or(usize::MAX)
}

fn create_openpanels_id(prefix: &str) -> String {
    let random: u128 = rand::rng().random();
    format!("{prefix}:{random:032x}")
}

fn read_active_session(paths: &OpenPanelsPaths) -> Result<Option<String>, CliError> {
    let value = read_json_object_or_null(&paths.context_dir.join("active-session.json"))?;
    Ok(value.and_then(|value| {
        value
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_owned)
    }))
}

fn write_active_session(paths: &OpenPanelsPaths, session_id: &str) -> Result<(), CliError> {
    write_json(
        &paths.context_dir.join("active-session.json"),
        &json!({ "sessionId": session_id, "updatedAt": now_iso() }),
    )
}

fn read_active_panel(paths: &OpenPanelsPaths) -> Result<Option<ActivePanel>, CliError> {
    let Some(value) = read_json_object_or_null(&paths.context_dir.join("active-panel.json"))?
    else {
        return Ok(None);
    };
    Ok(Some(ActivePanel {
        kind: value
            .get("kind")
            .and_then(Value::as_str)
            .and_then(PanelKind::parse),
        session_id: value
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }))
}

fn write_active_panel(paths: &OpenPanelsPaths, panel: &Panel) -> Result<(), CliError> {
    write_json(
        &paths.context_dir.join("active-panel.json"),
        &json!({
            "sessionId": panel.session_id,
            "panelId": panel.id,
            "kind": panel.kind,
            "updatedAt": now_iso(),
        }),
    )
}

#[derive(Debug)]
struct ActivePanel {
    kind: Option<PanelKind>,
    session_id: Option<String>,
}

fn read_json_object_or_null(path: &Path) -> Result<Option<Value>, CliError> {
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path).map_err(to_cli_error)?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    Ok(serde_json::from_str::<Value>(&content).ok())
}

fn write_json(path: &Path, value: &Value) -> Result<(), CliError> {
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

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::resolve_openpanels_paths;

    #[test]
    fn bootstrap_creates_project_with_wiki_and_canvas_panels() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_openpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");

        let bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");

        assert_eq!(bootstrap.session.title, "Project 1");
        assert_eq!(bootstrap.active_panel_kind, PanelKind::Wiki);
        assert_eq!(
            bootstrap
                .panels
                .iter()
                .map(|snapshot| snapshot.panel.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["wiki", "canvas"]
        );
        assert_eq!(bootstrap.state["schemaVersion"], json!(2));
        assert!(paths.context_dir.join("active-session.json").exists());
        assert!(paths.context_dir.join("active-panel.json").exists());
        assert!(storage_dir
            .join(crate::storage::DATABASE_FILE_NAME)
            .exists());
    }

    #[test]
    fn bootstrap_keeps_contexts_isolated_by_context_id() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let first_paths = resolve_openpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("thread-a"),
        )
        .expect("first paths");
        let second_paths = resolve_openpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("thread-b"),
        )
        .expect("second paths");

        let first = ensure_project_bootstrap(&first_paths, BootstrapRequest::new()).expect("first");
        let second =
            ensure_project_bootstrap(&second_paths, BootstrapRequest::new()).expect("second");
        let first_again =
            ensure_project_bootstrap(&first_paths, BootstrapRequest::new()).expect("again");

        assert_ne!(first.session.id, second.session.id);
        assert_eq!(first.session.title, "Project 1");
        assert_eq!(second.session.title, "Project 2");
        assert_eq!(first.session.id, first_again.session.id);
    }

    #[test]
    fn bootstrap_migrates_wiki_v1_state_to_v2_without_clearing_pages() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_openpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");
        let bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let wiki_panel = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
            .expect("wiki panel")
            .panel
            .clone();
        let storage = Storage::open(&paths).expect("storage");
        storage
            .write_panel_state(
                &bootstrap.session.id,
                &wiki_panel.id,
                &json!({
                    "schemaVersion": 1,
                    "pages": [{
                        "id": "page:notes",
                        "title": "Research Notes",
                        "path": "research/notes.md",
                        "markdown": "# Research Notes\n\nKept during migration.",
                        "createdAt": "2026-01-01T00:00:00.000Z",
                        "updatedAt": "2026-01-02T00:00:00.000Z"
                    }],
                    "activePageId": "page:notes"
                }),
            )
            .expect("write v1 state");
        let panel_dir = storage.panel_dir(&bootstrap.session.id, &wiki_panel.id);
        drop(storage);

        let migrated = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("migrated");
        assert_eq!(migrated.state["schemaVersion"], json!(2));
        assert_eq!(
            migrated.state["activeWikiPagePath"],
            json!("research/notes.md")
        );
        assert_eq!(
            migrated.state["wikiSpaces"][0]["pageIndex"][0]["path"],
            json!("research/notes.md")
        );
        assert_eq!(
            fs::read_to_string(
                panel_dir
                    .join("wikis")
                    .join(sanitize_path_part(DEFAULT_WIKI_SPACE_ID))
                    .join("pages")
                    .join("research")
                    .join("notes.md")
            )
            .expect("migrated markdown"),
            "# Research Notes\n\nKept during migration."
        );
    }

    #[test]
    fn bootstrap_rejects_malformed_wiki_state_instead_of_clearing_it() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_openpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");
        let bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let wiki_panel = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
            .expect("wiki panel")
            .panel
            .clone();
        let storage = Storage::open(&paths).expect("storage");
        storage
            .write_panel_state(
                &bootstrap.session.id,
                &wiki_panel.id,
                &json!({ "schemaVersion": 2, "rawDocuments": [] }),
            )
            .expect("write malformed state");
        drop(storage);

        let error = match read_project_bootstrap(&paths, BootstrapRequest::new()) {
            Ok(_) => panic!("malformed"),
            Err(error) => error,
        };
        assert!(error.message().contains("Malformed wiki panel state"));
    }

    #[test]
    fn bootstrap_rejects_future_wiki_state_version() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_openpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");
        let bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let wiki_panel = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
            .expect("wiki panel")
            .panel
            .clone();
        let storage = Storage::open(&paths).expect("storage");
        storage
            .write_panel_state(
                &bootstrap.session.id,
                &wiki_panel.id,
                &json!({ "schemaVersion": 3, "rawDocuments": [] }),
            )
            .expect("write future wiki state");
        drop(storage);

        let error = match read_project_bootstrap(&paths, BootstrapRequest::new()) {
            Ok(_) => panic!("future wiki"),
            Err(error) => error,
        };
        assert!(error.message().contains("Unsupported future wiki"));
    }

    #[test]
    fn bootstrap_rejects_future_canvas_state_version() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_openpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");
        let bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let canvas_panel = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Canvas)
            .expect("canvas panel")
            .panel
            .clone();
        let storage = Storage::open(&paths).expect("storage");
        storage
            .write_panel_state(
                &bootstrap.session.id,
                &canvas_panel.id,
                &json!({ "schema": { "schemaVersion": 2 }, "store": {} }),
            )
            .expect("write future canvas state");
        drop(storage);

        let error = match read_project_bootstrap(&paths, BootstrapRequest::new()) {
            Ok(_) => panic!("future canvas"),
            Err(error) => error,
        };
        assert!(error.message().contains("Unsupported future canvas"));
    }
}
