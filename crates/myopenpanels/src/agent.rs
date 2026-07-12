use crate::control::{read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::MyOpenPanelsPaths;
use crate::selection::read_selection;
use crate::types::{PanelKind, ProjectBootstrap};
use crate::wiki::{read_agent_selection, selected_agent_skill_id, WIKI_PANEL_SKILL_ID};
use include_dir::{include_dir, Dir};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

static AGENT_SKILLS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../agent-resources/skills");
pub const CANVAS_PANEL_SKILL_ID: &str = "canvas-panel";
pub const TASK_QUEUE_SKILL_ID: &str = "task-queue";
pub const AGENT_GUIDANCE_PROTOCOL_VERSION: u32 = 5;
pub const MAX_BOOTSTRAP_ENVELOPE_BYTES: usize = 8192;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillMetadata {
    pub applies_to: Vec<String>,
    pub description: String,
    pub id: String,
    pub load_when: Vec<String>,
    pub requires_capabilities: Vec<String>,
    pub source: String,
    pub task_types: Vec<String>,
    pub title: String,
    pub tokens: String,
}

#[derive(Debug, Clone)]
pub struct AgentSkill {
    pub metadata: AgentSkillMetadata,
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillListing {
    pub skill: AgentSkillMetadata,
    pub local_dir: String,
    pub local_path: String,
    pub source: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillReadPayload {
    pub skill: AgentSkillMetadata,
    pub local_dir: String,
    pub local_path: String,
    pub markdown: String,
    pub next_actions: Vec<Value>,
    pub next_required_action: Value,
}

pub fn agent_bootstrap(paths: &MyOpenPanelsPaths, cli_version: &str) -> Result<Value, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let storage = crate::storage::Storage::open(paths)?;
    if let Some(update) =
        crate::agent_control::pending_entry_skill_update_with_storage(paths, &storage, cli_version)?
    {
        return Ok(entry_skill_update_bootstrap(
            paths,
            &bootstrap,
            cli_version,
            &update,
        ));
    }
    sync_builtin_agent_skills(paths)?;
    let skills = load_agent_skills()?;
    let operations = storage.list_agent_operations(Some(&paths.context_id), Some("active"))?;
    let focus_revision = crate::control::read_focus_revision(paths)?;
    let focus = json!({
        "focusRevision": focus_revision,
        "projectId": bootstrap.project.id,
        "panelId": bootstrap.panel.id,
        "panelKind": bootstrap.panel.kind,
    });
    let selection = crate::panel::selection_for_bootstrap(paths, &bootstrap, focus.clone()).ok();
    let mut context_truncated = false;
    let context = bounded_json(
        crate::panel::context_for_bootstrap(&bootstrap),
        0,
        &mut context_truncated,
    );
    let available_panel_kinds = bootstrap
        .panels
        .iter()
        .map(|snapshot| snapshot.panel.kind.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let selection_summary = selection
        .as_ref()
        .map(|selection| {
            let mut truncated = false;
            json!({
                "supported": selection.supported,
                "selectionKind": selection.selection_kind,
                "isExplicit": selection.is_explicit,
                "summary": bounded_json(selection.summary.clone(), 0, &mut truncated),
            })
        })
        .unwrap_or_else(|| {
            json!({
                "supported": false,
                "selectionKind": null,
                "isExplicit": false,
                "summary": { "itemCount": 0 },
            })
        });
    let recommended_scopes = [
        "panel",
        bootstrap.active_panel_kind.as_str(),
        "task",
        "operation",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>()
    .into_iter()
    .collect::<Vec<_>>();
    let detailed_selection = read_selection(paths, None, false).ok();
    let wiki_selection = read_agent_selection(paths).ok();
    let mut required_skills = Vec::new();
    let mut required_capabilities = BTreeSet::new();
    if let Some(skill_id) = panel_skill_id(bootstrap.active_panel_kind) {
        let skill = required_agent_skill(&skills, skill_id)?;
        required_capabilities.extend(skill.metadata.requires_capabilities.iter().cloned());
        required_skills.push(write_required_skill_loader(
            paths,
            skill,
            &bootstrap,
            detailed_selection.as_ref(),
            wiki_selection.as_ref(),
            None,
        )?);
    }
    if let Some((task_id, skill_id)) = next_wiki_task_authoring_skill(&bootstrap) {
        let skill = required_agent_skill(&skills, skill_id)?;
        required_capabilities.extend(skill.metadata.requires_capabilities.iter().cloned());
        required_skills.push(write_required_skill_loader(
            paths,
            skill,
            &bootstrap,
            detailed_selection.as_ref(),
            wiki_selection.as_ref(),
            Some(task_id),
        )?);
    }
    let mut next_actions = Vec::new();
    next_actions.extend(required_capabilities.into_iter().map(|intent| {
        let mut action = command_action(
            "agent.capability.read",
            vec![
                "--intent".to_owned(),
                intent,
                "--format".to_owned(),
                "json".to_owned(),
            ],
        );
        action["loadWhen"] = json!("The user request needs this capability from a required Skill.");
        action
    }));
    if !bootstrap.tasks.is_empty() {
        let mut action = project_command_action(
            paths,
            "agent.skill.read",
            vec![
                "--skill-id".to_owned(),
                TASK_QUEUE_SKILL_ID.to_owned(),
                "--format".to_owned(),
                "json".to_owned(),
            ],
        );
        action["loadWhen"] = json!("The user request handles work from the project Task queue.");
        action["skillId"] = json!(TASK_QUEUE_SKILL_ID);
        next_actions.push(action);
    }
    next_actions.extend(
        recommended_scopes
            .iter()
            .map(|scope| {
                let mut action = command_action(
                    "agent.capability.list",
                    vec![
                        "--scope".to_owned(),
                        (*scope).to_owned(),
                        "--format".to_owned(),
                        "json".to_owned(),
                    ],
                );
                action["loadWhen"] = json!(format!("The user request matches the {scope} scope."));
                action
            })
            .collect::<Vec<_>>(),
    );
    let mut capability_index_action = command_action(
        "agent.capability.list",
        vec!["--format".to_owned(), "json".to_owned()],
    );
    capability_index_action["loadWhen"] = json!("No recommended scope matches the user request.");
    next_actions.push(capability_index_action);
    if let Some(task_id) = next_task_id(&bootstrap) {
        let mut action = project_command_action(
            paths,
            "task.read",
            vec![
                "--task-id".to_owned(),
                task_id.to_owned(),
                "--format".to_owned(),
                "json".to_owned(),
            ],
        );
        action["loadWhen"] = json!("The user request should continue the next ready Task.");
        next_actions.push(action);
    }
    next_actions.extend(operations.iter().take(3).filter_map(|operation| {
        let operation_id = operation.get("id").and_then(Value::as_str)?;
        let mut action = project_command_action(
            paths,
            "operation.read",
            vec![
                "--operation-id".to_owned(),
                operation_id.to_owned(),
                "--format".to_owned(),
                "json".to_owned(),
            ],
        );
        action["loadWhen"] = json!("The user request continues this active Operation.");
        Some(action)
    }));
    Ok(json!({
        "protocolVersion": AGENT_GUIDANCE_PROTOCOL_VERSION,
        "commandCatalogVersion": crate::cli::registry::COMMAND_CATALOG_VERSION,
        "cliVersion": cli_version,
        "bootstrapBudget": {
            "maxBytes": MAX_BOOTSTRAP_ENVELOPE_BYTES,
            "unit": "utf8",
        },
        "focus": {
            "focusRevision": focus_revision,
            "projectId": bootstrap.project.id,
            "panelId": bootstrap.panel.id,
            "panelKind": bootstrap.panel.kind,
            "availablePanelKinds": available_panel_kinds,
        },
        "panel": {
            "context": context,
            "contextTruncated": context_truncated,
            "selection": selection_summary,
        },
        "tasks": compact_task_summary(&bootstrap),
        "operations": compact_operation_summary(&operations),
        "discovery": {
            "recommendedScopes": recommended_scopes,
        },
        "nextActions": next_actions,
        "nextRequiredAction": {
            "intent": "complete-required-steps",
            "required": true,
            "steps": [{
                "intent": "agent-host.skill.read-required",
                "executor": "agent-host",
                "required": true,
                "skills": required_skills,
                "instruction": "For each Skill in order, read contextPath first and localPath second. Complete every read before evaluating or executing nextActions.",
            }],
            "instruction": "Complete every step sequentially before evaluating or executing nextActions.",
        },
    }))
}

fn entry_skill_update_bootstrap(
    paths: &MyOpenPanelsPaths,
    bootstrap: &ProjectBootstrap,
    cli_version: &str,
    update: &crate::agent_control::EntrySkillUpdate,
) -> Value {
    let host_action = json!({
        "intent": "agent-host.skill.update-required",
        "executor": "agent-host",
        "required": true,
        "instruction": "Compare the currently loaded MyOpenPanels Entry Skill metadata with skill.version. If it is missing or older, install skill.source with the Agent host's Skill installer. Do not acknowledge until the installed version is current.",
        "skill": {
            "id": update.id,
            "version": update.required_version,
            "source": update.source,
        },
    });
    let mut acknowledge_action = project_command_action(
        paths,
        "agent.entry-skill.acknowledge",
        vec![
            "--event-id".to_owned(),
            update.event_id.clone(),
            "--installed-version".to_owned(),
            update.required_version.clone(),
            "--format".to_owned(),
            "json".to_owned(),
        ],
    );
    acknowledge_action["required"] = json!(true);
    json!({
        "protocolVersion": AGENT_GUIDANCE_PROTOCOL_VERSION,
        "commandCatalogVersion": crate::cli::registry::COMMAND_CATALOG_VERSION,
        "cliVersion": cli_version,
        "bootstrapBudget": {
            "maxBytes": MAX_BOOTSTRAP_ENVELOPE_BYTES,
            "unit": "utf8",
        },
        "entrySkillUpdate": update,
        "focus": {
            "projectId": bootstrap.project.id,
            "panelId": bootstrap.panel.id,
            "panelKind": bootstrap.panel.kind,
        },
        "nextActions": [],
        "nextRequiredAction": {
            "intent": "complete-required-steps",
            "required": true,
            "reason": "update-entry-skill",
            "steps": [host_action, acknowledge_action],
            "instruction": "Complete every step sequentially, then rerun Agent Bootstrap. Do not read or operate on a panel before the refreshed Bootstrap completes.",
        },
    })
}

fn panel_skill_id(kind: PanelKind) -> Option<&'static str> {
    crate::panel::skill_id(kind)
}

fn next_task_id(bootstrap: &ProjectBootstrap) -> Option<&str> {
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
        .and_then(|task| task.get("id"))
        .and_then(Value::as_str)
}

fn next_wiki_task_authoring_skill(bootstrap: &ProjectBootstrap) -> Option<(&str, &str)> {
    if bootstrap.active_panel_kind != PanelKind::Wiki {
        return None;
    }
    let task = next_project_task(bootstrap)?;
    if task.get("queue").and_then(Value::as_str) != Some("wiki") {
        return None;
    }
    let task_id = task.get("id").and_then(Value::as_str)?;
    let skill_id = task
        .get("agentSkillId")
        .or_else(|| {
            task.get("input")
                .and_then(|input| input.get("agentSkillId"))
        })
        .or_else(|| {
            task.get("source")
                .and_then(|source| source.get("agentSkillId"))
        })
        .and_then(Value::as_str)
        .or_else(|| {
            bootstrap
                .panels
                .iter()
                .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
                .map(|snapshot| selected_agent_skill_id(&snapshot.state))
        })?;
    (skill_id != WIKI_PANEL_SKILL_ID).then_some((task_id, skill_id))
}

fn required_agent_skill<'a>(
    skills: &'a [AgentSkill],
    skill_id: &str,
) -> Result<&'a AgentSkill, CliError> {
    skills
        .iter()
        .find(|skill| skill.metadata.id == skill_id)
        .ok_or_else(|| {
            CliError::with_code(
                "required_agent_skill_not_found",
                format!("Required MyOpenPanels Agent Skill not found: {skill_id}"),
            )
        })
}

fn write_required_skill_loader(
    paths: &MyOpenPanelsPaths,
    skill: &AgentSkill,
    bootstrap: &ProjectBootstrap,
    selection: Option<&crate::selection::SelectionPayload>,
    wiki_selection: Option<&Value>,
    task_id: Option<&str>,
) -> Result<Value, CliError> {
    let (local_dir, local_path) = agent_skill_local_paths(paths, &skill.metadata.id);
    let task = task_id.and_then(|id| find_wiki_task(bootstrap, id));
    let selected_wiki_skill_id = task
        .as_ref()
        .and_then(|task| task.get("agentSkillId"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            selected_agent_skill_id(
                bootstrap
                    .panels
                    .iter()
                    .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
                    .map(|snapshot| &snapshot.state)
                    .unwrap_or(&bootstrap.state),
            )
        });
    let markdown = render_agent_skill(
        skill,
        bootstrap,
        selection,
        wiki_selection,
        task_id,
        &local_dir,
        &local_path,
        selected_wiki_skill_id,
    )?;
    let loader_dir = paths.context_dir.join("agent-skill-loaders");
    fs::create_dir_all(&loader_dir).map_err(to_cli_error)?;
    let context_path = loader_dir.join(format!(
        "{}.md",
        crate::paths::sanitize_path_part(&skill.metadata.id)
    ));
    fs::write(&context_path, format!("{markdown}\n")).map_err(to_cli_error)?;
    Ok(json!({
        "id": skill.metadata.id,
        "contextPath": context_path.display().to_string(),
        "localPath": local_path.display().to_string(),
        "taskId": task_id,
    }))
}

fn command_action(intent: &str, args: Vec<String>) -> Value {
    crate::cli::registry::command_action(crate::cli::registry::CommandId::registered(intent), args)
        .unwrap_or_else(|| panic!("missing Command Registry action for {intent}"))
}

fn project_command_action(paths: &MyOpenPanelsPaths, intent: &str, args: Vec<String>) -> Value {
    let mut contextual_args = vec![
        "--project-dir".to_owned(),
        paths.project_dir.display().to_string(),
    ];
    contextual_args.extend(args);
    command_action(intent, contextual_args)
}

fn compact_task_summary(bootstrap: &ProjectBootstrap) -> Value {
    let ready_count = bootstrap
        .tasks
        .iter()
        .filter(|task| task.get("ready").and_then(Value::as_bool).unwrap_or(false))
        .count();
    let running_count = bootstrap
        .tasks
        .iter()
        .filter(|task| {
            matches!(
                task.get("status").and_then(Value::as_str),
                Some("reserved" | "running" | "claimed" | "converting" | "indexing")
            )
        })
        .count();
    let next = next_project_task(bootstrap)
        .and_then(|task| {
            let task_id = task.get("id").and_then(Value::as_str)?;
            Some(json!({
                "taskId": task_id,
                "queue": task.get("queue").cloned().unwrap_or(Value::Null),
                "type": task.get("type").cloned().unwrap_or(Value::Null),
                "capability": task.get("capability").cloned().unwrap_or(Value::Null),
                "readCommand": format!("myopenpanels task read --task-id {} --format json", shell_quote(task_id)),
            }))
        })
        .unwrap_or(Value::Null);
    json!({
        "pendingCount": bootstrap.pending_task_count,
        "readyCount": ready_count,
        "runningCount": running_count,
        "next": next,
    })
}

fn compact_operation_summary(operations: &[Value]) -> Value {
    const MAX_ITEMS: usize = 3;
    let items = operations
        .iter()
        .take(MAX_ITEMS)
        .filter_map(|operation| {
            let operation_id = operation.get("id").and_then(Value::as_str)?;
            Some(json!({
                "operationId": operation_id,
                "intent": operation.get("intent").cloned().unwrap_or(Value::Null),
                "panelKind": operation.get("panelKind").cloned().unwrap_or(Value::Null),
                "status": operation.get("status").cloned().unwrap_or(Value::Null),
                "readCommand": format!("myopenpanels operation read --operation-id {} --format json", shell_quote(operation_id)),
            }))
        })
        .collect::<Vec<_>>();
    json!({
        "activeCount": operations.len(),
        "items": items,
        "truncated": operations.len() > MAX_ITEMS,
    })
}

fn bounded_json(value: Value, depth: usize, truncated: &mut bool) -> Value {
    const MAX_DEPTH: usize = 4;
    const MAX_STRING_BYTES: usize = 256;
    const MAX_ARRAY_ITEMS: usize = 16;
    const MAX_OBJECT_FIELDS: usize = 32;

    match value {
        Value::String(value) => {
            if value.len() <= MAX_STRING_BYTES {
                return Value::String(value);
            }
            *truncated = true;
            let mut end = MAX_STRING_BYTES.saturating_sub(3).min(value.len());
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            Value::String(format!("{}...", &value[..end]))
        }
        Value::Array(values) => {
            if depth >= MAX_DEPTH {
                *truncated = true;
                return Value::Null;
            }
            if values.len() > MAX_ARRAY_ITEMS {
                *truncated = true;
            }
            Value::Array(
                values
                    .into_iter()
                    .take(MAX_ARRAY_ITEMS)
                    .map(|value| bounded_json(value, depth + 1, truncated))
                    .collect(),
            )
        }
        Value::Object(values) => {
            if depth >= MAX_DEPTH {
                *truncated = true;
                return Value::Null;
            }
            if values.len() > MAX_OBJECT_FIELDS {
                *truncated = true;
            }
            Value::Object(
                values
                    .into_iter()
                    .take(MAX_OBJECT_FIELDS)
                    .map(|(key, value)| (key, bounded_json(value, depth + 1, truncated)))
                    .collect(),
            )
        }
        value => value,
    }
}

pub fn sync_builtin_agent_skills(paths: &MyOpenPanelsPaths) -> Result<(), CliError> {
    let skills_dir = paths.storage_dir.join("skills");
    fs::create_dir_all(&skills_dir).map_err(to_cli_error)?;
    for (skill, skill_dir) in load_agent_skill_dirs()? {
        let local_dir = skills_dir.join(&skill.metadata.id);
        if local_dir.exists() {
            fs::remove_dir_all(&local_dir).map_err(to_cli_error)?;
        }
        fs::create_dir_all(&local_dir).map_err(to_cli_error)?;
        extract_embedded_dir_contents(skill_dir, skill_dir.path(), &local_dir)?;
    }
    Ok(())
}

pub fn list_agent_skills(paths: &MyOpenPanelsPaths) -> Result<Vec<AgentSkillListing>, CliError> {
    sync_builtin_agent_skills(paths)?;
    Ok(load_agent_skills()?
        .into_iter()
        .map(|skill| agent_skill_listing(paths, skill.metadata))
        .collect())
}

pub fn list_agent_skill_summaries(
    panel_kind: Option<&str>,
    task_type: Option<&str>,
) -> Result<Vec<Value>, CliError> {
    Ok(load_agent_skills()?
        .into_iter()
        .filter(|skill| metadata_matches(&skill.metadata.applies_to, &skill.metadata.task_types, panel_kind, task_type))
        .map(|skill| {
            let metadata = skill.metadata;
            json!({
                "id": metadata.id,
                "title": metadata.title,
                "description": metadata.description,
                "appliesTo": metadata.applies_to,
                "taskTypes": metadata.task_types,
                "loadWhen": metadata.load_when,
                "readCommand": format!("myopenpanels agent skill read --skill-id {} --format json", shell_quote(&metadata.id)),
            })
        })
        .collect())
}

fn metadata_matches(
    applies_to: &[String],
    task_types: &[String],
    panel_kind: Option<&str>,
    task_type: Option<&str>,
) -> bool {
    panel_kind.is_none_or(|kind| {
        applies_to
            .iter()
            .any(|candidate| candidate == kind || candidate == "any")
    }) && task_type.is_none_or(|kind| task_types.iter().any(|candidate| candidate == kind))
}

pub fn read_agent_skill(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
    task_id: Option<&str>,
) -> Result<AgentSkillReadPayload, CliError> {
    sync_builtin_agent_skills(paths)?;
    let skill = load_agent_skills()?
        .into_iter()
        .find(|skill| skill.metadata.id == skill_id)
        .ok_or_else(|| CliError::new(format!("MyOpenPanels agent skill not found: {skill_id}")))?;
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let selection = read_selection(paths, None, false).ok();
    let wiki_selection = read_agent_selection(paths).ok();
    let (local_dir, local_path) = agent_skill_local_paths(paths, &skill.metadata.id);
    let task = task_id.and_then(|id| find_wiki_task(&bootstrap, id));
    let selected_wiki_skill_id = task
        .as_ref()
        .and_then(|value| value.get("agentSkillId"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            selected_agent_skill_id(
                bootstrap
                    .panels
                    .iter()
                    .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
                    .map(|snapshot| &snapshot.state)
                    .unwrap_or(&bootstrap.state),
            )
        });
    let markdown = render_agent_skill(
        &skill,
        &bootstrap,
        selection.as_ref(),
        wiki_selection.as_ref(),
        task_id,
        &local_dir,
        &local_path,
        selected_wiki_skill_id,
    )?;
    Ok(AgentSkillReadPayload {
        next_actions: capability_read_actions(&skill.metadata.requires_capabilities),
        next_required_action: json!({
            "intent": "read-skill-file",
            "executor": "agent-host",
            "required": true,
            "localPath": local_path.display().to_string(),
            "instruction": "Read SKILL.md directly from localPath before performing panel work. After reading it, load an entry from nextActions only when the Skill requires that capability.",
        }),
        skill: skill.metadata,
        local_dir: local_dir.display().to_string(),
        local_path: local_path.display().to_string(),
        markdown,
    })
}

fn capability_read_actions(intents: &[String]) -> Vec<Value> {
    intents
        .iter()
        .map(|intent| {
            let mut action = command_action(
                "agent.capability.read",
                vec![
                    "--intent".to_owned(),
                    intent.clone(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            );
            action["loadWhen"] = json!("This Skill requires the referenced capability.");
            action
        })
        .collect()
}

pub fn capabilities() -> Vec<Value> {
    crate::cli::registry::capabilities()
}

pub fn render_agent_skills_markdown(skills: &[AgentSkillListing]) -> String {
    format!(
        "# MyOpenPanels Agent Skills\n\n{}\n",
        render_skill_table(skills)
    )
}

#[allow(clippy::too_many_arguments)]
fn render_agent_skill(
    skill: &AgentSkill,
    bootstrap: &ProjectBootstrap,
    selection: Option<&crate::selection::SelectionPayload>,
    wiki_selection: Option<&Value>,
    task_id: Option<&str>,
    local_dir: &Path,
    local_path: &Path,
    selected_wiki_skill_id: &str,
) -> Result<String, CliError> {
    let task = task_id.and_then(|id| find_wiki_task(bootstrap, id));
    if task_id.is_some() && task.is_none() {
        return Err(CliError::new(format!(
            "Wiki task not found: {}",
            task_id.unwrap_or_default()
        )));
    }
    Ok(format!(
        "# Skill: {}\n\nTitle: {}\nSource: {}\nLocal dir: {}\nLocal path: {}\nApplies to: {}\n\n## How To Load This Skill\n\nRead `SKILL.md` directly from the local path above. Treat this CLI output as the task-specific loader and command context, not as the skill body. Resolve referenced files relative to the local dir above.\n\n## Current Context\n\n{}\n\n## Commands For This Skill\n\n{}\n",
        skill.metadata.id,
        skill.metadata.title,
        skill.metadata.source,
        local_dir.display(),
        local_path.display(),
        if skill.metadata.applies_to.is_empty() { "any".to_owned() } else { skill.metadata.applies_to.join(", ") },
        render_current_context(bootstrap, selection, wiki_selection, task.as_ref()),
        render_skill_commands(
            skill,
            task.as_ref(),
            wiki_selection,
            selected_wiki_skill_id,
        ),
    ))
}

fn load_agent_skills() -> Result<Vec<AgentSkill>, CliError> {
    let mut skills = load_agent_skill_dirs()?
        .into_iter()
        .map(|(skill, _dir)| skill)
        .collect::<Vec<_>>();
    skills.sort_by(|left, right| left.metadata.id.cmp(&right.metadata.id));
    Ok(skills)
}

fn load_agent_skill_dirs() -> Result<Vec<(AgentSkill, &'static Dir<'static>)>, CliError> {
    let mut seen = BTreeSet::new();
    let mut skills = Vec::new();
    for dir in AGENT_SKILLS.dirs() {
        let skill_path = dir.path().join("SKILL.md");
        let file = AGENT_SKILLS.get_file(&skill_path).ok_or_else(|| {
            CliError::new(format!(
                "MyOpenPanels agent skill is missing SKILL.md: {}",
                dir.path().display()
            ))
        })?;
        let source = std::str::from_utf8(file.contents()).map_err(to_cli_error)?;
        let skill = parse_skill(source, &skill_path.display().to_string())?;
        if !seen.insert(skill.metadata.id.clone()) {
            return Err(CliError::new(format!(
                "Duplicate MyOpenPanels agent skill id: {}",
                skill.metadata.id
            )));
        }
        skills.push((skill, dir));
    }
    skills.sort_by(|left, right| left.0.metadata.id.cmp(&right.0.metadata.id));
    Ok(skills)
}

fn extract_embedded_dir_contents(
    dir: &Dir<'_>,
    root: &Path,
    destination: &Path,
) -> Result<(), CliError> {
    for file in dir.files() {
        let relative_path = file.path().strip_prefix(root).map_err(to_cli_error)?;
        let target_path = destination.join(relative_path);
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(to_cli_error)?;
        }
        fs::write(target_path, file.contents()).map_err(to_cli_error)?;
    }
    for child_dir in dir.dirs() {
        extract_embedded_dir_contents(child_dir, root, destination)?;
    }
    Ok(())
}

fn parse_skill(source: &str, file_name: &str) -> Result<AgentSkill, CliError> {
    let normalized_source;
    let source = if source.contains("\r\n") {
        normalized_source = source.replace("\r\n", "\n");
        normalized_source.as_str()
    } else {
        source
    };
    let rest = source
        .strip_prefix("---\n")
        .ok_or_else(|| CliError::new(format!("Agent skill is missing frontmatter: {file_name}")))?;
    let (frontmatter, body) = rest
        .split_once("\n---")
        .ok_or_else(|| CliError::new(format!("Agent skill is missing frontmatter: {file_name}")))?;
    let frontmatter = parse_frontmatter(frontmatter);
    let id = scalar(&frontmatter, "id")
        .ok_or_else(|| CliError::new(format!("Agent skill requires id and title: {file_name}")))?;
    let title = scalar(&frontmatter, "title")
        .ok_or_else(|| CliError::new(format!("Agent skill requires id and title: {file_name}")))?;
    Ok(AgentSkill {
        metadata: AgentSkillMetadata {
            applies_to: list(&frontmatter, "appliesTo"),
            description: scalar(&frontmatter, "description").unwrap_or_default(),
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

fn render_skill_table(skills: &[AgentSkillListing]) -> String {
    if skills.is_empty() {
        return "- none".to_owned();
    }
    let rows = skills
        .iter()
        .map(|item| {
            format!(
                "| `{}` | {} | {} | {} | {} |",
                item.skill.id,
                item.source,
                item.skill.applies_to.join(", "),
                item.skill.task_types.join(", "),
                item.local_path
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("| ID | Source | Applies To | Task Types | Local Path |\n| --- | --- | --- | --- | --- |\n{rows}")
}

fn agent_skill_listing(paths: &MyOpenPanelsPaths, skill: AgentSkillMetadata) -> AgentSkillListing {
    let (local_dir, local_path) = agent_skill_local_paths(paths, &skill.id);
    AgentSkillListing {
        source: skill.source.clone(),
        skill,
        local_dir: local_dir.display().to_string(),
        local_path: local_path.display().to_string(),
    }
}

fn agent_skill_local_paths(paths: &MyOpenPanelsPaths, skill_id: &str) -> (PathBuf, PathBuf) {
    let local_dir = paths.storage_dir.join("skills").join(skill_id);
    let local_path = local_dir.join("SKILL.md");
    (local_dir, local_path)
}

fn wiki_summary(bootstrap: &ProjectBootstrap, selection: Option<&Value>) -> Value {
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
    let active_space_id = state
        .get("activeWikiSpaceId")
        .and_then(Value::as_str)
        .unwrap_or("wiki:default");
    let active_space = state
        .get("wikiSpaces")
        .and_then(Value::as_array)
        .and_then(|spaces| {
            spaces
                .iter()
                .find(|space| space.get("id").and_then(Value::as_str) == Some(active_space_id))
                .or_else(|| spaces.first())
        });
    let selected_documents = selection
        .and_then(|value| value.get("selectedRawDocuments"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|document| {
            json!({
                "documentId": document.get("id").cloned().unwrap_or(Value::Null),
                "title": document.get("title").cloned().unwrap_or(Value::Null),
                "mimeType": document.get("mimeType").cloned().unwrap_or(Value::Null),
                "markdownVersion": document.get("markdownVersion").cloned().unwrap_or(Value::Null),
                "originalFilePath": document.get("originalFilePath").cloned().unwrap_or(Value::Null),
            })
        })
        .collect::<Vec<_>>();
    let selected_generated_documents = selection
        .and_then(|value| value.get("selectedGeneratedDocuments"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|document| {
            json!({
                "documentId": document.get("id").cloned().unwrap_or(Value::Null),
                "title": document.get("title").cloned().unwrap_or(Value::Null),
                "format": document.get("format").cloned().unwrap_or(Value::Null),
                "contentVersion": document.get("contentVersion").cloned().unwrap_or(Value::Null),
                "contentFilePath": document.get("contentFilePath").cloned().unwrap_or(Value::Null),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "agentSkillId": selected_agent_skill_id(state),
        "nextTaskAgentSkillId": next_task.as_ref().and_then(|task| task.get("agentSkillId")).and_then(Value::as_str).unwrap_or_else(|| selected_agent_skill_id(state)),
        "available": state.get("wikiSpaces").and_then(Value::as_array).is_some_and(|spaces| !spaces.is_empty()),
        "selected": selection.and_then(|value| value.get("selection")).and_then(|value| value.get("isWikiSelected")).and_then(Value::as_bool).unwrap_or(false),
        "wikiSpaceId": selection.and_then(|value| value.get("wiki")).and_then(|value| value.get("wikiSpaceId")).cloned().unwrap_or_else(|| json!(active_space_id)),
        "wikiTitle": selection.and_then(|value| value.get("wiki")).and_then(|value| value.get("title")).cloned().or_else(|| active_space.and_then(|space| space.get("title")).cloned()).unwrap_or_else(|| json!("Wiki")),
        "pageCount": selection.and_then(|value| value.get("wiki")).and_then(|value| value.get("pageCount")).cloned().unwrap_or_else(|| json!(active_space.and_then(|space| space.get("pageIndex")).and_then(Value::as_array).map(Vec::len).unwrap_or(0))),
        "querySkillId": WIKI_PANEL_SKILL_ID,
        "querySkillLoadCommand": format!("myopenpanels agent skill read --skill-id {WIKI_PANEL_SKILL_ID} --format json"),
        "selectedRawDocumentCount": selected_documents.len(),
        "selectedRawDocuments": selected_documents,
        "selectedGeneratedDocumentCount": selected_generated_documents.len(),
        "selectedGeneratedDocuments": selected_generated_documents,
        "nextTask": next_task,
        "pendingTaskCount": tasks.len(),
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
    wiki_selection: Option<&Value>,
    task: Option<&Value>,
) -> String {
    let wiki = wiki_summary(bootstrap, wiki_selection);
    let selected_shape_count = canvas_summary(selection)["selectedShapeCount"]
        .as_u64()
        .unwrap_or(0);
    let mut lines = vec![
        format!(
            "- project: {} ({})",
            bootstrap.project.title, bootstrap.project.id
        ),
        format!(
            "- active panel: {} ({})",
            bootstrap.active_panel_kind.as_str(),
            bootstrap.panel.title
        ),
        format!(
            "- wiki agent skill: {}",
            task.and_then(|value| value.get("agentSkillId"))
                .and_then(Value::as_str)
                .unwrap_or_else(|| wiki["agentSkillId"].as_str().unwrap_or("karpathy-llm-wiki"))
        ),
        format!(
            "- wiki selected as context: {}",
            wiki["selected"].as_bool().unwrap_or(false)
        ),
        format!(
            "- selected raw document count: {}",
            wiki["selectedRawDocumentCount"].as_u64().unwrap_or(0)
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

fn render_skill_commands(
    skill: &AgentSkill,
    task: Option<&Value>,
    wiki_selection: Option<&Value>,
    selected_wiki_skill_id: &str,
) -> String {
    if skill.metadata.id == WIKI_PANEL_SKILL_ID && task.is_none() {
        let wiki_space_id = wiki_selection
            .and_then(|selection| selection.get("wiki"))
            .and_then(|wiki| wiki.get("wikiSpaceId"))
            .and_then(Value::as_str)
            .unwrap_or("<wiki-space-id>");
        return format!(
            "```bash\nmyopenpanels panel selection read --format json\nmyopenpanels wiki page read --wiki-space-id {wiki_space_id} --path SCHEMA.md --format json\nmyopenpanels wiki page read --wiki-space-id {wiki_space_id} --path index.md --format json\nmyopenpanels wiki page search --wiki-space-id {wiki_space_id} --query <query> --format json\nmyopenpanels wiki page read --wiki-space-id {wiki_space_id} --path <relevant-page> --format json\n```"
        );
    }
    if skill.metadata.id == CANVAS_PANEL_SKILL_ID {
        return "```bash\nmyopenpanels panel selection read --format json\nmyopenpanels canvas generation begin --display-width <w> --display-height <h> [--use-selection] --expect-focus-revision <focus-revision> --format json\nmyopenpanels operation complete --operation-id <operation-id> --artifact-file <generated-path> --metadata-file <metadata.json> --format json\n```".to_owned();
    }
    if skill.metadata.id == WIKI_PANEL_SKILL_ID {
        if let Some(task) = task {
            let task_id = task["id"].as_str().unwrap_or("<task-id>");
            return format!(
                "```bash\nmyopenpanels task next --format json\nmyopenpanels agent skill read --skill-id {selected_wiki_skill_id} --task-id {task_id} --format json\nmyopenpanels task claim --task-id {task_id} --target-id <target-id> --format json\nmyopenpanels task complete --task-id {task_id} --lease-token \"$MYOPENPANELS_TASK_LEASE_TOKEN\" --format json\n```"
            );
        }
    }
    if let Some(task) = task {
        let task_id = task["id"].as_str().unwrap_or("<task-id>");
        let document_id = task["documentId"].as_str().unwrap_or("<document-id>");
        let wiki_space_id = task["wikiSpaceId"].as_str().unwrap_or("<wiki-space-id>");
        let task_type = task["type"].as_str().unwrap_or("");
        if task_type == "convert_document_to_markdown" {
            return format!(
                "```bash\nmyopenpanels task claim --task-id {task_id} --target-id <target-id> --format json\nmyopenpanels wiki raw-document list --format json\nmyopenpanels wiki raw-document markdown write --raw-document-id {document_id} --content-file <md-file> --task-id {task_id} --format json\nmyopenpanels task complete --task-id {task_id} --lease-token \"$MYOPENPANELS_TASK_LEASE_TOKEN\" --format json\n```"
            );
        }
        if task_type == "ingest_markdown_into_wiki" {
            return format!(
                "```bash\nmyopenpanels task claim --task-id {task_id} --target-id <target-id> --format json\nmyopenpanels wiki raw-document markdown read --raw-document-id {document_id} --format json\nmyopenpanels wiki page list --wiki-space-id {wiki_space_id} --format json\nmyopenpanels wiki page read --wiki-space-id {wiki_space_id} --path <page-path> --format json\nmyopenpanels wiki page write --wiki-space-id {wiki_space_id} --path <page-path> --content-file <md-file> --task-id {task_id} --format json\nmyopenpanels task complete --task-id {task_id} --lease-token \"$MYOPENPANELS_TASK_LEASE_TOKEN\" --format json\n```"
            );
        }
        if task_type == "rebuild_wiki_index" {
            return format!(
                "```bash\nmyopenpanels task claim --task-id {task_id} --target-id <target-id> --format json\nmyopenpanels wiki page list --wiki-space-id {wiki_space_id} --format json\nmyopenpanels wiki page read --wiki-space-id {wiki_space_id} --path <page-path> --format json\nmyopenpanels wiki page write --wiki-space-id {wiki_space_id} --path <page-path> --content-file <md-file> --task-id {task_id} --format json\nmyopenpanels task complete --task-id {task_id} --lease-token \"$MYOPENPANELS_TASK_LEASE_TOKEN\" --format json\n```"
            );
        }
    }
    "- No task-specific commands.".to_owned()
}

fn find_wiki_task(bootstrap: &ProjectBootstrap, task_id: &str) -> Option<Value> {
    bootstrap
        .tasks
        .iter()
        .find(|task| task.get("id").and_then(Value::as_str) == Some(task_id))
        .map(|task| {
            let mut normalized = task.as_object().cloned().unwrap_or_default();
            for field in ["input", "source"] {
                if let Some(values) = normalized
                    .remove(field)
                    .and_then(|value| value.as_object().cloned())
                {
                    normalized.extend(values);
                }
            }
            Value::Object(normalized)
        })
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_json_limits_utf8_depth_arrays_and_objects() {
        let mut object = serde_json::Map::new();
        for index in 0..40 {
            object.insert(format!("field{index:02}"), json!(index));
        }
        object.insert("long".to_owned(), json!("界".repeat(200)));
        object.insert(
            "array".to_owned(),
            Value::Array((0..20).map(|value| json!(value)).collect()),
        );
        object.insert(
            "deep".to_owned(),
            json!({ "one": { "two": { "three": { "four": true } } } }),
        );
        let mut truncated = false;
        let bounded = bounded_json(Value::Object(object), 0, &mut truncated);

        assert!(truncated);
        assert!(serde_json::to_vec(&bounded).expect("json").len() < 1024);
        assert!(bounded.as_object().unwrap().len() <= 32);
        let mut string_truncated = false;
        let string = bounded_json(json!("界".repeat(200)), 0, &mut string_truncated);
        assert!(string_truncated);
        assert!(string.as_str().unwrap().len() <= 256);
        assert!(string
            .as_str()
            .unwrap()
            .is_char_boundary(string.as_str().unwrap().len()));
    }

    #[test]
    fn compact_operation_references_leave_actions_at_the_response_root() {
        let summary = compact_operation_summary(&[json!({
            "id": "operation:1",
            "intent": "canvas.generation.begin",
            "panelKind": "canvas",
            "status": "active",
        })]);

        assert!(summary["items"][0].get("readAction").is_none());
        assert!(summary["items"][0]["readCommand"]
            .as_str()
            .unwrap()
            .starts_with("myopenpanels operation read"));
    }
}
