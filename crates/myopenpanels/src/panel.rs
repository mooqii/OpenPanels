use crate::control::{
    read_active_panel_value, read_focus_revision, read_project_bootstrap, BootstrapRequest,
};
use crate::error::CliError;
use crate::paths::MyOpenPanelsPaths;
use crate::selection::read_selection_for_panel;
use crate::types::{PanelKind, ProjectBootstrap};
use serde::Serialize;
use serde_json::{json, Value};

type SelectionReader =
    fn(&MyOpenPanelsPaths, &ProjectBootstrap, Value) -> Result<PanelSelectionEnvelope, CliError>;

struct PanelModule {
    kind: PanelKind,
    skill_id: Option<&'static str>,
    context: fn(&ProjectBootstrap) -> Value,
    selection: Option<SelectionReader>,
}

static PANEL_MODULES: &[PanelModule] = &[
    PanelModule {
        kind: PanelKind::Wiki,
        skill_id: Some(crate::agent::PANELS_SKILL_ID),
        context: wiki_context,
        selection: Some(wiki_selection),
    },
    PanelModule {
        kind: PanelKind::Writing,
        skill_id: Some(crate::agent::PANELS_SKILL_ID),
        context: crate::writing::panel_context,
        selection: Some(writing_selection),
    },
    PanelModule {
        kind: PanelKind::Canvas,
        skill_id: Some(crate::agent::PANELS_SKILL_ID),
        context: canvas_context,
        selection: Some(canvas_selection),
    },
    PanelModule {
        kind: PanelKind::Typesetting,
        skill_id: Some(crate::agent::PANELS_SKILL_ID),
        context: typesetting_context,
        selection: None,
    },
    PanelModule {
        kind: PanelKind::Publishing,
        skill_id: Some(crate::agent::PANELS_SKILL_ID),
        context: publishing_context,
        selection: None,
    },
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelContextPayload {
    pub focus: Value,
    pub context: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelStatePayload {
    pub focus: Value,
    pub revision: i64,
    pub state: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelSelectionEnvelope {
    pub focus: Value,
    pub supported: bool,
    pub selection_kind: Option<String>,
    pub is_explicit: bool,
    pub updated_at: Option<Value>,
    pub summary: Value,
    pub value: Value,
    pub actions: Value,
}

pub fn read_context(
    paths: &MyOpenPanelsPaths,
    panel_kind: Option<PanelKind>,
) -> Result<PanelContextPayload, CliError> {
    let bootstrap = read_panel_bootstrap(paths, panel_kind)?;
    let context = context_for_bootstrap(&bootstrap);
    Ok(PanelContextPayload {
        focus: focus(paths, &bootstrap)?,
        context,
    })
}

pub fn context_for_bootstrap(bootstrap: &ProjectBootstrap) -> Value {
    module(bootstrap.active_panel_kind)
        .map(|module| (module.context)(bootstrap))
        .unwrap_or_else(|| {
            json!({
                "panelKind": bootstrap.active_panel_kind,
                "available": true,
            })
        })
}

pub fn read_state(
    paths: &MyOpenPanelsPaths,
    panel_kind: Option<PanelKind>,
) -> Result<PanelStatePayload, CliError> {
    let bootstrap = read_panel_bootstrap(paths, panel_kind)?;
    Ok(PanelStatePayload {
        focus: focus(paths, &bootstrap)?,
        revision: bootstrap.revision,
        state: bootstrap.state,
    })
}

pub fn read_selection(paths: &MyOpenPanelsPaths) -> Result<PanelSelectionEnvelope, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let focus = focus(paths, &bootstrap)?;
    if bootstrap.active_panel_kind == PanelKind::Canvas {
        let payload = crate::selection::read_selection_for_panel_materialized(
            paths,
            &bootstrap.project.id,
            &bootstrap.panel.id,
        )?;
        return canvas_selection_envelope(payload.selection, focus);
    }
    selection_for_bootstrap(paths, &bootstrap, focus)
}

pub fn selection_for_bootstrap(
    paths: &MyOpenPanelsPaths,
    bootstrap: &ProjectBootstrap,
    focus: Value,
) -> Result<PanelSelectionEnvelope, CliError> {
    if let Some(selection) = module(bootstrap.active_panel_kind).and_then(|module| module.selection)
    {
        return selection(paths, bootstrap, focus);
    }
    Ok(PanelSelectionEnvelope {
        focus,
        supported: false,
        selection_kind: None,
        is_explicit: false,
        updated_at: None,
        summary: json!({ "itemCount": 0 }),
        value: Value::Null,
        actions: json!({ "required": [], "suggested": [] }),
    })
}

pub fn skill_id(kind: PanelKind) -> Option<&'static str> {
    module(kind).and_then(|module| module.skill_id)
}

fn module(kind: PanelKind) -> Option<&'static PanelModule> {
    PANEL_MODULES.iter().find(|module| module.kind == kind)
}

