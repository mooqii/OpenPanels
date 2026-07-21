use crate::control::{now_iso, read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::{sanitize_path_part, MyOpenPanelsPaths};
use crate::storage::{Storage, TaskInsert};
use crate::types::{PanelKind, ProjectBootstrap, ProjectPanelSnapshot};
use rusqlite::OptionalExtension;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const WRITING_CAPABILITY: &str = "writing.generateDocument";
pub const WRITING_REFINEMENT_CAPABILITY: &str = "writing.refineSkill";
pub const WRITING_SKILL_REFINER_ID: &str = "writing-skill-refiner";

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
    let is_wiki_selected = stored
        .get("isWikiSelected")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let agent_context = crate::wiki::agent_content_context(
        paths,
        &bootstrap.project.id,
        &wiki.panel.id,
        &raw_ids,
        &generated_ids,
        is_wiki_selected,
    )?;
    Ok(json!({
        "kind": "writing",
        "projectId": bootstrap.project.id,
        "panelId": bootstrap.panel.id,
        "isWikiSelected": is_wiki_selected,
        "selectedRawDocumentIds": raw_ids,
        "selectedGeneratedDocumentIds": generated_ids,
        "selectedRawDocuments": agent_context["selectedRawDocuments"],
        "selectedGeneratedDocuments": agent_context["selectedGeneratedDocuments"],
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
    selected_raw_document_ids: &[String],
    selected_generated_document_ids: &[String],
) -> Result<Value, CliError> {
    let bootstrap = writing_panel(paths)?;
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
    save_draft_with_refinement_skill(
        paths,
        draft,
        mode,
        refinement_name,
        target_generated_document_id,
        selected_create_writing_skill_ids,
        selected_revision_writing_skill_id,
        None,
        None,
        None,
    )
}

pub fn save_draft_with_refinement_skill(
    paths: &MyOpenPanelsPaths,
    draft: &str,
    mode: &str,
    refinement_name: &str,
    target_generated_document_id: Option<&str>,
    selected_create_writing_skill_ids: &[String],
    selected_revision_writing_skill_id: Option<&str>,
    selected_refinement_skill_id: Option<&str>,
    create_draft: Option<&str>,
    revision_draft: Option<&str>,
) -> Result<Value, CliError> {
    let bootstrap = writing_panel(paths)?;
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
    let refinement_skill_id = selected_refinement_skill_id
        .or_else(|| bootstrap.state["selectedRefinementSkillId"].as_str())
        .unwrap_or(WRITING_SKILL_REFINER_ID);
    crate::agent::writing_refinement_agent_skill(paths, refinement_skill_id)?;
    let create_draft = create_draft
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if mode == "create" {
                draft.to_owned()
            } else {
                saved_mode_draft(&bootstrap.state, "createDraft", "create")
            }
        });
    let revision_draft = revision_draft
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if mode == "revise" {
                draft.to_owned()
            } else {
                saved_mode_draft(&bootstrap.state, "revisionDraft", "revise")
            }
        });
    let state = json!({
        "schemaVersion": 5,
        "createDraft": create_draft,
        "draft": draft,
        "mode": mode,
        "refinementName": refinement_name,
        "revisionDraft": revision_draft,
        "targetGeneratedDocumentId": if mode == "revise" { target_generated_document_id } else { None },
        "selectedCreateWritingSkillIds": selected_create_writing_skill_ids,
        "selectedRevisionWritingSkillId": selected_revision_writing_skill_id,
        "selectedRefinementSkillId": refinement_skill_id,
    });
    let revision = Storage::open(paths)?.write_panel_state(
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &state,
    )?;
    Ok(json!({ "state": state, "revision": revision }))
}

