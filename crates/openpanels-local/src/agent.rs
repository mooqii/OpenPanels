use crate::control::{read_project_bootstrap, BootstrapRequest};
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
    let bootstrap = read_project_bootstrap(paths, request)?;
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
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let selection = read_selection(paths, None, false).ok();
    let markdown = render_agent_guide(&guide, &bootstrap, selection.as_ref(), task_id)?;
    Ok(AgentGuideReadPayload {
        guide: guide.metadata,
        markdown,
    })
}

pub fn capabilities() -> Vec<Value> {
    vec![
        capability(
            "agent.context.read",
            "Read current agent context",
            "openpanels-local agent context",
            "current_user_project",
            false,
            vec![],
        ),
        capability(
            "agent.guides.list",
            "List loadable agent guides",
            "openpanels-local agent guides",
            "none",
            false,
            vec![],
        ),
        capability(
            "agent.bridge.run",
            "Run the task bridge",
            "openpanels-local agent bridge",
            "current_user_project",
            false,
            vec![
                arg("command", "--command", "string", false),
                arg("once", "--once", "bool", false),
                arg("queue", "--queue", "string", false),
                arg("timeoutMs", "--timeout-ms", "number", false),
            ],
        ),
        capability(
            "agent.bridge.status",
            "Read task bridge status",
            "openpanels-local agent bridge status",
            "current_user_project",
            false,
            vec![],
        ),
        capability(
            "agent.guide.read",
            "Read one full agent guide",
            "openpanels-local agent guide <guide-id>",
            "current_user_project",
            false,
            vec![arg("taskId", "--task-id", "string", false)],
        ),
        capability(
            "project.current.read",
            "Read the current user-visible project",
            "openpanels-local project current",
            "current_user_project",
            false,
            vec![],
        ),
        capability(
            "project.list",
            "List available projects",
            "openpanels-local project list",
            "current_studio_or_storage",
            false,
            vec![],
        ),
        capability(
            "project.create",
            "Create a project explicitly",
            "openpanels-local project create",
            "storage",
            true,
            vec![arg("title", "--title", "string", false)],
        ),
        capability(
            "panel.list",
            "List panels in the current project",
            "openpanels-local panel list",
            "current_user_project",
            false,
            vec![],
        ),
        capability(
            "panel.current.read",
            "Read the current panel",
            "openpanels-local panel current",
            "current_user_project.current_panel",
            false,
            vec![],
        ),
        capability(
            "panel.switch",
            "Switch the current panel by kind",
            "openpanels-local panel switch",
            "current_user_project",
            false,
            vec![enum_arg(
                "kind",
                "--kind",
                true,
                &["wiki", "canvas", "image", "diff", "preview", "files"],
                None,
            )],
        ),
        capability(
            "canvas.state.read",
            "Read current canvas state",
            "openpanels-local canvas state",
            "current_user_project.current_canvas",
            false,
            vec![],
        ),
        capability(
            "canvas.selection.read",
            "Read current canvas selection",
            "openpanels-local canvas selection read",
            "current_user_project.current_canvas",
            false,
            vec![],
        ),
        capability(
            "canvas.selection.asset.read",
            "Export selected canvas pixels to a file",
            "openpanels-local canvas selection export",
            "current_user_project.current_canvas",
            false,
            vec![
                arg("output", "--output", "path", true),
                arg("allowFallback", "--allow-fallback", "bool", false),
            ],
        ),
        capability(
            "canvas.placeholder.create",
            "Insert a generation placeholder",
            "openpanels-local canvas placeholder create",
            "current_user_project.current_canvas",
            false,
            vec![
                arg("displayWidth", "--display-width", "number", true),
                arg("displayHeight", "--display-height", "number", true),
                arg("text", "--text", "string", false),
            ],
        ),
        capability(
            "canvas.image.insert",
            "Insert a local image into the current canvas",
            "openpanels-local canvas image insert",
            "current_user_project.current_canvas",
            false,
            vec![
                arg("image", "--image", "path", true),
                enum_arg(
                    "placement",
                    "--placement",
                    false,
                    &["auto", "right", "below", "left"],
                    Some("auto"),
                ),
                arg("metadataFile", "--metadata-file", "path", false),
                arg("replaceShapeId", "--replace-shape-id", "string", false),
            ],
        ),
        capability(
            "tasks.list",
            "List project tasks across panels",
            "openpanels-local tasks list",
            "current_user_project",
            false,
            vec![
                arg("pending", "--pending", "bool", false),
                arg("queue", "--queue", "string", false),
                arg("status", "--status", "string", false),
            ],
        ),
        capability(
            "tasks.next",
            "Read the next pending project task",
            "openpanels-local tasks next",
            "current_user_project",
            false,
            vec![
                arg("queue", "--queue", "string", false),
                arg("status", "--status", "string", false),
            ],
        ),
        capability(
            "tasks.inspect",
            "Read one project task by id",
            "openpanels-local tasks inspect",
            "current_user_project",
            false,
            vec![arg("taskId", "--task-id", "string", true)],
        ),
        capability(
            "wiki.context.read",
            "Read current wiki context",
            "openpanels-local wiki context",
            "current_user_project.current_wiki",
            false,
            vec![],
        ),
        capability(
            "wiki.document.list",
            "List raw wiki documents",
            "openpanels-local wiki documents list",
            "current_user_project.current_wiki",
            false,
            vec![],
        ),
        capability(
            "wiki.document.add",
            "Add a raw wiki document",
            "openpanels-local wiki documents add",
            "current_user_project.current_wiki",
            false,
            vec![
                arg("file", "--file", "path", true),
                arg("title", "--title", "string", false),
            ],
        ),
        capability(
            "wiki.document.createMarkdown",
            "Create a markdown raw document",
            "openpanels-local wiki documents create-markdown",
            "current_user_project.current_wiki",
            false,
            vec![
                arg("title", "--title", "string", true),
                arg("file", "--file", "path", false),
                arg("content", "--content", "string", false),
            ],
        ),
        capability(
            "wiki.markdown.read",
            "Read markdown for a raw document",
            "openpanels-local wiki markdown read",
            "current_user_project.current_wiki",
            false,
            vec![arg("documentId", "--document-id", "string", true)],
        ),
        capability(
            "wiki.markdown.write",
            "Write markdown for a raw document",
            "openpanels-local wiki markdown write",
            "current_user_project.current_wiki",
            false,
            vec![
                arg("documentId", "--document-id", "string", true),
                arg("file", "--file", "path", true),
                arg("taskId", "--task-id", "string", false),
            ],
        ),
        capability(
            "wiki.task.list",
            "List wiki tasks",
            "openpanels-local wiki tasks list",
            "current_user_project.current_wiki",
            false,
            vec![],
        ),
        capability(
            "wiki.task.next",
            "Read the next wiki task",
            "openpanels-local wiki tasks next",
            "current_user_project.current_wiki",
            false,
            vec![],
        ),
        capability(
            "wiki.task.claim",
            "Claim a wiki task",
            "openpanels-local wiki tasks claim",
            "current_user_project.current_wiki",
            false,
            vec![arg("taskId", "--task-id", "string", true)],
        ),
        capability(
            "wiki.task.complete",
            "Complete a wiki task",
            "openpanels-local wiki tasks complete",
            "current_user_project.current_wiki",
            false,
            vec![arg("taskId", "--task-id", "string", true)],
        ),
        capability(
            "wiki.task.fail",
            "Fail a wiki task",
            "openpanels-local wiki tasks fail",
            "current_user_project.current_wiki",
            false,
            vec![
                arg("taskId", "--task-id", "string", true),
                arg("message", "--message", "string", true),
            ],
        ),
        capability(
            "wiki.space.list",
            "List wiki spaces",
            "openpanels-local wiki spaces list",
            "current_user_project.current_wiki",
            false,
            vec![],
        ),
        capability(
            "wiki.space.switch",
            "Switch active wiki space",
            "openpanels-local wiki spaces switch",
            "current_user_project.current_wiki",
            false,
            vec![arg("wikiSpaceId", "--wiki-space-id", "string", true)],
        ),
        capability(
            "wiki.page.list",
            "List pages in a wiki space",
            "openpanels-local wiki pages list",
            "current_user_project.current_wiki",
            false,
            vec![arg("wikiSpaceId", "--wiki-space-id", "string", true)],
        ),
        capability(
            "wiki.page.read",
            "Read one wiki page",
            "openpanels-local wiki pages read",
            "current_user_project.current_wiki",
            false,
            vec![
                arg("wikiSpaceId", "--wiki-space-id", "string", true),
                arg("path", "--path", "path", true),
            ],
        ),
        capability(
            "wiki.page.write",
            "Write one wiki page",
            "openpanels-local wiki pages write",
            "current_user_project.current_wiki",
            false,
            vec![
                arg("wikiSpaceId", "--wiki-space-id", "string", true),
                arg("path", "--path", "path", true),
                arg("file", "--file", "path", true),
                arg("taskId", "--task-id", "string", false),
            ],
        ),
        capability(
            "studio.start",
            "Start or reuse the local Studio",
            "openpanels-local studio start",
            "project_directory",
            false,
            vec![],
        ),
        capability(
            "studio.status",
            "Read local Studio process status",
            "openpanels-local studio status",
            "project_directory",
            false,
            vec![],
        ),
        capability(
            "studio.stop",
            "Stop the local Studio binding",
            "openpanels-local studio stop",
            "project_directory",
            false,
            vec![],
        ),
    ]
}

