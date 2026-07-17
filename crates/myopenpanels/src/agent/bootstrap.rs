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
pub const AGENT_GUIDANCE_PROTOCOL_VERSION: u32 = 6;
pub const MAX_BOOTSTRAP_ENVELOPE_BYTES: usize = 8192;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillMetadata {
    pub applies_to: Vec<String>,
    pub description: String,
    pub id: String,
    pub load_when: Vec<String>,
    pub requires_commands: Vec<String>,
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

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillReadPayload {
    pub skill: AgentSkillMetadata,
    pub local_dir: String,
    pub local_path: String,
    pub markdown: String,
    pub actions: Value,
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
    let skills = load_agent_skills(paths, &bootstrap.project.id)?;
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
    let recommended_domains = recommended_catalog_domains(bootstrap.active_panel_kind);
    let detailed_selection = (bootstrap.active_panel_kind == PanelKind::Canvas)
        .then(|| read_selection(paths, None, false).ok())
        .flatten();
    let wiki_selection = (bootstrap.active_panel_kind == PanelKind::Wiki)
        .then(|| read_agent_selection(paths).ok())
        .flatten();
    let mut required_skills = Vec::new();
    let mut required_commands = BTreeSet::new();
    if let Some(skill_id) = panel_skill_id(bootstrap.active_panel_kind) {
        let skill = required_agent_skill(&skills, skill_id)?;
        required_commands.extend(skill.metadata.requires_commands.iter().cloned());
        required_skills.push(write_required_skill_loader(
            paths,
            skill,
            &bootstrap,
            detailed_selection.as_ref(),
            wiki_selection.as_ref(),
            None,
        )?);
    }
    if let Some((task_id, skill_id)) = next_wiki_task_authoring_skill(&bootstrap)
        .or_else(|| next_writing_task_authoring_skill(&bootstrap))
    {
        let skill = required_agent_skill(&skills, skill_id)?;
        required_commands.extend(skill.metadata.requires_commands.iter().cloned());
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
    let required_domains = required_commands
        .iter()
        .filter_map(|intent| crate::cli::registry::catalog_domain_for_intent(intent))
        .collect::<BTreeSet<_>>();
    next_actions.extend(required_domains.into_iter().map(catalog_action));
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
        action["condition"] = json!({
            "type": "agent-judgment",
            "description": "The user request handles work from the project Task queue."
        });
        action["skillId"] = json!(TASK_QUEUE_SKILL_ID);
        next_actions.push(action);
    }
    next_actions.extend(
        recommended_domains
            .iter()
            .map(|scope| {
                let mut action = command_action(
                    "agent.catalog",
                    vec![
                        "--domain".to_owned(),
                        (*scope).to_owned(),
                        "--format".to_owned(),
                        "json".to_owned(),
                    ],
                );
                action["condition"] = json!({
                    "type": "agent-judgment",
                    "description": format!("The user request matches the {scope} domain.")
                });
                action
            })
            .collect::<Vec<_>>(),
    );
    let mut catalog_index_action = command_action(
        "agent.catalog",
        vec!["--format".to_owned(), "json".to_owned()],
    );
    catalog_index_action["condition"] = json!({
        "type": "fallback",
        "when": "no-recommended-domain-matches"
    });
    next_actions.push(catalog_index_action);
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
        action["condition"] = json!({
            "type": "resource-status",
            "resource": "next-task",
            "statuses": ["queued", "failed"]
        });
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
        action["condition"] = json!({
            "type": "resource-status",
            "resource": "operation",
            "statuses": ["active"]
        });
        Some(action)
    }));
    let required_actions = required_skills
        .iter()
        .flat_map(|skill| {
            let id = skill.get("id").and_then(Value::as_str).unwrap_or("skill");
            [
                json!({
                    "id": format!("skill.{id}.context"),
                    "intent": "agent-host.file.read",
                    "executor": "agent-host",
                    "kind": "read-file",
                    "path": skill.get("contextPath").cloned().unwrap_or(Value::Null),
                }),
                json!({
                    "id": format!("skill.{id}.body"),
                    "intent": "agent-host.file.read",
                    "executor": "agent-host",
                    "kind": "read-file",
                    "path": skill.get("localPath").cloned().unwrap_or(Value::Null),
                }),
            ]
        })
        .collect::<Vec<_>>();
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
            "recommendedDomains": recommended_domains,
        },
        "skills": required_skills,
        "actions": { "required": required_actions, "suggested": next_actions },
    }))
}

fn entry_skill_update_bootstrap(
    paths: &MyOpenPanelsPaths,
    bootstrap: &ProjectBootstrap,
    cli_version: &str,
    update: &crate::agent_control::EntrySkillUpdate,
) -> Value {
    let host_action = json!({
        "id": "entry-skill.ensure",
        "intent": "agent-host.skill.update-required",
        "executor": "agent-host",
        "kind": "ensure-skill",
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
    acknowledge_action["id"] = json!("entry-skill.acknowledge");
    acknowledge_action["required"] = json!(true);
    let mut rerun_action = project_command_action(
        paths,
        "agent.bootstrap.read",
        vec!["--format".to_owned(), "json".to_owned()],
    );
    rerun_action["id"] = json!("agent.bootstrap.refresh");
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
        "actions": {
            "required": [host_action, acknowledge_action, rerun_action],
            "suggested": []
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
    let task = next_project_task_for_queue(bootstrap, "wiki")?;
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

fn next_writing_task_authoring_skill(bootstrap: &ProjectBootstrap) -> Option<(&str, &str)> {
    if bootstrap.active_panel_kind != PanelKind::Writing {
        return None;
    }
    let task = next_project_task_for_queue(bootstrap, "writing")?;
    let skill_id = match task.get("type").and_then(Value::as_str) {
        Some("refine_writing_skill") => task.pointer("/input/refinerSkillId"),
        _ => task.pointer("/input/writingSkillId"),
    }
    .and_then(Value::as_str)?;
    Some((task.get("id").and_then(Value::as_str)?, skill_id))
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
    let (local_dir, local_path) =
        agent_skill_local_paths(paths, &bootstrap.project.id, &skill.metadata);
    let markdown = render_agent_skill(
        skill,
        bootstrap,
        selection,
        wiki_selection,
        task_id,
        &local_dir,
        &local_path,
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
