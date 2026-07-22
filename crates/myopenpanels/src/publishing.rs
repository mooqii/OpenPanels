use crate::control::{now_iso, read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::MyOpenPanelsPaths;
use crate::storage::{Storage, TaskInsert};
use crate::types::PanelKind;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

include!("publishing/task_lifecycle.rs");

pub const DEFAULT_XIAOHONGSHU_SKILL_ID: &str = "publishing-xiaohongshu";
pub const XIAOHONGSHU_TASK_TYPE: &str = "publish_xiaohongshu_note";
pub const XIAOHONGSHU_CAPABILITY: &str = "publishing.xiaohongshu";
pub const WECHAT_OFFICIAL_ACCOUNT_TASK_TYPE: &str = "publish_wechat_official_account_draft";
pub const WECHAT_OFFICIAL_ACCOUNT_CAPABILITY: &str = "publishing.wechat_official_account";

#[derive(Clone, Copy)]
struct PublishingTarget {
    capability: &'static str,
    platform: &'static str,
    task_type: &'static str,
}

const XIAOHONGSHU_TARGET: PublishingTarget = PublishingTarget {
    capability: XIAOHONGSHU_CAPABILITY,
    platform: "xiaohongshu",
    task_type: XIAOHONGSHU_TASK_TYPE,
};

const WECHAT_OFFICIAL_ACCOUNT_TARGET: PublishingTarget = PublishingTarget {
    capability: WECHAT_OFFICIAL_ACCOUNT_CAPABILITY,
    platform: "wechat_official_account",
    task_type: WECHAT_OFFICIAL_ACCOUNT_TASK_TYPE,
};

pub fn is_publishing_task_type(task_type: &str) -> bool {
    matches!(
        task_type,
        XIAOHONGSHU_TASK_TYPE | WECHAT_OFFICIAL_ACCOUNT_TASK_TYPE
    )
}

fn publishing_target_for_skill(
    skill: &crate::agent::AgentSkillListing,
) -> Result<PublishingTarget, CliError> {
    if skill
        .skill
        .task_types
        .iter()
        .any(|value| value == WECHAT_OFFICIAL_ACCOUNT_TASK_TYPE)
    {
        return Ok(WECHAT_OFFICIAL_ACCOUNT_TARGET);
    }
    if skill
        .skill
        .task_types
        .iter()
        .any(|value| value == XIAOHONGSHU_TASK_TYPE)
    {
        return Ok(XIAOHONGSHU_TARGET);
    }
    Err(CliError::with_code(
        "publishing_skill_not_supported",
        format!(
            "Publishing Skill has no supported publishing target: {}",
            skill.skill.id
        ),
    ))
}

fn publishing_source_has_content(body_text: &str, media_count: usize) -> bool {
    !body_text.trim().is_empty() || media_count > 0
}

pub fn empty_state() -> Value {
    json!({
        "schemaVersion": 1,
        "selectedPublicationId": null,
        "selectedSkillIds": { "xiaohongshu": DEFAULT_XIAOHONGSHU_SKILL_ID },
        "releases": [],
    })
}

pub fn normalize_state(mut state: Value) -> Value {
    if state.get("schemaVersion").and_then(Value::as_u64) != Some(1) || !state.is_object() {
        return empty_state();
    }
    if !state
        .get("selectedPublicationId")
        .is_some_and(|value| value.is_null() || value.is_string())
    {
        state["selectedPublicationId"] = Value::Null;
    }
    if !state.get("selectedSkillIds").is_some_and(Value::is_object) {
        state["selectedSkillIds"] = json!({});
    }
    if !state
        .pointer("/selectedSkillIds/xiaohongshu")
        .is_some_and(Value::is_string)
    {
        state["selectedSkillIds"]["xiaohongshu"] = json!(DEFAULT_XIAOHONGSHU_SKILL_ID);
    }
    if !state.get("releases").is_some_and(Value::is_array) {
        state["releases"] = json!([]);
    }
    state
}

pub fn validate_state(state: &Value) -> bool {
    state.get("schemaVersion").and_then(Value::as_u64) == Some(1)
        && state
            .get("selectedPublicationId")
            .is_some_and(|value| value.is_null() || value.is_string())
        && state
            .pointer("/selectedSkillIds/xiaohongshu")
            .is_some_and(Value::is_string)
        && state
            .get("releases")
            .and_then(Value::as_array)
            .is_some_and(|releases| releases.iter().all(validate_release))
}

fn validate_release(release: &Value) -> bool {
    let Some(media) = release.pointer("/snapshot/media").and_then(Value::as_array) else {
        return false;
    };
    let Some(body_text) = release
        .pointer("/snapshot/bodyText")
        .and_then(Value::as_str)
    else {
        return false;
    };
    let Some(attempts) = release.get("attempts").and_then(Value::as_array) else {
        return false;
    };
    release.get("id").is_some_and(Value::is_string)
        && matches!(
            release.get("platform").and_then(Value::as_str),
            Some("xiaohongshu" | "wechat_official_account")
        )
        && release
            .get("sourcePublicationId")
            .is_some_and(Value::is_string)
        && release
            .pointer("/snapshot/title")
            .is_some_and(Value::is_string)
        && release
            .pointer("/snapshot/bodyText")
            .is_some_and(Value::is_string)
        && release.pointer("/snapshot/tags").is_none_or(|tags| {
            tags.as_array()
                .is_some_and(|items| items.iter().all(Value::is_string))
        })
        && publishing_source_has_content(body_text, media.len())
        && media.iter().enumerate().all(|(index, item)| {
            item.get("assetRef").is_some_and(Value::is_string)
                && item.get("fileName").is_some_and(Value::is_string)
                && item.get("src").is_some_and(Value::is_string)
                && item
                    .get("contentHash")
                    .is_none_or(|value| value.is_string())
                && item.get("isPrimary").and_then(Value::as_bool) == Some(index == 0)
        })
        && attempts.iter().all(validate_attempt)
}

fn validate_attempt(attempt: &Value) -> bool {
    attempt.get("id").is_some_and(Value::is_string)
        && attempt.get("taskId").is_some_and(Value::is_string)
        && attempt.get("requestId").is_some_and(Value::is_string)
        && matches!(
            attempt.get("mode").and_then(Value::as_str),
            Some("auto" | "manual")
        )
        && matches!(
            attempt.get("phase").and_then(Value::as_str),
            Some("queued" | "prepared" | "committing" | "completed")
        )
        && attempt.get("skillId").is_some_and(Value::is_string)
        && attempt.get("skillHash").is_some_and(Value::is_string)
        && attempt.get("outcome").is_some_and(|value| {
            value.is_null()
                || matches!(
                    value.as_str(),
                    Some("published" | "needs_user_action" | "not_published" | "unknown")
                )
        })
}

fn publishing_bootstrap(
    paths: &MyOpenPanelsPaths,
) -> Result<crate::types::ProjectBootstrap, CliError> {
    let mut request = BootstrapRequest::new();
    request.requested_panel_kind = Some(PanelKind::Publishing);
    read_project_bootstrap(paths, request)
}

pub fn update_preferences(
    paths: &MyOpenPanelsPaths,
    selected_publication_id: Option<&str>,
    skill_id: &str,
) -> Result<Value, CliError> {
    crate::agent::publishing_skill(paths, skill_id)?;
    let bootstrap = publishing_bootstrap(paths)?;
    let storage = Storage::open(paths)?;
    for _ in 0..5 {
        let base_revision =
            storage.read_panel_state_revision(&bootstrap.project.id, &bootstrap.panel.id)?;
        let mut state = normalize_state(
            storage
                .read_panel_state(&bootstrap.project.id, &bootstrap.panel.id)?
                .unwrap_or_else(empty_state),
        );
        state["selectedPublicationId"] =
            selected_publication_id.map_or(Value::Null, |id| json!(id));
        state["selectedSkillIds"]["xiaohongshu"] = json!(skill_id);
        if let Ok(revision) = storage.write_panel_state_if_current(
            &bootstrap.project.id,
            &bootstrap.panel.id,
            &state,
            Some(base_revision),
        )? {
            return Ok(json!({ "state": state, "revision": revision }));
        }
    }
    Err(CliError::with_code(
        "content_conflict",
        "Publishing preferences changed concurrently. Try again.",
    ))
}

pub fn create_release(
    paths: &MyOpenPanelsPaths,
    publication_id: &str,
    skill_id: &str,
    request_id: &str,
) -> Result<Value, CliError> {
    if publication_id.trim().is_empty() || request_id.trim().is_empty() {
        return Err(CliError::with_code(
            "invalid_publishing_request",
            "Publication id and request id are required.",
        ));
    }
    let selected_skill = crate::agent::publishing_skill(paths, skill_id)?;
    let target = publishing_target_for_skill(&selected_skill)?;
    let bootstrap = publishing_bootstrap(paths)?;
    let storage = Storage::open(paths)?;
    let mut state = normalize_state(bootstrap.state.clone());
    if let Some(existing) = find_attempt_by_request_id(&state, request_id) {
        return existing_attempt_payload(&storage, &bootstrap, &state, existing);
    }
    let typesetting = bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == PanelKind::Typesetting)
        .ok_or_else(|| CliError::with_code("target_not_found", "Typesetting panel not found."))?;
    let publication = typesetting
        .state
        .get("publications")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(publication_id))
        .cloned()
        .ok_or_else(|| {
            CliError::with_code(
                "publishing_source_not_found",
                format!("Typesetting publication not found: {publication_id}"),
            )
        })?;
    let title = selected_publication_title(&publication);
    let body_text = typesetting_plain_text(publication.get("content").unwrap_or(&Value::Null));
    let tags = publication
        .get("tags")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let covers = publication
        .get("covers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !publishing_source_has_content(&body_text, covers.len()) {
        return Err(CliError::with_code(
            "publishing_source_incomplete",
            "Publishing requires text content or at least one image.",
        ));
    }

    let release_id = crate::ids::random_id("release");
    let release_dir = storage
        .panel_dir(&bootstrap.project.id, &bootstrap.panel.id)
        .join("assets")
        .join("releases")
        .join(crate::paths::sanitize_path_part(&release_id));
    let result = (|| {
        let media = snapshot_media(
            &storage,
            &bootstrap.project.id,
            &bootstrap.panel.id,
            &release_id,
            &covers,
        )?;
        let now = now_iso();
        let mut release = json!({
            "id": release_id,
            "platform": target.platform,
            "sourcePublicationId": publication_id,
            "sourceUpdatedAt": publication.get("updatedAt").cloned().unwrap_or(Value::Null),
            "snapshot": {
                "title": title,
                "bodyText": body_text,
                "tags": tags,
                "media": media,
            },
            "attempts": [],
            "createdAt": now,
            "updatedAt": now,
        });
        let (attempt, task) = build_attempt(
            paths,
            &storage,
            &bootstrap.project.id,
            &bootstrap.panel.id,
            &release,
            skill_id,
            request_id,
            "auto",
        )?;
        release["attempts"] = json!([attempt]);
        state["selectedPublicationId"] = json!(publication_id);
        state["selectedSkillIds"]["xiaohongshu"] = json!(skill_id);
        state
            .get_mut("releases")
            .and_then(Value::as_array_mut)
            .expect("normalized releases")
            .insert(0, release.clone());
        let (tasks, revisions) = storage.insert_tasks_with_panel_states(
            &bootstrap.project.id,
            &bootstrap.panel.id,
            &[task],
            &[(bootstrap.panel.id.as_str(), &state)],
        )?;
        Ok(json!({
            "release": release,
            "task": tasks.into_iter().next(),
            "state": state,
            "revision": revisions.first().copied().unwrap_or(bootstrap.revision),
        }))
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(release_dir);
    }
    result
}

