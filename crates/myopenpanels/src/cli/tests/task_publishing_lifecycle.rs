use sha2::Digest;

fn add_scope_publishing_task(
    paths: &crate::paths::MyOpenPanelsPaths,
    bootstrap: &crate::types::ProjectBootstrap,
    platform: &str,
    task_type: &str,
    capability: &str,
) -> Value {
    let publishing = bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == crate::types::PanelKind::Publishing)
        .expect("publishing panel");
    let storage = Storage::open(paths).expect("storage");
    let release_id = format!("release:{platform}");
    let attempt_id = format!("publish-attempt:{platform}");
    let task_id = format!("task:publishing-{platform}");
    let skill_id = format!("publishing-{platform}");
    let skill_bytes = format!("# {platform} publishing\n").into_bytes();
    let skill_content_hash = format!("sha256:{:x}", sha2::Sha256::digest(&skill_bytes));
    let written = storage
        .write_asset_from_buffer(
            &bootstrap.project.id,
            &publishing.panel.id,
            &format!("releases/{release_id}/attempts/{attempt_id}/skill/SKILL.md"),
            &skill_bytes,
            true,
        )
        .expect("publishing Skill snapshot");
    let mut manifest_hash = sha2::Sha256::new();
    manifest_hash.update(b"SKILL.md");
    manifest_hash.update(skill_content_hash.as_bytes());
    let skill_hash = format!("sha256:{:x}", manifest_hash.finalize());
    let now = crate::control::now_iso();
    let attempt = json!({
        "id": attempt_id,
        "taskId": task_id,
        "requestId": format!("publishing-request:{platform}"),
        "mode": "manual",
        "skillId": skill_id,
        "skillName": format!("{platform} publishing"),
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
    let state = json!({
        "schemaVersion": 1,
        "selectedPublicationId": "publication:publishing-handoff",
        "selectedSkillIds": { "xiaohongshu": skill_id },
        "releases": [{
            "id": release_id,
            "platform": platform,
            "sourcePublicationId": "publication:publishing-handoff",
            "sourceUpdatedAt": now,
            "snapshot": {
                "title": "Runtime publishing",
                "bodyText": "Publish this immutable snapshot.",
                "media": [],
            },
            "attempts": [attempt],
            "createdAt": now,
            "updatedAt": now,
        }],
    });
    let task = crate::storage::TaskInsert {
        id: task_id,
        queue: "publishing".to_owned(),
        task_type: task_type.to_owned(),
        capability: capability.to_owned(),
        target_ref: release_id,
        input: json!({
            "platform": platform,
            "releaseId": state["releases"][0]["id"],
            "attemptId": attempt_id,
            "snapshot": state["releases"][0]["snapshot"],
            "publishingSkillId": skill_id,
            "publishingSkillSnapshot": {
                "id": skill_id,
                "name": format!("{platform} publishing"),
                "source": "test",
                "contentHash": skill_hash,
                "files": [{
                    "path": "SKILL.md",
                    "assetRef": written.asset_ref,
                    "contentHash": skill_content_hash,
                    "sizeBytes": skill_bytes.len(),
                }],
            },
        }),
        source: json!({
            "publishingPanelId": publishing.panel.id,
            "sourcePublicationId": "publication:publishing-handoff",
        }),
        max_attempts: 1,
        dispatch_mode: "manual".to_owned(),
        idempotency_key: Some(format!("publishing-request:{platform}")),
        exclusive_non_terminal: false,
    };
    storage
        .insert_tasks_with_panel_states(
            &bootstrap.project.id,
            &publishing.panel.id,
            &[task],
            &[(&publishing.panel.id, &state)],
        )
        .expect("publishing Task")
        .0
        .into_iter()
        .next()
        .expect("inserted publishing Task")
}

#[test]
fn publishing_task_handoff_claims_heartbeats_and_completes_supported_platforms() {
    for (platform, task_type, capability, handler_key) in [
        (
            "xiaohongshu",
            crate::publishing::XIAOHONGSHU_TASK_TYPE,
            crate::publishing::XIAOHONGSHU_CAPABILITY,
            "handler.publishing.xiaohongshu",
        ),
        (
            "wechat_official_account",
            crate::publishing::WECHAT_OFFICIAL_ACCOUNT_TASK_TYPE,
            crate::publishing::WECHAT_OFFICIAL_ACCOUNT_CAPABILITY,
            "handler.publishing.wechat-official-account",
        ),
    ] {
        let (_temp, paths, bootstrap) = scope_test_project();
        let task = add_scope_publishing_task(
            &paths,
            &bootstrap,
            platform,
            task_type,
            capability,
        );
        let task_id = task["id"].as_str().unwrap().to_owned();
        let release_id = task["input"]["releaseId"].as_str().unwrap().to_owned();
        let attempt_id = task["input"]["attemptId"].as_str().unwrap().to_owned();
        let _broker = crate::content::enable_test_task_broker();

        let started = tasks::start_task_handoff(
            &paths,
            &tasks::TaskExecutionScope::ExactTask {
                task_id: task_id.clone(),
            },
        )
        .expect("start publishing handoff");
        assert_eq!(started["scopeState"], "running", "{started}");
        assert_eq!(started["executionBundle"]["handlerKey"], handler_key);
        let delivery_prompt = started["delivery"]["prompt"].as_str().unwrap();
        assert!(delivery_prompt.contains("limited to this claimed Task"));
        assert!(!delivery_prompt.contains("continue with every Task returned"));
        let handoff_id = started["handoff"]["id"].as_str().unwrap();
        assert!(delivery_prompt.contains(&format!(
            "task handoff complete --handoff-id {handoff_id} --format json"
        )));
        assert!(delivery_prompt.contains(&format!(
            "publishing checkpoint --task-id {task_id} --phase prepared --format json"
        )));
        assert!(delivery_prompt.contains("# Publishing Panel Contract"));
        let agent_cli_instructions = started["executionBundle"]["instructions"]
            .as_str()
            .unwrap();
        assert!(agent_cli_instructions.contains(&format!(
            "publishing checkpoint --task-id {task_id} --phase prepared --format json"
        )));
        assert!(!agent_cli_instructions.contains("task handoff exec"));
        assert!(delivery_prompt.contains("task handoff exec"));
        assert_eq!(
            started["executionBundle"]["allowedAgentCommandIntents"],
            json!(["publishing.checkpoint"])
        );
        tasks::heartbeat_task_handoff(&paths, handoff_id).expect("publishing heartbeat");
        crate::publishing::checkpoint_attempt_for_broker(&paths, &task_id, "prepared")
            .expect("prepared checkpoint");
        crate::publishing::checkpoint_attempt_for_broker(&paths, &task_id, "committing")
            .expect("committing checkpoint");

        let result_path = started["executionBundle"]["workspace"]["resultFilePath"]
            .as_str()
            .unwrap();
        fs::write(
            result_path,
            serde_json::to_vec(&json!({
                "schemaVersion": 2,
                "outcome": "published",
                "summary": format!("Completed {platform} publishing."),
                "artifacts": [],
                "platform": platform,
                "releaseId": release_id,
                "attemptId": attempt_id,
                "reasonCode": null,
                "remoteUrl": null,
                "publishedAt": "2026-07-21T10:00:00.000Z",
            }))
            .unwrap(),
        )
        .expect("publishing result");

        let completed =
            tasks::complete_task_handoff(&paths, handoff_id).expect("complete publishing handoff");
        assert_eq!(completed["scopeState"], "complete", "{completed}");
        assert_eq!(completed["previousExecution"]["status"], "succeeded");
        assert_eq!(
            tasks::inspect_task(&paths, &task_id).unwrap()["task"]["status"],
            "succeeded"
        );
        let publishing_panel_id = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == crate::types::PanelKind::Publishing)
            .unwrap()
            .panel
            .id
            .as_str();
        let state = Storage::open(&paths)
            .unwrap()
            .read_panel_state(&bootstrap.project.id, publishing_panel_id)
            .unwrap()
            .unwrap();
        let completed_attempt = &state["releases"][0]["attempts"][0];
        assert_eq!(completed_attempt["phase"], "completed");
        assert_eq!(completed_attempt["outcome"], "published");
        let deleted = tasks::delete_task(&paths, &task_id).expect("delete completed task");
        assert_eq!(deleted["task"]["status"], "succeeded");
        assert!(deleted["task"]["archivedAt"].is_string());
        let state_after_delete = Storage::open(&paths)
            .unwrap()
            .read_panel_state(&bootstrap.project.id, publishing_panel_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            state_after_delete["releases"][0]["attempts"][0]["outcome"],
            "published"
        );
    }
}