fn saved_mode_draft(state: &Value, field: &str, legacy_mode: &str) -> String {
    state
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| {
            let mode = state.get("mode").and_then(Value::as_str);
            if mode == Some(legacy_mode) || (legacy_mode == "create" && mode == Some("refine")) {
                state
                    .get("draft")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            } else {
                String::new()
            }
        })
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
    let bootstrap = writing_panel(paths)?;
    let wiki = wiki_snapshot(&bootstrap)?;
    if !matches!(mode, "create" | "revise") {
        return Err(CliError::with_code(
            "invalid_writing_mode",
            "Document requests must use create or revise mode.",
        ));
    }
    validate_mode(mode, target_generated_document_id, wiki, true)?;
    crate::agent::sync_builtin_agent_skills(paths)?;
    let writing_skills = validate_writing_skills(paths, writing_skill_ids, true, mode)?;
    let selection = writing_selection_value(paths, &bootstrap)?;
    let wiki_space_id = wiki
        .state
        .get("activeWikiSpaceId")
        .and_then(Value::as_str)
        .unwrap_or("wiki:default");
    let storage = Storage::open(paths)?;
    let context_snapshot = capture_writing_context_snapshot(paths, wiki, &selection)?;
    let now = now_iso();
    let mut wiki_state = wiki.state.clone();
    let mut created_placeholder_dirs = Vec::new();
    let mut targets = Vec::with_capacity(writing_skills.len());

    if mode == "create" {
        if !wiki_state["generatedDocuments"].is_array() {
            return Err(CliError::new("Wiki generatedDocuments state is invalid."));
        }
        let panel_dir = storage.panel_dir(&bootstrap.project.id, &wiki.panel.id);
        let mut placeholders = Vec::with_capacity(writing_skills.len());
        for _ in &writing_skills {
            let task_id = crate::ids::random_id("task");
            let document_id = crate::ids::random_id("generated");
            let document_dir = panel_dir
                .join("generated")
                .join(sanitize_path_part(&document_id));
            if let Err(error) = fs::create_dir_all(&document_dir)
                .and_then(|_| fs::write(document_dir.join("content.md"), b""))
            {
                cleanup_placeholder_dirs(&created_placeholder_dirs);
                return Err(to_cli_error(error));
            }
            created_placeholder_dirs.push(document_dir);
            let content_ref = format!("generated/{}/content.md", sanitize_path_part(&document_id));
            let document = json!({
                "id": document_id,
                "title": "",
                "originalFileName": "untitled.md",
                "format": "markdown",
                "mimeType": "text/markdown",
                "contentRef": content_ref,
                "contentVersion": 0,
                "taskId": task_id,
                "threadId": null,
                "publishHistory": [],
                "wordCount": 0,
                "createdAt": now,
                "updatedAt": now,
            });
            let snapshot = snapshot_writing_document(&panel_dir, &document, "contentRef");
            placeholders.push(document.clone());
            targets.push((task_id, document_id, 0_u64, document, snapshot));
        }
        let documents = wiki_state
            .get_mut("generatedDocuments")
            .and_then(Value::as_array_mut)
            .expect("generatedDocuments was validated above");
        for document in placeholders.into_iter().rev() {
            documents.insert(0, document);
        }
    } else {
        let target_id = target_generated_document_id.unwrap_or_default();
        let document = wiki
            .state
            .get("generatedDocuments")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|document| document.get("id").and_then(Value::as_str) == Some(target_id))
            .cloned()
            .ok_or_else(|| {
                CliError::with_code(
                    "writing_target_not_found",
                    format!("Generated document not found: {target_id}"),
                )
            })?;
        let target_version = document["contentVersion"].as_u64().unwrap_or(0);
        let panel_dir = storage.panel_dir(&bootstrap.project.id, &wiki.panel.id);
        let snapshot = snapshot_writing_document(&panel_dir, &document, "contentRef");
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
            TaskInsert {
                id: task_id.clone(),
                queue: "writing".to_owned(),
                task_type: "generate_document".to_owned(),
                capability: WRITING_CAPABILITY.to_owned(),
                target_ref: target_id.clone(),
                input: json!({
                    "instruction": instruction,
                    "mode": mode,
                    "targetGeneratedDocumentId": target_id,
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
                        "selectedRawDocumentIds": selection["selectedRawDocumentIds"],
                        "selectedGeneratedDocumentIds": selection["selectedGeneratedDocumentIds"],
                    },
                }),
                source: json!({
                    "writingPanelId": bootstrap.panel.id,
                    "wikiPanelId": wiki.panel.id,
                    "wikiSpaceId": wiki_space_id,
                    "panelSkillId": crate::agent::PANELS_SKILL_ID,
                    "writingSkillId": listing.skill.id.clone(),
                    "writingSkillSource": listing.source.clone(),
                }),
                max_attempts: 8,
                dispatch_mode: "auto".to_owned(),
                idempotency_key: None,
            }
        })
        .collect::<Vec<_>>();
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
        saved_mode_draft(&bootstrap.state, "createDraft", "create")
    };
    let revision_draft = if mode == "revise" {
        String::new()
    } else {
        saved_mode_draft(&bootstrap.state, "revisionDraft", "revise")
    };
    let state = json!({
        "schemaVersion": 5,
        "createDraft": create_draft,
        "draft": "",
        "mode": mode,
        "refinementName": bootstrap.state.get("refinementName").and_then(Value::as_str).unwrap_or(""),
        "revisionDraft": revision_draft,
        "targetGeneratedDocumentId": null,
        "selectedCreateWritingSkillIds": selected_create_writing_skill_ids,
        "selectedRevisionWritingSkillId": selected_revision_writing_skill_id,
        "selectedRefinementSkillId": bootstrap.state.get("selectedRefinementSkillId").cloned().unwrap_or_else(|| json!(WRITING_SKILL_REFINER_ID)),
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
    let (tasks, revisions) = match inserted {
        Ok(value) => value,
        Err(error) => {
            cleanup_placeholder_dirs(&created_placeholder_dirs);
            return Err(error);
        }
    };
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

fn cleanup_placeholder_dirs(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_dir_all(path);
    }
}

fn capture_writing_context_snapshot(
    paths: &MyOpenPanelsPaths,
    wiki: &ProjectPanelSnapshot,
    selection: &Value,
) -> Result<Value, CliError> {
    let storage = Storage::open(paths)?;
    let panel_dir = storage.panel_dir(&wiki.panel.project_id, &wiki.panel.id);
    let capture = |collection: &str, selected_key: &str, reference_key: &str| {
        let selected = selection
            .get(selected_key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<BTreeSet<_>>();
        wiki.state
            .get(collection)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|document| {
                document
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| selected.contains(id))
            })
            .map(|document| snapshot_writing_document(&panel_dir, document, reference_key))
            .collect::<Vec<_>>()
    };
    let wiki_space_id = wiki
        .state
        .get("activeWikiSpaceId")
        .and_then(Value::as_str)
        .unwrap_or("wiki:default");
    let wiki_selected = selection
        .get("isWikiSelected")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let wiki_content_revision_id = if wiki_selected {
        storage
            .connection()
            .query_row(
                "SELECT active_revision_id FROM content_resources WHERE project_id = ? AND resource_kind = 'wiki_space' AND resource_key = ? AND archived_at IS NULL",
                rusqlite::params![wiki.panel.project_id, wiki_space_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(to_cli_error)?
            .flatten()
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
        "rawDocuments": capture("rawDocuments", "selectedRawDocumentIds", "markdownRef"),
        "generatedDocuments": capture("generatedDocuments", "selectedGeneratedDocumentIds", "contentRef"),
    }))
}

