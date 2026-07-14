use crate::control::{now_iso, read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::MyOpenPanelsPaths;
use crate::storage::{Storage, TaskInsert};
use crate::types::{PanelKind, ProjectBootstrap, ProjectPanelSnapshot};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub const WRITING_PANEL_SKILL_ID: &str = "writing-panel";
pub const WRITING_CAPABILITY: &str = "writing.generateDocument";
pub const WRITING_REFINEMENT_CAPABILITY: &str = "writing.refineSkill";
pub const WRITING_SKILL_REFINER_ID: &str = "writing-skill-refiner";

fn active_writing(paths: &MyOpenPanelsPaths) -> Result<ProjectBootstrap, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    if bootstrap.active_panel_kind != PanelKind::Writing {
        return Err(CliError::with_recovery(
            "panel_kind_mismatch",
            format!(
                "The active panel is {}, but this command requires writing.",
                bootstrap.active_panel_kind.as_str()
            ),
            true,
            "Activate the Writing panel, refresh Agent Bootstrap, and retry.",
        ));
    }
    Ok(bootstrap)
}

fn wiki_snapshot(bootstrap: &ProjectBootstrap) -> Result<&ProjectPanelSnapshot, CliError> {
    bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
        .ok_or_else(|| CliError::with_code("target_not_found", "The Project has no Wiki panel."))
}

fn writing_selection_value(
    paths: &MyOpenPanelsPaths,
    bootstrap: &ProjectBootstrap,
) -> Result<Value, CliError> {
    let stored = Storage::open(paths)?
        .read_panel_selection(&bootstrap.project.id, &bootstrap.panel.id)?
        .unwrap_or_else(|| json!({}));
    let wiki = wiki_snapshot(bootstrap)?;
    let raw_ids = known_selected_ids(
        &stored,
        "selectedRawDocumentIds",
        &wiki.state,
        "rawDocuments",
    );
    let generated_ids = known_selected_ids(
        &stored,
        "selectedGeneratedDocumentIds",
        &wiki.state,
        "generatedDocuments",
    );
    Ok(json!({
        "kind": "writing",
        "projectId": bootstrap.project.id,
        "panelId": bootstrap.panel.id,
        "isWikiSelected": stored.get("isWikiSelected").and_then(Value::as_bool).unwrap_or(false),
        "selectedRawDocumentIds": raw_ids,
        "selectedGeneratedDocumentIds": generated_ids,
        "updatedAt": stored.get("updatedAt").cloned().unwrap_or(Value::Null),
    }))
}

fn known_selected_ids(
    stored: &Value,
    selection_key: &str,
    wiki_state: &Value,
    collection_key: &str,
) -> Vec<String> {
    let known = wiki_state
        .get(collection_key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    stored
        .get(selection_key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|id| known.contains(*id) && seen.insert((*id).to_owned()))
        .map(str::to_owned)
        .collect()
}

pub fn read_selection(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let bootstrap = active_writing(paths)?;
    Ok(json!({ "selection": writing_selection_value(paths, &bootstrap)? }))
}

pub fn write_selection(
    paths: &MyOpenPanelsPaths,
    is_wiki_selected: bool,
    selected_raw_document_ids: &[String],
    selected_generated_document_ids: &[String],
) -> Result<Value, CliError> {
    let bootstrap = active_writing(paths)?;
    let requested = json!({
        "isWikiSelected": is_wiki_selected,
        "selectedRawDocumentIds": selected_raw_document_ids,
        "selectedGeneratedDocumentIds": selected_generated_document_ids,
    });
    let wiki = wiki_snapshot(&bootstrap)?;
    let selection = json!({
        "kind": "writing",
        "projectId": bootstrap.project.id,
        "panelId": bootstrap.panel.id,
        "isWikiSelected": is_wiki_selected,
        "selectedRawDocumentIds": known_selected_ids(&requested, "selectedRawDocumentIds", &wiki.state, "rawDocuments"),
        "selectedGeneratedDocumentIds": known_selected_ids(&requested, "selectedGeneratedDocumentIds", &wiki.state, "generatedDocuments"),
        "updatedAt": now_iso(),
    });
    Storage::open(paths)?.write_panel_selection(
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &selection,
    )?;
    Ok(json!({ "selection": selection }))
}

pub fn save_draft(
    paths: &MyOpenPanelsPaths,
    draft: &str,
    mode: &str,
    refinement_name: &str,
    target_generated_document_id: Option<&str>,
    selected_create_writing_skill_ids: &[String],
    selected_revision_writing_skill_id: Option<&str>,
) -> Result<Value, CliError> {
    let bootstrap = active_writing(paths)?;
    validate_mode(
        mode,
        target_generated_document_id,
        wiki_snapshot(&bootstrap)?,
        false,
    )?;
    validate_writing_skills(paths, selected_create_writing_skill_ids, false, "create")?;
    let revision_skill_ids = selected_revision_writing_skill_id
        .map(|id| vec![id.to_owned()])
        .unwrap_or_default();
    validate_writing_skills(paths, &revision_skill_ids, false, "revise")?;
    let state = json!({
        "schemaVersion": 5,
        "draft": draft,
        "mode": mode,
        "refinementName": refinement_name,
        "targetGeneratedDocumentId": if mode == "revise" { target_generated_document_id } else { None },
        "selectedCreateWritingSkillIds": selected_create_writing_skill_ids,
        "selectedRevisionWritingSkillId": selected_revision_writing_skill_id,
    });
    let revision = Storage::open(paths)?.write_panel_state(
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &state,
    )?;
    Ok(json!({ "state": state, "revision": revision }))
}

fn validate_mode(
    mode: &str,
    target_generated_document_id: Option<&str>,
    wiki: &ProjectPanelSnapshot,
    target_required: bool,
) -> Result<(), CliError> {
    if !matches!(mode, "create" | "revise" | "refine") {
        return Err(CliError::with_code(
            "invalid_writing_mode",
            "Writing mode must be create, revise, or refine.",
        ));
    }
    if mode == "revise" {
        let target = target_generated_document_id.filter(|id| !id.trim().is_empty());
        if target_required && target.is_none() {
            return Err(CliError::with_code(
                "writing_target_required",
                "Revision mode requires a generated document target.",
            ));
        }
        if let Some(target) = target {
            let exists = wiki
                .state
                .get("generatedDocuments")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|document| document.get("id").and_then(Value::as_str) == Some(target));
            if !exists {
                return Err(CliError::with_code(
                    "writing_target_not_found",
                    format!("Generated document not found: {target}"),
                ));
            }
        }
    }
    Ok(())
}

