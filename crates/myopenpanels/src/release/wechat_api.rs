const WECHAT_APP_ID_ENV: &str = "MYOPENPANELS_WECHAT_APP_ID";
const WECHAT_APP_SECRET_ENV: &str = "MYOPENPANELS_WECHAT_APP_SECRET";
const WECHAT_API_ORIGIN: &str = "https://api.weixin.qq.com";
const WECHAT_DRAFT_LIST_URL: &str = "https://mp.weixin.qq.com/cgi-bin/appmsg?t=media/appmsg_list&action=list_card&begin=0&count=10&type=10&lang=zh_CN";
const WECHAT_HTTP_TIMEOUT_SECS: u64 = 30;
const WECHAT_MAX_TITLE_CHARS: usize = 32;
const WECHAT_MAX_ARTICLE_CHARS: usize = 20_000;
const WECHAT_MAX_ARTICLE_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
struct WechatDraftInput {
    title: String,
    body: String,
    tags: Vec<String>,
    media: Vec<WechatDraftMedia>,
}

#[derive(Debug)]
struct WechatDraftMedia {
    bytes: Vec<u8>,
    file_name: String,
    mime_type: String,
}

#[derive(Debug)]
enum WechatApiFailure {
    Api {
        code: i64,
        message: Option<String>,
    },
    InvalidResponse,
    Transport,
}

trait WechatDraftApi {
    fn access_token(&self, app_id: &str, app_secret: &str)
        -> Result<String, WechatApiFailure>;
    fn upload_permanent_image(
        &self,
        access_token: &str,
        media: &WechatDraftMedia,
    ) -> Result<String, WechatApiFailure>;
    fn upload_article_image(
        &self,
        access_token: &str,
        media: &WechatDraftMedia,
    ) -> Result<String, WechatApiFailure>;
    fn add_draft(
        &self,
        access_token: &str,
        title: &str,
        content: &str,
        cover_media_id: &str,
    ) -> Result<String, WechatApiFailure>;
}

struct WechatHttpApi {
    agent: ureq::Agent,
}

impl WechatHttpApi {
    fn new() -> Self {
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(WECHAT_HTTP_TIMEOUT_SECS))
                .build(),
        }
    }

    fn multipart_request(
        &self,
        url: &str,
        media: &WechatDraftMedia,
    ) -> Result<Value, WechatApiFailure> {
        let boundary = multipart_boundary(&media.bytes);
        let body = multipart_body(&boundary, media);
        parse_wechat_response(
            self.agent
                .post(url)
                .set(
                    "content-type",
                    &format!("multipart/form-data; boundary={boundary}"),
                )
                .send_bytes(&body),
        )
    }

    fn validate_draft_permission(&self, access_token: &str) -> Result<(), WechatApiFailure> {
        parse_wechat_response(
            self.agent
                .post(&format!(
                    "{WECHAT_API_ORIGIN}/cgi-bin/draft/count?access_token={access_token}"
                ))
                .set("content-type", "application/json")
                .send_json(json!({})),
        )?;
        Ok(())
    }
}

impl WechatDraftApi for WechatHttpApi {
    fn access_token(
        &self,
        app_id: &str,
        app_secret: &str,
    ) -> Result<String, WechatApiFailure> {
        let payload = parse_wechat_response(
            self.agent
                .get(&format!("{WECHAT_API_ORIGIN}/cgi-bin/token"))
                .query("grant_type", "client_credential")
                .query("appid", app_id)
                .query("secret", app_secret)
                .call(),
        )?;
        required_wechat_string(&payload, "access_token")
    }

    fn upload_permanent_image(
        &self,
        access_token: &str,
        media: &WechatDraftMedia,
    ) -> Result<String, WechatApiFailure> {
        let payload = self.multipart_request(
            &format!(
                "{WECHAT_API_ORIGIN}/cgi-bin/material/add_material?access_token={access_token}&type=image"
            ),
            media,
        )?;
        required_wechat_string(&payload, "media_id")
    }

    fn upload_article_image(
        &self,
        access_token: &str,
        media: &WechatDraftMedia,
    ) -> Result<String, WechatApiFailure> {
        let payload = self.multipart_request(
            &format!(
                "{WECHAT_API_ORIGIN}/cgi-bin/media/uploadimg?access_token={access_token}"
            ),
            media,
        )?;
        required_wechat_string(&payload, "url")
    }

