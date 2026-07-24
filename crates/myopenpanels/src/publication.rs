use crate::control::{read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::MyOpenPanelsPaths;
use crate::storage::{Storage, TaskInsert};
use crate::types::PanelKind;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

include!("publication/task_lifecycle.rs");

pub const DEFAULT_COVER_SKILL_ID: &str = "publication-cover-default";
pub const COVER_TASK_TYPE: &str = "generate_publication_cover";
const COVER_TASK_CAPABILITY_KEY: &str = "publication.cover.generate";
pub const DEFAULT_TITLE_SKILL_ID: &str = "publication-title-default";
pub const TITLE_TASK_TYPE: &str = "generate_publication_titles";
const TITLE_TASK_CAPABILITY_KEY: &str = "publication.title.generate";
pub const DEFAULT_LAYOUT_SKILL_ID: &str = "publication-layout-default";
pub const LAYOUT_TASK_TYPE: &str = "format_publication_content";
const LAYOUT_TASK_CAPABILITY_KEY: &str = "publication.content.format";

#[cfg(test)]
fn publication_task_capability(capability_key: &str, task_type: &str) -> &'static str {
    &crate::capabilities::task_route_for_capability(capability_key, task_type)
        .expect("Typesetting Task route")
        .capability
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasAssetListing {
    pub id: String,
    pub project_id: String,
    pub project_title: String,
    pub canvas_panel_id: String,
    pub asset_id: String,
    pub asset_ref: String,
    pub src: String,
    pub name: String,
    pub mime_type: String,
    pub width: Option<f64>,
    pub height: Option<f64>,
}

pub fn cover_skills(
    paths: &MyOpenPanelsPaths,
) -> Result<Vec<crate::agent::AgentSkillListing>, CliError> {
    crate::agent::list_publication_cover_skills(paths)
}

pub fn title_skills(
    paths: &MyOpenPanelsPaths,
) -> Result<Vec<crate::agent::AgentSkillListing>, CliError> {
    crate::agent::list_publication_title_skills(paths)
}

pub fn layout_skills(
    paths: &MyOpenPanelsPaths,
) -> Result<Vec<crate::agent::AgentSkillListing>, CliError> {
    crate::agent::list_publication_layout_skills(paths)
}

pub fn create_title_request(
    paths: &MyOpenPanelsPaths,
    publication_id: &str,
    skill_id: &str,
    instruction: &str,
    request_id: &str,
) -> Result<Value, CliError> {
    let publication_id = publication_id.trim();
    let skill_id = skill_id.trim();
    let request_id = request_id.trim();
    if publication_id.is_empty() || skill_id.is_empty() || request_id.is_empty() {
        return Err(CliError::with_code(
            "invalid_title_request",
            "Publication, Title Skill, and request id are required.",
        ));
    }
    if instruction.chars().count() > 4_000 {
        return Err(CliError::with_code(
            "title_instruction_too_long",
            "Title instructions cannot exceed 4000 characters.",
        ));
    }

    let mut request = BootstrapRequest::new();
    request.requested_panel_kind = Some(PanelKind::Typesetting);
    let bootstrap = read_project_bootstrap(paths, request)?;
    let storage = Storage::open(paths)?;
    if let Some(existing) = storage
        .list_tasks(&bootstrap.project.id)?
        .into_iter()
        .find(|task| {
            task.get("queue").and_then(Value::as_str) == Some("publication")
                && task.get("type").and_then(Value::as_str) == Some(TITLE_TASK_TYPE)
                && task.pointer("/input/requestId").and_then(Value::as_str) == Some(request_id)
        })
    {
        return Ok(json!({ "task": existing }));
    }

    let publication = bootstrap
        .state
        .get("publications")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|publication| publication.get("id").and_then(Value::as_str) == Some(publication_id))
        .cloned()
        .ok_or_else(|| {
            CliError::with_code(
                "publication_not_found",
                format!("Typesetting publication not found: {publication_id}"),
            )
        })?;
    let selected_title = selected_publication_title(&publication);
    let body_text = plain_text(publication.get("content").unwrap_or(&Value::Null));
    if selected_title.trim().is_empty() && body_text.trim().is_empty() {
        return Err(CliError::with_code(
            "title_source_empty",
            "Add a title or article content before generating titles.",
        ));
    }

    let skill = crate::agent::publication_title_skill(paths, skill_id)?;
    let task_id = crate::ids::random_id("task");
    let skill_files = snapshot_skill_package(
        &storage,
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &task_id,
        "title-tasks",
        Path::new(&skill.local_dir),
    )?;
    let skill_hash = hash_file_manifest(&skill_files);
    let mut task_insert = TaskInsert::for_capability(
        TITLE_TASK_CAPABILITY_KEY,
        TITLE_TASK_TYPE,
        task_id,
        publication_id.to_owned(),
        json!({
            "requestId": request_id,
            "publicationId": publication_id,
            "publicationUpdatedAt": publication.get("updatedAt").cloned().unwrap_or(Value::Null),
            "instruction": instruction.trim(),
            "snapshot": {
                "title": selected_title,
                "existingTitles": publication_title_values(&publication),
                "bodyText": body_text,
            },
            "titleSkillId": skill_id,
            "titleSkillSnapshot": {
                "id": skill_id,
                "name": skill.skill.name,
                "source": skill.source,
                "contentHash": skill_hash,
                "files": skill_files,
            },
        }),
        json!({
            "typesettingPanelId": bootstrap.panel.id,
            "sourcePublicationId": publication_id,
        }),
    )?;
    task_insert.idempotency_key = Some(request_id.to_owned());
    let (mut tasks, _) = storage.insert_tasks_with_panel_states(
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &[task_insert],
        &[],
    )?;
    let task = tasks
        .pop()
        .ok_or_else(|| CliError::new("Created Title Task was not found."))?;
    Ok(json!({ "task": task }))
}