pub fn create_requests(
    paths: &MyOpenPanelsPaths,
    instruction: &str,
    mode: &str,
    target_generated_document_id: Option<&str>,
    writing_skill_ids: &[String],
) -> Result<Value, CliError> {
    let instruction = instruction.trim();
    if instruction.is_empty() {
        return Err(CliError::with_code(
            "writing_instruction_required",
            "Writing instruction cannot be empty.",
        ));
    }
    let bootstrap = active_writing(paths)?;
    let wiki = wiki_snapshot(&bootstrap)?;
    if !matches!(mode, "create" | "revise") {
        return Err(CliError::with_code(
            "invalid_writing_mode",
            "Document requests must use create or revise mode.",
        ));
    }
    validate_mode(mode, target_generated_document_id, wiki, true)?;
    let writing_skills = validate_writing_skills(paths, writing_skill_ids, true, mode)?;
    let selection = writing_selection_value(paths, &bootstrap)?;
    let wiki_space_id = wiki
        .state
        .get("activeWikiSpaceId")
        .and_then(Value::as_str)
        .unwrap_or("wiki:default");
    let target_version = target_generated_document_id.and_then(|target| {
        wiki.state
            .get("generatedDocuments")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|document| document.get("id").and_then(Value::as_str) == Some(target))
            .and_then(|document| document.get("contentVersion"))
            .and_then(Value::as_u64)
    });
    let target_ref = target_generated_document_id.unwrap_or("new");
    let storage = Storage::open(paths)?;
    let task_inserts = writing_skills
        .iter()
        .map(|listing| TaskInsert {
            queue: "writing".to_owned(),
            task_type: "generate_document".to_owned(),
            capability: WRITING_CAPABILITY.to_owned(),
            target_ref: target_ref.to_owned(),
            input: json!({
                "instruction": instruction,
                "mode": mode,
                "targetGeneratedDocumentId": target_generated_document_id,
                "targetContentVersion": target_version,
                "writingSkillId": listing.skill.id.clone(),
                "writingSkill": {
                    "id": listing.skill.id.clone(),
                    "title": listing.skill.title.clone(),
                    "description": listing.skill.description.clone(),
                    "source": listing.source.clone(),
                },
                "context": {
                    "isWikiSelected": selection["isWikiSelected"],
                    "selectedRawDocumentIds": selection["selectedRawDocumentIds"],
                    "selectedGeneratedDocumentIds": selection["selectedGeneratedDocumentIds"],
                },
            }),
            source: json!({
                "writingPanelId": bootstrap.panel.id,
                "wikiPanelId": wiki.panel.id,
                "wikiSpaceId": wiki_space_id,
                "panelSkillId": WRITING_PANEL_SKILL_ID,
                "writingSkillId": listing.skill.id.clone(),
                "writingSkillSource": listing.source.clone(),
            }),
        })
        .collect::<Vec<_>>();
    let tasks = storage.insert_tasks(&bootstrap.project.id, &bootstrap.panel.id, &task_inserts)?;
    let mut selected_create_writing_skill_ids = bootstrap.state["selectedCreateWritingSkillIds"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut selected_revision_writing_skill_id = bootstrap.state["selectedRevisionWritingSkillId"]
        .as_str()
        .map(str::to_owned);
    if mode == "create" {
        selected_create_writing_skill_ids = writing_skill_ids
            .iter()
            .cloned()
            .map(Value::String)
            .collect();
    } else {
        selected_revision_writing_skill_id = writing_skill_ids.first().cloned();
    }
    let state = json!({
        "schemaVersion": 5,
        "draft": "",
        "mode": mode,
        "refinementName": bootstrap.state.get("refinementName").and_then(Value::as_str).unwrap_or(""),
        "targetGeneratedDocumentId": null,
        "selectedCreateWritingSkillIds": selected_create_writing_skill_ids,
        "selectedRevisionWritingSkillId": selected_revision_writing_skill_id,
    });
    let revision = storage.write_panel_state(&bootstrap.project.id, &bootstrap.panel.id, &state)?;
    Ok(json!({ "tasks": tasks, "state": state, "revision": revision }))
}

