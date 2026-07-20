use crate::error::CliError;
use crate::paths::MyOpenPanelsPaths;
use crate::storage::Storage;
use crate::tasks::{annotate_tasks, pending_task_count};
use crate::types::{Panel, PanelKind, Project, ProjectBootstrap, ProjectPanelSnapshot};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

const DEFAULT_PANEL_KINDS: &[PanelKind] = &[
    PanelKind::Wiki,
    PanelKind::Writing,
    PanelKind::Canvas,
    PanelKind::Typesetting,
    PanelKind::Publishing,
];
const DEFAULT_ACTIVE_PANEL_KIND: PanelKind = PanelKind::Wiki;
const DEFAULT_WIKI_SPACE_ID: &str = "wiki:default";
const DEFAULT_WRITING_SKILL_ID: &str = "writing-default";
const DEFAULT_REFINEMENT_SKILL_ID: &str = "writing-skill-refiner";

pub struct BootstrapRequest {
    pub requested_panel_id: Option<String>,
    pub requested_panel_kind: Option<PanelKind>,
    pub requested_project_id: Option<String>,
}

impl BootstrapRequest {
    pub fn new() -> Self {
        Self {
            requested_panel_id: None,
            requested_panel_kind: None,
            requested_project_id: None,
        }
    }
}

impl Default for BootstrapRequest {
    fn default() -> Self {
        Self::new()
    }
}