#[test]
fn publishing_task_handoff_failure_and_stop_preserve_single_attempt_safety() {
    let (_temp, paths, bootstrap) = scope_test_project();
    let task = add_scope_publishing_task(
        &paths,
        &bootstrap,
        "xiaohongshu",
        crate::publishing::XIAOHONGSHU_TASK_TYPE,
        crate::publishing::XIAOHONGSHU_CAPABILITY,
    );
    let task_id = task["id"].as_str().unwrap().to_owned();
    let _broker = crate::content::enable_test_task_broker();
    let started = tasks::start_task_handoff(
        &paths,
        &tasks::TaskExecutionScope::ExactTask {
            task_id: task_id.clone(),
        },
    )
    .expect("start publishing handoff");
    let handoff_id = started["handoff"]["id"].as_str().unwrap();
    crate::publishing::checkpoint_attempt_for_broker(&paths, &task_id, "prepared")
        .expect("prepared checkpoint");
    crate::publishing::checkpoint_attempt_for_broker(&paths, &task_id, "committing")
        .expect("committing checkpoint");

    let failed = tasks::fail_task_handoff(
        &paths,
        handoff_id,
        "The publishing result could not be confirmed.",
        tasks::TaskFailureClass::TerminalTask,
    )
    .expect("fail publishing handoff");
    assert_eq!(failed["scopeState"], "blocked", "{failed}");
    assert_eq!(failed["previousExecution"]["status"], "failed");
    assert_eq!(
        tasks::inspect_task(&paths, &task_id).unwrap()["task"]["status"],
        "failed"
    );
    let deleted = tasks::delete_task(&paths, &task_id).expect("delete failed publishing task");
    assert_eq!(deleted["task"]["status"], "cancelled");
    assert!(deleted["task"]["archivedAt"].is_string());
    let publishing_panel_id = bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == crate::types::PanelKind::Publishing)
        .unwrap()
        .panel
        .id
        .as_str();
    let state = Storage::open(&paths)
        .unwrap()
        .read_panel_state(&bootstrap.project.id, publishing_panel_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        state["releases"][0]["attempts"][0]["outcome"],
        "not_published"
    );

    let (_temp, paths, bootstrap) = scope_test_project();
    let task = add_scope_publishing_task(
        &paths,
        &bootstrap,
        "xiaohongshu",
        crate::publishing::XIAOHONGSHU_TASK_TYPE,
        crate::publishing::XIAOHONGSHU_CAPABILITY,
    );
    let task_id = task["id"].as_str().unwrap().to_owned();
    let _broker = crate::content::enable_test_task_broker();
    let started = tasks::start_task_handoff(
        &paths,
        &tasks::TaskExecutionScope::ExactTask {
            task_id: task_id.clone(),
        },
    )
    .expect("start publishing handoff");
    tasks::stop_task_handoff(&paths, started["handoff"]["id"].as_str().unwrap())
        .expect("stop publishing handoff");
    assert_eq!(
        tasks::inspect_task(&paths, &task_id).unwrap()["task"]["status"],
        "cancelled"
    );
    assert_eq!(
        tasks::read_task_scope(
            &paths,
            &tasks::TaskExecutionScope::ExactTask { task_id }
        )
        .unwrap()["scopeState"],
        "complete"
    );
}