pub fn create_refinement_request(
    paths: &MyOpenPanelsPaths,
    requested_name: &str,
) -> Result<Value, CliError> {
    let name = normalize_skill_name(requested_name)?;
    let bootstrap = active_writing(paths)?;
    let wiki = wiki_snapshot(&bootstrap)?;
    let selection = writing_selection_value(paths, &bootstrap)?;
    let raw_ids = selection["selectedRawDocumentIds"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let generated_ids = selection["selectedGeneratedDocumentIds"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if raw_ids.is_empty() && generated_ids.is_empty() {
        return Err(CliError::with_code(
            "writing_refinement_source_required",
            "Select at least one raw or generated document to refine.",
        ));
    }
    validate_refinement_sources(wiki, &raw_ids, &generated_ids)?;
    validate_available_skill_name(paths, &bootstrap.project.id, &name, None)?;

    let skill_id = format!("writing-project-{:032x}", rand::random::<u128>());
    let input = json!({
        "name": name,
        "skillId": skill_id,
        "refinerSkillId": WRITING_SKILL_REFINER_ID,
        "context": {
            "selectedRawDocumentIds": raw_ids,
            "selectedGeneratedDocumentIds": generated_ids,
        },
    });
    let source = json!({
        "writingPanelId": bootstrap.panel.id,
        "wikiPanelId": wiki.panel.id,
        "panelSkillId": WRITING_PANEL_SKILL_ID,
        "refinerSkillId": WRITING_SKILL_REFINER_ID,
    });
    let task = Storage::open(paths)?.insert_task(
        &bootstrap.project.id,
        &bootstrap.panel.id,
        "writing",
        "refine_writing_skill",
        WRITING_REFINEMENT_CAPABILITY,
        &skill_id,
        &input,
        &source,
    )?;
    let state = json!({
        "schemaVersion": 5,
        "draft": bootstrap.state.get("draft").and_then(Value::as_str).unwrap_or(""),
        "mode": "refine",
        "refinementName": "",
        "targetGeneratedDocumentId": bootstrap.state.get("targetGeneratedDocumentId").cloned().unwrap_or(Value::Null),
        "selectedCreateWritingSkillIds": bootstrap.state.get("selectedCreateWritingSkillIds").cloned().unwrap_or_else(|| json!([])),
        "selectedRevisionWritingSkillId": bootstrap.state.get("selectedRevisionWritingSkillId").cloned().unwrap_or(Value::Null),
    });
    let revision = Storage::open(paths)?.write_panel_state(
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &state,
    )?;
    Ok(json!({ "task": task, "state": state, "revision": revision }))
}

fn normalize_skill_name(requested_name: &str) -> Result<String, CliError> {
    let name = requested_name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() {
        return Err(CliError::with_code(
            "writing_skill_name_required",
            "Writing Skill name cannot be empty.",
        ));
    }
    if name.chars().count() > 80 {
        return Err(CliError::with_code(
            "writing_skill_name_too_long",
            "Writing Skill name cannot exceed 80 characters.",
        ));
    }
    Ok(name)
}

fn validate_refinement_sources(
    wiki: &ProjectPanelSnapshot,
    raw_ids: &[Value],
    generated_ids: &[Value],
) -> Result<(), CliError> {
    let raw_documents = wiki.state["rawDocuments"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for id in raw_ids.iter().filter_map(Value::as_str) {
        let ready = raw_documents.iter().any(|document| {
            document.get("id").and_then(Value::as_str) == Some(id)
                && document
                    .get("markdownRef")
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.is_empty())
        });
        if !ready {
            return Err(CliError::with_code(
                "writing_refinement_source_not_ready",
                format!("Raw document is not ready for refinement: {id}"),
            ));
        }
    }
    let generated_documents = wiki.state["generatedDocuments"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for id in generated_ids.iter().filter_map(Value::as_str) {
        let ready = generated_documents.iter().any(|document| {
            document.get("id").and_then(Value::as_str) == Some(id)
                && document
                    .get("contentRef")
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.is_empty())
                && matches!(
                    document
                        .pointer("/generation/status")
                        .and_then(Value::as_str),
                    None | Some("completed")
                )
        });
        if !ready {
            return Err(CliError::with_code(
                "writing_refinement_source_not_ready",
                format!("Generated document is not ready for refinement: {id}"),
            ));
        }
    }
    Ok(())
}

fn normalized_name_key(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn validate_available_skill_name(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    name: &str,
    allowed_skill_id: Option<&str>,
) -> Result<(), CliError> {
    let key = normalized_name_key(name);
    if crate::agent::list_agent_skills_for_project(paths, project_id)?
        .into_iter()
        .any(|item| {
            Some(item.skill.id.as_str()) != allowed_skill_id
                && normalized_name_key(&item.skill.title) == key
        })
    {
        return Err(CliError::with_code(
            "writing_skill_name_conflict",
            format!("A Writing Skill with this name already exists: {name}"),
        ));
    }
    let pending_conflict = Storage::open(paths)?
        .list_tasks(project_id)?
        .into_iter()
        .any(|task| {
            task.get("queue").and_then(Value::as_str) == Some("writing")
                && task.get("type").and_then(Value::as_str) == Some("refine_writing_skill")
                && matches!(
                    task.get("status").and_then(Value::as_str),
                    Some("queued" | "reserved" | "running" | "claimed" | "failed")
                )
                && task
                    .pointer("/input/name")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| normalized_name_key(candidate) == key)
                && task.pointer("/input/skillId").and_then(Value::as_str) != allowed_skill_id
        });
    if pending_conflict {
        return Err(CliError::with_code(
            "writing_skill_name_conflict",
            format!("A Writing Skill refinement with this name already exists: {name}"),
        ));
    }
    Ok(())
}

fn validate_writing_skills(
    paths: &MyOpenPanelsPaths,
    writing_skill_ids: &[String],
    required: bool,
    mode: &str,
) -> Result<Vec<crate::agent::AgentSkillListing>, CliError> {
    if required && writing_skill_ids.is_empty() {
        return Err(CliError::with_code(
            "writing_skill_required",
            "Select at least one Writing Skill.",
        ));
    }
    if required && mode == "revise" && writing_skill_ids.len() > 1 {
        return Err(CliError::with_code(
            "writing_revision_skill_limit",
            "Revision mode accepts exactly one Writing Skill.",
        ));
    }
    let mut seen = BTreeSet::new();
    let mut skills = Vec::with_capacity(writing_skill_ids.len());
    for skill_id in writing_skill_ids {
        if !seen.insert(skill_id.as_str()) {
            return Err(CliError::with_code(
                "duplicate_writing_skill",
                format!("Writing Skill was selected more than once: {skill_id}"),
            ));
        }
        skills.push(crate::agent::writing_agent_skill(paths, skill_id)?);
    }
    Ok(skills)
}

pub fn read_request(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let mut payload = crate::tasks::inspect_task(paths, task_id)?;
    if payload["task"]["queue"].as_str() != Some("writing") {
        return Err(CliError::with_code(
            "task_kind_mismatch",
            format!("Task is not a writing request: {task_id}"),
        ));
    }
    let skill_id = payload["task"]["input"]["writingSkillId"]
        .as_str()
        .ok_or_else(|| {
            CliError::with_code(
                "writing_skill_missing",
                format!("Writing task has no captured Writing Skill: {task_id}"),
            )
        })?;
    let skill_action = crate::cli::registry::command_action(
        crate::cli::registry::CommandId::registered("agent.skill.read"),
        vec![
            "--skill-id".to_owned(),
            skill_id.to_owned(),
            "--task-id".to_owned(),
            task_id.to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ],
    )
    .ok_or_else(|| CliError::new("Agent Skill read capability is unavailable."))?;
    payload["writingSkill"] = payload["task"]["input"]["writingSkill"].clone();
    payload["nextRequiredAction"] = json!({
        "intent": "load-writing-skill",
        "required": true,
        "steps": [skill_action],
        "instruction": "Load the task-selected Writing Skill before beginning the generation Operation.",
    });
    Ok(payload)
}

pub fn read_refinement(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let mut payload = crate::tasks::inspect_task(paths, task_id)?;
    if payload["task"]["queue"].as_str() != Some("writing")
        || payload["task"]["type"].as_str() != Some("refine_writing_skill")
    {
        return Err(CliError::with_code(
            "task_kind_mismatch",
            format!("Task is not a Writing Skill refinement: {task_id}"),
        ));
    }
    let skill_action = crate::cli::registry::command_action(
        crate::cli::registry::CommandId::registered("agent.skill.read"),
        vec![
            "--skill-id".to_owned(),
            WRITING_SKILL_REFINER_ID.to_owned(),
            "--task-id".to_owned(),
            task_id.to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ],
    )
    .ok_or_else(|| CliError::new("Agent Skill read capability is unavailable."))?;
    payload["nextRequiredAction"] = json!({
        "intent": "load-writing-refinement-skill",
        "required": true,
        "steps": [skill_action],
        "instruction": "Load the built-in refinement Skill before reading the captured source documents.",
    });
    Ok(payload)
}

pub fn install_project_skill(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    skill_file: &str,
) -> Result<Value, CliError> {
    let payload = read_refinement(paths, task_id)?;
    let task = &payload["task"];
    if !matches!(
        task.get("status").and_then(Value::as_str),
        Some("reserved" | "running" | "claimed")
    ) {
        return Err(CliError::with_code(
            "task_not_claimed",
            "Claim the refinement task before installing its Writing Skill.",
        ));
    }
    let project_id = task["projectId"].as_str().unwrap_or_default();
    let skill_id = task["input"]["skillId"].as_str().unwrap_or_default();
    let name = task["input"]["name"].as_str().unwrap_or_default();
    if project_id.is_empty() || skill_id.is_empty() || name.is_empty() {
        return Err(CliError::with_code(
            "writing_refinement_invalid",
            "The refinement task is missing its project, Skill id, or name.",
        ));
    }
    let source = fs::read_to_string(skill_file).map_err(|error| {
        CliError::with_code(
            "writing_skill_file_invalid",
            format!("Could not read Writing Skill file: {error}"),
        )
    })?;
    let skill = crate::agent::parse_skill(&source, skill_file)?;
    validate_generated_project_skill(&skill, skill_id, name)?;
    validate_available_skill_name(paths, project_id, name, Some(skill_id))?;

    let skills_dir = crate::agent::project_agent_skills_dir(paths, project_id);
    fs::create_dir_all(&skills_dir).map_err(to_cli_error)?;
    let final_dir = skills_dir.join(crate::paths::sanitize_path_part(skill_id));
    let manifest = json!({
        "schemaVersion": 1,
        "source": "project",
        "projectId": project_id,
        "taskId": task_id,
        "skillId": skill_id,
        "title": name,
        "createdAt": now_iso(),
    });
    if final_dir.exists() {
        let existing_manifest = read_skill_manifest(&final_dir)?;
        let existing_source =
            fs::read_to_string(final_dir.join("SKILL.md")).map_err(to_cli_error)?;
        if existing_manifest["taskId"].as_str() == Some(task_id) && existing_source == source {
            let listing = crate::agent::project_agent_skill_listing(
                paths,
                project_id,
                skill.metadata.clone(),
            );
            return Ok(json!({ "skill": listing }));
        }
        return Err(CliError::with_code(
            "writing_skill_conflict",
            format!("Writing Skill target already exists: {skill_id}"),
        ));
    }
    let staging_dir = skills_dir.join(format!(".{skill_id}.tmp-{:032x}", rand::random::<u128>()));
    fs::create_dir_all(&staging_dir).map_err(to_cli_error)?;
    let install_result = (|| -> Result<(), CliError> {
        fs::write(staging_dir.join("SKILL.md"), source.as_bytes()).map_err(to_cli_error)?;
        fs::write(
            staging_dir.join("manifest.json"),
            format!(
                "{}\n",
                serde_json::to_string_pretty(&manifest).map_err(to_cli_error)?
            ),
        )
        .map_err(to_cli_error)?;
        fs::rename(&staging_dir, &final_dir).map_err(to_cli_error)
    })();
    if install_result.is_err() {
        let _ = fs::remove_dir_all(&staging_dir);
    }
    install_result?;
    let listing =
        crate::agent::project_agent_skill_listing(paths, project_id, skill.metadata.clone());
    Ok(json!({ "skill": listing }))
}

fn validate_generated_project_skill(
    skill: &crate::agent::AgentSkill,
    expected_id: &str,
    expected_title: &str,
) -> Result<(), CliError> {
    let metadata = &skill.metadata;
    let valid = metadata.id == expected_id
        && metadata.title == expected_title
        && metadata.source == "project"
        && metadata.applies_to == ["writing"]
        && metadata.task_types == ["generate_document"]
        && metadata.requires_capabilities.is_empty()
        && !metadata.description.trim().is_empty()
        && !skill.body.trim().is_empty();
    if !valid {
        return Err(CliError::with_code(
            "writing_skill_file_invalid",
            "Generated Writing Skill frontmatter or body does not match the refinement task.",
        ));
    }
    Ok(())
}

fn read_skill_manifest(skill_dir: &Path) -> Result<Value, CliError> {
    let source = fs::read_to_string(skill_dir.join("manifest.json")).map_err(to_cli_error)?;
    serde_json::from_str(&source).map_err(to_cli_error)
}

fn installed_project_skill_for_task(
    paths: &MyOpenPanelsPaths,
    task: &Value,
) -> Result<bool, CliError> {
    let project_id = task["projectId"].as_str().unwrap_or_default();
    let skill_id = task["input"]["skillId"].as_str().unwrap_or_default();
    if project_id.is_empty() || skill_id.is_empty() {
        return Ok(false);
    }
    let skill_dir = crate::agent::project_agent_skills_dir(paths, project_id)
        .join(crate::paths::sanitize_path_part(skill_id));
    if !skill_dir.join("SKILL.md").is_file() || !skill_dir.join("manifest.json").is_file() {
        return Ok(false);
    }
    Ok(read_skill_manifest(&skill_dir)?["taskId"].as_str()
        == task.get("id").and_then(Value::as_str))
}

fn read_writing_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let payload = crate::tasks::inspect_task(paths, task_id)?;
    match payload["task"]["type"].as_str() {
        Some("generate_document") => read_request(paths, task_id),
        Some("refine_writing_skill") => read_refinement(paths, task_id),
        _ => Err(CliError::with_code(
            "task_kind_mismatch",
            format!("Unsupported writing task: {task_id}"),
        )),
    }
}

pub fn panel_context(bootstrap: &ProjectBootstrap) -> Value {
    let state = &bootstrap.state;
    let writing_tasks = bootstrap
        .tasks
        .iter()
        .filter(|task| task.get("queue").and_then(Value::as_str) == Some("writing"))
        .count();
    json!({
        "panelKind": "writing",
        "draftLength": state.get("draft").and_then(Value::as_str).map(str::len).unwrap_or(0),
        "mode": state.get("mode").cloned().unwrap_or_else(|| json!("create")),
        "selectedWritingSkillCount": if state.get("mode").and_then(Value::as_str) == Some("revise") {
            usize::from(state.get("selectedRevisionWritingSkillId").is_some_and(Value::is_string))
        } else {
            state.get("selectedCreateWritingSkillIds").and_then(Value::as_array).map(Vec::len).unwrap_or(0)
        },
        "writingTaskCount": writing_tasks,
    })
}

pub fn panel_selection(
    paths: &MyOpenPanelsPaths,
    bootstrap: &ProjectBootstrap,
) -> Result<Value, CliError> {
    writing_selection_value(paths, bootstrap)
}

pub fn claim_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let mut payload = read_writing_task(paths, task_id)?;
    let attempt = payload["task"]["attempt"].as_i64().unwrap_or(0) + 1;
    payload["task"]["status"] = json!("running");
    payload["task"]["attempt"] = json!(attempt);
    Ok(payload)
}