pub fn ensure_project_bootstrap(
    paths: &MyOpenPanelsPaths,
    request: BootstrapRequest,
) -> Result<ProjectBootstrap, CliError> {
    let storage = Storage::open(paths)?;
    let projects = storage.list_projects()?;
    let active_project_id = read_active_project(paths)?;
    let mut project = if let Some(project) = requested_or_active_project(
        &storage,
        &request,
        active_project_id.as_deref(),
        Some(&projects),
    )? {
        project
    } else {
        create_project_record(&storage, next_project_title(&projects))?
    };

    for kind in DEFAULT_PANEL_KINDS {
        project = ensure_panel_for_project(&storage, &project, *kind)?;
    }

    let panels = read_panel_snapshots(&storage, &project)?;
    let tasks = annotate_tasks(storage.list_tasks(&project.id)?);
    let pending_task_count = pending_task_count(&tasks);
    let active_panel = read_active_panel(paths)?;
    let preferred_kind = request
        .requested_panel_kind
        .or_else(|| {
            active_panel.as_ref().and_then(|active| {
                if active.project_id.as_deref() == Some(&project.id) {
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
        .ok_or_else(|| {
            CliError::new(format!(
                "MyOpenPanels project has no panels: {}",
                project.id
            ))
        })?
        .clone();

    write_active_project(paths, &project.id)?;
    write_active_panel(paths, &snapshot.panel)?;

    Ok(ProjectBootstrap {
        active_panel_id: snapshot.panel.id.clone(),
        active_panel_kind: snapshot.panel.kind,
        context_dir: paths.context_dir.display().to_string(),
        context_id: paths.context_id.clone(),
        context_id_source: paths.context_id_source.clone(),
        panel: snapshot.panel.clone(),
        panel_dir: storage
            .panel_dir(&project.id, &snapshot.panel.id)
            .display()
            .to_string(),
        panels,
        pending_task_count,
        revision: snapshot.revision,
        project,
        projects: storage.list_projects()?,
        state: snapshot.state.clone(),
        storage_dir: paths.storage_dir.display().to_string(),
        tasks,
    })
}

pub fn activate_project_panel(
    paths: &MyOpenPanelsPaths,
    kind: PanelKind,
) -> Result<ProjectBootstrap, CliError> {
    let bootstrap = read_project_bootstrap(
        paths,
        BootstrapRequest {
            requested_panel_id: None,
            requested_panel_kind: Some(kind),
            requested_project_id: None,
        },
    )?;
    write_active_project(paths, &bootstrap.project.id)?;
    write_active_panel(paths, &bootstrap.panel)?;
    Ok(bootstrap)
}

pub fn read_project_bootstrap(
    paths: &MyOpenPanelsPaths,
    request: BootstrapRequest,
) -> Result<ProjectBootstrap, CliError> {
    let storage = Storage::open(paths)?;
    let projects = storage.list_projects()?;
    let active_project_id = read_active_project(paths)?;
    let project = requested_or_active_project(
        &storage,
        &request,
        active_project_id.as_deref(),
        Some(&projects),
    )?
    .ok_or_else(|| {
        CliError::with_recovery(
            "no_current_project",
            "No current MyOpenPanels project is available. Start Studio to prepare the current project context.",
            true,
            "Run `myopenpanels studio start --project-dir <dir> --format json`, then retry.",
        )
    })?;

    let panels = read_panel_snapshots(&storage, &project)?;
    let tasks = annotate_tasks(storage.list_tasks(&project.id)?);
    let pending_task_count = pending_task_count(&tasks);
    let active_panel = read_active_panel(paths)?;
    let preferred_kind = request
        .requested_panel_kind
        .or_else(|| {
            active_panel.as_ref().and_then(|active| {
                if active.project_id.as_deref() == Some(&project.id) {
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
        .ok_or_else(|| {
            CliError::new(format!(
                "MyOpenPanels project has no panels: {}",
                project.id
            ))
        })?
        .clone();

    Ok(ProjectBootstrap {
        active_panel_id: snapshot.panel.id.clone(),
        active_panel_kind: snapshot.panel.kind,
        context_dir: paths.context_dir.display().to_string(),
        context_id: paths.context_id.clone(),
        context_id_source: paths.context_id_source.clone(),
        panel: snapshot.panel.clone(),
        panel_dir: storage
            .panel_dir(&project.id, &snapshot.panel.id)
            .display()
            .to_string(),
        panels,
        pending_task_count,
        revision: snapshot.revision,
        project,
        projects,
        state: snapshot.state.clone(),
        storage_dir: paths.storage_dir.display().to_string(),
        tasks,
    })
}

pub fn require_active_panel(
    paths: &MyOpenPanelsPaths,
    expected_kind: PanelKind,
    expected_focus_revision: Option<u64>,
) -> Result<ProjectBootstrap, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    if bootstrap.active_panel_kind != expected_kind {
        return Err(CliError::with_recovery(
            "panel_kind_mismatch",
            format!(
                "The active panel is {}, but this command requires {}.",
                bootstrap.active_panel_kind.as_str(),
                expected_kind.as_str()
            ),
            true,
            format!(
                "Run `myopenpanels panel activate --panel-kind {} --format json`, read the new focus revision, and retry.",
                expected_kind.as_str()
            ),
        ));
    }
    if let Some(expected) = expected_focus_revision {
        let current = read_focus_revision(paths)?;
        if current != expected {
            return Err(CliError::with_recovery(
                "focus_changed",
                format!("Expected focus revision {expected}, but the current revision is {current}."),
                true,
                "Read `myopenpanels panel context read --format json` and retry against the new focus.",
            ));
        }
    }
    Ok(bootstrap)
}

pub fn create_project(
    paths: &MyOpenPanelsPaths,
    title: Option<&str>,
) -> Result<ProjectBootstrap, CliError> {
    let storage = Storage::open(paths)?;
    let projects = storage.list_projects()?;
    let project_title = title
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_owned())
        .unwrap_or_else(|| next_project_title(&projects));
    let project = create_project_record(&storage, project_title)?;
    write_active_project_id(paths, &project.id)?;
    ensure_project_bootstrap(
        paths,
        BootstrapRequest {
            requested_project_id: Some(project.id),
            requested_panel_id: None,
            requested_panel_kind: None,
        },
    )
}

pub fn create_runtime_project(
    paths: &MyOpenPanelsPaths,
    title: Option<&str>,
) -> Result<Project, CliError> {
    let storage = Storage::open(paths)?;
    let project = create_project_record(
        &storage,
        title
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().to_owned())
            .unwrap_or_else(|| "MyOpenPanels Project".to_owned()),
    )?;
    write_active_project_id(paths, &project.id)?;
    Ok(project)
}

pub fn open_runtime_panel(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: PanelKind,
    title: Option<&str>,
    initial_state: Option<Value>,
) -> Result<Panel, CliError> {
    let storage = Storage::open(paths)?;
    let Some(project) = storage.read_project(project_id)? else {
        return Err(CliError::new(format!(
            "MyOpenPanels project not found: {project_id}"
        )));
    };
    let timestamp = now_iso();
    let mut panel = Panel {
        id: create_myopenpanels_id("panel"),
        project_id: project.id.clone(),
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
        panel.project_id, panel.id
    ));
    storage.write_panel(&panel)?;
    storage.write_panel_state(
        &project.id,
        &panel.id,
        &initial_state.unwrap_or_else(|| initial_panel_state(kind)),
    )?;
    let mut next_project = project;
    next_project.updated_at = timestamp;
    next_project.panel_ids.push(panel.id.clone());
    storage.write_project(&next_project)?;
    Ok(panel)
}

pub fn rename_project(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    title: Option<&str>,
) -> Result<Project, CliError> {
    let storage = Storage::open(paths)?;
    let Some(mut project) = storage.read_project(project_id)? else {
        return Err(CliError::new(format!(
            "MyOpenPanels project not found: {project_id}"
        )));
    };
    let Some(title) = title.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(CliError::new("Project title is required"));
    };
    project.title = title.to_owned();
    project.updated_at = now_iso();
    storage.write_project(&project)?;
    Ok(project)
}

pub fn delete_project(paths: &MyOpenPanelsPaths, project_id: &str) -> Result<Value, CliError> {
    let storage = Storage::open(paths)?;
    let projects = storage.list_projects()?;
    if projects.len() <= 1 {
        return Err(CliError::new("At least one project must remain"));
    }
    if !projects.iter().any(|project| project.id == project_id) {
        return Err(CliError::new(format!(
            "MyOpenPanels project not found: {project_id}"
        )));
    }
    storage.delete_project(project_id)?;
    crate::context_cleanup::clear_deleted_project_references(paths, project_id);
    let remaining_projects = storage.list_projects()?;
    let current_active_project_id = read_active_project(paths)?;
    let next_active_project = remaining_projects
        .iter()
        .find(|project| Some(project.id.as_str()) == current_active_project_id.as_deref())
        .or_else(|| remaining_projects.first())
        .ok_or_else(|| CliError::new("At least one project must remain"))?;
    write_active_project(paths, &next_active_project.id)?;
    Ok(json!({
        "activeProjectId": next_active_project.id,
        "deletedProjectId": project_id,
        "projects": remaining_projects,
    }))
}

pub fn read_active_project_id(paths: &MyOpenPanelsPaths) -> Result<Option<String>, CliError> {
    read_active_project(paths)
}

pub fn write_active_project_id(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
) -> Result<(), CliError> {
    write_active_project(paths, project_id)
}

pub fn read_active_panel_value(paths: &MyOpenPanelsPaths) -> Result<Option<Value>, CliError> {
    read_json_object_or_null(&paths.focus_dir.join("active-panel.json"))
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn requested_or_active_project(
    storage: &Storage,
    request: &BootstrapRequest,
    active_project_id: Option<&str>,
    stale_active_fallback: Option<&[Project]>,
) -> Result<Option<Project>, CliError> {
    if let Some(project_id) = request.requested_project_id.as_deref() {
        if let Some(project) = storage.read_project(project_id)? {
            return Ok(Some(project));
        }
        return Err(CliError::with_recovery(
            "project_not_found",
            format!("MyOpenPanels project not found: {project_id}"),
            false,
            "Run `myopenpanels project list --format json` and select an existing Project id.",
        ));
    }
    if let Some(project_id) = active_project_id {
        if let Some(project) = storage.read_project(project_id)? {
            return Ok(Some(project));
        }
    }
    Ok(stale_active_fallback.and_then(most_recently_updated_project))
}

fn most_recently_updated_project(projects: &[Project]) -> Option<Project> {
    projects
        .iter()
        .max_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then_with(|| left.created_at.cmp(&right.created_at))
                .then_with(|| left.id.cmp(&right.id))
        })
        .cloned()
}

fn create_project_record(storage: &Storage, title: String) -> Result<Project, CliError> {
    let timestamp = now_iso();
    let project = Project {
        id: create_myopenpanels_id("project"),
        title,
        created_at: timestamp.clone(),
        updated_at: timestamp,
        panel_ids: Vec::new(),
    };
    storage.write_project(&project)?;
    Ok(project)
}

fn ensure_panel_for_project(
    storage: &Storage,
    project: &Project,
    kind: PanelKind,
) -> Result<Project, CliError> {
    for panel_id in &project.panel_ids {
        if storage
            .read_panel(&project.id, panel_id)?
            .is_some_and(|panel| panel.kind == kind)
        {
            return Ok(project.clone());
        }
    }

    let timestamp = now_iso();
    let mut panel = Panel {
        id: create_myopenpanels_id("panel"),
        project_id: project.id.clone(),
        kind,
        title: initial_panel_title(kind).to_owned(),
        created_at: timestamp.clone(),
        updated_at: timestamp.clone(),
        state_ref: None,
    };
    panel.state_ref = Some(format!(
        "sqlite:panel-states/{}/{}",
        panel.project_id, panel.id
    ));
    storage.write_panel(&panel)?;
    storage.write_panel_state(&project.id, &panel.id, &initial_panel_state(kind))?;

    let mut next_project = project.clone();
    next_project.updated_at = timestamp;
    next_project.panel_ids.push(panel.id);
    storage.write_project(&next_project)?;
    Ok(next_project)
}

fn read_panel_snapshots(
    storage: &Storage,
    project: &Project,
) -> Result<Vec<ProjectPanelSnapshot>, CliError> {
    let mut snapshots = Vec::new();
    for panel_id in &project.panel_ids {
        let Some(panel) = storage.read_panel(&project.id, panel_id)? else {
            continue;
        };
        let raw_state = storage.read_panel_state(&project.id, &panel.id)?;
        let resolved_state = resolve_panel_state(storage, project, &panel, raw_state)?;
        let revision = if resolved_state.changed {
            storage.write_panel_state(&project.id, &panel.id, &resolved_state.state)?
        } else {
            storage.read_panel_state_revision(&project.id, &panel.id)?
        };
        snapshots.push(ProjectPanelSnapshot {
            state: resolved_state.state,
            revision,
            panel,
        });
    }
    snapshots.sort_by_key(|snapshot| panel_sort_index(snapshot.panel.kind));
    Ok(snapshots)
}

struct PanelStateResolution {
    state: Value,
    changed: bool,
}

fn resolve_panel_state(
    storage: &Storage,
    project: &Project,
    panel: &Panel,
    state: Option<Value>,
) -> Result<PanelStateResolution, CliError> {
    match panel.kind {
        PanelKind::Canvas => resolve_canvas_state(state),
        PanelKind::Wiki => resolve_wiki_state(storage, project, panel, state),
        PanelKind::Writing => resolve_writing_state(state),
        PanelKind::Typesetting => resolve_typesetting_state(state),
        PanelKind::Publishing => resolve_publishing_state(state),
    }
}

fn resolve_publishing_state(state: Option<Value>) -> Result<PanelStateResolution, CliError> {
    let Some(state) = state else {
        return Ok(PanelStateResolution {
            state: empty_publishing_state(),
            changed: true,
        });
    };
    match state.get("schemaVersion").and_then(Value::as_i64) {
        Some(1) if state.as_object().is_some() => {
            let normalized = crate::publishing::normalize_state(state.clone());
            if !crate::publishing::validate_state(&normalized) {
                return Err(CliError::new("Malformed publishing panel state."));
            }
            Ok(PanelStateResolution {
                changed: normalized != state,
                state: normalized,
            })
        }
        Some(1) => Err(CliError::new("Malformed publishing panel state.")),
        Some(version) => Err(CliError::new(format!(
            "Unsupported future publishing panel state schemaVersion: {version}"
        ))),
        None => Err(CliError::new(
            "Malformed publishing panel state: missing schemaVersion.",
        )),
    }
}

fn resolve_typesetting_state(state: Option<Value>) -> Result<PanelStateResolution, CliError> {
    let Some(state) = state else {
        return Ok(PanelStateResolution {
            state: empty_typesetting_state(),
            changed: true,
        });
    };
    match state.get("schemaVersion").and_then(Value::as_i64) {
        Some(1) if is_typesetting_state_v1(&state) => Ok(PanelStateResolution {
            state,
            changed: false,
        }),
        Some(1) => Err(CliError::new("Malformed typesetting panel state.")),
        Some(version) => Err(CliError::new(format!(
            "Unsupported future typesetting panel state schemaVersion: {version}"
        ))),
        None => Err(CliError::new(
            "Malformed typesetting panel state: missing schemaVersion.",
        )),
    }
}

pub(crate) fn validate_typesetting_state(state: &Value) -> Result<(), CliError> {
    resolve_typesetting_state(Some(state.clone()))
        .map(|_| ())
        .map_err(|error| CliError::with_code("invalid_target", error.message()))
}

fn is_typesetting_state_v1(state: &Value) -> bool {
    state
        .get("publications")
        .and_then(Value::as_array)
        .is_some_and(|publications| publications.iter().all(is_typesetting_publication_v1))
}

fn is_typesetting_publication_v1(publication: &Value) -> bool {
    publication.get("id").is_some_and(Value::is_string)
        && publication.get("title").is_some_and(Value::is_string)
        && publication.get("createdAt").is_some_and(Value::is_string)
        && publication.get("updatedAt").is_some_and(Value::is_string)
        && publication
            .get("content")
            .is_some_and(is_typesetting_document_v1)
        && publication
            .get("covers")
            .and_then(Value::as_array)
            .is_some_and(|covers| covers.iter().all(is_typesetting_image_v1))
}

fn is_typesetting_document_v1(content: &Value) -> bool {
    content.get("type").and_then(Value::as_str) == Some("doc")
        && is_typesetting_document_node_v1(content)
}

fn is_typesetting_document_node_v1(node: &Value) -> bool {
    node.as_object().is_some()
        && node.get("type").is_some_and(Value::is_string)
        && node.get("text").map_or(true, Value::is_string)
        && node.get("attrs").map_or(true, Value::is_object)
        && node.get("content").map_or(true, |content| {
            content
                .as_array()
                .is_some_and(|children| children.iter().all(is_typesetting_document_node_v1))
        })
        && node.get("marks").map_or(true, |marks| {
            marks.as_array().is_some_and(|marks| {
                marks.iter().all(|mark| {
                    mark.as_object().is_some()
                        && mark.get("type").is_some_and(Value::is_string)
                        && mark.get("attrs").map_or(true, Value::is_object)
                })
            })
        })
}

fn is_typesetting_image_v1(image: &Value) -> bool {
    [
        "assetRef",
        "src",
        "fileName",
        "mimeType",
        "sourceAssetRef",
        "sourceProjectId",
        "sourceCanvasPanelId",
    ]
    .iter()
    .all(|key| image.get(key).is_some_and(Value::is_string))
        && image
            .get("src")
            .and_then(Value::as_str)
            .is_some_and(|src| src.starts_with('/'))
        && ["width", "height"].iter().all(|key| {
            image
                .get(key)
                .map_or(true, |value| value.as_f64().is_some())
        })
}

fn resolve_writing_state(state: Option<Value>) -> Result<PanelStateResolution, CliError> {
    let Some(mut state) = state else {
        return Ok(PanelStateResolution {
            state: empty_writing_state(),
            changed: true,
        });
    };
    let valid = state.get("schemaVersion").and_then(Value::as_i64) == Some(5)
        && state.get("draft").is_some_and(Value::is_string)
        && state.get("refinementName").is_some_and(Value::is_string)
        && matches!(
            state.get("mode").and_then(Value::as_str),
            Some("create" | "revise" | "refine")
        )
        && state
            .get("selectedCreateWritingSkillIds")
            .and_then(Value::as_array)
            .is_some_and(|ids| ids.iter().all(Value::is_string))
        && state
            .get("selectedRevisionWritingSkillId")
            .is_some_and(|id| id.is_null() || id.is_string());
    if valid {
        let changed = !state
            .get("selectedRefinementSkillId")
            .is_some_and(Value::is_string);
        if changed {
            state["selectedRefinementSkillId"] = json!(DEFAULT_REFINEMENT_SKILL_ID);
        }
        Ok(PanelStateResolution {
            state,
            changed,
        })
    } else {
        Err(CliError::new(
            "Malformed writing panel state: expected schemaVersion 5.",
        ))
    }
}

fn resolve_canvas_state(state: Option<Value>) -> Result<PanelStateResolution, CliError> {
    let Some(state) = state else {
        return Ok(PanelStateResolution {
            state: empty_canvas_snapshot(),
            changed: true,
        });
    };
    match state
        .get("schema")
        .and_then(|schema| schema.get("schemaVersion"))
        .and_then(Value::as_i64)
    {
        Some(1) => Ok(PanelStateResolution {
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

fn resolve_wiki_state(
    _storage: &Storage,
    _project: &Project,
    _panel: &Panel,
    state: Option<Value>,
) -> Result<PanelStateResolution, CliError> {
    let Some(state) = state else {
        return Ok(PanelStateResolution {
            state: empty_wiki_state(),
            changed: true,
        });
    };
    let valid = state.get("schemaVersion").and_then(Value::as_i64) == Some(4)
        && state.get("rawDocuments").is_some_and(Value::is_array)
        && state.get("ruleSets").is_some_and(Value::is_array)
        && state.get("wikiSpaces").is_some_and(Value::is_array)
        && state.get("generatedDocuments").is_some_and(Value::is_array)
        && state.get("tasks").is_none();
    if valid {
        Ok(PanelStateResolution {
            state,
            changed: false,
        })
    } else {
        Err(CliError::new(
            "Malformed wiki panel state: expected schemaVersion 4.",
        ))
    }
}

fn initial_panel_state(kind: PanelKind) -> Value {
    match kind {
        PanelKind::Canvas => empty_canvas_snapshot(),
        PanelKind::Wiki => empty_wiki_state(),
        PanelKind::Writing => empty_writing_state(),
        PanelKind::Typesetting => empty_typesetting_state(),
        PanelKind::Publishing => empty_publishing_state(),
    }
}

fn empty_publishing_state() -> Value {
    crate::publishing::empty_state()
}

fn empty_typesetting_state() -> Value {
    json!({
        "schemaVersion": 1,
        "publications": [],
    })
}

fn empty_writing_state() -> Value {
    json!({
        "schemaVersion": 5,
        "draft": "",
        "mode": "create",
        "refinementName": "",
        "targetGeneratedDocumentId": null,
        "selectedCreateWritingSkillIds": [DEFAULT_WRITING_SKILL_ID],
        "selectedRevisionWritingSkillId": DEFAULT_WRITING_SKILL_ID,
        "selectedRefinementSkillId": DEFAULT_REFINEMENT_SKILL_ID,
    })
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
        "schemaVersion": 4,
        "rawDocuments": [],
        "generatedDocuments": [],
        "ruleSets": [],
        "wikiSpaces": [{
            "id": DEFAULT_WIKI_SPACE_ID,
            "title": "Wiki",
            "ruleSetId": null,
            "ruleSetVersion": null,
            "rootRef": "wikis/wiki:default",
            "pageIndex": [],
            "createdAt": now,
            "updatedAt": now,
        }],
        "activeRawDocumentId": null,
        "activeWikiSpaceId": DEFAULT_WIKI_SPACE_ID,
        "activeWikiPagePath": null,
        "wikiAgentSkillConfigured": false,
        "wikiAgentSkillId": "karpathy-llm-wiki",
    })
}

fn initial_panel_title(kind: PanelKind) -> &'static str {
    match kind {
        PanelKind::Wiki => "文档库",
        PanelKind::Writing => "写作",
        PanelKind::Canvas => "Design canvas",
        PanelKind::Typesetting => "排版",
        PanelKind::Publishing => "发布",
    }
}

fn next_project_title(projects: &[Project]) -> String {
    let max_project_number = projects
        .iter()
        .filter_map(|project| project.title.strip_prefix("Project "))
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

fn create_myopenpanels_id(prefix: &str) -> String {
    crate::ids::random_id(prefix)
}

fn read_active_project(paths: &MyOpenPanelsPaths) -> Result<Option<String>, CliError> {
    let value = read_json_object_or_null(&paths.focus_dir.join("active-project.json"))?;
    Ok(value.and_then(|value| {
        value
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::to_owned)
    }))
}

fn write_active_project(paths: &MyOpenPanelsPaths, project_id: &str) -> Result<(), CliError> {
    write_json(
        &paths.focus_dir.join("active-project.json"),
        &json!({ "projectId": project_id, "updatedAt": now_iso() }),
    )
}

fn read_active_panel(paths: &MyOpenPanelsPaths) -> Result<Option<ActivePanel>, CliError> {
    let Some(value) = read_json_object_or_null(&paths.focus_dir.join("active-panel.json"))? else {
        return Ok(None);
    };
    Ok(Some(ActivePanel {
        kind: value
            .get("kind")
            .and_then(Value::as_str)
            .and_then(PanelKind::parse),
        project_id: value
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }))
}

fn write_active_panel(paths: &MyOpenPanelsPaths, panel: &Panel) -> Result<(), CliError> {
    let current = read_json_object_or_null(&paths.focus_dir.join("active-panel.json"))?;
    let unchanged = current.as_ref().is_some_and(|value| {
        value.get("projectId").and_then(Value::as_str) == Some(panel.project_id.as_str())
            && value.get("panelId").and_then(Value::as_str) == Some(panel.id.as_str())
            && value.get("kind").and_then(Value::as_str) == Some(panel.kind.as_str())
    });
    let focus_revision = current
        .as_ref()
        .and_then(|value| value.get("focusRevision"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
        + u64::from(!unchanged);
    write_json(
        &paths.focus_dir.join("active-panel.json"),
        &json!({
            "projectId": panel.project_id,
            "panelId": panel.id,
            "kind": panel.kind,
            "focusRevision": focus_revision,
            "updatedAt": now_iso(),
        }),
    )
}

pub fn read_focus_revision(paths: &MyOpenPanelsPaths) -> Result<u64, CliError> {
    Ok(
        read_json_object_or_null(&paths.focus_dir.join("active-panel.json"))?
            .and_then(|value| value.get("focusRevision").and_then(Value::as_u64))
            .unwrap_or(0),
    )
}

#[derive(Debug)]
struct ActivePanel {
    kind: Option<PanelKind>,
    project_id: Option<String>,
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
