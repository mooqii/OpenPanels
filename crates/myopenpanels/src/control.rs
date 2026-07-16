use crate::error::CliError;
use crate::paths::{sanitize_path_part, MyOpenPanelsPaths};
use crate::storage::Storage;
use crate::tasks::{annotate_tasks, pending_task_count};
use crate::types::{Panel, PanelKind, Project, ProjectBootstrap, ProjectPanelSnapshot};
use rand::Rng;
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
const DEFAULT_WIKI_RULE_SET_ID: &str = "rule-set:default";
const DEFAULT_WIKI_SPACE_ID: &str = "wiki:default";
const DEFAULT_WRITING_SKILL_ID: &str = "writing-default";

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
        let migrated_state = migrate_panel_state(storage, project, &panel, raw_state)?;
        let revision = if migrated_state.changed {
            storage.write_panel_state(&project.id, &panel.id, &migrated_state.state)?
        } else {
            storage.read_panel_state_revision(&project.id, &panel.id)?
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
    project: &Project,
    panel: &Panel,
    state: Option<Value>,
) -> Result<PanelStateMigrationResult, CliError> {
    match panel.kind {
        PanelKind::Canvas => migrate_canvas_state(state),
        PanelKind::Wiki => migrate_wiki_state(storage, project, panel, state),
        PanelKind::Writing => migrate_writing_state(state),
        PanelKind::Typesetting => migrate_typesetting_state(state),
        PanelKind::Publishing => migrate_publishing_state(state),
    }
}

fn migrate_publishing_state(state: Option<Value>) -> Result<PanelStateMigrationResult, CliError> {
    let Some(state) = state else {
        return Ok(PanelStateMigrationResult {
            state: empty_publishing_state(),
            changed: true,
        });
    };
    match state.get("schemaVersion").and_then(Value::as_i64) {
        Some(1) if state.as_object().is_some() => Ok(PanelStateMigrationResult {
            state,
            changed: false,
        }),
        Some(1) => Err(CliError::new("Malformed publishing panel state.")),
        Some(version) => Err(CliError::new(format!(
            "Unsupported future publishing panel state schemaVersion: {version}"
        ))),
        None => Err(CliError::new(
            "Malformed publishing panel state: missing schemaVersion.",
        )),
    }
}