pub fn heartbeat_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    read_writing_task(paths, task_id)
}

pub fn complete_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let payload = read_writing_task(paths, task_id)?;
    if payload["task"]["type"].as_str() == Some("refine_writing_skill")
        && !installed_project_skill_for_task(paths, &payload["task"])?
    {
        return Err(CliError::with_code(
            "writing_skill_not_installed",
            "Install the refined project Writing Skill before completing its Task.",
        ));
    }
    if payload["task"]["type"].as_str() == Some("generate_document")
        && active_task_operations(paths, task_id)?.next().is_some()
    {
        return Err(CliError::with_code(
            "writing_operation_active",
            "Complete the Writing generation Operation before completing its Task.",
        ));
    }
    Ok(payload)
}

pub fn fail_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    message: &str,
) -> Result<Value, CliError> {
    let payload = read_writing_task(paths, task_id)?;
    finish_task_operations(paths, task_id, "failed", message)?;
    remove_uncommitted_project_skill(paths, &payload["task"])?;
    Ok(payload)
}

pub fn release_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let payload = read_writing_task(paths, task_id)?;
    finish_task_operations(paths, task_id, "cancelled", "Writing task released.")?;
    remove_uncommitted_project_skill(paths, &payload["task"])?;
    Ok(payload)
}