    fn add_draft(
        &self,
        access_token: &str,
        title: &str,
        content: &str,
        cover_media_id: &str,
    ) -> Result<String, WechatApiFailure> {
        let payload = parse_wechat_response(
            self.agent
                .post(&format!(
                    "{WECHAT_API_ORIGIN}/cgi-bin/draft/add?access_token={access_token}"
                ))
                .set("content-type", "application/json")
                .send_json(json!({
                    "articles": [{
                        "article_type": "news",
                        "title": title,
                        "content": content,
                        "thumb_media_id": cover_media_id
                    }]
                })),
        )?;
        required_wechat_string(&payload, "media_id")
    }
}

pub fn save_wechat_draft_for_claimed_task(task_id: &str) -> Result<Value, CliError> {
    if !crate::content::broker_execution_available() {
        return Err(CliError::with_code(
            "broker_unavailable",
            "WeChat draft API execution requires the Studio Task Broker.",
        ));
    }
    crate::content::broker_wechat_draft(&crate::content::WechatDraftRequest {
        task_id: task_id.to_owned(),
    })
}

pub(crate) fn save_wechat_draft_for_broker(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
) -> Result<Value, CliError> {
    let task = crate::tasks::inspect_task(paths, task_id)?["task"].clone();
    if task.get("status").and_then(Value::as_str) != Some("running")
        || task.get("type").and_then(Value::as_str) != Some(WECHAT_OFFICIAL_ACCOUNT_TASK_TYPE)
        || task.get("capability").and_then(Value::as_str) != Some("release.wechat_official_account")
    {
        return Err(CliError::with_code(
            "execution_fenced",
            "WeChat draft API execution requires its exact running publishing Task.",
        ));
    }
    let input = read_wechat_draft_input_from_task(paths, &task)?;
    if let Some(existing) = begin_wechat_draft_invocation(paths, &task)? {
        return Ok(existing);
    }
    let credentials = read_wechat_credentials(paths)?;
    let result = credentials.map_or_else(
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
    finish_wechat_draft_invocation(paths, &task, &result)?;
    Ok(result)
}

fn save_wechat_draft(
    api: &impl WechatDraftApi,
    app_id: &str,
    app_secret: &str,
    input: &WechatDraftInput,
) -> Value {
    if input.title.trim().is_empty() {
        return wechat_draft_outcome(
            "not_published",
            "The WeChat draft title is empty.",
            Some("wechat_title_missing"),
            None,
        );
    }
    if input.media.is_empty() {
        return wechat_draft_outcome(
            "not_published",
            "The WeChat draft requires a cover image.",
            Some("wechat_cover_missing"),
            None,
        );
    }
    if input
        .media
        .iter()
        .any(|media| !media.mime_type.starts_with("image/"))
    {
        return wechat_draft_outcome(
            "not_published",
            "The WeChat article API accepts only image media for this workflow.",
            Some("wechat_media_type_unsupported"),
            None,
        );
    }

    let access_token = match api.access_token(app_id, app_secret) {
        Ok(access_token) => access_token,
        Err(error) => return wechat_failure_outcome(error, false),
    };
    let cover_media_id = match api.upload_permanent_image(&access_token, &input.media[0]) {
        Ok(media_id) => media_id,
        Err(error) => return wechat_failure_outcome(error, false),
    };
    let mut inline_urls = Vec::new();
    for media in input.media.iter().skip(1) {
        match api.upload_article_image(&access_token, media) {
            Ok(url) => inline_urls.push(url),
            Err(error) => return wechat_failure_outcome(error, false),
        }
    }
    let normalized_title = input.title.trim();
    let title = truncate_characters(normalized_title, WECHAT_MAX_TITLE_CHARS);
    let Some((content, content_truncated)) =
        article_html_with_limit(&input.body, &inline_urls)
    else {
        return wechat_draft_outcome(
            "not_published",
            "The WeChat inline images alone exceed the official draft API limit.",
            Some("wechat_content_too_large"),
            None,
        );
    };
    let mut result = match api.add_draft(
        &access_token,
        &title,
        &content,
        &cover_media_id,
    ) {
        Ok(media_id) => {
            let mut outcome = wechat_draft_outcome(
                "published",
                "WeChat accepted the article and returned a new draft media id.",
                None,
                Some(media_id),
            );
            outcome["remoteUrl"] = json!(WECHAT_DRAFT_LIST_URL);
            outcome
        }
        Err(error) => wechat_failure_outcome(error, true),
    };
    let mut truncated_fields = Vec::new();
    if title != normalized_title {
        truncated_fields.push("title");
    }
    if content_truncated {
        truncated_fields.push("content");
    }
    if input.tags.iter().any(|tag| !tag.trim().is_empty()) {
        truncated_fields.push("tags");
    }
    if !truncated_fields.is_empty() {
        result["truncatedFields"] = json!(truncated_fields);
        if result.get("outcome").and_then(Value::as_str) == Some("published") {
            result["summary"] = json!(
                "WeChat accepted the draft after unsupported or over-limit fields were truncated."
            );
        }
    }
    result
}

fn begin_wechat_draft_invocation(
    paths: &MyOpenPanelsPaths,
    task: &Value,
) -> Result<Option<Value>, CliError> {
    let project_id = task.get("projectId").and_then(Value::as_str).unwrap_or("");
    let panel_id = task.get("panelId").and_then(Value::as_str).unwrap_or("");
    let attempt_id = task
        .pointer("/input/attemptId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let storage = Storage::open(paths)?;
    for _ in 0..5 {
        let (state, base_revision) = storage
            .read_panel_state_snapshot(project_id, panel_id)?
            .ok_or_else(|| {
                CliError::with_code("target_not_found", "Publishing state not found.")
            })?;
        let mut state = normalize_state(state);
        let attempt = find_attempt_mut(&mut state, attempt_id)?;
        if let Some(result) = attempt.pointer("/apiDraftInvocation/result") {
            return Ok(Some(result.clone()));
        }
        if attempt.pointer("/apiDraftInvocation/startedAt").is_some() {
            return Ok(Some(wechat_draft_outcome(
                "unknown",
                "A previous WeChat draft API invocation started without recording a final result; it was not retried.",
                Some("wechat_draft_result_unknown"),
                None,
            )));
        }
        if attempt.get("phase").and_then(Value::as_str) != Some("committing") {
            return Err(CliError::with_code(
                "publishing_checkpoint_required",
                "Run the committing checkpoint immediately before the WeChat draft API command.",
            ));
        }
        attempt["apiDraftInvocation"] = json!({ "startedAt": now_iso() });
        if storage
            .write_panel_state_if_current(project_id, panel_id, &state, Some(base_revision))?
            .is_ok()
        {
            return Ok(None);
        }
    }
    Err(CliError::with_code(
        "publishing_state_conflict",
        "Publishing state changed while fencing the WeChat draft API invocation.",
    ))
}

fn finish_wechat_draft_invocation(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    result: &Value,
) -> Result<(), CliError> {
    let project_id = task.get("projectId").and_then(Value::as_str).unwrap_or("");
    let panel_id = task.get("panelId").and_then(Value::as_str).unwrap_or("");
    let attempt_id = task
        .pointer("/input/attemptId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let storage = Storage::open(paths)?;
    for _ in 0..5 {
        let (state, base_revision) = storage
            .read_panel_state_snapshot(project_id, panel_id)?
            .ok_or_else(|| {
                CliError::with_code("target_not_found", "Publishing state not found.")
            })?;
        let mut state = normalize_state(state);
        let attempt = find_attempt_mut(&mut state, attempt_id)?;
        if attempt.pointer("/apiDraftInvocation/result").is_some() {
            return Ok(());
        }
        attempt["apiDraftInvocation"]["finishedAt"] = json!(now_iso());
        attempt["apiDraftInvocation"]["result"] = result.clone();
        if storage
            .write_panel_state_if_current(project_id, panel_id, &state, Some(base_revision))?
            .is_ok()
        {
            return Ok(());
        }
    }
    Err(CliError::with_code(
        "publishing_state_conflict",
        "Publishing state changed while recording the WeChat draft API result.",
    ))
}

fn read_wechat_draft_input_from_task(
    paths: &MyOpenPanelsPaths,
    task: &Value,
) -> Result<WechatDraftInput, CliError> {
    let title = task
        .pointer("/input/snapshot/title")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CliError::with_code(
                "publishing_snapshot_corrupt",
                "The WeChat title snapshot is missing.",
            )
        })?
        .to_owned();
    let body = task
        .pointer("/input/snapshot/bodyText")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CliError::with_code(
                "publishing_snapshot_corrupt",
                "The WeChat body snapshot is missing.",
            )
        })?
        .to_owned();
    let tags = match task.pointer("/input/snapshot/tags") {
        None => Vec::new(),
        Some(Value::Array(tags)) if tags.iter().all(Value::is_string) => tags
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Some(_) => {
            return Err(CliError::with_code(
                "publishing_snapshot_corrupt",
                "The WeChat tags snapshot is invalid.",
            ));
        }
    };
    let storage = Storage::open(paths)?;
    let media = task
        .pointer("/input/snapshot/media")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            CliError::with_code(
                "publishing_snapshot_corrupt",
                "The WeChat media snapshot is missing.",
            )
        })?
        .iter()
        .enumerate()
        .map(|(index, media)| {
            if media.get("isPrimary").and_then(Value::as_bool) != Some(index == 0) {
                return Err(CliError::with_code(
                    "publishing_snapshot_corrupt",
                    "The WeChat media order or primary cover marker is invalid.",
                ));
            }
            let asset_ref = media
                .get("assetRef")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    CliError::with_code(
                        "publishing_snapshot_corrupt",
                        "A WeChat media asset reference is missing.",
                    )
                })?;
            let bytes = storage.read_asset(asset_ref)?;
            let actual_hash = format!("sha256:{:x}", Sha256::digest(&bytes));
            if media
                .get("contentHash")
                .and_then(Value::as_str)
                .is_some_and(|expected| expected != actual_hash)
            {
                return Err(CliError::with_code(
                    "publishing_snapshot_corrupt",
                    "A WeChat media input failed integrity validation.",
                ));
            }
            let file_name = media
                .get("fileName")
                .and_then(Value::as_str)
                .unwrap_or("image")
                .to_owned();
            let mime_type = media
                .get("mimeType")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| {
                    mime_guess::from_path(&file_name)
                        .first_raw()
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| "application/octet-stream".to_owned());
            Ok(WechatDraftMedia {
                bytes,
                file_name,
                mime_type,
            })
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    Ok(WechatDraftInput {
        title,
        body,
        tags,
        media,
    })
}

