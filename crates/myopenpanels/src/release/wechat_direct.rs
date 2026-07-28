const WECHAT_DIRECT_SKILL_HASH: &str = "builtin:wechat-official-account-api:v1";
const WECHAT_DIRECT_CHANNEL_ID: &str = "release-wechat-official-account";

fn publishing_task_id_is_valid(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::Null | Value::String(_)))
}

pub fn submit_wechat_draft_direct(
    paths: &MyOpenPanelsPaths,
    publication_id: &str,
    request_id: &str,
) -> Result<Value, CliError> {
    if publication_id.trim().is_empty() || request_id.trim().is_empty() {
        return Err(CliError::with_code(
            "invalid_publishing_request",
            "Publication id and request id are required.",
        ));
    }
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
    let title = selected_publication_title(&publication).to_owned();
    let body = publication_plain_text(publication.get("content").unwrap_or(&Value::Null));
    let covers = publication
        .get("covers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let tags = publication
        .get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if title.trim().is_empty() {
        return Err(CliError::with_code(
            "wechat_title_missing",
            "A WeChat draft requires a title.",
        ));
    }
    if covers.is_empty() {
        return Err(CliError::with_code(
            "wechat_cover_missing",
            "A WeChat draft requires a cover image.",
        ));
    }

    let release_id = crate::ids::random_id("release");
    let attempt_id = crate::ids::random_id("publish-attempt");
    let media_snapshot = snapshot_media(
        &storage,
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &release_id,
        &covers,
    )?;
    let media = direct_wechat_media(&storage, &media_snapshot)?;
    let now = now_iso();
    let attempt = json!({
        "id": attempt_id,
        "taskId": null,
        "requestId": request_id,
        "mode": "direct",
        "skillId": WECHAT_DIRECT_CHANNEL_ID,
        "skillName": "WeChat Official Account Draft API",
        "skillHash": WECHAT_DIRECT_SKILL_HASH,
        "phase": "committing",
        "outcome": null,
        "summary": null,
        "reasonCode": null,
        "remoteUrl": null,
        "publishedAt": null,
        "createdAt": now,
        "completedAt": null,
        "apiDraftInvocation": { "startedAt": now },
    });
    let release = json!({
        "id": release_id,
        "platform": "wechat_official_account",
        "sourcePublicationId": publication_id,
        "sourceUpdatedAt": publication.get("updatedAt").cloned().unwrap_or(Value::Null),
        "snapshot": {
            "title": title,
            "bodyText": body,
            "tags": tags,
            "media": media_snapshot,
        },
        "attempts": [attempt],
        "createdAt": now,
        "updatedAt": now,
    });
    let mut persisted = false;
    for _ in 0..5 {
        let (current, base_revision) = storage
            .read_panel_state_snapshot(&bootstrap.project.id, &bootstrap.panel.id)?
            .ok_or_else(|| {
                CliError::with_code("target_not_found", "Publishing state not found.")
            })?;
        state = normalize_state(current);
        if let Some(existing) = find_attempt_by_request_id(&state, request_id) {
            return existing_attempt_payload(&storage, &bootstrap, &state, existing);
        }
        state["selectedPublicationId"] = json!(publication_id);
        state
            .get_mut("releases")
            .and_then(Value::as_array_mut)
            .expect("normalized releases")
            .insert(0, release.clone());
        if storage
            .write_panel_state_if_current(
                &bootstrap.project.id,
                &bootstrap.panel.id,
                &state,
                Some(base_revision),
            )?
            .is_ok()
        {
            persisted = true;
            break;
        }
    }
    if !persisted {
        return Err(CliError::with_code(
            "content_conflict",
            "Publishing state kept changing before the WeChat submission. Try again.",
        ));
    }

    let input = WechatDraftInput {
        title,
        body,
        tags,
        media,
    };
    let result = read_wechat_credentials(paths)?.map_or_else(
        || {
            wechat_draft_outcome(
                "needs_user_action",
                "WeChat API credentials are not configured in Studio.",
                Some("wechat_credentials_missing"),
                None,
            )
        },
        |credentials| {
            save_wechat_draft(
                &WechatHttpApi::new(),
                &credentials.app_id,
                &credentials.app_secret,
                &input,
            )
        },
    );
    finish_direct_wechat_submission(
        &storage,
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &release_id,
        &attempt_id,
        &result,
    )
}

fn finish_direct_wechat_submission(
    storage: &Storage,
    project_id: &str,
    panel_id: &str,
    release_id: &str,
    attempt_id: &str,
    result: &Value,
) -> Result<Value, CliError> {
    for _ in 0..5 {
        let (current, base_revision) = storage
            .read_panel_state_snapshot(project_id, panel_id)?
            .ok_or_else(|| {
                CliError::with_code("target_not_found", "Publishing state not found.")
            })?;
        let mut state = normalize_state(current);
        let completed_at = now_iso();
        {
            let attempt = find_attempt_mut(&mut state, attempt_id)?;
            attempt["phase"] = json!("completed");
            attempt["outcome"] = result.get("outcome").cloned().unwrap_or(Value::Null);
            attempt["summary"] = result.get("summary").cloned().unwrap_or(Value::Null);
            attempt["reasonCode"] = result.get("reasonCode").cloned().unwrap_or(Value::Null);
            attempt["remoteUrl"] = result.get("remoteUrl").cloned().unwrap_or(Value::Null);
            attempt["publishedAt"] = result.get("publishedAt").cloned().unwrap_or(Value::Null);
            attempt["completedAt"] = json!(completed_at);
            attempt["updatedAt"] = json!(completed_at);
            attempt["apiDraftInvocation"]["finishedAt"] = json!(completed_at);
            attempt["apiDraftInvocation"]["result"] = result.clone();
            if let Some(fields) = result.get("truncatedFields") {
                attempt["truncatedFields"] = fields.clone();
            }
        }
        let release = state
            .get_mut("releases")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
            .find(|release| release.get("id").and_then(Value::as_str) == Some(release_id))
            .ok_or_else(|| {
                CliError::with_code("publishing_release_not_found", "Release not found.")
            })?;
        release["updatedAt"] = json!(completed_at);
        let release = release.clone();
        let attempt = release
            .get("attempts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|attempt| attempt.get("id").and_then(Value::as_str) == Some(attempt_id))
            .cloned()
            .ok_or_else(|| {
                CliError::with_code(
                    "publishing_attempt_not_found",
                    "Publishing attempt not found.",
                )
            })?;
        if let Ok(revision) =
            storage.write_panel_state_if_current(project_id, panel_id, &state, Some(base_revision))?
        {
            return Ok(json!({
                "attempt": attempt,
                "release": release,
                "state": state,
                "revision": revision,
            }));
        }
    }
    Err(CliError::with_code(
        "publishing_state_conflict",
        "WeChat returned a result, but Studio could not record it after repeated conflicts.",
    ))
}

fn direct_wechat_media(
    storage: &Storage,
    media: &[Value],
) -> Result<Vec<WechatDraftMedia>, CliError> {
    media
        .iter()
        .map(|item| {
            let asset_ref = item
                .get("assetRef")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    CliError::with_code(
                        "publishing_snapshot_corrupt",
                        "A WeChat media asset reference is missing.",
                    )
                })?;
            let file_name = item
                .get("fileName")
                .and_then(Value::as_str)
                .unwrap_or("image")
                .to_owned();
            Ok(WechatDraftMedia {
                bytes: storage.read_asset(asset_ref)?,
                mime_type: item
                    .get("mimeType")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or_else(|| {
                        mime_guess::from_path(&file_name)
                            .first_raw()
                            .map(str::to_owned)
                    })
                    .unwrap_or_else(|| "application/octet-stream".to_owned()),
                file_name,
            })
        })
        .collect()
}

#[cfg(test)]
mod wechat_direct_tests {
    use super::*;

    #[test]
    fn release_validation_accepts_a_direct_attempt_without_a_task() {
        let state = json!({
            "selectedPublicationId": "publication:1",
            "selectedSkillIds": { "xiaohongshu": DEFAULT_XIAOHONGSHU_SKILL_ID },
            "releases": [{
                "id": "release:direct",
                "platform": "wechat_official_account",
                "sourcePublicationId": "publication:1",
                "snapshot": {
                    "title": "Title",
                    "bodyText": "Body",
                    "media": []
                },
                "attempts": [{
                    "id": "attempt:direct",
                    "taskId": null,
                    "requestId": "request:direct",
                    "mode": "direct",
                    "phase": "completed",
                    "skillId": WECHAT_DIRECT_CHANNEL_ID,
                    "skillHash": WECHAT_DIRECT_SKILL_HASH,
                    "outcome": "published"
                }]
            }]
        });

        assert!(validate_state(&state));
    }
}