fn migrate_typesetting_state(state: Option<Value>) -> Result<PanelStateMigrationResult, CliError> {
    let Some(state) = state else {
        return Ok(PanelStateMigrationResult {
            state: empty_typesetting_state(),
            changed: true,
        });
    };
    match state.get("schemaVersion").and_then(Value::as_i64) {
        Some(1) if is_typesetting_state_v1(&state) => Ok(PanelStateMigrationResult {
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
    migrate_typesetting_state(Some(state.clone()))
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

fn migrate_writing_state(state: Option<Value>) -> Result<PanelStateMigrationResult, CliError> {
    let Some(state) = state else {
        return Ok(PanelStateMigrationResult {
            state: empty_writing_state(),
            changed: true,
        });
    };
    match state.get("schemaVersion").and_then(Value::as_i64) {
        Some(1)
            if state.get("draft").is_some_and(Value::is_string)
                && matches!(
                    state.get("mode").and_then(Value::as_str),
                    Some("create" | "revise")
                ) =>
        {
            let mut migrated = state;
            migrated["schemaVersion"] = json!(5);
            migrated["refinementName"] = json!("");
            migrated["selectedCreateWritingSkillIds"] = json!([DEFAULT_WRITING_SKILL_ID]);
            migrated["selectedRevisionWritingSkillId"] = json!(DEFAULT_WRITING_SKILL_ID);
            Ok(PanelStateMigrationResult {
                state: migrated,
                changed: true,
            })
        }
        Some(2)
            if state.get("draft").is_some_and(Value::is_string)
                && matches!(
                    state.get("mode").and_then(Value::as_str),
                    Some("create" | "revise")
                )
                && state
                    .get("selectedWritingSkillIds")
                    .and_then(Value::as_array)
                    .is_some_and(|ids| ids.iter().all(Value::is_string)) =>
        {
            let mut migrated = state;
            let mut selected = migrated["selectedWritingSkillIds"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if selected.is_empty() {
                selected.push(json!(DEFAULT_WRITING_SKILL_ID));
            }
            let revision = selected
                .first()
                .filter(|_| selected.len() == 1)
                .cloned()
                .unwrap_or_else(|| json!(DEFAULT_WRITING_SKILL_ID));
            migrated["schemaVersion"] = json!(5);
            migrated["refinementName"] = json!("");
            migrated["selectedCreateWritingSkillIds"] = Value::Array(selected);
            migrated["selectedRevisionWritingSkillId"] = revision;
            migrated
                .as_object_mut()
                .expect("writing state object")
                .remove("selectedWritingSkillIds");
            Ok(PanelStateMigrationResult {
                state: migrated,
                changed: true,
            })
        }
        Some(3)
            if state.get("draft").is_some_and(Value::is_string)
                && matches!(
                    state.get("mode").and_then(Value::as_str),
                    Some("create" | "revise")
                )
                && state
                    .get("selectedWritingSkillIds")
                    .and_then(Value::as_array)
                    .is_some_and(|ids| ids.iter().all(Value::is_string)) =>
        {
            let selected = state["selectedWritingSkillIds"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let revision = selected
                .first()
                .filter(|_| selected.len() == 1)
                .cloned()
                .unwrap_or_else(|| json!(DEFAULT_WRITING_SKILL_ID));
            let mut migrated = state;
            migrated["schemaVersion"] = json!(5);
            migrated["refinementName"] = json!("");
            migrated["selectedCreateWritingSkillIds"] = Value::Array(selected);
            migrated["selectedRevisionWritingSkillId"] = revision;
            migrated
                .as_object_mut()
                .expect("writing state object")
                .remove("selectedWritingSkillIds");
            Ok(PanelStateMigrationResult {
                state: migrated,
                changed: true,
            })
        }
        Some(4)
            if state.get("draft").is_some_and(Value::is_string)
                && matches!(
                    state.get("mode").and_then(Value::as_str),
                    Some("create" | "revise")
                )
                && state
                    .get("selectedCreateWritingSkillIds")
                    .and_then(Value::as_array)
                    .is_some_and(|ids| ids.iter().all(Value::is_string))
                && state
                    .get("selectedRevisionWritingSkillId")
                    .is_some_and(|id| id.is_null() || id.is_string()) =>
        {
            let mut migrated = state;
            migrated["schemaVersion"] = json!(5);
            migrated["refinementName"] = json!("");
            Ok(PanelStateMigrationResult {
                state: migrated,
                changed: true,
            })
        }
        Some(5)
            if state.get("draft").is_some_and(Value::is_string)
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
                    .is_some_and(|id| id.is_null() || id.is_string()) =>
        {
            Ok(PanelStateMigrationResult {
                state,
                changed: false,
            })
        }
        Some(1) => Err(CliError::new("Malformed writing panel state.")),
        Some(2) => Err(CliError::new("Malformed writing panel state.")),
        Some(3) => Err(CliError::new("Malformed writing panel state.")),
        Some(4) => Err(CliError::new("Malformed writing panel state.")),
        Some(5) => Err(CliError::new("Malformed writing panel state.")),
        Some(version) => Err(CliError::new(format!(
            "Unsupported future writing panel state schemaVersion: {version}"
        ))),
        None => Err(CliError::new(
            "Malformed writing panel state: missing schemaVersion.",
        )),
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
    project: &Project,
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
        Some(4) => {
            if is_wiki_state_v4(&state) {
                Ok(normalize_wiki_state_v4(state))
            } else {
                Err(CliError::new(
                    "Malformed wiki panel state: schemaVersion 4 is missing required arrays.",
                ))
            }
        }
        Some(3) => {
            if is_wiki_state_v3(&state) {
                storage.upsert_tasks(
                    &project.id,
                    &panel.id,
                    "wiki",
                    state["tasks"].as_array().expect("validated Wiki v3 tasks"),
                )?;
                let mut migrated = state;
                migrated
                    .as_object_mut()
                    .map(|object| object.remove("tasks"));
                migrated["schemaVersion"] = json!(4);
                let mut normalized = normalize_wiki_state_v4(migrated);
                normalized.changed = true;
                Ok(normalized)
            } else {
                Err(CliError::new(
                    "Malformed wiki panel state: schemaVersion 3 is missing required arrays.",
                ))
            }
        }
        Some(2) => {
            if is_wiki_state_v2(&state) {
                storage.upsert_tasks(
                    &project.id,
                    &panel.id,
                    "wiki",
                    state["tasks"].as_array().expect("validated Wiki v2 tasks"),
                )?;
                let mut migrated = state;
                migrated["schemaVersion"] = json!(4);
                migrated["generatedDocuments"] = json!([]);
                migrated
                    .as_object_mut()
                    .map(|object| object.remove("tasks"));
                let normalized = normalize_wiki_state_v4(migrated);
                Ok(PanelStateMigrationResult {
                    changed: true,
                    state: normalized.state,
                })
            } else {
                Err(CliError::new(
                    "Malformed wiki panel state: schemaVersion 2 is missing required arrays.",
                ))
            }
        }
        Some(1) => Ok(PanelStateMigrationResult {
            state: migrate_wiki_state_v1_to_v4(storage, project, panel, &state)?,
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

fn normalize_wiki_state_v2(mut state: Value) -> PanelStateMigrationResult {
    let mut changed = false;
    if let Some(object) = state.as_object_mut() {
        if object.remove("wikiLanguage").is_some() {
            changed = true;
        }
        let current_skill = object
            .get("wikiAgentSkillId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let normalized_skill = match current_skill {
            "karpathy-llm-wiki" => Some("karpathy-llm-wiki"),
            "karpathy-llm-wiki-zh" => Some("karpathy-llm-wiki-zh"),
            _ => None,
        };
        if normalized_skill != Some(current_skill) {
            object.insert(
                "wikiAgentSkillId".to_owned(),
                json!(normalized_skill.unwrap_or("karpathy-llm-wiki")),
            );
            changed = true;
        }
        if !object
            .get("wikiAgentSkillConfigured")
            .is_some_and(Value::is_boolean)
        {
            object.insert("wikiAgentSkillConfigured".to_owned(), json!(false));
            changed = true;
        }
        if let Some(spaces) = object.get_mut("wikiSpaces").and_then(Value::as_array_mut) {
            for space in spaces {
                let is_default_space =
                    space.get("id").and_then(Value::as_str) == Some(DEFAULT_WIKI_SPACE_ID);
                let has_legacy_title =
                    space.get("title").and_then(Value::as_str) == Some("Default Wiki");
                if is_default_space && has_legacy_title {
                    space["title"] = json!("Wiki");
                    changed = true;
                }
            }
        }
    }
    PanelStateMigrationResult { state, changed }
}

fn normalize_wiki_state_v4(state: Value) -> PanelStateMigrationResult {
    normalize_wiki_state_v2(state)
}

fn is_wiki_state_v4(state: &Value) -> bool {
    state.get("rawDocuments").is_some_and(Value::is_array)
        && state.get("ruleSets").is_some_and(Value::is_array)
        && state.get("wikiSpaces").is_some_and(Value::is_array)
        && state.get("generatedDocuments").is_some_and(Value::is_array)
        && state.get("tasks").is_none()
}

fn is_wiki_state_v3(state: &Value) -> bool {
    is_wiki_state_v2(state) && state.get("generatedDocuments").is_some_and(Value::is_array)
}

fn is_wiki_state_v2(state: &Value) -> bool {
    state.get("rawDocuments").is_some_and(Value::is_array)
        && state.get("ruleSets").is_some_and(Value::is_array)
        && state.get("wikiSpaces").is_some_and(Value::is_array)
        && state.get("tasks").is_some_and(Value::is_array)
}

fn migrate_wiki_state_v1_to_v4(
    storage: &Storage,
    project: &Project,
    panel: &Panel,
    state: &Value,
) -> Result<Value, CliError> {
    let pages = state
        .get("pages")
        .and_then(Value::as_array)
        .ok_or_else(|| CliError::new("Malformed wiki v1 state: pages must be an array."))?;
    let active_page_id = state.get("activePageId").and_then(Value::as_str);
    let panel_dir = storage.panel_dir(&project.id, &panel.id);
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
            "wordCount": markdown.chars().filter(|character| !character.is_whitespace()).count(),
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
        PanelKind::Writing => empty_writing_state(),
        PanelKind::Typesetting => empty_typesetting_state(),
        PanelKind::Publishing => empty_publishing_state(),
    }
}

fn empty_publishing_state() -> Value {
    json!({ "schemaVersion": 1 })
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
            "title": "Wiki",
            "ruleSetId": DEFAULT_WIKI_RULE_SET_ID,
            "ruleSetVersion": 1,
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
    let random: u128 = rand::rng().random();
    format!("{prefix}:{random:032x}")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::resolve_myopenpanels_paths;

    #[test]
    fn bootstrap_creates_project_with_all_default_panels() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");

        let bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");

        assert_eq!(bootstrap.project.title, "Project 1");
        assert_eq!(bootstrap.active_panel_kind, PanelKind::Wiki);
        assert_eq!(
            bootstrap
                .panels
                .iter()
                .map(|snapshot| snapshot.panel.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["wiki", "writing", "canvas", "typesetting", "publishing"]
        );
        assert_eq!(bootstrap.state["schemaVersion"], json!(4));
        assert_eq!(bootstrap.state["wikiSpaces"][0]["title"], json!("Wiki"));
        assert_eq!(bootstrap.state["activeWikiPagePath"], Value::Null);
        let writing = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Writing)
            .expect("writing panel");
        assert_eq!(writing.state["schemaVersion"], json!(5));
        assert_eq!(
            writing.state["selectedCreateWritingSkillIds"],
            json!(["writing-default"])
        );
        assert_eq!(
            writing.state["selectedRevisionWritingSkillId"],
            json!("writing-default")
        );
        let typesetting = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Typesetting)
            .expect("typesetting panel");
        assert_eq!(typesetting.panel.title, "排版");
        assert_eq!(
            typesetting.state,
            json!({ "schemaVersion": 1, "publications": [] })
        );
        let publishing = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Publishing)
            .expect("publishing panel");
        assert_eq!(publishing.panel.title, "发布");
        assert_eq!(publishing.state, json!({ "schemaVersion": 1 }));
        assert!(paths.focus_dir.join("active-project.json").exists());
        assert!(paths.focus_dir.join("active-panel.json").exists());
        assert!(storage_dir
            .join(crate::storage::DATABASE_FILE_NAME)
            .exists());
    }

    #[test]
    fn bootstrap_backfills_new_default_panels_for_an_existing_project() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");
        let storage = Storage::open(&paths).expect("storage");
        let mut project =
            create_project_record(&storage, "Existing".to_owned()).expect("existing project");
        for kind in [PanelKind::Wiki, PanelKind::Writing, PanelKind::Canvas] {
            project = ensure_panel_for_project(&storage, &project, kind).expect("default panel");
        }
        drop(storage);

        let first = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let second = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        assert_eq!(
            first
                .panels
                .iter()
                .filter(|snapshot| snapshot.panel.kind == PanelKind::Typesetting)
                .count(),
            1
        );
        assert_eq!(
            first
                .panels
                .iter()
                .filter(|snapshot| snapshot.panel.kind == PanelKind::Publishing)
                .count(),
            1
        );
        assert_eq!(
            second
                .panels
                .iter()
                .map(|snapshot| snapshot.panel.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["wiki", "writing", "canvas", "typesetting", "publishing"]
        );
    }

    #[test]
    fn publishing_state_rejects_future_schema_versions() {
        let future = match migrate_publishing_state(Some(json!({
            "schemaVersion": 2
        }))) {
            Ok(_) => panic!("future state"),
            Err(error) => error,
        };
        assert!(future.message().contains("Unsupported future publishing"));
    }

    #[test]
    fn typesetting_state_rejects_future_and_malformed_documents() {
        let future = match migrate_typesetting_state(Some(json!({
            "schemaVersion": 2,
            "publications": []
        }))) {
            Ok(_) => panic!("future state"),
            Err(error) => error,
        };
        assert!(future.message().contains("Unsupported future typesetting"));

        let malformed = match migrate_typesetting_state(Some(json!({
            "schemaVersion": 1,
            "publications": [{
                "id": "publication:1",
                "title": "Broken",
                "covers": [],
                "content": null,
                "createdAt": "2026-07-14T00:00:00Z",
                "updatedAt": "2026-07-14T00:00:00Z"
            }]
        }))) {
            Ok(_) => panic!("malformed state"),
            Err(error) => error,
        };
        assert!(malformed.message().contains("Malformed typesetting"));
    }

    #[test]
    fn writing_v1_state_migrates_with_the_default_skill_selected() {
        let migrated = migrate_writing_state(Some(json!({
            "schemaVersion": 1,
            "draft": "Keep this draft",
            "mode": "create",
            "targetGeneratedDocumentId": null,
        })))
        .expect("writing migration");

        assert!(migrated.changed);
        assert_eq!(migrated.state["schemaVersion"], json!(5));
        assert_eq!(migrated.state["draft"], json!("Keep this draft"));
        assert_eq!(
            migrated.state["selectedCreateWritingSkillIds"],
            json!(["writing-default"])
        );
        assert_eq!(
            migrated.state["selectedRevisionWritingSkillId"],
            json!("writing-default")
        );
    }

    #[test]
    fn writing_v2_empty_selection_migrates_once_but_v3_empty_is_preserved() {
        let migrated = migrate_writing_state(Some(json!({
            "schemaVersion": 2,
            "draft": "",
            "mode": "create",
            "targetGeneratedDocumentId": null,
            "selectedWritingSkillIds": [],
        })))
        .expect("writing v2 migration");
        assert!(migrated.changed);
        assert_eq!(migrated.state["schemaVersion"], json!(5));
        assert_eq!(
            migrated.state["selectedCreateWritingSkillIds"],
            json!(["writing-default"])
        );
        assert_eq!(
            migrated.state["selectedRevisionWritingSkillId"],
            json!("writing-default")
        );

        let explicit_empty = migrate_writing_state(Some(json!({
            "schemaVersion": 3,
            "draft": "",
            "mode": "create",
            "targetGeneratedDocumentId": null,
            "selectedWritingSkillIds": [],
        })))
        .expect("writing v3 state");
        assert!(explicit_empty.changed);
        assert_eq!(
            explicit_empty.state["selectedCreateWritingSkillIds"],
            json!([])
        );
        assert_eq!(
            explicit_empty.state["selectedRevisionWritingSkillId"],
            json!("writing-default")
        );

        let independent = migrate_writing_state(Some(json!({
            "schemaVersion": 4,
            "draft": "",
            "mode": "revise",
            "targetGeneratedDocumentId": null,
            "selectedCreateWritingSkillIds": [
                "writing-xiaohongshu-note",
                "writing-long-article"
            ],
            "selectedRevisionWritingSkillId": "writing-default",
        })))
        .expect("writing v4 state");
        assert!(independent.changed);
        assert_eq!(independent.state["schemaVersion"], json!(5));
        assert_eq!(independent.state["refinementName"], json!(""));
        assert_eq!(
            independent.state["selectedCreateWritingSkillIds"],
            json!(["writing-xiaohongshu-note", "writing-long-article"])
        );
        assert_eq!(
            independent.state["selectedRevisionWritingSkillId"],
            json!("writing-default")
        );
    }

    #[test]
    fn normalize_wiki_state_renames_the_legacy_default_space() {
        let mut state = empty_wiki_state();
        state["wikiSpaces"][0]["title"] = json!("Default Wiki");

        let normalized = normalize_wiki_state_v2(state);

        assert!(normalized.changed);
        assert_eq!(normalized.state["wikiSpaces"][0]["title"], json!("Wiki"));
    }

    #[test]
    fn normalize_wiki_state_marks_an_unconfigured_agent_skill() {
        let mut state = empty_wiki_state();
        state
            .as_object_mut()
            .unwrap()
            .remove("wikiAgentSkillConfigured");

        let normalized = normalize_wiki_state_v4(state);

        assert!(normalized.changed);
        assert_eq!(normalized.state["wikiAgentSkillConfigured"], json!(false));
    }

    #[test]
    fn migrate_wiki_v2_to_v4_preserves_existing_content() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
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
        let mut state = empty_wiki_state();
        state["schemaVersion"] = json!(2);
        state["tasks"] = json!([]);
        state["rawDocuments"] = json!([{ "id": "raw:kept" }]);
        state
            .as_object_mut()
            .expect("state")
            .remove("generatedDocuments");
        let migrated = migrate_wiki_state(
            &Storage::open(&paths).expect("storage"),
            &bootstrap.project,
            &wiki_panel,
            Some(state),
        )
        .expect("migration");
        assert!(migrated.changed);
        assert_eq!(migrated.state["schemaVersion"], 4);
        assert_eq!(migrated.state["rawDocuments"][0]["id"], "raw:kept");
        assert_eq!(
            migrated.state["generatedDocuments"]
                .as_array()
                .map(Vec::len),
            Some(0)
        );
    }

    #[test]
    fn bootstrap_uses_the_global_studio_project_for_every_context() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let first_paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("thread-a"),
        )
        .expect("first paths");
        let second_paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("thread-b"),
        )
        .expect("second paths");
        let third_paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("thread-c"),
        )
        .expect("third paths");

        let first = ensure_project_bootstrap(&first_paths, BootstrapRequest::new()).expect("first");
        let latest = create_project(&second_paths, Some("Latest")).expect("latest");
        let third =
            ensure_project_bootstrap(&third_paths, BootstrapRequest::new()).expect("new context");
        let first_again =
            ensure_project_bootstrap(&first_paths, BootstrapRequest::new()).expect("again");

        assert_eq!(first.project.title, "Project 1");
        assert_eq!(first_again.project.id, latest.project.id);
        assert_eq!(third.project.id, latest.project.id);
        assert_eq!(third.projects.len(), 2);
    }

    #[test]
    fn newest_project_prefers_updated_time_then_created_time_and_id() {
        let project = |id: &str, created_at: &str, updated_at: &str| Project {
            id: id.to_owned(),
            title: id.to_owned(),
            created_at: created_at.to_owned(),
            updated_at: updated_at.to_owned(),
            panel_ids: Vec::new(),
        };
        let projects = vec![
            project(
                "project:older",
                "2026-07-11T10:00:00Z",
                "2026-07-11T12:00:00Z",
            ),
            project(
                "project:newer",
                "2026-07-11T11:00:00Z",
                "2026-07-11T13:00:00Z",
            ),
        ];

        assert_eq!(
            most_recently_updated_project(&projects).unwrap().id,
            "project:newer"
        );
    }

    #[test]
    fn bootstrap_errors_for_missing_requested_project_instead_of_creating_one() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");

        create_project(&paths, Some("Demo")).expect("demo project");
        write_active_project_id(&paths, "project:deleted").expect("stale active project");

        let error = ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_id: None,
                requested_panel_kind: None,
                requested_project_id: Some("project:deleted".to_owned()),
            },
        )
        .expect_err("missing requested project should fail");

        assert_eq!(error.code(), Some("project_not_found"));
        let projects = Storage::open(&paths)
            .expect("storage")
            .list_projects()
            .expect("projects");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].title, "Demo");
    }

    #[test]
    fn bootstrap_recovers_stale_active_project_without_creating_one() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");

        let demo = create_project(&paths, Some("Demo")).expect("demo project");
        write_active_project_id(&paths, "project:deleted").expect("stale active project");

        let recovered =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");

        assert_eq!(recovered.project.id, demo.project.id);
        let projects = Storage::open(&paths)
            .expect("storage")
            .list_projects()
            .expect("projects");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].title, "Demo");
        assert_eq!(
            read_active_project_id(&paths).expect("active project"),
            Some(demo.project.id)
        );
    }

    #[test]
    fn bootstrap_migrates_wiki_v1_state_to_v4_without_clearing_pages() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
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
                &bootstrap.project.id,
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
        let panel_dir = storage.panel_dir(&bootstrap.project.id, &wiki_panel.id);
        drop(storage);

        let migrated = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("migrated");
        assert_eq!(migrated.state["schemaVersion"], json!(4));
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
        let paths = resolve_myopenpanels_paths(
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
                &bootstrap.project.id,
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
        let paths = resolve_myopenpanels_paths(
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
                &bootstrap.project.id,
                &wiki_panel.id,
                &json!({ "schemaVersion": 5, "rawDocuments": [] }),
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
        let paths = resolve_myopenpanels_paths(
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
                &bootstrap.project.id,
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