pub fn create_attempt(
    paths: &MyOpenPanelsPaths,
    release_id: &str,
    skill_id: &str,
    request_id: &str,
    mode: &str,
    acknowledged_unknown: bool,
) -> Result<Value, CliError> {
    if !matches!(mode, "auto" | "manual") || request_id.trim().is_empty() {
        return Err(CliError::with_code(
            "invalid_publishing_request",
            "Publishing attempt mode must be auto or manual and request id is required.",
        ));
    }
    let bootstrap = publishing_bootstrap(paths)?;
    let storage = Storage::open(paths)?;
    let mut state = normalize_state(bootstrap.state.clone());
    if let Some(existing) = find_attempt_by_request_id(&state, request_id) {
        return existing_attempt_payload(&storage, &bootstrap, &state, existing);
    }
    let release_index = state
        .get("releases")
        .and_then(Value::as_array)
        .and_then(|releases| {
            releases
                .iter()
                .position(|release| release.get("id").and_then(Value::as_str) == Some(release_id))
        })
        .ok_or_else(|| CliError::with_code("publishing_release_not_found", "Release not found."))?;
    let release = state["releases"][release_index].clone();
    let has_unknown_commit = release
        .get("attempts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .rev()
        .take(1)
        .any(|attempt| {
            attempt.get("phase").and_then(Value::as_str) == Some("committing")
                || attempt.get("outcome").and_then(Value::as_str) == Some("unknown")
        });
    if has_unknown_commit && !acknowledged_unknown {
        let platform = release
            .get("platform")
            .and_then(Value::as_str)
            .unwrap_or("the target platform");
        return Err(CliError::with_code(
            "publishing_unknown_unacknowledged",
            format!(
                "Check {platform} for possibly published content before starting another attempt."
            ),
        ));
    }
    let (attempt, task) = build_attempt(
        paths,
        &storage,
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &release,
        skill_id,
        request_id,
        mode,
    )?;
    state["releases"][release_index]["attempts"]
        .as_array_mut()
        .expect("validated attempts")
        .push(attempt.clone());
    state["releases"][release_index]["updatedAt"] = json!(now_iso());
    state["selectedSkillIds"]["xiaohongshu"] = json!(skill_id);
    let (tasks, revisions) = storage.insert_tasks_with_panel_states(
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &[task],
        &[(bootstrap.panel.id.as_str(), &state)],
    )?;
    Ok(json!({
        "attempt": attempt,
        "task": tasks.into_iter().next(),
        "state": state,
        "revision": revisions.first().copied().unwrap_or(bootstrap.revision),
    }))
}

fn build_attempt(
    paths: &MyOpenPanelsPaths,
    storage: &Storage,
    project_id: &str,
    panel_id: &str,
    release: &Value,
    skill_id: &str,
    request_id: &str,
    mode: &str,
) -> Result<(Value, TaskInsert), CliError> {
    let skill = crate::agent::publishing_skill(paths, skill_id)?;
    let target = publishing_target_for_skill(&skill)?;
    let release_platform = release
        .get("platform")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if release_platform != target.platform {
        return Err(CliError::with_code(
            "publishing_skill_target_mismatch",
            format!(
                "Publishing Skill {} cannot publish a {release_platform} release.",
                skill.skill.id
            ),
        ));
    }
    let release_id = release.get("id").and_then(Value::as_str).unwrap_or("");
    let attempt_id = crate::ids::random_id("publish-attempt");
    let task_id = crate::ids::random_id("task");
    let skill_files = snapshot_skill_package(
        storage,
        project_id,
        panel_id,
        release_id,
        &attempt_id,
        Path::new(&skill.local_dir),
    )?;
    let skill_hash = hash_file_manifest(&skill_files);
    let now = now_iso();
    let attempt = json!({
        "id": attempt_id,
        "taskId": task_id,
        "requestId": request_id,
        "mode": mode,
        "skillId": skill_id,
        "skillName": skill.skill.name,
        "skillHash": skill_hash,
        "phase": "queued",
        "outcome": null,
        "summary": null,
        "reasonCode": null,
        "remoteUrl": null,
        "publishedAt": null,
        "createdAt": now,
        "completedAt": null,
    });
    let task = TaskInsert {
        id: task_id,
        queue: "publishing".to_owned(),
        task_type: target.task_type.to_owned(),
        capability: target.capability.to_owned(),
        target_ref: release_id.to_owned(),
        input: json!({
            "platform": target.platform,
            "releaseId": release_id,
            "attemptId": attempt_id,
            "snapshot": release.get("snapshot").cloned().unwrap_or(Value::Null),
            "publishingSkillId": skill_id,
            "publishingSkillSnapshot": {
                "id": skill_id,
                "name": skill.skill.name,
                "source": skill.source,
                "contentHash": skill_hash,
                "files": skill_files,
            },
        }),
        source: json!({
            "publishingPanelId": panel_id,
            "sourcePublicationId": release.get("sourcePublicationId"),
        }),
        max_attempts: 1,
        dispatch_mode: mode.to_owned(),
        idempotency_key: Some(request_id.to_owned()),
        exclusive_non_terminal: false,
    };
    Ok((attempt, task))
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

fn snapshot_media(
    storage: &Storage,
    project_id: &str,
    panel_id: &str,
    release_id: &str,
    covers: &[Value],
) -> Result<Vec<Value>, CliError> {
    covers
        .iter()
        .enumerate()
        .map(|(index, cover)| {
            let source_ref = cover
                .get("assetRef")
                .and_then(Value::as_str)
                .ok_or_else(|| CliError::with_code("invalid_target", "Cover asset is missing."))?;
            let bytes = storage.read_asset(source_ref)?;
            let file_name = cover
                .get("fileName")
                .and_then(Value::as_str)
                .unwrap_or("image");
            let requested = format!(
                "releases/{release_id}/media/{:03}-{}",
                index + 1,
                crate::paths::sanitize_path_part(file_name)
            );
            let written = storage.write_asset_from_buffer(
                project_id,
                panel_id,
                &requested,
                &bytes,
                true,
            )?;
            Ok(json!({
                "assetRef": written.asset_ref,
                "contentHash": format!("sha256:{:x}", Sha256::digest(&bytes)),
                "fileName": written.file_name,
                "mimeType": cover.get("mimeType").cloned().unwrap_or_else(|| json!("image/*")),
                "width": cover.get("width").cloned().unwrap_or(Value::Null),
                "height": cover.get("height").cloned().unwrap_or(Value::Null),
                "isPrimary": index == 0,
                "src": format!("/api/projects/{project_id}/panels/{panel_id}/assets/{}", written.file_name),
            }))
        })
        .collect()
}

fn snapshot_skill_package(
    storage: &Storage,
    project_id: &str,
    panel_id: &str,
    release_id: &str,
    attempt_id: &str,
    root: &Path,
) -> Result<Vec<Value>, CliError> {
    let mut source_files = Vec::new();
    collect_regular_files(root, root, &mut source_files)?;
    let mut files = Vec::new();
    for (relative, bytes) in source_files {
        let relative_text = relative.to_string_lossy().replace('\\', "/");
        let requested =
            format!("releases/{release_id}/attempts/{attempt_id}/skill/{relative_text}");
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
    let mut hash = Sha256::new();
    for file in files {
        hash.update(
            file.get("path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .as_bytes(),
        );
        hash.update(
            file.get("contentHash")
                .and_then(Value::as_str)
                .unwrap_or("")
                .as_bytes(),
        );
    }
    format!("sha256:{:x}", hash.finalize())
}

fn find_attempt_by_request_id<'a>(state: &'a Value, request_id: &str) -> Option<&'a Value> {
    state
        .get("releases")?
        .as_array()?
        .iter()
        .flat_map(|release| {
            release
                .get("attempts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .find(|attempt| attempt.get("requestId").and_then(Value::as_str) == Some(request_id))
}

fn existing_attempt_payload(
    storage: &Storage,
    bootstrap: &crate::types::ProjectBootstrap,
    state: &Value,
    attempt: &Value,
) -> Result<Value, CliError> {
    let task_id = attempt.get("taskId").and_then(Value::as_str).unwrap_or("");
    let task = storage
        .list_tasks(&bootstrap.project.id)?
        .into_iter()
        .find(|task| task.get("id").and_then(Value::as_str) == Some(task_id));
    let attempt_id = attempt.get("id").and_then(Value::as_str);
    let release = state
        .get("releases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|release| {
            release
                .get("attempts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|candidate| candidate.get("id").and_then(Value::as_str) == attempt_id)
        });
    Ok(json!({
        "attempt": attempt,
        "release": release,
        "task": task,
        "state": state,
        "revision": storage.read_panel_state_revision(&bootstrap.project.id, &bootstrap.panel.id)?,
        "idempotent": true,
    }))
}

pub fn checkpoint_attempt(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    phase: &str,
) -> Result<Value, CliError> {
    if crate::content::broker_execution_available() {
        return crate::content::broker_publishing_checkpoint(
            &crate::content::PublishingCheckpointRequest {
                task_id: task_id.to_owned(),
                phase: phase.to_owned(),
            },
        );
    }
    crate::tasks::verify_task_write_access(paths, task_id)?;
    checkpoint_attempt_for_broker(paths, task_id, phase)
}

pub(crate) fn checkpoint_attempt_for_broker(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    phase: &str,
) -> Result<Value, CliError> {
    if !matches!(phase, "prepared" | "committing") {
        return Err(CliError::with_code(
            "invalid_publishing_phase",
            "Publishing checkpoint phase must be prepared or committing.",
        ));
    }
    let task = crate::tasks::inspect_task(paths, task_id)?["task"].clone();
    let task_type = task.get("type").and_then(Value::as_str).unwrap_or_default();
    if task.get("queue").and_then(Value::as_str) != Some("publishing")
        || !is_publishing_task_type(task_type)
    {
        return Err(CliError::with_code(
            "invalid_target",
            "Publishing checkpoint requires a supported publishing Task.",
        ));
    }
    if !matches!(
        task.get("status").and_then(Value::as_str),
        Some("reserved" | "running" | "claimed")
    ) {
        return Err(CliError::with_code(
            "task_not_claimed",
            "Claim the publishing Task before checkpointing it.",
        ));
    }
    let project_id = task.get("projectId").and_then(Value::as_str).unwrap_or("");
    let panel_id = task.get("panelId").and_then(Value::as_str).unwrap_or("");
    let attempt_id = task
        .pointer("/input/attemptId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let storage = Storage::open(paths)?;
    let mut state = normalize_state(
        storage
            .read_panel_state(project_id, panel_id)?
            .unwrap_or_else(empty_state),
    );
    let attempt = find_attempt_mut(&mut state, attempt_id)?;
    let current = attempt
        .get("phase")
        .and_then(Value::as_str)
        .unwrap_or("queued");
    if phase == "prepared" && current == "committing" {
        return Err(CliError::with_code(
            "invalid_publishing_phase",
            "A committing attempt cannot return to prepared.",
        ));
    }
    attempt["phase"] = json!(phase);
    attempt["updatedAt"] = json!(now_iso());
    let revision = storage.write_panel_state(project_id, panel_id, &state)?;
    Ok(json!({ "taskId": task_id, "attemptId": attempt_id, "phase": phase, "revision": revision }))
}

pub fn prepare_task_completion(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    result: Option<Value>,
) -> Result<Option<(String, Value)>, CliError> {
    let task = crate::tasks::inspect_task(paths, task_id)?["task"].clone();
    let result = result.ok_or_else(|| {
        CliError::with_code("invalid_output", "Publishing Task result is missing.")
    })?;
    let project_id = task.get("projectId").and_then(Value::as_str).unwrap_or("");
    let panel_id = task.get("panelId").and_then(Value::as_str).unwrap_or("");
    let release_id = task
        .pointer("/input/releaseId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let attempt_id = task
        .pointer("/input/attemptId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let platform = task
        .pointer("/input/platform")
        .and_then(Value::as_str)
        .unwrap_or("");
    if result.get("releaseId").and_then(Value::as_str) != Some(release_id)
        || result.get("attemptId").and_then(Value::as_str) != Some(attempt_id)
        || result.get("platform").and_then(Value::as_str) != Some(platform)
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Publishing result does not match the claimed release and attempt.",
        ));
    }
    let storage = Storage::open(paths)?;
    let mut state = normalize_state(
        storage
            .read_panel_state(project_id, panel_id)?
            .unwrap_or_else(empty_state),
    );
    let attempt = find_attempt_mut(&mut state, attempt_id)?;
    attempt["phase"] = json!("completed");
    attempt["outcome"] = result.get("outcome").cloned().unwrap_or(Value::Null);
    attempt["summary"] = result.get("summary").cloned().unwrap_or(Value::Null);
    attempt["reasonCode"] = result.get("reasonCode").cloned().unwrap_or(Value::Null);
    attempt["remoteUrl"] = result.get("remoteUrl").cloned().unwrap_or(Value::Null);
    attempt["publishedAt"] = result.get("publishedAt").cloned().unwrap_or(Value::Null);
    attempt["completedAt"] = json!(now_iso());
    Ok(Some((panel_id.to_owned(), state)))
}

fn find_attempt_mut<'a>(state: &'a mut Value, attempt_id: &str) -> Result<&'a mut Value, CliError> {
    state
        .get_mut("releases")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .flat_map(|release| {
            release
                .get_mut("attempts")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
        })
        .find(|attempt| attempt.get("id").and_then(Value::as_str) == Some(attempt_id))
        .ok_or_else(|| {
            CliError::with_code(
                "publishing_attempt_not_found",
                "Publishing attempt not found.",
            )
        })
}

pub fn typesetting_plain_text(document: &Value) -> String {
    let mut output = String::new();
    render_node(document, &mut output);
    let mut normalized = Vec::new();
    let mut blank = false;
    for line in output.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            if !blank && !normalized.is_empty() {
                normalized.push(String::new());
            }
            blank = true;
        } else {
            normalized.push(line.to_owned());
            blank = false;
        }
    }
    normalized.join("\n").trim().to_owned()
}

fn render_node(node: &Value, output: &mut String) {
    match node.get("type").and_then(Value::as_str).unwrap_or("") {
        "text" => output.push_str(node.get("text").and_then(Value::as_str).unwrap_or("")),
        "hardBreak" => output.push('\n'),
        "image" => {}
        "bulletList" | "orderedList" => {
            let ordered = node.get("type").and_then(Value::as_str) == Some("orderedList");
            for (index, item) in node
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
            {
                if ordered {
                    output.push_str(&format!("{}. ", index + 1));
                } else {
                    output.push_str("- ");
                }
                render_children(item, output);
                output.push('\n');
            }
            output.push('\n');
        }
        "paragraph" | "heading" | "blockquote" => {
            render_children(node, output);
            output.push_str("\n\n");
        }
        _ => render_children(node, output),
    }
}

fn render_children(node: &Value, output: &mut String) {
    for child in node
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        render_node(child, output);
    }
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn publishing_skill_listing(task_type: &str) -> crate::agent::AgentSkillListing {
        crate::agent::AgentSkillListing {
            skill: crate::agent::AgentSkillMetadata {
                applies_to: vec!["publishing".to_owned()],
                description: "Publish prepared content.".to_owned(),
                id: "publishing-test".to_owned(),
                load_when: Vec::new(),
                requires_commands: Vec::new(),
                source: "builtin".to_owned(),
                task_types: vec![task_type.to_owned()],
                name: "Publishing Test".to_owned(),
                tokens: "short".to_owned(),
            },
            local_dir: String::new(),
            local_path: String::new(),
            source: "builtin".to_owned(),
        }
    }

    #[test]
    fn publishing_skills_select_their_platform_route() {
        let xiaohongshu =
            publishing_target_for_skill(&publishing_skill_listing(XIAOHONGSHU_TASK_TYPE))
                .expect("Xiaohongshu target");
        assert_eq!(xiaohongshu.platform, "xiaohongshu");
        assert_eq!(xiaohongshu.capability, XIAOHONGSHU_CAPABILITY);

        let wechat = publishing_target_for_skill(&publishing_skill_listing(
            WECHAT_OFFICIAL_ACCOUNT_TASK_TYPE,
        ))
        .expect("WeChat target");
        assert_eq!(wechat.platform, "wechat_official_account");
        assert_eq!(wechat.capability, WECHAT_OFFICIAL_ACCOUNT_CAPABILITY);
    }

    #[test]
    fn legacy_state_is_migrated_without_losing_schema_compatibility() {
        let state = normalize_state(json!({ "schemaVersion": 1 }));
        assert_eq!(state["selectedPublicationId"], Value::Null);
        assert_eq!(
            state["selectedSkillIds"]["xiaohongshu"],
            DEFAULT_XIAOHONGSHU_SKILL_ID
        );
        assert_eq!(state["releases"], json!([]));
        assert!(validate_state(&state));
    }

    #[test]
    fn plain_text_preserves_lists_and_excludes_images() {
        let document = json!({
            "type": "doc",
            "content": [
                { "type": "paragraph", "content": [
                    { "type": "text", "text": "First" },
                    { "type": "image", "attrs": { "src": "/inline.png" } },
                    { "type": "hardBreak" },
                    { "type": "text", "text": "Second" }
                ]},
                { "type": "bulletList", "content": [
                    { "type": "listItem", "content": [
                        { "type": "paragraph", "content": [{ "type": "text", "text": "Item" }] }
                    ]}
                ]}
            ]
        });
        let text = typesetting_plain_text(&document);
        assert!(text.contains("First\nSecond"));
        assert!(text.contains("- Item"));
        assert!(!text.contains("inline.png"));
    }

    #[test]
    fn publishing_source_accepts_body_or_media() {
        assert!(publishing_source_has_content("Body", 0));
        assert!(publishing_source_has_content("", 1));
        assert!(!publishing_source_has_content("  ", 0));
    }

    #[test]
    fn publishing_uses_the_selected_title_alternative() {
        let publication = json!({
            "title": "Legacy title",
            "selectedTitleId": "title:channel",
            "titles": [
                { "id": "title:primary", "value": "Primary title" },
                { "id": "title:channel", "value": "Channel title" }
            ]
        });
        assert_eq!(selected_publication_title(&publication), "Channel title");
    }

    #[test]
    fn release_validation_requires_the_first_media_item_to_be_primary() {
        let attempt = json!({
            "id": "attempt:1",
            "taskId": "task:1",
            "requestId": "request:1",
            "mode": "auto",
            "phase": "queued",
            "skillId": DEFAULT_XIAOHONGSHU_SKILL_ID,
            "skillHash": "sha256:test",
            "outcome": null
        });
        let release = |first_primary| {
            json!({
                "id": "release:1",
                "platform": "xiaohongshu",
                "sourcePublicationId": "publication:1",
                "snapshot": {
                    "title": "Title",
                    "bodyText": "Body",
                    "media": [{
                        "assetRef": "projects/p/panels/x/assets/cover.png",
                        "contentHash": "sha256:test",
                        "fileName": "cover.png",
                        "src": "/cover.png",
                        "isPrimary": first_primary
                    }]
                },
                "attempts": [attempt.clone()]
            })
        };
        assert!(validate_release(&release(true)));
        assert!(!validate_release(&release(false)));
        let mut wechat_release = release(true);
        wechat_release["platform"] = json!("wechat_official_account");
        assert!(validate_release(&wechat_release));
    }

    #[test]
    fn release_validation_accepts_text_only_or_image_only_snapshots() {
        let release = |body_text: &str, media: Value| {
            json!({
                "id": "release:1",
                "platform": "xiaohongshu",
                "sourcePublicationId": "publication:1",
                "snapshot": {
                    "title": "",
                    "bodyText": body_text,
                    "media": media
                },
                "attempts": []
            })
        };
        let image = json!([{
            "assetRef": "projects/p/panels/x/assets/cover.png",
            "fileName": "cover.png",
            "src": "/cover.png",
            "isPrimary": true
        }]);

        assert!(validate_release(&release("Body", json!([]))));
        assert!(validate_release(&release("", image)));
        assert!(!validate_release(&release("", json!([]))));
    }

    #[test]
    fn release_validation_accepts_optional_string_tags() {
        let release = |tags: Value| {
            json!({
                "id": "release:1",
                "platform": "xiaohongshu",
                "sourcePublicationId": "publication:1",
                "snapshot": {
                    "title": "Title",
                    "bodyText": "Body",
                    "media": [],
                    "tags": tags
                },
                "attempts": []
            })
        };

        assert!(validate_release(&release(json!(["writing", "AI"]))));
        assert!(!validate_release(&release(json!(["writing", 42]))));
    }
}