fn capability(
    intent: &str,
    title: &str,
    command: &str,
    target: &str,
    creates_project: bool,
    args: Vec<Value>,
) -> Value {
    json!({
        "intent": intent,
        "title": title,
        "command": command,
        "target": target,
        "createsProject": creates_project,
        "args": args,
        "output": "json",
    })
}

fn arg(name: &str, flag: &str, value_type: &str, required: bool) -> Value {
    json!({
        "name": name,
        "flag": flag,
        "type": value_type,
        "required": required,
    })
}

fn enum_arg(
    name: &str,
    flag: &str,
    required: bool,
    values: &[&str],
    default: Option<&str>,
) -> Value {
    let mut value = arg(name, flag, "enum", required);
    if let Some(object) = value.as_object_mut() {
        object.insert("values".to_owned(), json!(values));
        if let Some(default) = default {
            object.insert("default".to_owned(), json!(default));
        }
    }
    value
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
    let tasks = project_tasks_summary(bootstrap);
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
            "tasks": tasks,
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
    let tasks = project_tasks_summary(bootstrap);
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
                capability["title"].as_str().unwrap_or(""),
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
        "# OpenPanels Agent Context\n\nProtocol version: 1\nCLI version: {cli_version}\nProject: {} ({})\nActive panel: {} ({})\n\n## Panels\n\n{panels}\n\n## State\n\n### Tasks\n\n- total task count: {}\n- pending task count: {}\n- next task: {}\n\n### Wiki\n\n- language: {}\n- pending task count: {}\n- next task: {}\n\n### Canvas\n\n- has selection: {}\n- selected shape count: {}\n- selected image asset: {}\n- fallback: {}\n\n## Capabilities\n\n{caps}\n\n## Suggested Next Commands\n\n{suggested}\n\n## Available Guides\n\n{}\n",
        bootstrap.session.title,
        bootstrap.session.id,
        bootstrap.active_panel_kind.as_str(),
        bootstrap.panel.title,
        tasks["totalTaskCount"].as_u64().unwrap_or(0),
        tasks["pendingTaskCount"].as_u64().unwrap_or(0),
        format_next_task(tasks.get("nextTask")),
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
    let normalized_source;
    let source = if source.contains("\r\n") {
        normalized_source = source.replace("\r\n", "\n");
        normalized_source.as_str()
    } else {
        source
    };
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

fn project_tasks_summary(bootstrap: &ProjectBootstrap) -> Value {
    let next_task = next_project_task(bootstrap).cloned();
    json!({
        "nextTask": next_task,
        "pendingTaskCount": bootstrap.pending_task_count,
        "totalTaskCount": bootstrap.tasks.len(),
    })
}

fn canvas_summary(selection: Option<&crate::selection::SelectionPayload>) -> Value {
    let is_explicit_selection = selection
        .and_then(|selection| selection.selection.get("isExplicitSelection"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
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
        "hasSelectedImageAsset": is_explicit_selection && selection.and_then(|selection| selection.selection.get("assetRef")).and_then(Value::as_str).is_some(),
        "hasSelection": is_explicit_selection && (selected_shapes > 0 || selected_ids > 0),
        "isExplicitSelection": is_explicit_selection,
        "selectedShapeCount": if is_explicit_selection { if selected_shapes > 0 { selected_shapes } else { selected_ids } } else { 0 },
    })
}

fn suggested_commands(bootstrap: &ProjectBootstrap, guides: &[AgentGuideMetadata]) -> Vec<Value> {
    let mut commands = vec![json!({
        "intent": "agent.context.read",
        "command": "openpanels-local agent context --format json",
    })];
    if let Some(task) = next_project_task(bootstrap) {
        commands.push(json!({
            "intent": "tasks.next",
            "command": "openpanels-local tasks next --format json",
        }));
        if let Some(task_id) = task.get("id").and_then(Value::as_str) {
            commands.push(json!({
                "intent": "tasks.inspect",
                "command": format!("openpanels-local tasks inspect --task-id {} --format json", shell_quote(task_id)),
            }));
        }
    }
    let wiki = wiki_summary(bootstrap);
    if let Some(task) = wiki.get("nextTask").filter(|value| !value.is_null()) {
        commands.push(json!({
            "intent": "wiki.task.next",
            "command": "openpanels-local wiki tasks next --format json",
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
                        "command": format!("openpanels-local agent guide {} --task-id {} --format json", guide.id, task_id),
                    }));
                }
            }
        }
    } else if bootstrap.active_panel_kind == PanelKind::Canvas {
        commands.push(json!({
            "intent": "canvas.selection.read",
            "command": "openpanels-local canvas selection read --format json",
        }));
    }
    commands
}