#[test]
fn deleting_a_queued_publishing_task_archives_it_and_closes_the_attempt() {
    let (_temp, paths, bootstrap) = scope_test_project();
    let task = add_scope_publishing_task(
        &paths,
        &bootstrap,
        "xiaohongshu",
        crate::publishing::XIAOHONGSHU_TASK_TYPE,
        crate::publishing::XIAOHONGSHU_CAPABILITY,
    );
    let task_id = task["id"].as_str().unwrap();

    let deleted = tasks::delete_task(&paths, task_id).expect("delete publishing task");
    assert_eq!(deleted["task"]["status"], "cancelled");
    assert!(deleted["task"]["archivedAt"].is_string());
    assert!(
        !tasks::list_tasks(&paths, tasks::TaskListFilter::default()).unwrap()["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|candidate| candidate["id"] == task_id)
    );
    assert_eq!(
        tasks::read_task_scope(
            &paths,
            &tasks::TaskExecutionScope::ExactTask {
                task_id: task_id.to_owned(),
            },
        )
        .unwrap()["scopeState"],
        "complete"
    );

    let publishing_panel_id = bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == crate::types::PanelKind::Publishing)
        .unwrap()
        .panel
        .id
        .as_str();
    let state = Storage::open(&paths)
        .unwrap()
        .read_panel_state(&bootstrap.project.id, publishing_panel_id)
        .unwrap()
        .unwrap();
    let attempt = &state["releases"][0]["attempts"][0];
    assert_eq!(attempt["phase"], "completed");
    assert_eq!(attempt["outcome"], "not_published");
    assert_eq!(attempt["reasonCode"], "user_cancelled");
}