pub fn create_cover_request(
    paths: &MyOpenPanelsPaths,
    publication_id: &str,
    skill_id: &str,
    instruction: &str,
    request_id: &str,
) -> Result<Value, CliError> {
    let publication_id = publication_id.trim();
    let skill_id = skill_id.trim();
    let request_id = request_id.trim();
    if publication_id.is_empty() || skill_id.is_empty() || request_id.is_empty() {
        return Err(CliError::with_code(
            "invalid_cover_request",
            "Publication, Cover Skill, and request id are required.",
        ));
    }
    if instruction.chars().count() > 4_000 {
        return Err(CliError::with_code(
            "cover_instruction_too_long",
            "Cover instructions cannot exceed 4000 characters.",
        ));
    }

    let mut request = BootstrapRequest::new();
    request.requested_panel_kind = Some(PanelKind::Typesetting);
    let bootstrap = read_project_bootstrap(paths, request)?;
    let storage = Storage::open(paths)?;
    if let Some(existing) = storage
        .list_tasks(&bootstrap.project.id)?
        .into_iter()
        .find(|task| {
            task.get("queue").and_then(Value::as_str) == Some("publication")
                && task.get("type").and_then(Value::as_str) == Some(COVER_TASK_TYPE)
                && task.pointer("/input/requestId").and_then(Value::as_str) == Some(request_id)
        })
    {
        return Ok(json!({ "task": existing }));
    }

    let publication = bootstrap
        .state
        .get("publications")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|publication| publication.get("id").and_then(Value::as_str) == Some(publication_id))
        .cloned()
        .ok_or_else(|| {
            CliError::with_code(
                "publication_not_found",
                format!("Typesetting publication not found: {publication_id}"),
            )
        })?;
    let title = selected_publication_title(&publication);
    let body_text = plain_text(publication.get("content").unwrap_or(&Value::Null));
    if title.trim().is_empty() && body_text.trim().is_empty() {
        return Err(CliError::with_code(
            "cover_source_empty",
            "Add a title or article content before creating a cover.",
        ));
    }

    let skill = crate::agent::publication_cover_skill(paths, skill_id)?;
    let task_id = crate::ids::random_id("task");
    let skill_files = snapshot_skill_package(
        &storage,
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &task_id,
        "cover-tasks",
        Path::new(&skill.local_dir),
    )?;
    let skill_hash = hash_file_manifest(&skill_files);
    let mut task_insert = TaskInsert::for_capability(
        COVER_TASK_CAPABILITY_KEY,
        COVER_TASK_TYPE,
        task_id,
        publication_id.to_owned(),
        json!({
            "requestId": request_id,
            "publicationId": publication_id,
            "publicationUpdatedAt": publication.get("updatedAt").cloned().unwrap_or(Value::Null),
            "instruction": instruction.trim(),
            "snapshot": { "title": title, "bodyText": body_text },
            "coverSkillId": skill_id,
            "coverSkillSnapshot": {
                "id": skill_id,
                "name": skill.skill.name,
                "source": skill.source,
                "contentHash": skill_hash,
                "files": skill_files,
            },
        }),
        json!({
            "typesettingPanelId": bootstrap.panel.id,
            "sourcePublicationId": publication_id,
        }),
    )?;
    task_insert.idempotency_key = Some(request_id.to_owned());
    let (mut tasks, _) = storage.insert_tasks_with_panel_states(
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &[task_insert],
        &[],
    )?;
    let task = tasks
        .pop()
        .ok_or_else(|| CliError::new("Created Cover Task was not found."))?;
    Ok(json!({ "task": task }))
}