fn next_project_task(bootstrap: &ProjectBootstrap) -> Option<&Value> {
    bootstrap
        .tasks
        .iter()
        .filter(|task| task.get("ready").and_then(Value::as_bool).unwrap_or(false))
        .find(|task| task.get("status").and_then(Value::as_str) == Some("queued"))
        .or_else(|| {
            bootstrap
                .tasks
                .iter()
                .filter(|task| task.get("ready").and_then(Value::as_bool).unwrap_or(false))
                .find(|task| task.get("status").and_then(Value::as_str) == Some("failed"))
        })
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

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | ':'))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
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
            "```bash\nopenpanels-local wiki tasks claim --task-id {task_id} --format json\nopenpanels-local wiki markdown read --document-id {document_id} --format json\nopenpanels-local wiki pages write --wiki-space-id {wiki_space_id} --path <page-path> --file <md-file> --task-id {task_id} --format json\nopenpanels-local wiki tasks complete --task-id {task_id} --format json\n```"
        );
    }
    if guide
        .metadata
        .applies_to
        .iter()
        .any(|value| value == "canvas")
    {
        return "```bash\nopenpanels-local canvas selection read --format json\nopenpanels-local canvas placeholder create --display-width <w> --display-height <h> --format json\nopenpanels-local canvas image insert --image <generated-path> --replace-shape-id <placeholder-shape-id> --format json\n```".to_owned();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_guide_accepts_crlf_frontmatter() {
        let guide = parse_guide(
            "---\r\nid: test.guide\r\ntitle: Test Guide\r\n---\r\n\r\nBody.\r\n",
            "test.md",
        )
        .expect("guide");

        assert_eq!(guide.metadata.id, "test.guide");
        assert_eq!(guide.metadata.title, "Test Guide");
        assert_eq!(guide.body, "Body.\n");
    }
}