pub fn retry_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let payload = read_writing_task(paths, task_id)?;
    finish_task_operations(paths, task_id, "cancelled", "Writing task retried.")?;
    remove_uncommitted_project_skill(paths, &payload["task"])?;
    Ok(payload)
}

pub fn cancel_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let payload = read_writing_task(paths, task_id)?;
    finish_task_operations(paths, task_id, "cancelled", "Writing task cancelled.")?;
    remove_uncommitted_project_skill(paths, &payload["task"])?;
    Ok(payload)
}

fn active_task_operations<'a>(
    paths: &MyOpenPanelsPaths,
    task_id: &'a str,
) -> Result<impl Iterator<Item = Value> + 'a, CliError> {
    let operations = Storage::open(paths)?.list_agent_operations(None, None)?;
    Ok(operations.into_iter().filter(move |operation| {
        operation
            .pointer("/target/writingTaskId")
            .and_then(Value::as_str)
            == Some(task_id)
            && operation.get("status").and_then(Value::as_str) == Some("active")
    }))
}

fn finish_task_operations(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    status: &str,
    message: &str,
) -> Result<(), CliError> {
    for operation in active_task_operations(paths, task_id)? {
        if let Some(operation_id) = operation.get("id").and_then(Value::as_str) {
            crate::operations::finish_any(paths, operation_id, status, Some(message))?;
        }
    }
    Ok(())
}

