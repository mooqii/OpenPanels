use crate::control::{ensure_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::OpenPanelsPaths;
use crate::selection::read_selection;
use crate::types::{PanelKind, ProjectBootstrap};
use include_dir::{include_dir, Dir};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

static AGENT_GUIDES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../agent-guides");

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentGuideMetadata {
    pub applies_to: Vec<String>,
    pub id: String,
    pub load_when: Vec<String>,
    pub requires_capabilities: Vec<String>,
    pub source: String,
    pub task_types: Vec<String>,
    pub title: String,
    pub tokens: String,
}

#[derive(Debug, Clone)]
pub struct AgentGuide {
    pub metadata: AgentGuideMetadata,
    pub body: String,
}

#[derive(Debug, Serialize)]
pub struct AgentGuideReadPayload {
    pub guide: AgentGuideMetadata,
    pub markdown: String,
}

pub fn agent_context(
    paths: &OpenPanelsPaths,
    cli_version: &str,
    panel_kind: Option<PanelKind>,
) -> Result<(Value, String), CliError> {
    let mut request = BootstrapRequest::new();
    request.requested_panel_kind = panel_kind;
    let bootstrap = ensure_project_bootstrap(paths, request)?;
    let guides = list_agent_guides()?;
    let selection = read_selection(paths, None, false).ok();
    let payload = agent_context_payload(&bootstrap, cli_version, &guides, selection.as_ref());
    let markdown = agent_context_markdown(&bootstrap, cli_version, &guides, selection.as_ref());
    Ok((payload, markdown))
}

pub fn list_agent_guides() -> Result<Vec<AgentGuideMetadata>, CliError> {
    Ok(load_agent_guides()?
        .into_iter()
        .map(|guide| guide.metadata)
        .collect())
}

pub fn read_agent_guide(
    paths: &OpenPanelsPaths,
    guide_id: &str,
    task_id: Option<&str>,
) -> Result<AgentGuideReadPayload, CliError> {
    let guide = load_agent_guides()?
        .into_iter()
        .find(|guide| guide.metadata.id == guide_id)
        .ok_or_else(|| CliError::new(format!("OpenPanels agent guide not found: {guide_id}")))?;
    let bootstrap = ensure_project_bootstrap(paths, BootstrapRequest::new())?;
    let selection = read_selection(paths, None, false).ok();
    let markdown = render_agent_guide(&guide, &bootstrap, selection.as_ref(), task_id)?;
    Ok(AgentGuideReadPayload {
        guide: guide.metadata,
        markdown,
    })
}

pub fn capabilities() -> Vec<Value> {
    [
        ("agent.context.read", "Read the compact OpenPanels agent context, current state, capabilities, and available guides.", "openpanels-local agent context --project \"$PWD\"", "Markdown agent context."),
        ("agent.guides.list", "List built-in agent guides that can be loaded on demand.", "openpanels-local agent guides --project \"$PWD\"", "Markdown guide table."),
        ("agent.guide.read", "Read one full guide, optionally enriched with task context.", "openpanels-local agent guide <guide-id> --project \"$PWD\" --task-id <task-id>", "Markdown guide body with dynamic context."),
        ("studio.start", "Start or reuse the local OpenPanels studio.", "openpanels-local studio start --project \"$PWD\" --format json", "Studio process and browser URL metadata."),
        ("studio.status", "Read local studio process status.", "openpanels-local studio status --project \"$PWD\" --format json", "Studio status metadata."),
        ("studio.stop", "Stop the conversation-local studio process.", "openpanels-local studio stop --project \"$PWD\" --format json", "Stop confirmation."),
        ("panel.list", "List panels in the current OpenPanels project.", "openpanels-local panels --project \"$PWD\" --format json", "Panel list and active panel metadata."),
        ("panel.switch", "Switch the active panel by kind.", "openpanels-local active-panel --project \"$PWD\" --kind <kind> --format json", "Active panel metadata."),
        ("panel.state.read", "Read panel state by kind.", "openpanels-local panel-state --project \"$PWD\" --kind <kind> --format json", "Panel state payload."),
        ("canvas.state.read", "Read the current canvas project, panel, and state.", "openpanels-local canvas-state --project \"$PWD\" --format json", "Canvas bootstrap payload."),
        ("canvas.selection.read", "Read current canvas selection summary.", "openpanels-local selection --project \"$PWD\" --format json", "Canvas selection payload."),
        ("canvas.selection.asset.read", "Write selected canvas pixels or fallback image asset to a file.", "openpanels-local read-selection-asset --project \"$PWD\" --output <path> --format json", "Written asset path and metadata."),
        ("canvas.placeholder.create", "Create a generation placeholder on the canvas.", "openpanels-local insert-placeholder --project \"$PWD\" --display-width <w> --display-height <h> --format json", "Placeholder shape id and placement metadata."),
        ("canvas.image.insert", "Insert or replace a local image in the canvas.", "openpanels-local insert-image --project \"$PWD\" --image <path> --placement right --format json", "Inserted image shape id and metadata."),
        ("wiki.context.read", "Read compact agent context with wiki prioritized.", "openpanels-local wiki context --project \"$PWD\"", "Markdown agent context."),
        ("wiki.task.list", "List wiki tasks.", "openpanels-local wiki tasks list --project \"$PWD\" --format json", "Wiki task list."),
        ("wiki.task.next", "Read the next queued or failed wiki task.", "openpanels-local wiki tasks next --project \"$PWD\" --format json", "Next wiki task or null."),
        ("wiki.task.claim", "Claim a wiki task before working on it.", "openpanels-local wiki tasks claim --project \"$PWD\" --task-id <task-id> --format json", "Claimed task and process context."),
        ("wiki.task.complete", "Mark a wiki task complete.", "openpanels-local wiki tasks complete --project \"$PWD\" --task-id <task-id> --format json", "Completed task payload."),
        ("wiki.task.fail", "Mark a wiki task failed with a message.", "openpanels-local wiki tasks fail --project \"$PWD\" --task-id <task-id> --message <message> --format json", "Failed task payload."),
        ("wiki.raw.add", "Add a raw source document to the wiki panel.", "openpanels-local wiki raw add --project \"$PWD\" --file <path> --format json", "Raw document metadata and queued tasks."),
        ("wiki.source.read", "Read markdown for a raw wiki document.", "openpanels-local wiki markdown read --project \"$PWD\" --document-id <document-id> --format json", "Raw document markdown."),
        ("wiki.source.write", "Write markdown for a raw wiki document.", "openpanels-local wiki markdown write --project \"$PWD\" --document-id <document-id> --file <path> --task-id <task-id> --format json", "Updated raw document metadata and queued tasks."),
        ("wiki.page.list", "List pages in a wiki space.", "openpanels-local wiki pages list --project \"$PWD\" --wiki-space-id <wiki-space-id> --format json", "Wiki page index items."),
        ("wiki.page.read", "Read one wiki page.", "openpanels-local wiki pages read --project \"$PWD\" --wiki-space-id <wiki-space-id> --path <page-path> --format json", "Wiki page markdown."),
        ("wiki.page.write", "Create or update one wiki page from a markdown file.", "openpanels-local wiki pages write --project \"$PWD\" --wiki-space-id <wiki-space-id> --path <page-path> --file <md-file> --task-id <task-id> --format json", "Written page metadata and queued tasks."),
        ("wiki.space.list", "List wiki spaces.", "openpanels-local wiki spaces list --project \"$PWD\" --format json", "Wiki spaces."),
        ("wiki.space.switch", "Switch the active wiki space.", "openpanels-local wiki spaces active --project \"$PWD\" --wiki-space-id <wiki-space-id> --format json", "Updated wiki state."),
    ]
    .into_iter()
    .map(|(intent, description, command, output)| {
        json!({
            "intent": intent,
            "description": description,
            "command": command,
            "args": [],
            "output": output,
        })
    })
    .collect()
}

pub fn render_agent_guides_markdown(guides: &[AgentGuideMetadata]) -> String {
    format!(
        "# OpenPanels Agent Guides\n\n{}\n",
        render_guide_table(guides)
    )
}

fn agent_context_payload(
    bootstrap: &ProjectBootstrap,
    cli_version: &str,
    guides: &[AgentGuideMetadata],
    selection: Option<&crate::selection::SelectionPayload>,
) -> Value {
    let wiki = wiki_summary(bootstrap);
    let canvas = canvas_summary(selection);
    json!({
        "protocolVersion": 1,
        "cliVersion": cli_version,
        "project": {
            "id": bootstrap.session.id,
            "title": bootstrap.session.title,
        },
        "activePanel": {
            "id": bootstrap.active_panel_id,
            "kind": bootstrap.active_panel_kind,
            "title": bootstrap.panel.title,
        },
        "panels": bootstrap.panels.iter().map(|snapshot| json!({
            "id": snapshot.panel.id,
            "kind": snapshot.panel.kind,
            "title": snapshot.panel.title,
        })).collect::<Vec<_>>(),
        "state": {
            "wiki": wiki,
            "canvas": canvas,
        },
        "capabilities": capabilities(),
        "suggestedCommands": suggested_commands(bootstrap, guides),
        "availableGuides": guides,
    })
}

fn agent_context_markdown(
    bootstrap: &ProjectBootstrap,
    cli_version: &str,
    guides: &[AgentGuideMetadata],
    selection: Option<&crate::selection::SelectionPayload>,
) -> String {
    let wiki = wiki_summary(bootstrap);
    let canvas = canvas_summary(selection);
    let panels = bootstrap
        .panels
        .iter()
        .map(|snapshot| {
            let marker = if snapshot.panel.id == bootstrap.active_panel_id {
                "*"
            } else {
                "-"
            };
            format!(
                "{marker} {}: {} ({})",
                snapshot.panel.kind.as_str(),
                snapshot.panel.title,
                snapshot.panel.id
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let caps = capabilities()
        .iter()
        .map(|capability| {
            format!(
                "### `{}`\n\n{}\n\nCommand:\n\n```bash\n{}\n```\n\nOutput:\n\n- {}",
                capability["intent"].as_str().unwrap_or(""),
                capability["description"].as_str().unwrap_or(""),
                capability["command"].as_str().unwrap_or(""),
                capability["output"].as_str().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let suggested = suggested_commands(bootstrap, guides)
        .into_iter()
        .map(|item| {
            format!(
                "### `{}`\n\n```bash\n{}\n```",
                item["intent"].as_str().unwrap_or(""),
                item["command"].as_str().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        "# OpenPanels Agent Context\n\nProtocol version: 1\nCLI version: {cli_version}\nProject: {} ({})\nActive panel: {} ({})\n\n## Panels\n\n{panels}\n\n## State\n\n### Wiki\n\n- language: {}\n- pending task count: {}\n- next task: {}\n\n### Canvas\n\n- has selection: {}\n- selected shape count: {}\n- selected image asset: {}\n- fallback: {}\n\n## Capabilities\n\n{caps}\n\n## Suggested Next Commands\n\n{suggested}\n\n## Available Guides\n\n{}\n",
        bootstrap.session.title,
        bootstrap.session.id,
        bootstrap.active_panel_kind.as_str(),
        bootstrap.panel.title,
        wiki["language"].as_str().unwrap_or("not set"),
        wiki["pendingTaskCount"].as_u64().unwrap_or(0),
        format_next_task(wiki.get("nextTask")),
        canvas["hasSelection"].as_bool().unwrap_or(false),
        canvas["selectedShapeCount"].as_u64().unwrap_or(0),
        canvas["hasSelectedImageAsset"].as_bool().unwrap_or(false),
        canvas["fallback"].as_str().unwrap_or("none"),
        render_guide_table(guides)
    )
}

fn render_agent_guide(
    guide: &AgentGuide,
    bootstrap: &ProjectBootstrap,
    selection: Option<&crate::selection::SelectionPayload>,
    task_id: Option<&str>,
) -> Result<String, CliError> {
    let task = task_id.and_then(|id| find_wiki_task(bootstrap, id));
    if task_id.is_some() && task.is_none() {
        return Err(CliError::new(format!(
            "Wiki task not found: {}",
            task_id.unwrap_or_default()
        )));
    }
    Ok(format!(
        "# Guide: {}\n\nTitle: {}\nSource: {}\nApplies to: {}\n\n## Current Context\n\n{}\n\n## Commands For This Guide\n\n{}\n\n## Instructions\n\n{}\n",
        guide.metadata.id,
        guide.metadata.title,
        guide.metadata.source,
        if guide.metadata.applies_to.is_empty() { "any".to_owned() } else { guide.metadata.applies_to.join(", ") },
        render_current_context(bootstrap, selection, task.as_ref()),
        render_guide_commands(guide, task.as_ref()),
        guide.body.trim(),
    ))
}

fn load_agent_guides() -> Result<Vec<AgentGuide>, CliError> {
    if let Ok(dir) = std::env::var("OPENPANELS_AGENT_GUIDES_DIR") {
        if !dir.trim().is_empty() {
            return load_agent_guides_from_dir(PathBuf::from(dir));
        }
    }
    let mut guides = AGENT_GUIDES
        .files()
        .filter(|file| file.path().extension().and_then(|ext| ext.to_str()) == Some("md"))
        .map(|file| {
            let name = file.path().display().to_string();
            let source = std::str::from_utf8(file.contents()).map_err(to_cli_error)?;
            parse_guide(source, &name)
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    guides.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));
    Ok(guides)
}

fn load_agent_guides_from_dir(dir: PathBuf) -> Result<Vec<AgentGuide>, CliError> {
    let mut guides = Vec::new();
    for entry in fs::read_dir(dir).map_err(to_cli_error)? {
        let entry = entry.map_err(to_cli_error)?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let source = fs::read_to_string(&path).map_err(to_cli_error)?;
        guides.push(parse_guide(&source, &path.display().to_string())?);
    }
    guides.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));
    Ok(guides)
}

fn parse_guide(source: &str, file_name: &str) -> Result<AgentGuide, CliError> {
    let rest = source
        .strip_prefix("---\n")
        .ok_or_else(|| CliError::new(format!("Agent guide is missing frontmatter: {file_name}")))?;
    let (frontmatter, body) = rest
        .split_once("\n---")
        .ok_or_else(|| CliError::new(format!("Agent guide is missing frontmatter: {file_name}")))?;
    let frontmatter = parse_frontmatter(frontmatter);
    let id = scalar(&frontmatter, "id")
        .ok_or_else(|| CliError::new(format!("Agent guide requires id and title: {file_name}")))?;
    let title = scalar(&frontmatter, "title")
        .ok_or_else(|| CliError::new(format!("Agent guide requires id and title: {file_name}")))?;
    Ok(AgentGuide {
        metadata: AgentGuideMetadata {
            applies_to: list(&frontmatter, "appliesTo"),
            id,
            load_when: list(&frontmatter, "loadWhen"),
            requires_capabilities: list(&frontmatter, "requiresCapabilities"),
            source: scalar(&frontmatter, "source").unwrap_or_else(|| "builtin".to_owned()),
            task_types: list(&frontmatter, "taskTypes"),
            title,
            tokens: scalar(&frontmatter, "tokens").unwrap_or_else(|| "medium".to_owned()),
        },
        body: body.trim_start_matches('\n').to_owned(),
    })
}

fn parse_frontmatter(source: &str) -> BTreeMap<String, Vec<String>> {
    let mut result = BTreeMap::new();
    let mut current_key: Option<String> = None;
    for line in source.lines() {
        if let Some(value) = line.trim_start().strip_prefix("- ") {
            if let Some(key) = &current_key {
                result
                    .entry(key.clone())
                    .or_insert_with(Vec::new)
                    .push(value.trim().to_owned());
            }
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let value = value.trim();
            current_key = Some(key.to_owned());
            result.insert(
                key.to_owned(),
                if value.is_empty() {
                    Vec::new()
                } else {
                    vec![value.to_owned()]
                },
            );
        }
    }
    result
}

fn scalar(frontmatter: &BTreeMap<String, Vec<String>>, key: &str) -> Option<String> {
    frontmatter
        .get(key)
        .and_then(|values| values.first())
        .cloned()
}

fn list(frontmatter: &BTreeMap<String, Vec<String>>, key: &str) -> Vec<String> {
    frontmatter.get(key).cloned().unwrap_or_default()
}

fn render_guide_table(guides: &[AgentGuideMetadata]) -> String {
    if guides.is_empty() {
        return "- none".to_owned();
    }
    let rows = guides
        .iter()
        .map(|guide| {
            format!(
                "| `{}` | {} | {} | {} | {} |",
                guide.id,
                guide.source,
                guide.applies_to.join(", "),
                guide.task_types.join(", "),
                guide.load_when.join("; ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("| ID | Source | Applies To | Task Types | Load When |\n| --- | --- | --- | --- | --- |\n{rows}")
}

fn wiki_summary(bootstrap: &ProjectBootstrap) -> Value {
    let state = bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
        .map(|snapshot| &snapshot.state)
        .unwrap_or(&bootstrap.state);
    let tasks = state
        .get("tasks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|task| {
            task.get("status")
                .and_then(Value::as_str)
                .is_some_and(|status| ["queued", "claimed", "running", "failed"].contains(&status))
        })
        .collect::<Vec<_>>();
    let next_task = tasks
        .iter()
        .find(|task| task.get("status").and_then(Value::as_str) == Some("queued"))
        .or_else(|| {
            tasks
                .iter()
                .find(|task| task.get("status").and_then(Value::as_str) == Some("failed"))
        })
        .or_else(|| tasks.first())
        .cloned();
    json!({
        "language": if matches!(state.get("wikiLanguage").and_then(Value::as_str), Some("en" | "zh-CN")) { state.get("wikiLanguage").cloned().unwrap_or(Value::Null) } else { Value::Null },
        "nextTask": next_task,
        "pendingTaskCount": tasks.len(),
    })
}

fn canvas_summary(selection: Option<&crate::selection::SelectionPayload>) -> Value {
    let selected_shapes = selection
        .and_then(|selection| selection.selection.get("selectedShapes"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let selected_ids = selection
        .and_then(|selection| selection.selection.get("selectedShapeIds"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    json!({
        "fallback": selection.and_then(|selection| selection.selection.get("fallback")).and_then(Value::as_str),
        "hasSelectedImageAsset": selection.and_then(|selection| selection.selection.get("assetRef")).and_then(Value::as_str).is_some(),
        "hasSelection": selected_shapes > 0 || selected_ids > 0,
        "selectedShapeCount": if selected_shapes > 0 { selected_shapes } else { selected_ids },
    })
}

fn suggested_commands(bootstrap: &ProjectBootstrap, guides: &[AgentGuideMetadata]) -> Vec<Value> {
    let mut commands = vec![json!({
        "intent": "agent.context.read",
        "command": "openpanels-local agent context --project \"$PWD\"",
    })];
    let wiki = wiki_summary(bootstrap);
    if let Some(task) = wiki.get("nextTask").filter(|value| !value.is_null()) {
        commands.push(json!({
            "intent": "wiki.task.next",
            "command": "openpanels-local wiki tasks next --project \"$PWD\" --format json",
        }));
        if let Some(task_type) = task.get("type").and_then(Value::as_str) {
            if let Some(guide) = guides.iter().find(|guide| {
                guide
                    .task_types
                    .iter()
                    .any(|candidate| candidate == task_type)
            }) {
                if let Some(task_id) = task.get("id").and_then(Value::as_str) {
                    commands.push(json!({
                        "intent": "agent.guide.read",
                        "command": format!("openpanels-local agent guide {} --project \"$PWD\" --task-id {}", guide.id, task_id),
                    }));
                }
            }
        }
    } else if bootstrap.active_panel_kind == PanelKind::Canvas {
        commands.push(json!({
            "intent": "canvas.selection.read",
            "command": "openpanels-local selection --project \"$PWD\" --format json",
        }));
    }
    commands
}

fn format_next_task(task: Option<&Value>) -> String {
    let Some(task) = task.filter(|value| !value.is_null()) else {
        return "none".to_owned();
    };
    format!(
        "{} / {} / {}",
        task.get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        task.get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        task.get("id").and_then(Value::as_str).unwrap_or("unknown")
    )
}

fn render_current_context(
    bootstrap: &ProjectBootstrap,
    selection: Option<&crate::selection::SelectionPayload>,
    task: Option<&Value>,
) -> String {
    let wiki = wiki_summary(bootstrap);
    let selected_shape_count = canvas_summary(selection)["selectedShapeCount"]
        .as_u64()
        .unwrap_or(0);
    let mut lines = vec![
        format!(
            "- project: {} ({})",
            bootstrap.session.title, bootstrap.session.id
        ),
        format!(
            "- active panel: {} ({})",
            bootstrap.active_panel_kind.as_str(),
            bootstrap.panel.title
        ),
        format!(
            "- wiki language: {}",
            wiki["language"].as_str().unwrap_or("not set")
        ),
        format!("- canvas selected shape count: {selected_shape_count}"),
    ];
    if let Some(task) = task {
        lines.push(format!("- task id: {}", task["id"].as_str().unwrap_or("")));
        lines.push(format!(
            "- task type: {}",
            task["type"].as_str().unwrap_or("")
        ));
        lines.push(format!(
            "- task status: {}",
            task["status"].as_str().unwrap_or("")
        ));
        lines.push(format!(
            "- document id: {}",
            task["documentId"].as_str().unwrap_or("none")
        ));
        lines.push(format!(
            "- wiki space id: {}",
            task["wikiSpaceId"].as_str().unwrap_or("none")
        ));
    }
    lines.join("\n")
}

fn render_guide_commands(guide: &AgentGuide, task: Option<&Value>) -> String {
    if let Some(task) = task {
        let task_id = task["id"].as_str().unwrap_or("<task-id>");
        let document_id = task["documentId"].as_str().unwrap_or("<document-id>");
        let wiki_space_id = task["wikiSpaceId"].as_str().unwrap_or("<wiki-space-id>");
        return format!(
            "```bash\nopenpanels-local wiki tasks claim --project \"$PWD\" --task-id {task_id} --format json\nopenpanels-local wiki markdown read --project \"$PWD\" --document-id {document_id} --format json\nopenpanels-local wiki pages write --project \"$PWD\" --wiki-space-id {wiki_space_id} --path <page-path> --file <md-file> --task-id {task_id} --format json\nopenpanels-local wiki tasks complete --project \"$PWD\" --task-id {task_id} --format json\n```"
        );
    }
    if guide
        .metadata
        .applies_to
        .iter()
        .any(|value| value == "canvas")
    {
        return "```bash\nopenpanels-local selection --project \"$PWD\" --format json\nopenpanels-local insert-placeholder --project \"$PWD\" --display-width <w> --display-height <h> --format json\nopenpanels-local insert-image --project \"$PWD\" --image <generated-path> --replace-shape-id <placeholder-shape-id> --format json\n```".to_owned();
    }
    "- No task-specific commands.".to_owned()
}

fn find_wiki_task(bootstrap: &ProjectBootstrap, task_id: &str) -> Option<Value> {
    bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
        .map(|snapshot| &snapshot.state)
        .unwrap_or(&bootstrap.state)
        .get("tasks")?
        .as_array()?
        .iter()
        .find(|task| task.get("id").and_then(Value::as_str) == Some(task_id))
        .cloned()
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