#[test]
fn deleting_a_legacy_cancelled_publishing_task_repairs_its_attempt() {
    let (_temp, paths, bootstrap) = scope_test_project();
    let task = add_scope_publishing_task(
        &paths,
        &bootstrap,
        "xiaohongshu",
        crate::publishing::XIAOHONGSHU_TASK_TYPE,
        crate::publishing::XIAOHONGSHU_CAPABILITY,
    );
    let task_id = task["id"].as_str().unwrap();
    let storage = Storage::open(&paths).expect("storage");
    storage
        .connection()
        .execute(
            "UPDATE tasks SET status = 'cancelled', error_json = json_object('code', 'user_cancelled') WHERE id = ?",
            [task_id],
        )
        .expect("simulate cancellation from a runtime without a publishing adapter");

    let publishing_panel_id = bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == crate::types::PanelKind::Publishing)
        .unwrap()
        .panel
        .id
        .as_str();
    let stale_state = storage
        .read_panel_state(&bootstrap.project.id, publishing_panel_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        stale_state["releases"][0]["attempts"][0]["phase"],
        "queued"
    );
    assert!(stale_state["releases"][0]["attempts"][0]["outcome"].is_null());

    let deleted = tasks::delete_task(&paths, task_id).expect("delete legacy cancelled task");
    assert!(deleted["task"]["archivedAt"].is_string());
    let repaired_state = Storage::open(&paths)
        .unwrap()
        .read_panel_state(&bootstrap.project.id, publishing_panel_id)
        .unwrap()
        .unwrap();
    let repaired_attempt = &repaired_state["releases"][0]["attempts"][0];
    assert_eq!(repaired_attempt["phase"], "completed");
    assert_eq!(repaired_attempt["outcome"], "not_published");
    assert_eq!(repaired_attempt["reasonCode"], "user_cancelled");
}

#[test]
fn deleting_a_running_task_is_rejected() {
    let (_temp, paths, bootstrap) = scope_test_project();
    let task = add_scope_publishing_task(
        &paths,
        &bootstrap,
        "xiaohongshu",
        crate::publishing::XIAOHONGSHU_TASK_TYPE,
        crate::publishing::XIAOHONGSHU_CAPABILITY,
    );
    let task_id = task["id"].as_str().unwrap();
    let _broker = crate::content::enable_test_task_broker();
    let started = tasks::start_task_handoff(
        &paths,
        &tasks::TaskExecutionScope::ExactTask {
            task_id: task_id.to_owned(),
        },
    )
    .expect("start publishing task");

    let error = tasks::delete_task(&paths, task_id).expect_err("reject running delete");
    assert_eq!(error.code(), Some("invalid_task_transition"));
    assert_eq!(
        tasks::inspect_task(&paths, task_id).unwrap()["task"]["status"],
        "running"
    );
    tasks::stop_task_handoff(&paths, started["handoff"]["id"].as_str().unwrap())
        .expect("stop publishing task");
}