pub fn create_layout_request(
    paths: &MyOpenPanelsPaths,
    publication_id: &str,
    skill_id: &str,
    instruction: &str,
    request_id: &str,
) -> Result<Value, CliError> {
    let publication_id = publication_id.trim();
    let skill_id = skill_id.trim();
    let request_id = request_id.trim();
    if publication_id.is_empty() || skill_id.is_empty() || request_id.is_empty() {
        return Err(CliError::with_code(
            "invalid_layout_request",
            "Publication, Layout Skill, and request id are required.",
        ));
    }
    if instruction.chars().count() > 4_000 {
        return Err(CliError::with_code(
            "layout_instruction_too_long",
            "Layout instructions cannot exceed 4000 characters.",
        ));
    }

    let mut request = BootstrapRequest::new();
    request.requested_panel_kind = Some(PanelKind::Typesetting);
    let bootstrap = read_project_bootstrap(paths, request)?;
    let storage = Storage::open(paths)?;
    if let Some(existing) = storage
        .list_tasks(&bootstrap.project.id)?
        .into_iter()
        .find(|task| {
            task.get("queue").and_then(Value::as_str) == Some("publication")
                && task.get("type").and_then(Value::as_str) == Some(LAYOUT_TASK_TYPE)
                && task.pointer("/input/requestId").and_then(Value::as_str) == Some(request_id)
        })
    {
        return Ok(json!({ "task": existing }));
    }

    let publication = bootstrap
        .state
        .get("publications")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|publication| publication.get("id").and_then(Value::as_str) == Some(publication_id))
        .cloned()
        .ok_or_else(|| {
            CliError::with_code(
                "publication_not_found",
                format!("Typesetting publication not found: {publication_id}"),
            )
        })?;
    let content = publication.get("content").cloned().unwrap_or(Value::Null);
    if !document_has_content(&content) {
        return Err(CliError::with_code(
            "layout_source_empty",
            "Add article content before starting automatic layout.",
        ));
    }

    let skill = crate::agent::publication_layout_skill(paths, skill_id)?;
    let task_id = crate::ids::random_id("task");
    let skill_files = snapshot_skill_package(
        &storage,
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &task_id,
        "layout-tasks",
        Path::new(&skill.local_dir),
    )?;
    let skill_hash = hash_file_manifest(&skill_files);
    let content_hash = hash_json(&content)?;
    let mut task_insert = TaskInsert::for_capability(
        LAYOUT_TASK_CAPABILITY_KEY,
        LAYOUT_TASK_TYPE,
        task_id,
        publication_id.to_owned(),
        json!({
            "requestId": request_id,
            "publicationId": publication_id,
            "publicationUpdatedAt": publication.get("updatedAt").cloned().unwrap_or(Value::Null),
            "instruction": instruction.trim(),
            "snapshot": {
                "title": selected_publication_title(&publication),
                "content": content,
                "contentHash": content_hash,
            },
            "layoutSkillId": skill_id,
            "layoutSkillSnapshot": {
                "id": skill_id,
                "name": skill.skill.name,
                "source": skill.source,
                "contentHash": skill_hash,
                "files": skill_files,
            },
        }),
        json!({
            "typesettingPanelId": bootstrap.panel.id,
            "sourcePublicationId": publication_id,
        }),
    )?;
    task_insert.idempotency_key = Some(request_id.to_owned());
    task_insert.exclusive_non_terminal = true;
    let inserted = storage.insert_tasks_with_panel_states(
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &[task_insert],
        &[],
    );
    let (mut tasks, _) = match inserted {
        Ok(created) => created,
        Err(error) => {
            if let Some(existing) =
                storage
                    .list_tasks(&bootstrap.project.id)?
                    .into_iter()
                    .find(|task| {
                        task.get("queue").and_then(Value::as_str) == Some("publication")
                            && task.get("type").and_then(Value::as_str) == Some(LAYOUT_TASK_TYPE)
                            && task.pointer("/input/requestId").and_then(Value::as_str)
                                == Some(request_id)
                    })
            {
                return Ok(json!({ "task": existing }));
            }
            if error.code() == Some("task_target_busy") {
                return Err(CliError::with_code(
                    "publication_layout_in_progress",
                    "This publication already has an active layout Task.",
                ));
            }
            return Err(error);
        }
    };
    let task = tasks
        .pop()
        .ok_or_else(|| CliError::new("Created Layout Task was not found."))?;
    Ok(json!({ "task": task }))
}