fn snapshot_writing_document(panel_dir: &Path, document: &Value, reference_key: &str) -> Value {
    let mut snapshot = document.clone();
    let content = document
        .get(reference_key)
        .and_then(Value::as_str)
        .and_then(|reference| fs::read_to_string(panel_dir.join(reference)).ok())
        .unwrap_or_default();
    snapshot["snapshotContent"] = json!(content);
    snapshot["snapshotHash"] = json!(format!("{:x}", Sha256::digest(content.as_bytes())));
    snapshot
}

pub fn create_refinement_request(
    paths: &MyOpenPanelsPaths,
    requested_name: &str,
) -> Result<Value, CliError> {
    create_refinement_request_with_skill(paths, requested_name, None)
}

pub fn create_refinement_request_with_skill(
    paths: &MyOpenPanelsPaths,
    requested_name: &str,
    requested_refiner_skill_id: Option<&str>,
) -> Result<Value, CliError> {
    let name = normalize_skill_name(requested_name)?;
    let bootstrap = writing_panel(paths)?;
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

    let captured = capture_writing_context_snapshot(paths, wiki, &selection)?;
    let context_snapshot = json!({
        "rawDocuments": captured["rawDocuments"],
        "generatedDocuments": captured["generatedDocuments"],
    });
    crate::agent::sync_builtin_agent_skills(paths)?;
    let refiner_skill_id = requested_refiner_skill_id
        .or_else(|| bootstrap.state["selectedRefinementSkillId"].as_str())
        .unwrap_or(WRITING_SKILL_REFINER_ID);
    let refiner = crate::agent::writing_refinement_agent_skill(paths, refiner_skill_id)?;
    let refiner_markdown = fs::read_to_string(PathBuf::from(&refiner.local_dir).join("SKILL.md"))
    .map_err(|error| {
        CliError::with_code(
            "invalid_skill_package",
            format!("Could not capture the Writing Skill refiner: {error}"),
        )
    })?;
    let refiner_content_hash = format!("{:x}", Sha256::digest(refiner_markdown.as_bytes()));

    let suffix = crate::ids::random_base64url_96()
        .to_ascii_lowercase()
        .replace(['_', '-'], "x");
    let skill_id = format!("writing-custom-{suffix}");
    let input = json!({
        "name": name,
        "skillId": skill_id,
        "refinerSkillId": refiner_skill_id,
        "refinerSkillSnapshot": {
            "id": refiner_skill_id,
            "name": refiner.skill.name,
            "source": refiner.source,
            "markdown": refiner_markdown,
            "contentHash": refiner_content_hash,
        },
        "context": {
            "selectedRawDocumentIds": raw_ids,
            "selectedGeneratedDocumentIds": generated_ids,
        },
        "contextSnapshot": context_snapshot,
    });
    let source = json!({
        "writingPanelId": bootstrap.panel.id,
        "wikiPanelId": wiki.panel.id,
        "panelSkillId": crate::agent::PANELS_SKILL_ID,
        "refinerSkillId": refiner_skill_id,
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
        "createDraft": saved_mode_draft(&bootstrap.state, "createDraft", "create"),
        "draft": bootstrap.state.get("draft").and_then(Value::as_str).unwrap_or(""),
        "mode": "refine",
        "refinementName": "",
        "revisionDraft": saved_mode_draft(&bootstrap.state, "revisionDraft", "revise"),
        "targetGeneratedDocumentId": bootstrap.state.get("targetGeneratedDocumentId").cloned().unwrap_or(Value::Null),
        "selectedCreateWritingSkillIds": bootstrap.state.get("selectedCreateWritingSkillIds").cloned().unwrap_or_else(|| json!([])),
        "selectedRevisionWritingSkillId": bootstrap.state.get("selectedRevisionWritingSkillId").cloned().unwrap_or(Value::Null),
        "selectedRefinementSkillId": refiner_skill_id,
    });
    let revision = Storage::open(paths)?.write_panel_state(
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &state,
    )?;
    Ok(json!({ "task": task, "state": state, "revision": revision }))
}
