fn finalize_publishing_result(outcome: &str) -> (Value, Value) {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    let workspace = temp.path().join("workspace");
    std::fs::create_dir_all(&project_dir).expect("project");
    std::fs::create_dir_all(&workspace).expect("workspace");
    let paths = crate::paths::resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("publishing-finalization-test"),
    )
    .expect("paths");
    let bootstrap = crate::control::ensure_project_bootstrap(
        &paths,
        crate::control::BootstrapRequest::new(),
    )
    .expect("bootstrap");
    let publishing = bootstrap
        .panels
        .iter()
        .find(|panel| panel.panel.kind == crate::types::PanelKind::Publishing)
        .expect("Publishing panel");
    let typesetting = bootstrap
        .panels
        .iter()
        .find(|panel| panel.panel.kind == crate::types::PanelKind::Typesetting)
        .expect("Typesetting panel");
    let publication_id = "publication:test";
    let release_id = "release:test";
    let publishing_attempt_id = "publish-attempt:test";
    let storage = crate::storage::Storage::open(&paths).expect("storage");
    storage
        .write_panel_state(
            &bootstrap.project.id,
            &typesetting.panel.id,
            &json!({
                "publications": [{
                    "id": publication_id,
                    "title": "Title",
                    "covers": [],
                    "content": {
                        "type": "doc",
                        "content": [{
                            "type": "paragraph",
                            "content": [{ "type": "text", "text": "Body" }]
                        }]
                    },
                    "createdAt": "2026-07-28T00:00:00Z",
                    "updatedAt": "2026-07-28T00:00:00Z"
                }]
            }),
        )
        .expect("Typesetting state");
    let task = storage
        .insert_task(
            &bootstrap.project.id,
            &publishing.panel.id,
            "release",
            crate::release::XIAOHONGSHU_TASK_TYPE,
            "release.xiaohongshu",
            release_id,
            &json!({
                "platform": "xiaohongshu",
                "releaseId": release_id,
                "attemptId": publishing_attempt_id,
                "executionMode": "auto",
                "publishingSkillSnapshot": { "id": "release-xiaohongshu" },
            }),
            &json!({ "publishingPanelId": publishing.panel.id }),
        )
        .expect("Publishing task");
    let task_id = task["id"].as_str().expect("task id");
    let state = json!({
        "selectedPublicationId": null,
        "selectedSkillIds": {
            "xiaohongshu": "release-xiaohongshu",
            "wechatOfficialAccount": "release-wechat-official-account"
        },
        "releases": [{
            "id": release_id,
            "platform": "xiaohongshu",
            "sourcePublicationId": publication_id,
            "snapshot": { "title": "Title", "bodyText": "Body", "media": [] },
            "attempts": [{
                "id": publishing_attempt_id,
                "taskId": task_id,
                "requestId": "request:test",
                "mode": "auto",
                "skillId": "release-xiaohongshu",
                "skillHash": "sha256:test",
                "phase": "queued",
                "outcome": null
            }]
        }]
    });
    assert!(
        crate::release::validate_state(&state),
        "test Publishing state must be valid: {state}"
    );
    storage
        .write_panel_state(&bootstrap.project.id, &publishing.panel.id, &state)
        .expect("Publishing state");
    let stored_state = storage
        .read_panel_state(&bootstrap.project.id, &publishing.panel.id)
        .expect("stored state")
        .expect("stored Publishing state");
    assert_eq!(
        stored_state["releases"][0]["attempts"][0]["id"],
        publishing_attempt_id,
        "stored state: {stored_state}"
    );
    let queued = crate::tasks::inspect_task(&paths, task_id).expect("queued task");
    assert_eq!(queued["task"]["dispatchState"], "manual");
    assert_eq!(queued["task"]["compatibleTargetCount"], 0);
    assert!(
        crate::tasks::claim_task(&paths, task_id, "agent-cli:codex").is_err(),
        "Publishing Tasks must not be claimable by Agent CLI"
    );
    let claim = crate::tasks::claim_task(
        &paths,
        task_id,
        "agent-cli:manual-task-handoff:test",
    )
    .expect("Agent Message handoff claim");
    assert_eq!(
        claim["task"]["input"]["attemptId"],
        publishing_attempt_id,
        "claimed task: {}",
        claim["task"]
    );
    assert_eq!(
        claim["task"]["executionMethod"]["kind"],
        "manualInstruction"
    );
    if outcome == "published" {
        crate::release::checkpoint_attempt_for_broker(&paths, task_id, "committing")
            .expect("committing checkpoint");
    }
    let result = json!({
        "outcome": outcome,
        "summary": if outcome == "published" {
            "Published successfully."
        } else {
            "User action is required."
        },
        "artifacts": [],
        "platform": "xiaohongshu",
        "releaseId": release_id,
        "attemptId": publishing_attempt_id,
        "reasonCode": if outcome == "published" {
            Value::Null
        } else {
            json!("login_required")
        },
        "remoteUrl": null,
        "publishedAt": if outcome == "published" {
            json!("2026-07-28T08:00:00Z")
        } else {
            Value::Null
        }
    });
    std::fs::write(
        workspace.join(EXECUTION_RESULT_FILE),
        serde_json::to_vec(&result).expect("result json"),
    )
    .expect("execution result");
    let finalized = finalize_execution_unit(
        &paths,
        FinalizeExecutionUnitRequest {
            task: &claim["task"],
            workspace: &workspace,
            handler_key: "handler.release.xiaohongshu",
            execution_bundle_hash: "sha256:test-bundle",
            attempt_id: claim["attemptId"].as_str().expect("attempt id"),
            execution_generation: claim["executionGeneration"]
                .as_i64()
                .expect("execution generation"),
            lease_token: claim["leaseToken"].as_str().expect("lease token"),
            execution_token: "",
        },
    )
    .expect("finalization");
    let panel_state = crate::storage::Storage::open(&paths)
        .expect("storage")
        .read_panel_state(&bootstrap.project.id, &publishing.panel.id)
        .expect("panel state")
        .expect("Publishing state");
    (finalized, panel_state)
}