fn canvas_selection(
    paths: &MyOpenPanelsPaths,
    bootstrap: &ProjectBootstrap,
    focus: Value,
) -> Result<PanelSelectionEnvelope, CliError> {
    let payload = read_selection_for_panel(paths, &bootstrap.project.id, &bootstrap.panel.id)?;
    canvas_selection_envelope(payload.selection, focus)
}

fn canvas_selection_envelope(
    value: Value,
    focus: Value,
) -> Result<PanelSelectionEnvelope, CliError> {
    let is_explicit = value
        .get("isExplicitSelection")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let item_count = value
        .get("selectedShapes")
        .and_then(Value::as_array)
        .map(Vec::len)
        .or_else(|| {
            value
                .get("selectedShapeIds")
                .and_then(Value::as_array)
                .map(Vec::len)
        })
        .unwrap_or(0);
    Ok(PanelSelectionEnvelope {
        focus,
        supported: true,
        selection_kind: Some("canvas.shapes".to_owned()),
        is_explicit,
        updated_at: value.get("updatedAt").cloned(),
        summary: json!({ "itemCount": if is_explicit { item_count } else { 0 } }),
        value,
        actions: json!({
            "required": [],
            "suggested": panel_skill_actions(
                crate::agent::PANELS_SKILL_ID,
                "The user request requires Canvas selection or generation guidance.",
            ),
        }),
    })
}

fn wiki_selection(
    paths: &MyOpenPanelsPaths,
    _bootstrap: &ProjectBootstrap,
    focus: Value,
) -> Result<PanelSelectionEnvelope, CliError> {
    let mut value = crate::wiki::read_agent_selection(paths)?;
    let suggested_actions = value
        .as_object_mut()
        .and_then(|object| object.remove("actions"))
        .and_then(|actions| actions.get("suggested").cloned())
        .and_then(|actions| actions.as_array().cloned())
        .unwrap_or_default();
    let selection = value.get("selection").cloned().unwrap_or_else(|| json!({}));
    let is_explicit = selection
        .get("isExplicitSelection")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let my_document_count = value
        .get("selectedMyDocuments")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    Ok(PanelSelectionEnvelope {
        focus,
        supported: true,
        selection_kind: Some("wiki.documents".to_owned()),
        is_explicit,
        updated_at: selection.get("updatedAt").cloned(),
        summary: json!({
            "myDocumentCount": my_document_count,
        }),
        value,
        actions: json!({ "required": [], "suggested": suggested_actions }),
    })
}

fn writing_selection(
    paths: &MyOpenPanelsPaths,
    bootstrap: &ProjectBootstrap,
    focus: Value,
) -> Result<PanelSelectionEnvelope, CliError> {
    let value = crate::writing::panel_selection(paths, bootstrap)?;
    let my_document_count = value
        .get("selectedMyDocumentIds")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let wiki_selected = value
        .get("isWikiSelected")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(PanelSelectionEnvelope {
        focus,
        supported: true,
        selection_kind: Some("writing.context".to_owned()),
        is_explicit: wiki_selected || my_document_count > 0,
        updated_at: value.get("updatedAt").cloned(),
        summary: json!({
            "myDocumentCount": my_document_count,
            "wikiSelected": wiki_selected,
        }),
        value,
        actions: json!({
            "required": [],
            "suggested": panel_skill_actions(
                crate::agent::PANELS_SKILL_ID,
                "The user request targets Writing context or My Document writing.",
            ),
        }),
    })
}

fn panel_skill_actions(skill_id: &str, load_when: &str) -> Vec<Value> {
    let mut action = crate::cli::registry::command_action(
        crate::cli::registry::CommandId::registered("agent.skill.read"),
        vec![
            "--skill-id".to_owned(),
            skill_id.to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ],
    )
    .expect("registered Panel Skill action");
    action["condition"] = json!({
        "type": "agent-judgment",
        "description": load_when,
    });
    vec![action]
}