pub fn is_cover_task_type(task_type: &str) -> bool {
    task_type == COVER_TASK_TYPE
}

pub fn is_title_task_type(task_type: &str) -> bool {
    task_type == TITLE_TASK_TYPE
}

pub fn is_layout_task_type(task_type: &str) -> bool {
    task_type == LAYOUT_TASK_TYPE
}

pub fn is_publication_task_type(task_type: &str) -> bool {
    is_cover_task_type(task_type) || is_title_task_type(task_type) || is_layout_task_type(task_type)
}

fn selected_publication_title(publication: &Value) -> &str {
    let fallback = publication
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("");
    let Some(titles) = publication.get("titles").and_then(Value::as_array) else {
        return fallback;
    };
    let selected_id = publication.get("selectedTitleId").and_then(Value::as_str);
    titles
        .iter()
        .find(|title| {
            selected_id.is_some_and(|selected_id| {
                title.get("id").and_then(Value::as_str) == Some(selected_id)
            })
        })
        .or_else(|| titles.first())
        .and_then(|title| title.get("value"))
        .and_then(Value::as_str)
        .unwrap_or(fallback)
}

fn publication_title_values(publication: &Value) -> Vec<String> {
    publication
        .get("titles")
        .and_then(Value::as_array)
        .map(|titles| {
            titles
                .iter()
                .filter_map(|title| title.get("value").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_else(|| vec![selected_publication_title(publication).to_owned()])
}

pub fn validate_content_write(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: &str,
    next_state: &Value,
) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    let locked_ids = storage
        .list_tasks(project_id)?
        .into_iter()
        .filter(|task| {
            task.get("panelId").and_then(Value::as_str) == Some(panel_id)
                && task.get("type").and_then(Value::as_str) == Some(LAYOUT_TASK_TYPE)
                && !matches!(
                    task.get("status").and_then(Value::as_str),
                    Some("failed" | "succeeded" | "cancelled" | "superseded")
                )
        })
        .filter_map(|task| {
            task.get("targetId")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect::<HashSet<_>>();
    if locked_ids.is_empty() {
        return Ok(());
    }
    let Some(current_state) = storage.read_panel_state(project_id, panel_id)? else {
        return Ok(());
    };
    for publication_id in locked_ids {
        let current = publication_content(&current_state, &publication_id);
        let next = publication_content(next_state, &publication_id);
        if current != next {
            return Err(CliError::with_code(
                "publication_content_locked",
                "Publication content cannot change while automatic layout is active.",
            ));
        }
    }
    Ok(())
}

fn publication_content<'a>(state: &'a Value, publication_id: &str) -> Option<&'a Value> {
    state
        .get("publications")
        .and_then(Value::as_array)?
        .iter()
        .find(|publication| publication.get("id").and_then(Value::as_str) == Some(publication_id))?
        .get("content")
}

pub fn hash_json(value: &Value) -> Result<String, CliError> {
    let bytes = serde_json::to_vec(value).map_err(to_cli_error)?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn document_has_content(document: &Value) -> bool {
    document
        .get("text")
        .and_then(Value::as_str)
        .is_some_and(|text| !text.is_empty())
        || document.get("type").and_then(Value::as_str) == Some("image")
        || document
            .get("content")
            .and_then(Value::as_array)
            .is_some_and(|children| children.iter().any(document_has_content))
}

pub fn plain_text(document: &Value) -> String {
    let mut output = String::new();
    append_plain_text(document, &mut output);
    output.trim().to_owned()
}

fn append_plain_text(node: &Value, output: &mut String) {
    if let Some(text) = node.get("text").and_then(Value::as_str) {
        output.push_str(text);
    }
    let node_type = node.get("type").and_then(Value::as_str).unwrap_or("");
    if node_type == "hardBreak" {
        output.push('\n');
    }
    for child in node
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        append_plain_text(child, output);
    }
    if matches!(
        node_type,
        "paragraph" | "heading" | "listItem" | "blockquote"
    ) && !output.ends_with('\n')
    {
        output.push('\n');
    }
}

fn snapshot_skill_package(
    storage: &Storage,
    project_id: &str,
    panel_id: &str,
    task_id: &str,
    task_directory: &str,
    root: &Path,
) -> Result<Vec<Value>, CliError> {
    let mut source_files = Vec::new();
    collect_regular_files(root, root, &mut source_files)?;
    let mut files = Vec::new();
    for (relative, bytes) in source_files {
        let relative_text = relative.to_string_lossy().replace('\\', "/");
        let requested = format!("{task_directory}/{task_id}/skill/{relative_text}");
        let written =
            storage.write_asset_from_buffer(project_id, panel_id, &requested, &bytes, true)?;
        files.push(json!({
            "path": relative_text,
            "assetRef": written.asset_ref,
            "contentHash": format!("sha256:{:x}", Sha256::digest(&bytes)),
            "sizeBytes": bytes.len(),
        }));
    }
    files.sort_by(|left, right| left["path"].as_str().cmp(&right["path"].as_str()));
    Ok(files)
}

pub(crate) fn recapture_retry_skill_snapshot(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    retry_task_id: &str,
) -> Result<Option<Value>, CliError> {
    let task_type = task.get("type").and_then(Value::as_str).unwrap_or("");
    let input = task.get("input").cloned().unwrap_or(Value::Null);
    let (skill_id_key, snapshot_key, task_directory, skill) = match task_type {
        COVER_TASK_TYPE => {
            let skill_id = retry_skill_id(&input, "coverSkillId", "coverSkillSnapshot")?;
            (
                "coverSkillId",
                "coverSkillSnapshot",
                "cover-tasks",
                crate::agent::publication_cover_skill(paths, &skill_id)?,
            )
        }
        TITLE_TASK_TYPE => {
            let skill_id = retry_skill_id(&input, "titleSkillId", "titleSkillSnapshot")?;
            (
                "titleSkillId",
                "titleSkillSnapshot",
                "title-tasks",
                crate::agent::publication_title_skill(paths, &skill_id)?,
            )
        }
        LAYOUT_TASK_TYPE => {
            let skill_id = retry_skill_id(&input, "layoutSkillId", "layoutSkillSnapshot")?;
            (
                "layoutSkillId",
                "layoutSkillSnapshot",
                "layout-tasks",
                crate::agent::publication_layout_skill(paths, &skill_id)?,
            )
        }
        _ => return Ok(None),
    };
    let project_id = task
        .get("projectId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliError::new("Retry Task is missing its Project id."))?;
    let panel_id = task
        .get("panelId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliError::new("Retry Task is missing its origin Panel id."))?;
    let storage = Storage::open(paths)?;
    let files = snapshot_skill_package(
        &storage,
        project_id,
        panel_id,
        retry_task_id,
        task_directory,
        Path::new(&skill.local_dir),
    )?;
    if !files
        .iter()
        .any(|file| file.get("path").and_then(Value::as_str) == Some("SKILL.md"))
    {
        return Err(CliError::new(
            "The Skill package does not contain SKILL.md.",
        ));
    }
    let content_hash = hash_file_manifest(&files);
    let mut refreshed = input;
    refreshed[skill_id_key] = json!(skill.skill.id);
    refreshed[snapshot_key] = json!({
        "id": skill.skill.id,
        "name": skill.skill.name,
        "source": skill.source,
        "contentHash": content_hash,
        "files": files,
    });
    Ok(Some(refreshed))
}

fn retry_skill_id(
    input: &Value,
    skill_id_key: &str,
    snapshot_key: &str,
) -> Result<String, CliError> {
    input
        .get(skill_id_key)
        .and_then(Value::as_str)
        .or_else(|| {
            input
                .pointer(&format!("/{snapshot_key}/id"))
                .and_then(Value::as_str)
        })
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| CliError::new("The original Task does not identify its required Skill."))
}

fn collect_regular_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(PathBuf, Vec<u8>)>,
) -> Result<(), CliError> {
    for entry in fs::read_dir(directory).map_err(to_cli_error)? {
        let entry = entry.map_err(to_cli_error)?;
        let file_type = entry.file_type().map_err(to_cli_error)?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_regular_files(root, &entry.path(), files)?;
        } else if file_type.is_file() {
            files.push((
                entry
                    .path()
                    .strip_prefix(root)
                    .map_err(to_cli_error)?
                    .to_owned(),
                fs::read(entry.path()).map_err(to_cli_error)?,
            ));
        }
    }
    Ok(())
}

fn hash_file_manifest(files: &[Value]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(
            file.get("path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .as_bytes(),
        );
        hasher.update(
            file.get("contentHash")
                .and_then(Value::as_str)
                .unwrap_or("")
                .as_bytes(),
        );
    }
    format!("sha256:{:x}", hasher.finalize())
}

pub fn list_canvas_assets(
    paths: &MyOpenPanelsPaths,
    current_project_id: &str,
    scope: &str,
) -> Result<Value, CliError> {
    if !matches!(scope, "current" | "all") {
        return Err(CliError::with_code(
            "invalid_target",
            "Typesetting asset scope must be current or all.",
        ));
    }
    let storage = Storage::open(paths)?;
    let mut projects = storage.list_projects()?;
    if !projects
        .iter()
        .any(|project| project.id == current_project_id)
    {
        return Err(CliError::with_code(
            "project_not_found",
            format!("MyOpenPanels project not found: {current_project_id}"),
        ));
    }
    if scope == "current" {
        projects.retain(|project| project.id == current_project_id);
    } else {
        projects.sort_by_key(|project| usize::from(project.id != current_project_id));
    }

    let mut assets = Vec::new();
    for project in projects {
        for panel_id in &project.panel_ids {
            let Some(panel) = storage.read_panel(&project.id, panel_id)? else {
                continue;
            };
            if panel.kind != PanelKind::Canvas {
                continue;
            }
            let Some(state) = storage.read_panel_state(&project.id, &panel.id)? else {
                continue;
            };
            assets.extend(canvas_assets_from_state(
                &project.id,
                &project.title,
                &panel.id,
                &state,
            ));
        }
    }
    Ok(json!({ "assets": assets, "scope": scope }))
}

pub fn import_canvas_asset(
    paths: &MyOpenPanelsPaths,
    target_project_id: &str,
    target_panel_id: &str,
    source_asset_ref: &str,
) -> Result<Value, CliError> {
    let storage = Storage::open(paths)?;
    let target = storage
        .read_panel(target_project_id, target_panel_id)?
        .ok_or_else(|| {
            CliError::with_code(
                "target_not_found",
                format!("Typesetting panel not found: {target_panel_id}"),
            )
        })?;
    if target.kind != PanelKind::Typesetting {
        return Err(CliError::with_code(
            "invalid_target",
            "Canvas assets can only be imported into a Typesetting panel.",
        ));
    }

    let source = parse_asset_ref(source_asset_ref)?;
    let source_asset = storage
        .list_assets(&source.project_id)?
        .into_iter()
        .find(|asset| asset.get("id").and_then(Value::as_str) == Some(&source.resource_id))
        .ok_or_else(|| CliError::with_code("target_not_found", "Canvas asset not found."))?;
    let source_panel_id = source_asset
        .pointer("/metadata/originPanelId")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::with_code("invalid_target", "Canvas asset origin is missing."))?;
    let source_panel = storage
        .read_panel(&source.project_id, source_panel_id)?
        .ok_or_else(|| {
            CliError::with_code(
                "target_not_found",
                format!("Source Canvas panel not found: {source_panel_id}"),
            )
        })?;
    if source_panel.kind != PanelKind::Canvas {
        return Err(CliError::with_code(
            "invalid_target",
            "The source asset must belong to a Canvas panel.",
        ));
    }
    let bytes = storage.read_asset_by_id(&source.project_id, &source.resource_id)?;
    let written = storage.write_asset_from_buffer(
        target_project_id,
        target_panel_id,
        &source.file_name,
        &bytes,
        false,
    )?;
    let mime_type = mime_guess::from_path(&written.file_name)
        .first_raw()
        .unwrap_or("application/octet-stream");
    Ok(json!({
        "assetRef": written.asset_ref,
        "fileName": written.file_name,
        "mimeType": mime_type,
        "sourceAssetRef": source_asset_ref,
        "sourceProjectId": source.project_id,
        "sourceCanvasPanelId": source_panel_id,
        "resourceId": written.resource_id,
        "src": format!("/api/assets/{}/content", written.resource_id),
    }))
}