fn remove_uncommitted_project_skill(
    paths: &MyOpenPanelsPaths,
    task: &Value,
) -> Result<(), CliError> {
    if task.get("type").and_then(Value::as_str) != Some("refine_writing_skill") {
        return Ok(());
    }
    let project_id = task
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let skill_id = task
        .pointer("/input/skillId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if project_id.is_empty() || skill_id.is_empty() {
        return Ok(());
    }
    let skill_dir = crate::agent::project_agent_skills_dir(paths, project_id)
        .join(crate::paths::sanitize_path_part(skill_id));
    match fs::remove_dir_all(skill_dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(to_cli_error(error)),
    }
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::ensure_project_bootstrap;
    use crate::paths::resolve_myopenpanels_paths;
    use std::fs;

    fn test_paths() -> (tempfile::TempDir, MyOpenPanelsPaths) {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("writing-test"),
        )
        .expect("paths");
        (temp, paths)
    }

    fn long_article_skill_ids() -> Vec<String> {
        vec!["writing-long-article".to_owned()]
    }

    #[test]
    fn writing_selection_is_independent_and_request_captures_it() {
        let (_temp, paths) = test_paths();
        let initial = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        crate::wiki::write_agent_selection(&paths, true, &[], &[]).expect("wiki selection");
        let writing = ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id.clone()),
            },
        )
        .expect("writing panel");
        write_selection(&paths, false, &[], &[]).expect("writing selection");

        let storage = Storage::open(&paths).expect("storage");
        let wiki_panel = writing
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
            .expect("wiki panel");
        let wiki_selection = storage
            .read_panel_selection(&writing.project.id, &wiki_panel.panel.id)
            .expect("read wiki selection")
            .expect("stored wiki selection");
        let writing_selection = storage
            .read_panel_selection(&writing.project.id, &writing.panel.id)
            .expect("read writing selection")
            .expect("stored writing selection");
        assert_eq!(wiki_selection["isWikiSelected"], json!(true));
        assert_eq!(writing_selection["isWikiSelected"], json!(false));

        let skill_ids = vec![
            "writing-xiaohongshu-note".to_owned(),
            "writing-long-article".to_owned(),
        ];
        let created = create_requests(&paths, "Write a concise report", "create", None, &skill_ids)
            .expect("writing requests");
        assert_eq!(created["tasks"].as_array().unwrap().len(), 2);
        assert_eq!(created["tasks"][0]["queue"], json!("writing"));
        assert_eq!(created["tasks"][0]["capability"], json!(WRITING_CAPABILITY));
        assert_eq!(
            created["tasks"][0]["input"]["instruction"],
            json!("Write a concise report")
        );
        assert_eq!(
            created["tasks"][0]["input"]["writingSkillId"],
            json!("writing-xiaohongshu-note")
        );
        assert_eq!(
            created["state"]["selectedCreateWritingSkillIds"],
            json!(skill_ids)
        );
        assert_eq!(
            created["state"]["selectedRevisionWritingSkillId"],
            json!("writing-default")
        );
        assert_eq!(created["state"]["draft"], json!(""));

        let request = read_request(&paths, created["tasks"][0]["id"].as_str().unwrap())
            .expect("read request");
        assert_eq!(request["writingSkill"]["title"], json!("小红书笔记"));
        assert_eq!(
            request["nextRequiredAction"]["intent"],
            json!("load-writing-skill")
        );
        let loaded = crate::agent::read_agent_skill(
            &paths,
            "writing-xiaohongshu-note",
            Some(created["tasks"][0]["id"].as_str().unwrap()),
        )
        .expect("task Writing Skill");
        assert!(loaded
            .markdown
            .contains("writing skill: writing-xiaohongshu-note"));
    }

    #[test]
    fn writing_skill_registry_and_submission_validation_are_authoritative() {
        let (_temp, paths) = test_paths();
        let initial = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let revision_target = crate::wiki::create_generated_document(
            &paths,
            "target.md",
            Some("Target"),
            Some("text/markdown"),
            None,
            None,
            b"Target",
        )
        .expect("revision target");
        let revision_target_id = revision_target["document"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id.clone()),
            },
        )
        .expect("writing panel");
        let skills = crate::agent::list_writing_agent_skills(&paths).expect("writing skills");
        assert_eq!(
            skills
                .iter()
                .map(|item| item.skill.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "writing-default",
                "writing-long-article",
                "writing-xiaohongshu-note"
            ]
        );
        assert!(skills
            .iter()
            .all(|item| item.skill.id != WRITING_PANEL_SKILL_ID));

        let empty =
            create_requests(&paths, "Write", "create", None, &[]).expect_err("skill required");
        assert_eq!(empty.code(), Some("writing_skill_required"));
        let duplicate_ids = vec![
            "writing-long-article".to_owned(),
            "writing-long-article".to_owned(),
        ];
        let duplicate = create_requests(&paths, "Write", "create", None, &duplicate_ids)
            .expect_err("duplicate skill");
        assert_eq!(duplicate.code(), Some("duplicate_writing_skill"));
        let unknown = create_requests(
            &paths,
            "Write",
            "create",
            None,
            &["writing-unknown".to_owned()],
        )
        .expect_err("unknown skill");
        assert_eq!(unknown.code(), Some("writing_skill_not_found"));
        let multi_revision = create_requests(
            &paths,
            "Revise",
            "revise",
            Some(&revision_target_id),
            &[
                "writing-long-article".to_owned(),
                "writing-xiaohongshu-note".to_owned(),
            ],
        )
        .expect_err("revision limit");
        assert_eq!(multi_revision.code(), Some("writing_revision_skill_limit"));
        assert!(Storage::open(&paths)
            .expect("storage")
            .list_tasks(&initial.project.id)
            .expect("tasks")
            .is_empty());
    }

    #[test]
    fn revision_requires_a_known_generated_document() {
        let (_temp, paths) = test_paths();
        let initial = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id),
            },
        )
        .expect("writing panel");
        let skill_ids = long_article_skill_ids();
        let draft = save_draft(
            &paths,
            "Revise this",
            "revise",
            "",
            None,
            &[
                "writing-default".to_owned(),
                "writing-xiaohongshu-note".to_owned(),
            ],
            skill_ids.first().map(String::as_str),
        )
        .expect("incomplete revision draft");
        assert_eq!(draft["state"]["mode"], json!("revise"));
        assert_eq!(draft["state"]["targetGeneratedDocumentId"], Value::Null);
        assert_eq!(
            draft["state"]["selectedCreateWritingSkillIds"],
            json!(["writing-default", "writing-xiaohongshu-note"])
        );
        assert_eq!(
            draft["state"]["selectedRevisionWritingSkillId"],
            json!("writing-long-article")
        );
        let error = create_requests(
            &paths,
            "Revise it",
            "revise",
            Some("generated:missing"),
            &skill_ids,
        )
        .expect_err("missing target");
        assert_eq!(error.code(), Some("writing_target_not_found"));
    }

    #[test]
    fn claimed_request_generates_into_the_linked_wiki_panel() {
        let (temp, paths) = test_paths();
        let initial = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id.clone()),
            },
        )
        .expect("writing panel");
        let skill_ids = long_article_skill_ids();
        let created = create_requests(&paths, "Write the report", "create", None, &skill_ids)
            .expect("writing request");
        let task_id = created["tasks"][0]["id"].as_str().unwrap();
        let registered = crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "writer",
                host: None,
                transport: "poll",
                endpoint: None,
                capabilities: vec![WRITING_CAPABILITY.to_owned()],
                priority: 0,
            },
        )
        .expect("target");
        let target_id = registered["target"]["id"].as_str().unwrap();
        let claimed = crate::tasks::claim_task(&paths, task_id, target_id).expect("claim");
        let lease_token = claimed["leaseToken"].as_str().unwrap();
        let started = crate::operations::begin_writing(&paths, task_id, "Report", "markdown")
            .expect("generation");
        let operation_id = started["operation"]["id"].as_str().unwrap();
        let early = crate::tasks::complete_task(&paths, task_id, lease_token, None)
            .expect_err("active operation");
        assert_eq!(early.code(), Some("writing_operation_active"));
        let artifact = temp.path().join("report.md");
        fs::write(&artifact, "# Report\n\nDone.\n").expect("artifact");
        let completed =
            crate::operations::complete(&paths, operation_id, artifact.to_str().unwrap(), None)
                .expect("complete operation");
        assert_eq!(completed["document"]["title"], json!("Report"));
        let task = crate::tasks::complete_task(
            &paths,
            task_id,
            lease_token,
            Some(json!({ "generatedDocumentId": completed["document"]["id"] })),
        )
        .expect("complete task");
        assert_eq!(task["task"]["status"], json!("succeeded"));

        let cancelled = create_requests(&paths, "Write another report", "create", None, &skill_ids)
            .expect("second request");
        let cancelled_task_id = cancelled["tasks"][0]["id"].as_str().unwrap();
        crate::tasks::claim_task(&paths, cancelled_task_id, target_id).expect("second claim");
        let cancelled_operation = crate::operations::begin_writing(
            &paths,
            cancelled_task_id,
            "Cancelled report",
            "markdown",
        )
        .expect("second generation");
        let cancelled_operation_id = cancelled_operation["operation"]["id"].as_str().unwrap();
        let cancelled_task =
            crate::tasks::cancel_task(&paths, cancelled_task_id).expect("cancel task");
        assert_eq!(cancelled_task["task"]["status"], json!("cancelled"));
        assert_eq!(
            crate::operations::inspect(&paths, cancelled_operation_id)
                .expect("cancelled operation")["status"],
            json!("cancelled")
        );
    }

    #[test]
    fn revision_rejects_a_target_changed_after_submission() {
        let (_temp, paths) = test_paths();
        let initial = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let document = crate::wiki::create_generated_document(
            &paths,
            "draft.md",
            Some("Draft"),
            Some("text/markdown"),
            None,
            None,
            b"Initial",
        )
        .expect("document");
        let document_id = document["document"]["id"].as_str().unwrap().to_owned();
        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id.clone()),
            },
        )
        .expect("writing panel");
        let created = create_requests(
            &paths,
            "Revise the draft",
            "revise",
            Some(&document_id),
            &long_article_skill_ids(),
        )
        .expect("writing request");
        let task_id = created["tasks"][0]["id"].as_str().unwrap();

        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Wiki),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id),
            },
        )
        .expect("wiki panel");
        crate::wiki::write_generated_document(
            &paths,
            &document_id,
            "draft.md",
            Some("text/markdown"),
            b"Changed after submission",
        )
        .expect("concurrent edit");
        let registered = crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "writer",
                host: None,
                transport: "poll",
                endpoint: None,
                capabilities: vec![WRITING_CAPABILITY.to_owned()],
                priority: 0,
            },
        )
        .expect("target");
        crate::tasks::claim_task(
            &paths,
            task_id,
            registered["target"]["id"].as_str().unwrap(),
        )
        .expect("claim");
        let error = crate::operations::begin_writing(&paths, task_id, "Draft", "markdown")
            .expect_err("content conflict");
        assert_eq!(error.code(), Some("content_conflict"));
    }

    #[test]
    fn refinement_ignores_wiki_and_requires_ready_selected_documents() {
        let (_temp, paths) = test_paths();
        let initial = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let unready = crate::wiki::add_raw_document(
            &paths,
            "sample.pdf",
            Some("Sample PDF"),
            Some("application/pdf"),
            "user",
            None,
            b"not converted yet",
        )
        .expect("unready raw document");
        let unready_id = unready["document"]["id"].as_str().unwrap().to_owned();
        let generated = crate::wiki::create_generated_document(
            &paths,
            "ready.md",
            Some("Ready"),
            Some("text/markdown"),
            None,
            None,
            b"# Ready\n",
        )
        .expect("ready generated document");
        let generated_id = generated["document"]["id"].as_str().unwrap().to_owned();
        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id),
            },
        )
        .expect("writing panel");
        write_selection(&paths, true, &[], &[]).expect("wiki-only selection");
        let missing = create_refinement_request(&paths, "My style").expect_err("source required");
        assert_eq!(missing.code(), Some("writing_refinement_source_required"));

        write_selection(&paths, true, std::slice::from_ref(&unready_id), &[])
            .expect("unready source selected");
        let unready = create_refinement_request(&paths, "My style").expect_err("unready source");
        assert_eq!(unready.code(), Some("writing_refinement_source_not_ready"));

        write_selection(&paths, true, &[], std::slice::from_ref(&generated_id))
            .expect("ready source selected");
        assert_eq!(
            create_refinement_request(&paths, "  ")
                .expect_err("empty name")
                .code(),
            Some("writing_skill_name_required")
        );
        assert_eq!(
            create_refinement_request(&paths, &"x".repeat(81))
                .expect_err("long name")
                .code(),
            Some("writing_skill_name_too_long")
        );
        assert_eq!(
            create_refinement_request(&paths, "默认写作")
                .expect_err("built-in conflict")
                .code(),
            Some("writing_skill_name_conflict")
        );
        create_refinement_request(&paths, "My Style").expect("valid refinement");
        assert_eq!(
            create_refinement_request(&paths, " my style ")
                .expect_err("pending conflict")
                .code(),
            Some("writing_skill_name_conflict")
        );
    }

    #[test]
    fn refinement_installs_a_project_scoped_writing_skill() {
        let (temp, paths) = test_paths();
        let initial = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let generated = crate::wiki::create_generated_document(
            &paths,
            "sample.md",
            Some("Sample"),
            Some("text/markdown"),
            None,
            None,
            b"# Sample\n\nShort, direct paragraphs.",
        )
        .expect("generated document");
        let generated_id = generated["document"]["id"].as_str().unwrap().to_owned();
        let second_generated = crate::wiki::create_generated_document(
            &paths,
            "second-sample.md",
            Some("Second sample"),
            Some("text/markdown"),
            None,
            None,
            b"# Second sample\n\nA second reusable example.",
        )
        .expect("second generated document");
        let second_generated_id = second_generated["document"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id.clone()),
            },
        )
        .expect("writing panel");
        write_selection(
            &paths,
            true,
            &[],
            &[generated_id.clone(), second_generated_id.clone()],
        )
        .expect("writing selection");

        let created =
            create_refinement_request(&paths, "Concise House Style").expect("refinement request");
        let task = &created["task"];
        assert_eq!(task["type"], json!("refine_writing_skill"));
        assert_eq!(task["capability"], json!(WRITING_REFINEMENT_CAPABILITY));
        assert!(task["input"]["context"].get("isWikiSelected").is_none());
        assert_eq!(
            task["input"]["context"]["selectedGeneratedDocumentIds"],
            json!([generated_id, second_generated_id])
        );
        let duplicate = create_refinement_request(&paths, " concise house style ")
            .expect_err("pending name conflict");
        assert_eq!(duplicate.code(), Some("writing_skill_name_conflict"));

        let task_id = task["id"].as_str().unwrap();
        let skill_id = task["input"]["skillId"].as_str().unwrap();
        let registered = crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "refiner",
                host: None,
                transport: "poll",
                endpoint: None,
                capabilities: vec![WRITING_REFINEMENT_CAPABILITY.to_owned()],
                priority: 0,
            },
        )
        .expect("target");
        let claimed = crate::tasks::claim_task(
            &paths,
            task_id,
            registered["target"]["id"].as_str().unwrap(),
        )
        .expect("claim");
        let lease = claimed["leaseToken"].as_str().unwrap();
        let early = crate::tasks::complete_task(&paths, task_id, lease, None)
            .expect_err("skill must be installed");
        assert_eq!(early.code(), Some("writing_skill_not_installed"));

        let skill_file = temp.path().join("SKILL.md");
        fs::write(
            &skill_file,
            format!(
                "---\nid: {skill_id}\ntitle: Concise House Style\ndescription: Write with concise, direct paragraphs.\nsource: project\nappliesTo:\n  - writing\ntaskTypes:\n  - generate_document\nrequiresCapabilities:\nloadWhen:\n  - The task requests the project house style.\ntokens: short\n---\n\nUse short, direct paragraphs and remove redundant setup.\n"
            ),
        )
        .expect("skill file");
        let installed =
            install_project_skill(&paths, task_id, skill_file.to_str().unwrap()).expect("install");
        assert_eq!(installed["skill"]["source"], json!("project"));
        install_project_skill(&paths, task_id, skill_file.to_str().unwrap())
            .expect("idempotent install");
        assert!(crate::agent::writing_agent_skill(&paths, skill_id).is_err());
        crate::tasks::complete_task(&paths, task_id, lease, None).expect("complete task");

        let project_skill =
            crate::agent::writing_agent_skill(&paths, skill_id).expect("project Writing Skill");
        assert!(Path::new(&project_skill.local_path)
            .components()
            .any(|component| component.as_os_str() == "projects"));
        create_requests(
            &paths,
            "Write with the extracted method",
            "create",
            None,
            &[skill_id.to_owned()],
        )
        .expect("use project skill");
        crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "writer",
                host: None,
                transport: "poll",
                endpoint: None,
                capabilities: vec![WRITING_CAPABILITY.to_owned()],
                priority: 0,
            },
        )
        .expect("writer target");
        let pending =
            crate::agent_control::pending_entry_skill_update(&paths, env!("CARGO_PKG_VERSION"))
                .expect("entry skill requirement")
                .expect("pending entry skill update");
        crate::agent_control::acknowledge_entry_skill_update(
            &paths,
            &pending.event_id,
            crate::agent_control::ENTRY_SKILL_VERSION,
        )
        .expect("acknowledge entry skill");
        let agent_bootstrap = crate::agent::agent_bootstrap(&paths, env!("CARGO_PKG_VERSION"))
            .expect("agent bootstrap");
        let bootstrap_loads_project_skill = agent_bootstrap["nextRequiredAction"]["steps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|step| step["skills"].as_array())
            .flatten()
            .any(|skill| skill["id"].as_str() == Some(skill_id));
        assert!(
            bootstrap_loads_project_skill,
            "bootstrap did not load the project skill: {agent_bootstrap:#}"
        );

        let other = crate::control::create_project(&paths, Some("Other")).expect("other project");
        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(other.project.id),
            },
        )
        .expect("other writing panel");
        assert!(crate::agent::list_writing_agent_skills(&paths)
            .expect("other skills")
            .iter()
            .all(|item| item.skill.id != skill_id));
    }
}