fn focus(paths: &MyOpenPanelsPaths, bootstrap: &ProjectBootstrap) -> Result<Value, CliError> {
    let active = read_active_panel_value(paths)?;
    let is_active_panel = active.as_ref().is_some_and(|value| {
        value.get("projectId").and_then(Value::as_str) == Some(bootstrap.project.id.as_str())
            && value.get("panelId").and_then(Value::as_str) == Some(bootstrap.panel.id.as_str())
    });
    Ok(json!({
        "focusRevision": is_active_panel.then(|| read_focus_revision(paths)).transpose()?,
        "isActivePanel": is_active_panel,
        "projectId": bootstrap.project.id,
        "panelId": bootstrap.panel.id,
        "panelKind": bootstrap.panel.kind,
    }))
}

fn read_panel_bootstrap(
    paths: &MyOpenPanelsPaths,
    panel_kind: Option<PanelKind>,
) -> Result<ProjectBootstrap, CliError> {
    let mut request = BootstrapRequest::new();
    request.requested_panel_kind = panel_kind;
    read_project_bootstrap(paths, request)
}

fn canvas_context(bootstrap: &ProjectBootstrap) -> Value {
    let shape_count = bootstrap
        .state
        .pointer("/document/store")
        .and_then(Value::as_object)
        .map(|store| {
            store
                .values()
                .filter(|item| item.get("typeName").and_then(Value::as_str) == Some("shape"))
                .count()
        })
        .unwrap_or(0);
    json!({
        "panelKind": "canvas",
        "revision": bootstrap.revision,
        "shapeCount": shape_count,
    })
}

fn typesetting_context(bootstrap: &ProjectBootstrap) -> Value {
    json!({
        "panelKind": "typesetting",
        "revision": bootstrap.revision,
        "publicationCount": array_len(&bootstrap.state, "publications"),
        "selectionSupported": false,
    })
}

fn publishing_context(bootstrap: &ProjectBootstrap) -> Value {
    json!({
        "panelKind": "publishing",
        "revision": bootstrap.revision,
        "selectionSupported": false,
    })
}

fn wiki_context(bootstrap: &ProjectBootstrap) -> Value {
    let active_space_id = bootstrap
        .state
        .get("activeWikiSpaceId")
        .cloned()
        .unwrap_or(Value::Null);
    let page_count = bootstrap
        .state
        .get("wikiSpaces")
        .and_then(Value::as_array)
        .and_then(|spaces| {
            spaces
                .iter()
                .find(|space| space.get("id") == bootstrap.state.get("activeWikiSpaceId"))
        })
        .and_then(|space| space.get("pageIndex"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    json!({
        "panelKind": "wiki",
        "revision": bootstrap.revision,
        "activeWikiSpaceId": active_space_id,
        "rawDocumentCount": array_len(&bootstrap.state, "rawDocuments"),
        "myDocumentCount": array_len(&bootstrap.state, "myDocuments"),
        "taskCount": array_len(&bootstrap.state, "tasks"),
        "pageCount": page_count,
    })
}

fn array_len(value: &Value, key: &str) -> usize {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{create_project, BootstrapRequest};
    use crate::paths::resolve_myopenpanels_paths;

    #[test]
    fn scaffold_panels_expose_basic_context_and_declared_agent_support() {
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
        let project_id = created.project.id;
        let bootstrap = read_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_project_id: Some(project_id.clone()),
                requested_panel_id: None,
                requested_panel_kind: Some(PanelKind::Typesetting),
            },
        )
        .expect("typesetting bootstrap");

        assert_eq!(
            context_for_bootstrap(&bootstrap)["panelKind"],
            "typesetting"
        );
        assert_eq!(context_for_bootstrap(&bootstrap)["publicationCount"], 0);
        assert_eq!(
            skill_id(PanelKind::Typesetting),
            Some(crate::agent::PANELS_SKILL_ID)
        );
        let selection =
            selection_for_bootstrap(&paths, &bootstrap, json!({})).expect("selection envelope");
        assert!(!selection.supported);
        assert!(!selection.is_explicit);

        let publishing = read_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_project_id: Some(project_id),
                requested_panel_id: None,
                requested_panel_kind: Some(PanelKind::Publishing),
            },
        )
        .expect("publishing bootstrap");
        assert_eq!(
            context_for_bootstrap(&publishing)["panelKind"],
            "publishing"
        );
        assert_eq!(
            skill_id(PanelKind::Publishing),
            Some(crate::agent::PANELS_SKILL_ID)
        );
        let selection =
            selection_for_bootstrap(&paths, &publishing, json!({})).expect("selection envelope");
        assert!(!selection.supported);
        assert!(!selection.is_explicit);
    }
}
