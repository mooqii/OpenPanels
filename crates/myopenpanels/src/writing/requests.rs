use crate::control::{now_iso, read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::MyOpenPanelsPaths;
use crate::storage::{Storage, TaskInsert};
use crate::types::{PanelKind, ProjectBootstrap, ProjectPanelSnapshot};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

const WRITING_TASK_CAPABILITY_KEY: &str = "writing.execute";
const WRITING_DISTILLATION_TASK_CAPABILITY_KEY: &str = "skill.writing.distill";

#[cfg(test)]
fn writing_task_capability(capability_key: &str, task_type: &str) -> &'static str {
    &crate::capabilities::task_route_for_capability(capability_key, task_type)
        .expect("Writing Task route")
        .capability
}
pub const DEFAULT_WRITING_DISTILLATION_SKILL_ID: &str = "writing-distillation-default";

fn active_writing_selection(paths: &MyOpenPanelsPaths) -> Result<ProjectBootstrap, CliError> {
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

fn writing_panel(paths: &MyOpenPanelsPaths) -> Result<ProjectBootstrap, CliError> {
    let mut request = BootstrapRequest::new();
    request.requested_panel_kind = Some(PanelKind::Writing);
    read_project_bootstrap(paths, request)
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
    let my_document_ids = known_selected_ids(
        &stored,
        "selectedMyDocumentIds",
        &wiki.state,
        "myDocuments",
    );
    let is_wiki_selected = stored
        .get("isWikiSelected")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let agent_context = crate::wiki::agent_content_context(
        paths,
        &bootstrap.project.id,
        &wiki.panel.id,
        &my_document_ids,
        is_wiki_selected,
    )?;
    Ok(json!({
        "kind": "writing",
        "projectId": bootstrap.project.id,
        "panelId": bootstrap.panel.id,
        "isWikiSelected": is_wiki_selected,
        "selectedMyDocumentIds": my_document_ids,
        "selectedMyDocuments": agent_context["selectedMyDocuments"],
        "wiki": agent_context["wiki"],
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
    let bootstrap = active_writing_selection(paths)?;
    Ok(json!({ "selection": writing_selection_value(paths, &bootstrap)? }))
}

pub fn write_selection(
    paths: &MyOpenPanelsPaths,
    is_wiki_selected: bool,
    selected_my_document_ids: &[String],
) -> Result<Value, CliError> {
    let bootstrap = writing_panel(paths)?;
    let requested = json!({
        "isWikiSelected": is_wiki_selected,
        "selectedMyDocumentIds": selected_my_document_ids,
    });
    let wiki = wiki_snapshot(&bootstrap)?;
    let selection = json!({
        "kind": "writing",
        "projectId": bootstrap.project.id,
        "panelId": bootstrap.panel.id,
        "isWikiSelected": is_wiki_selected,
        "selectedMyDocumentIds": known_selected_ids(&requested, "selectedMyDocumentIds", &wiki.state, "myDocuments"),
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
    distillation_name: &str,
    target_my_document_id: Option<&str>,
    selected_create_writing_skill_ids: &[String],
    selected_revision_writing_skill_id: Option<&str>,
) -> Result<Value, CliError> {
    save_draft_with_distillation_skill(
        paths,
        draft,
        mode,
        distillation_name,
        target_my_document_id,
        selected_create_writing_skill_ids,
        selected_revision_writing_skill_id,
        None,
        None,
        None,
    )
}

pub fn save_draft_with_distillation_skill(
    paths: &MyOpenPanelsPaths,
    draft: &str,
    mode: &str,
    distillation_name: &str,
    target_my_document_id: Option<&str>,
    selected_create_writing_skill_ids: &[String],
    selected_revision_writing_skill_id: Option<&str>,
    selected_distillation_skill_id: Option<&str>,
    create_draft: Option<&str>,
    revision_draft: Option<&str>,
) -> Result<Value, CliError> {
    let bootstrap = writing_panel(paths)?;
    validate_mode(
        mode,
        target_my_document_id,
        wiki_snapshot(&bootstrap)?,
        false,
    )?;
    validate_writing_skills(paths, selected_create_writing_skill_ids, false, "create")?;
    let revision_skill_ids = selected_revision_writing_skill_id
        .map(|id| vec![id.to_owned()])
        .unwrap_or_default();
    validate_writing_skills(paths, &revision_skill_ids, false, "revise")?;
    let distillation_skill_id = selected_distillation_skill_id
        .or_else(|| bootstrap.state["selectedDistillationSkillId"].as_str())
        .unwrap_or(DEFAULT_WRITING_DISTILLATION_SKILL_ID);
    crate::agent::writing_distillation_agent_skill(paths, distillation_skill_id)?;
    let create_draft = create_draft
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if mode == "create" {
                draft.to_owned()
            } else {
                saved_mode_draft(&bootstrap.state, "createDraft")
            }
        });
    let revision_draft = revision_draft
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if mode == "revise" {
                draft.to_owned()
            } else {
                saved_mode_draft(&bootstrap.state, "revisionDraft")
            }
        });
    let state = json!({
        "createDraft": create_draft,
        "draft": draft,
        "mode": mode,
        "distillationName": distillation_name,
        "revisionDraft": revision_draft,
        "targetMyDocumentId": if mode == "revise" { target_my_document_id } else { None },
        "selectedCreateWritingSkillIds": selected_create_writing_skill_ids,
        "selectedRevisionWritingSkillId": selected_revision_writing_skill_id,
        "selectedDistillationSkillId": distillation_skill_id,
    });
    let revision = Storage::open(paths)?.write_panel_state(
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &state,
    )?;
    Ok(json!({ "state": state, "revision": revision }))
}

fn saved_mode_draft(state: &Value, field: &str) -> String {
    state
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_default()
}

fn validate_mode(
    mode: &str,
    target_my_document_id: Option<&str>,
    wiki: &ProjectPanelSnapshot,
    target_required: bool,
) -> Result<(), CliError> {
    if !matches!(mode, "create" | "revise" | "distill") {
        return Err(CliError::with_code(
            "invalid_writing_mode",
            "Writing mode must be create, revise, or distill.",
        ));
    }
    if mode == "revise" {
        let target = target_my_document_id.filter(|id| !id.trim().is_empty());
        if target_required && target.is_none() {
            return Err(CliError::with_code(
                "writing_target_required",
                "Revision mode requires a My Document target.",
            ));
        }
        if let Some(target) = target {
            let exists = wiki
                .state
                .get("myDocuments")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|document| document.get("id").and_then(Value::as_str) == Some(target));
            if !exists {
                return Err(CliError::with_code(
                    "writing_target_not_found",
                    format!("My Document not found: {target}"),
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
    target_my_document_id: Option<&str>,
    writing_skill_ids: &[String],
) -> Result<Value, CliError> {
    let instruction = instruction.trim();
    if instruction.is_empty() {
        return Err(CliError::with_code(
            "writing_instruction_required",
            "Writing instruction cannot be empty.",
        ));
    }
    let bootstrap = writing_panel(paths)?;
    let wiki = wiki_snapshot(&bootstrap)?;
    if !matches!(mode, "create" | "revise") {
        return Err(CliError::with_code(
            "invalid_writing_mode",
            "Document requests must use create or revise mode.",
        ));
    }
    validate_mode(mode, target_my_document_id, wiki, true)?;
    crate::agent::sync_builtin_agent_skills(paths)?;
    let writing_skills = validate_writing_skills(paths, writing_skill_ids, true, mode)?;
    let selection = writing_selection_value(paths, &bootstrap)?;
    let wiki_space_id = wiki
        .state
        .get("activeWikiSpaceId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CliError::new("Writing request has no active Wiki Space."))?;
    let storage = Storage::open(paths)?;
    let context_snapshot = capture_writing_context_snapshot(paths, wiki, &selection)?;
    let now = now_iso();
    let mut wiki_state = wiki.state.clone();
    let mut targets = Vec::with_capacity(writing_skills.len());

    if mode == "create" {
        if !wiki_state["myDocuments"].is_array() {
            return Err(CliError::new("Wiki myDocuments state is invalid."));
        }
        let mut placeholders = Vec::with_capacity(writing_skills.len());
        for _ in &writing_skills {
            let task_id = crate::ids::random_id("task");
            let document_id = crate::ids::random_id("my-document");
            let document = json!({
                "id": document_id,
                "title": "",
                "originalFileName": "untitled.md",
                "format": "markdown",
                "mimeType": "text/markdown",
                "contentRef": "content.md",
                "contentVersion": 0,
                "taskId": task_id,
                "threadId": null,
                "publishHistory": [],
                "wordCount": 0,
                "createdAt": now,
                "updatedAt": now,
            });
            let snapshot =
                snapshot_writing_document(paths, &bootstrap.project.id, &document);
            placeholders.push(document.clone());
            targets.push((task_id, document_id, 0_u64, document, snapshot));
        }
        let documents = wiki_state
            .get_mut("myDocuments")
            .and_then(Value::as_array_mut)
            .expect("myDocuments was validated above");
        for document in placeholders.into_iter().rev() {
            documents.insert(0, document);
        }
    } else {
        let target_id = target_my_document_id.unwrap_or_default();
        let document = wiki
            .state
            .get("myDocuments")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|document| document.get("id").and_then(Value::as_str) == Some(target_id))
            .cloned()
            .ok_or_else(|| {
                CliError::with_code(
                    "writing_target_not_found",
                    format!("My Document not found: {target_id}"),
                )
            })?;
        let target_version = document["contentVersion"].as_u64().unwrap_or(0);
        let snapshot = snapshot_writing_document(paths, &bootstrap.project.id, &document);
        targets.push((
            crate::ids::random_id("task"),
            target_id.to_owned(),
            target_version,
            document,
            snapshot,
        ));
    }
    let task_inserts = writing_skills
        .iter()
        .zip(&targets)
        .map(|(listing, (task_id, target_id, target_version, _, target_snapshot))| {
            let skill_markdown = fs::read_to_string(&listing.local_path).unwrap_or_default();
            TaskInsert::for_capability(
                WRITING_TASK_CAPABILITY_KEY,
                "write_my_document",
                task_id.clone(),
                target_id.clone(),
                json!({
                    "instruction": instruction,
                    "mode": mode,
                    "targetMyDocumentId": target_id,
                    "targetContentVersion": target_version,
                    "targetDocumentSnapshot": target_snapshot,
                    "writingSkillId": listing.skill.id.clone(),
                    "writingSkill": {
                        "id": listing.skill.id.clone(),
                        "name": listing.skill.name.clone(),
                        "description": listing.skill.description.clone(),
                        "source": listing.source.clone(),
                    },
                    "writingSkillSnapshot": {
                        "id": listing.skill.id.clone(),
                        "markdown": skill_markdown,
                        "contentHash": format!("{:x}", Sha256::digest(skill_markdown.as_bytes())),
                    },
                    "contextSnapshot": context_snapshot,
                    "context": {
                        "isWikiSelected": selection["isWikiSelected"],
                        "selectedMyDocumentIds": selection["selectedMyDocumentIds"],
                    },
                }),
                json!({
                    "writingPanelId": bootstrap.panel.id,
                    "wikiPanelId": wiki.panel.id,
                    "wikiSpaceId": wiki_space_id,
                    "panelSkillId": crate::agent::PANELS_SKILL_ID,
                    "writingSkillId": listing.skill.id.clone(),
                    "writingSkillSource": listing.source.clone(),
                }),
            )
        })
        .collect::<Result<Vec<_>, CliError>>()?;
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
    let create_draft = if mode == "create" {
        String::new()
    } else {
        saved_mode_draft(&bootstrap.state, "createDraft")
    };
    let revision_draft = if mode == "revise" {
        String::new()
    } else {
        saved_mode_draft(&bootstrap.state, "revisionDraft")
    };
    let state = json!({
        "createDraft": create_draft,
        "draft": "",
        "mode": mode,
        "distillationName": bootstrap.state.get("distillationName").and_then(Value::as_str).unwrap_or(""),
        "revisionDraft": revision_draft,
        "targetMyDocumentId": null,
        "selectedCreateWritingSkillIds": selected_create_writing_skill_ids,
        "selectedRevisionWritingSkillId": selected_revision_writing_skill_id,
        "selectedDistillationSkillId": bootstrap.state.get("selectedDistillationSkillId").cloned().unwrap_or_else(|| json!(DEFAULT_WRITING_DISTILLATION_SKILL_ID)),
    });
    let mut panel_states = vec![(bootstrap.panel.id.as_str(), &state)];
    if mode == "create" {
        panel_states.push((wiki.panel.id.as_str(), &wiki_state));
    }
    let inserted = storage.insert_tasks_with_panel_states(
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &task_inserts,
        &panel_states,
    );
    let (tasks, revisions) = inserted?;
    let documents = targets
        .into_iter()
        .map(|(_, _, _, document, _)| document)
        .collect::<Vec<_>>();
    Ok(json!({
        "tasks": tasks,
        "documents": documents,
        "state": state,
        "revision": revisions.first().copied().unwrap_or(bootstrap.revision),
    }))
}

fn capture_writing_context_snapshot(
    paths: &MyOpenPanelsPaths,
    wiki: &ProjectPanelSnapshot,
    selection: &Value,
) -> Result<Value, CliError> {
    let selected = selection
        .get("selectedMyDocumentIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    let my_documents = wiki
        .state
        .get("myDocuments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|document| {
            document
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| selected.contains(id))
        })
        .map(|document| snapshot_writing_document(paths, &wiki.panel.project_id, document))
        .collect::<Vec<_>>();
    let wiki_space_id = wiki
        .state
        .get("activeWikiSpaceId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CliError::new("Writing context has no active Wiki Space."))?;
    let wiki_selected = selection
        .get("isWikiSelected")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let wiki_content_revision_id = if wiki_selected {
        crate::content::active_resource_descriptor(
            paths,
            &wiki.panel.project_id,
            crate::content::ResourceKind::WikiSpace,
            wiki_space_id,
        )?
        .and_then(|descriptor| {
            descriptor
                .get("revisionId")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
    } else {
        None
    };
    Ok(json!({
        "wikiRevision": wiki.revision,
        "wikiSpaceId": wiki_space_id,
        "wikiSelection": {
            "selected": wiki_selected,
            "wikiSpaceId": wiki_space_id,
            "contentRevisionId": wiki_content_revision_id,
        },
        "myDocuments": my_documents,
    }))
}

fn snapshot_writing_document(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    document: &Value,
) -> Value {
    let mut snapshot = document.clone();
    let document_id = document.get("id").and_then(Value::as_str).unwrap_or_default();
    let logical_path = document
        .get("contentRef")
        .and_then(Value::as_str)
        .unwrap_or("content.md");
    let content = crate::content::read_active_text(
        paths,
        project_id,
        crate::content::ResourceKind::MyDocument,
        document_id,
        logical_path,
    )
    .ok()
    .flatten()
    .unwrap_or_default();
    snapshot["snapshotContent"] = json!(content);
    snapshot["snapshotHash"] = json!(format!("{:x}", Sha256::digest(content.as_bytes())));
    snapshot
}

pub fn create_distillation_request(
    paths: &MyOpenPanelsPaths,
    requested_name: &str,
) -> Result<Value, CliError> {
    create_distillation_request_with_skill(paths, requested_name, None)
}

pub fn create_distillation_request_with_skill(
    paths: &MyOpenPanelsPaths,
    requested_name: &str,
    requested_distiller_skill_id: Option<&str>,
) -> Result<Value, CliError> {
    let name = normalize_skill_name(requested_name)?;
    let bootstrap = writing_panel(paths)?;
    let wiki = wiki_snapshot(&bootstrap)?;
    let selection = writing_selection_value(paths, &bootstrap)?;
    let my_document_ids = selection["selectedMyDocumentIds"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if my_document_ids.is_empty() {
        return Err(CliError::with_code(
            "writing_distillation_source_required",
            "Select at least one My Document to distill.",
        ));
    }
    validate_distillation_sources(wiki, &my_document_ids)?;
    validate_available_skill_name(paths, &bootstrap.project.id, &name, None)?;

    let captured = capture_writing_context_snapshot(paths, wiki, &selection)?;
    let context_snapshot = json!({
        "myDocuments": captured["myDocuments"],
    });
    crate::agent::sync_builtin_agent_skills(paths)?;
    let distiller_skill_id = requested_distiller_skill_id
        .or_else(|| bootstrap.state["selectedDistillationSkillId"].as_str())
        .unwrap_or(DEFAULT_WRITING_DISTILLATION_SKILL_ID);
    let distiller = crate::agent::writing_distillation_agent_skill(paths, distiller_skill_id)?;
    let distiller_markdown = fs::read_to_string(PathBuf::from(&distiller.local_dir).join("SKILL.md"))
    .map_err(|error| {
        CliError::with_code(
            "invalid_skill_package",
            format!("Could not capture the Writing Skill distiller: {error}"),
        )
    })?;
    let distiller_content_hash = format!("{:x}", Sha256::digest(distiller_markdown.as_bytes()));

    let suffix = crate::ids::random_base64url_96()
        .to_ascii_lowercase()
        .replace(['_', '-'], "x");
    let skill_id = format!("writing-custom-{suffix}");
    let input = json!({
        "name": name,
        "skillId": skill_id,
        "distillerSkillId": distiller_skill_id,
        "distillerSkillSnapshot": {
            "id": distiller_skill_id,
            "name": distiller.skill.name,
            "source": distiller.source,
            "markdown": distiller_markdown,
            "contentHash": distiller_content_hash,
        },
        "context": {
            "selectedMyDocumentIds": my_document_ids,
        },
        "contextSnapshot": context_snapshot,
    });
    let source = json!({
        "writingPanelId": bootstrap.panel.id,
        "wikiPanelId": wiki.panel.id,
        "panelSkillId": crate::agent::PANELS_SKILL_ID,
        "distillerSkillId": distiller_skill_id,
    });
    let task = Storage::open(paths)?.insert_capability_task(
        &bootstrap.project.id,
        &bootstrap.panel.id,
        WRITING_DISTILLATION_TASK_CAPABILITY_KEY,
        "distill_writing_skill",
        &skill_id,
        &input,
        &source,
    )?;
    let state = json!({
        "createDraft": saved_mode_draft(&bootstrap.state, "createDraft"),
        "draft": bootstrap.state.get("draft").and_then(Value::as_str).unwrap_or(""),
        "mode": "distill",
        "distillationName": "",
        "revisionDraft": saved_mode_draft(&bootstrap.state, "revisionDraft"),
        "targetMyDocumentId": bootstrap.state.get("targetMyDocumentId").cloned().unwrap_or(Value::Null),
        "selectedCreateWritingSkillIds": bootstrap.state.get("selectedCreateWritingSkillIds").cloned().unwrap_or_else(|| json!([])),
        "selectedRevisionWritingSkillId": bootstrap.state.get("selectedRevisionWritingSkillId").cloned().unwrap_or(Value::Null),
        "selectedDistillationSkillId": distiller_skill_id,
    });
    let revision = Storage::open(paths)?.write_panel_state(
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &state,
    )?;
    Ok(json!({ "task": task, "state": state, "revision": revision }))
}