fn parse_wechat_response(
    response: Result<ureq::Response, ureq::Error>,
) -> Result<Value, WechatApiFailure> {
    let response = match response {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(ureq::Error::Transport(_)) => return Err(WechatApiFailure::Transport),
    };
    let payload = response
        .into_json::<Value>()
        .map_err(|_| WechatApiFailure::InvalidResponse)?;
    if let Some(code) = payload.get("errcode").and_then(Value::as_i64) {
        if code != 0 {
            return Err(WechatApiFailure::Api {
                code,
                message: payload
                    .get("errmsg")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            });
        }
    }
    Ok(payload)
}

fn wechat_error_observed_ip(message: Option<&str>) -> Option<String> {
    message?.split_whitespace().find_map(|part| {
        let candidate = part.trim_matches(|character: char| {
            matches!(
                character,
                ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\''
            )
        });
        let candidate = candidate
            .strip_prefix("ip:")
            .or_else(|| candidate.strip_prefix("ip="))
            .unwrap_or(candidate);
        candidate
            .parse::<std::net::IpAddr>()
            .ok()
            .map(|ip| ip.to_string())
    })
}

fn required_wechat_string(payload: &Value, key: &str) -> Result<String, WechatApiFailure> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or(WechatApiFailure::InvalidResponse)
}