fn canvas_assets_from_state(
    project_id: &str,
    project_title: &str,
    panel_id: &str,
    state: &Value,
) -> Vec<CanvasAssetListing> {
    let Some(store) = state.get("store").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut shapes = store
        .values()
        .filter(|record| {
            record.get("typeName").and_then(Value::as_str) == Some("shape")
                && record.get("type").and_then(Value::as_str) == Some("image")
        })
        .collect::<Vec<_>>();
    shapes.sort_by_key(|shape| {
        std::cmp::Reverse(
            shape
                .get("index")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
        )
    });

    let mut seen = HashSet::new();
    shapes
        .into_iter()
        .filter_map(|shape| {
            let asset_id = shape.pointer("/props/assetId").and_then(Value::as_str)?;
            if !seen.insert(asset_id.to_owned()) {
                return None;
            }
            let asset = store.get(asset_id)?;
            if asset.get("typeName").and_then(Value::as_str) != Some("asset")
                || asset.get("type").and_then(Value::as_str) != Some("image")
            {
                return None;
            }
            let asset_ref = asset
                .pointer("/meta/assetRef")
                .and_then(Value::as_str)?
                .to_owned();
            let resource_id = asset.pointer("/meta/resourceId").and_then(Value::as_str)?;
            let parsed = parse_asset_ref(&asset_ref).ok()?;
            if parsed.project_id != project_id || parsed.resource_id != resource_id {
                return None;
            }
            Some(CanvasAssetListing {
                id: format!("{project_id}:{panel_id}:{asset_id}"),
                project_id: project_id.to_owned(),
                project_title: project_title.to_owned(),
                canvas_panel_id: panel_id.to_owned(),
                asset_id: asset_id.to_owned(),
                asset_ref,
                src: format!("/api/assets/{resource_id}/content"),
                name: asset
                    .pointer("/props/name")
                    .and_then(Value::as_str)
                    .unwrap_or(&parsed.file_name)
                    .to_owned(),
                mime_type: asset
                    .pointer("/props/mimeType")
                    .and_then(Value::as_str)
                    .unwrap_or("image/*")
                    .to_owned(),
                width: asset.pointer("/props/w").and_then(Value::as_f64),
                height: asset.pointer("/props/h").and_then(Value::as_f64),
            })
        })
        .collect()
}

struct ParsedAssetRef {
    project_id: String,
    resource_id: String,
    file_name: String,
}

fn parse_asset_ref(asset_ref: &str) -> Result<ParsedAssetRef, CliError> {
    let parts = asset_ref.split('/').collect::<Vec<_>>();
    if parts.len() < 7
        || parts[0] != "projects"
        || parts[2] != "content"
        || parts[3] != "asset"
        || parts[1].is_empty()
        || parts[4].is_empty()
        || parts[5].is_empty()
        || parts[6..].iter().any(|part| part.is_empty())
    {
        return Err(CliError::with_code(
            "invalid_target",
            "Expected a Project Asset content reference.",
        ));
    }
    let file_name = parts[6..].join("/");
    if file_name.is_empty() {
        return Err(CliError::with_code(
            "invalid_target",
            "Asset content reference is missing a file name.",
        ));
    }
    Ok(ParsedAssetRef {
        project_id: parts[1].to_owned(),
        resource_id: parts[4].to_owned(),
        file_name,
    })
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
include!("publication/tests.rs");
