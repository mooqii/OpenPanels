use crate::control::{read_focus_revision, read_project_bootstrap, BootstrapRequest};
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
        skill_id: Some(crate::wiki::WIKI_PANEL_SKILL_ID),
        context: wiki_context,
        selection: Some(wiki_selection),
    },
    PanelModule {
        kind: PanelKind::Canvas,
        skill_id: Some(crate::agent::CANVAS_PANEL_SKILL_ID),
        context: canvas_context,
        selection: Some(canvas_selection),
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
}

pub fn read_context(paths: &MyOpenPanelsPaths) -> Result<PanelContextPayload, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
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

pub fn read_state(paths: &MyOpenPanelsPaths) -> Result<PanelStatePayload, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    Ok(PanelStatePayload {
        focus: focus(paths, &bootstrap)?,
        revision: bootstrap.revision,
        state: bootstrap.state,
    })
}

pub fn read_selection(paths: &MyOpenPanelsPaths) -> Result<PanelSelectionEnvelope, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let focus = focus(paths, &bootstrap)?;
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
    let payload = read_selection_for_panel(paths, &bootstrap.session.id, &bootstrap.panel.id)?;
    let value = payload.selection;
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
    })
}

fn wiki_selection(
    paths: &MyOpenPanelsPaths,
    _bootstrap: &ProjectBootstrap,
    focus: Value,
) -> Result<PanelSelectionEnvelope, CliError> {
    let value = crate::wiki::read_agent_selection(paths)?;
    let selection = value.get("selection").cloned().unwrap_or_else(|| json!({}));
    let is_explicit = selection
        .get("isExplicitSelection")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let raw_count = value
        .get("selectedRawDocuments")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let generated_count = value
        .get("selectedGeneratedDocuments")
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
            "rawDocumentCount": raw_count,
            "generatedDocumentCount": generated_count,
            "wikiSelected": selection.get("isWikiSelected").and_then(Value::as_bool).unwrap_or(false),
        }),
        value,
    })
}

fn focus(paths: &MyOpenPanelsPaths, bootstrap: &ProjectBootstrap) -> Result<Value, CliError> {
    Ok(json!({
        "focusRevision": read_focus_revision(paths)?,
        "projectId": bootstrap.session.id,
        "panelId": bootstrap.panel.id,
        "panelKind": bootstrap.panel.kind,
    }))
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
        "generatedDocumentCount": array_len(&bootstrap.state, "generatedDocuments"),
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