fn wechat_failure_outcome(error: WechatApiFailure, final_action: bool) -> Value {
    match error {
        WechatApiFailure::Transport if final_action => wechat_draft_outcome(
            "unknown",
            "The WeChat draft request ended without a confirmable response; it was not retried.",
            Some("wechat_draft_result_unknown"),
            None,
        ),
        WechatApiFailure::Transport => wechat_draft_outcome(
            "not_published",
            "A WeChat API request failed before the draft save request.",
            Some("wechat_api_unavailable"),
            None,
        ),
        WechatApiFailure::InvalidResponse if final_action => wechat_draft_outcome(
            "unknown",
            "WeChat returned an unreadable response to the draft save request; it was not retried.",
            Some("wechat_draft_result_unknown"),
            None,
        ),
        WechatApiFailure::InvalidResponse => wechat_draft_outcome(
            "not_published",
            "WeChat returned an unreadable response before the draft save request.",
            Some("wechat_api_invalid_response"),
            None,
        ),
        WechatApiFailure::Api { code, message } => {
            let (outcome, reason, summary) = match code {
                40013 | 40125 => (
                    "needs_user_action",
                    "wechat_credentials_rejected",
                    "WeChat rejected the configured AppID or AppSecret.",
                ),
                40164 => (
                    "needs_user_action",
                    "wechat_ip_not_allowed",
                    "The Studio server IP is not in the WeChat API allowlist.",
                ),
                48001 => (
                    "needs_user_action",
                    "wechat_api_unauthorized",
                    "This Official Account does not grant the required draft API permission.",
                ),
                45009 => (
                    "not_published",
                    "wechat_api_quota_exceeded",
                    "The WeChat API daily quota has been exhausted.",
                ),
                _ => (
                    "not_published",
                    "wechat_api_rejected",
                    "WeChat rejected the draft API request.",
                ),
            };
            let mut result = wechat_draft_outcome(outcome, summary, Some(reason), None);
            result["wechatErrorCode"] = json!(code);
            if code == 40164 {
                if let Some(observed_ip) = wechat_error_observed_ip(message.as_deref()) {
                    result["wechatObservedIp"] = json!(observed_ip);
                }
            }
            result
        }
    }
}