#[test]
fn publishing_attempts_reject_agent_cli_mode() {
    let temp = tempfile::tempdir().expect("temp");
    let project = temp.path().join("project");
    let storage = temp.path().join("storage");
    std::fs::create_dir_all(&project).expect("project");
    let paths = crate::paths::resolve_myopenpanels_paths(
        Some(project.to_str().unwrap()),
        Some(storage.to_str().unwrap()),
        Some("publishing-agent-message-test"),
    )
    .expect("paths");

    let error = crate::release::create_attempt(
        &paths,
        "release:test",
        crate::release::DEFAULT_XIAOHONGSHU_SKILL_ID,
        "request:test",
        "auto",
        false,
    )
    .expect_err("Agent CLI mode must be rejected");

    assert_eq!(error.code(), Some("invalid_publishing_request"));
    assert!(error.message().contains("Agent Message"));
}

#[test]
fn non_published_release_result_fails_the_task_and_preserves_the_observed_outcome() {
    let (finalized, panel_state) = finalize_publishing_result("needs_user_action");

    assert_eq!(finalized["status"], "failed");
    assert_eq!(finalized["lifecycle"]["task"]["status"], "failed");
    assert_eq!(
        finalized["lifecycle"]["task"]["result"]["outcome"],
        "needs_user_action"
    );
    assert_eq!(
        finalized["lifecycle"]["task"]["attempts"][0]["failureClass"],
        "terminal_task"
    );
    assert_eq!(
        panel_state["releases"][0]["attempts"][0]["outcome"],
        "needs_user_action"
    );
    assert_eq!(
        panel_state["releases"][0]["attempts"][0]["phase"],
        "completed"
    );
}

#[test]
fn observed_published_release_result_is_the_only_successful_task_outcome() {
    let (finalized, panel_state) = finalize_publishing_result("published");

    assert_eq!(finalized["status"], "succeeded");
    assert_eq!(finalized["lifecycle"]["task"]["status"], "succeeded");
    assert_eq!(
        panel_state["releases"][0]["attempts"][0]["outcome"],
        "published"
    );
}