fn wechat_draft_outcome(
    outcome: &str,
    summary: &str,
    reason_code: Option<&str>,
    media_id: Option<String>,
) -> Value {
    json!({
        "outcome": outcome,
        "summary": summary,
        "reasonCode": reason_code,
        "mediaId": media_id,
        "remoteUrl": null,
        "publishedAt": if outcome == "published" {
            Value::String(now_iso())
        } else {
            Value::Null
        }
    })
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod wechat_api_tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct FakeWechatApi {
        calls: RefCell<Vec<String>>,
        draft_failure: RefCell<Option<WechatApiFailure>>,
    }

    impl WechatDraftApi for FakeWechatApi {
        fn access_token(
            &self,
            _app_id: &str,
            _app_secret: &str,
        ) -> Result<String, WechatApiFailure> {
            self.calls.borrow_mut().push("token".to_owned());
            Ok("access-token".to_owned())
        }

        fn upload_permanent_image(
            &self,
            _access_token: &str,
            _media: &WechatDraftMedia,
        ) -> Result<String, WechatApiFailure> {
            self.calls.borrow_mut().push("cover".to_owned());
            Ok("cover-id".to_owned())
        }

        fn upload_article_image(
            &self,
            _access_token: &str,
            _media: &WechatDraftMedia,
        ) -> Result<String, WechatApiFailure> {
            self.calls.borrow_mut().push("inline".to_owned());
            Ok("https://mmbiz.qpic.cn/inline.png".to_owned())
        }

        fn add_draft(
            &self,
            _access_token: &str,
            title: &str,
            content: &str,
            cover_media_id: &str,
        ) -> Result<String, WechatApiFailure> {
            self.calls.borrow_mut().push(format!(
                "draft:{title}:{cover_media_id}:{content}"
            ));
            if let Some(error) = self.draft_failure.borrow_mut().take() {
                Err(error)
            } else {
                Ok("draft-media-id".to_owned())
            }
        }
    }

    fn image(name: &str) -> WechatDraftMedia {
        WechatDraftMedia {
            bytes: vec![1, 2, 3],
            file_name: name.to_owned(),
            mime_type: "image/png".to_owned(),
        }
    }

    #[test]
    fn official_api_uploads_cover_and_inline_images_before_saving_one_draft() {
        let api = FakeWechatApi::default();
        let result = save_wechat_draft(
            &api,
            "app-id",
            "secret",
            &WechatDraftInput {
                title: "Title".to_owned(),
                body: "A & B\n<text>".to_owned(),
                tags: Vec::new(),
                media: vec![image("cover.png"), image("inline.png")],
            },
        );

        assert_eq!(result["outcome"], "published");
        assert_eq!(result["mediaId"], "draft-media-id");
        assert_eq!(result["remoteUrl"], WECHAT_DRAFT_LIST_URL);
        assert!(result["publishedAt"].is_string());
        assert_eq!(
            api.calls.borrow()[..3],
            ["token", "cover", "inline"]
        );
        let draft = &api.calls.borrow()[3];
        assert!(draft.contains("draft:Title:cover-id:"));
        assert!(draft.contains("<p>A &amp; B</p><p>&lt;text&gt;</p>"));
        assert!(draft.contains("https://mmbiz.qpic.cn/inline.png"));
    }

    #[test]
    fn unsupported_topics_are_omitted_from_the_api_payload() {
        let api = FakeWechatApi::default();
        let result = save_wechat_draft(
            &api,
            "app-id",
            "secret",
            &WechatDraftInput {
                title: "Title".to_owned(),
                body: "Body".to_owned(),
                tags: vec!["topic".to_owned()],
                media: vec![image("cover.png")],
            },
        );

        assert_eq!(result["outcome"], "published");
        assert_eq!(result["truncatedFields"], json!(["tags"]));
        assert!(api
            .calls
            .borrow()
            .iter()
            .any(|call| call.starts_with("draft:Title:")));
    }

    #[test]
    fn over_limit_title_and_content_are_truncated_before_submission() {
        let api = FakeWechatApi::default();
        let result = save_wechat_draft(
            &api,
            "app-id",
            "secret",
            &WechatDraftInput {
                title: "标".repeat(WECHAT_MAX_TITLE_CHARS + 5),
                body: "文".repeat(WECHAT_MAX_ARTICLE_CHARS + 500),
                tags: Vec::new(),
                media: vec![image("cover.png")],
            },
        );

        assert_eq!(result["outcome"], "published");
        assert_eq!(result["truncatedFields"], json!(["title", "content"]));
        let calls = api.calls.borrow();
        let draft = calls
            .iter()
            .find(|call| call.starts_with("draft:"))
            .expect("draft call");
        let submitted_title = draft
            .strip_prefix("draft:")
            .and_then(|value| value.split(':').next())
            .expect("submitted title");
        assert_eq!(submitted_title.chars().count(), WECHAT_MAX_TITLE_CHARS);
        let submitted_content = draft.splitn(4, ':').nth(3).expect("submitted content");
        assert!(wechat_content_fits(submitted_content));
    }

    #[test]
    fn ambiguous_final_transport_failure_is_unknown_and_not_retried() {
        let api = FakeWechatApi::default();
        *api.draft_failure.borrow_mut() = Some(WechatApiFailure::Transport);
        let result = save_wechat_draft(
            &api,
            "app-id",
            "secret",
            &WechatDraftInput {
                title: "Title".to_owned(),
                body: "Body".to_owned(),
                tags: Vec::new(),
                media: vec![image("cover.png")],
            },
        );

        assert_eq!(result["outcome"], "unknown");
        assert_eq!(result["reasonCode"], "wechat_draft_result_unknown");
        assert_eq!(
            api.calls
                .borrow()
                .iter()
                .filter(|call| call.starts_with("draft:"))
                .count(),
            1
        );
    }

    #[test]
    fn api_permission_error_requires_account_configuration() {
        let result = wechat_failure_outcome(
            WechatApiFailure::Api {
                code: 48001,
                message: None,
            },
            true,
        );
        assert_eq!(result["outcome"], "needs_user_action");
        assert_eq!(result["reasonCode"], "wechat_api_unauthorized");
        assert_eq!(result["wechatErrorCode"], 48001);
    }

    #[test]
    fn extracts_only_the_observed_ip_from_a_wechat_allowlist_error() {
        assert_eq!(
            wechat_error_observed_ip(Some(
                "invalid ip 203.0.113.7 ipv6 ::ffff:203.0.113.7, not in whitelist"
            )),
            Some("203.0.113.7".to_owned())
        );
        assert_eq!(
            wechat_error_observed_ip(Some("invalid credential hint")),
            None
        );
    }

    #[test]
    fn multipart_body_sanitizes_the_filename_and_keeps_exact_bytes() {
        let media = WechatDraftMedia {
            bytes: vec![0, 1, 2, 255],
            file_name: "bad\"\r\nname.png".to_owned(),
            mime_type: "image/png".to_owned(),
        };
        let boundary = multipart_boundary(&media.bytes);
        let body = multipart_body(&boundary, &media);
        let rendered = String::from_utf8_lossy(&body);
        assert!(rendered.contains("filename=\"bad___name.png\""));
        assert!(body
            .windows(media.bytes.len())
            .any(|window| window == media.bytes));
    }

    #[test]
    fn invocation_fence_prevents_a_second_draft_request_and_replays_the_result() {
        let temp = tempfile::tempdir().expect("temp");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        fs::create_dir_all(&project_dir).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("wechat-invocation-fence"),
        )
        .expect("paths");
        let mut request = BootstrapRequest::new();
        request.requested_panel_kind = Some(PanelKind::Publishing);
        let bootstrap =
            crate::control::ensure_project_bootstrap(&paths, request).expect("Publishing bootstrap");
        let attempt_id = "publish-attempt:wechat";
        let storage = Storage::open(&paths).expect("storage");
        let typesetting = bootstrap
            .project
            .panel_ids
            .iter()
            .filter_map(|panel_id| {
                storage
                    .read_panel(&bootstrap.project.id, panel_id)
                    .expect("panel")
            })
            .find(|panel| panel.kind == PanelKind::Typesetting)
            .expect("Typesetting panel");
        storage
            .write_panel_state(
                &bootstrap.project.id,
                &typesetting.id,
                &json!({
                    "publications": [{
                        "id": "publication:wechat",
                        "title": "Publication",
                        "contentVersion": 1,
                        "content": [],
                        "createdAt": "2026-01-01T00:00:00.000Z",
                        "updatedAt": "2026-01-01T00:00:00.000Z"
                    }]
                }),
            )
            .expect("publication state");
        let state = json!({
            "selectedPublicationId": null,
            "selectedSkillIds": { "xiaohongshu": DEFAULT_XIAOHONGSHU_SKILL_ID },
            "releases": [{
                "id": "release:wechat",
                "platform": "wechat_official_account",
                "sourcePublicationId": "publication:wechat",
                "snapshot": { "title": "Title", "bodyText": "Body", "media": [] },
                "attempts": [{
                    "id": attempt_id,
                    "taskId": "task:wechat",
                    "requestId": "request:wechat",
                    "mode": "manual",
                    "phase": "committing",
                    "skillId": DEFAULT_XIAOHONGSHU_SKILL_ID,
                    "skillHash": "sha256:test",
                    "outcome": null
                }]
            }]
        });
        storage
            .write_panel_state(&bootstrap.project.id, &bootstrap.panel.id, &state)
            .expect("Publishing state");
        let stored = storage
            .read_panel_state(&bootstrap.project.id, &bootstrap.panel.id)
            .expect("read Publishing state")
            .expect("stored Publishing state");
        assert_eq!(
            stored.pointer("/releases/0/attempts/0/id"),
            Some(&json!(attempt_id))
        );
        let task = json!({
            "projectId": bootstrap.project.id,
            "panelId": bootstrap.panel.id,
            "input": { "attemptId": attempt_id }
        });

        assert!(begin_wechat_draft_invocation(&paths, &task)
            .expect("first invocation")
            .is_none());
        let interrupted = begin_wechat_draft_invocation(&paths, &task)
            .expect("interrupted invocation")
            .expect("unknown result");
        assert_eq!(interrupted["outcome"], "unknown");

        let saved = wechat_draft_outcome(
            "published",
            "saved",
            None,
            Some("draft-media-id".to_owned()),
        );
        finish_wechat_draft_invocation(&paths, &task, &saved).expect("record result");
        assert_eq!(
            begin_wechat_draft_invocation(&paths, &task)
                .expect("replayed invocation")
                .expect("stored result"),
            saved
        );
    }
}
