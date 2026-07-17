#[test]
fn retryable_failure_falls_through_ordered_model_channels() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let bootstrap = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let wiki_panel = bootstrap
        .panels
        .iter()
        .find(|panel| panel.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");
    let mut storage = Storage::open(&paths).expect("storage");
    crate::model_gateway::sync_builtin_local_cli_registry(&mut storage).expect("gateway registry");
    storage
        .connection()
        .execute(
            "UPDATE model_gateway_connections SET enabled = 1 WHERE id IN ('local-cli:codex', 'local-cli:hermes')",
            [],
        )
        .expect("enable test channels");
    let task = storage
        .insert_task(
            &bootstrap.project.id,
            &wiki_panel.panel.id,
            "wiki",
            "maintain_wiki",
            "wiki.maintain",
            "index.md",
            &json!({}),
            &json!({ "wikiSpaceId": "wiki:default" }),
        )
        .expect("task");
    let register = |name: &str, priority: i64, connection_id: &str| {
        tasks::register_target(
            &paths,
            tasks::TargetRegistration {
                name,
                host: Some("test"),
                transport: "poll",
                capabilities: vec!["wiki.maintain".to_owned()],
                priority,
                protocol_version: 3,
                max_concurrency: 1,
                model_gateway_connection_id: Some(connection_id),
            },
        )
        .expect("target")
    };
    let primary = register("primary-channel", 100, "local-cli:codex");
    let fallback = register("fallback-channel", 50, "local-cli:hermes");
    tasks::set_agent_route(
        &paths,
        "wiki.maintain",
        &[
            primary["target"]["id"].as_str().unwrap().to_owned(),
            fallback["target"]["id"].as_str().unwrap().to_owned(),
        ],
    )
    .expect("route");

    let first = tasks::claim_task(
        &paths,
        task["id"].as_str().unwrap(),
        primary["target"]["id"].as_str().unwrap(),
    )
    .expect("primary claim");
    let failed = tasks::fail_task_with_class(
        &paths,
        task["id"].as_str().unwrap(),
        first["leaseToken"].as_str().unwrap(),
        "primary unavailable",
        None,
        tasks::TaskFailureClass::RetryableChannel,
    )
    .expect("retryable failure");
    assert_eq!(failed["task"]["ready"], true);

    let second = tasks::claim_task(
        &paths,
        task["id"].as_str().unwrap(),
        fallback["target"]["id"].as_str().unwrap(),
    )
    .expect("fallback claim");
    assert_eq!(
        second["target"]["modelGatewayConnectionId"],
        "local-cli:hermes"
    );
    let attempts =
        tasks::list_task_attempts(&paths, task["id"].as_str().unwrap()).expect("attempt history");
    assert_eq!(attempts["attempts"][0]["failureClass"], "retryable_channel");
    assert_eq!(
        attempts["attempts"][0]["modelGatewayConnectionId"],
        "local-cli:codex"
    );
    assert_eq!(
        attempts["attempts"][1]["modelGatewayConnectionId"],
        "local-cli:hermes"
    );
    assert_eq!(
        attempts["attempts"][0]["executorSnapshot"]["targetName"],
        "primary-channel"
    );
    let round_complete = tasks::fail_task_with_class(
        &paths,
        task["id"].as_str().unwrap(),
        second["leaseToken"].as_str().unwrap(),
        "fallback unavailable",
        None,
        tasks::TaskFailureClass::RetryableChannel,
    )
    .expect("fallback failure");
    assert_eq!(round_complete["task"]["blockedReason"], "retryLater");
    tasks::retry_task(&paths, task["id"].as_str().unwrap()).expect("start next round");
    let third = tasks::claim_task(
        &paths,
        task["id"].as_str().unwrap(),
        primary["target"]["id"].as_str().unwrap(),
    )
    .expect("next round returns to primary");
    assert_eq!(
        third["target"]["modelGatewayConnectionId"],
        "local-cli:codex"
    );
}

#[test]
fn studio_restart_recovers_only_builtin_attempts_without_spending_retry_budget() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let bootstrap = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let wiki_panel = bootstrap
        .panels
        .iter()
        .find(|panel| panel.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");
    let storage = Storage::open(&paths).expect("storage");
    let local_task = storage
        .insert_task(
            &bootstrap.project.id,
            &wiki_panel.panel.id,
            "wiki",
            "maintain_wiki",
            "wiki.maintain",
            "local-index.md",
            &json!({}),
            &json!({ "wikiSpaceId": "wiki:default" }),
        )
        .expect("local task");
    let external_task = storage
        .insert_task(
            &bootstrap.project.id,
            &wiki_panel.panel.id,
            "wiki",
            "maintain_wiki",
            "wiki.maintain",
            "external-index.md",
            &json!({}),
            &json!({ "wikiSpaceId": "wiki:default" }),
        )
        .expect("external task");
    let register = |name: &str| {
        tasks::register_target(
            &paths,
            tasks::TargetRegistration {
                name,
                host: Some("test"),
                transport: "command",
                capabilities: vec!["wiki.maintain".to_owned()],
                priority: 10,
                protocol_version: 3,
                max_concurrency: 1,
                model_gateway_connection_id: None,
            },
        )
        .expect("target")
    };
    let local_target = register("model-gateway:ctx:custom");
    let external_target = register("external-command");
    let local_claim = tasks::claim_task(
        &paths,
        local_task["id"].as_str().unwrap(),
        local_target["target"]["id"].as_str().unwrap(),
    )
    .expect("local claim");
    tasks::claim_task(
        &paths,
        external_task["id"].as_str().unwrap(),
        external_target["target"]["id"].as_str().unwrap(),
    )
    .expect("external claim");
    let before = tasks::inspect_task(&paths, local_task["id"].as_str().unwrap())
        .expect("local before recovery");

    let recovered = tasks::recover_builtin_worker_tasks_after_restart(&paths).expect("recovery");

    assert_eq!(recovered, 1);
    let local = tasks::inspect_task(&paths, local_task["id"].as_str().unwrap())
        .expect("local after recovery");
    assert_eq!(local["task"]["status"], "failed");
    assert_eq!(local["task"]["ready"], true);
    assert_eq!(local["task"]["attempt"], before["task"]["attempt"]);
    assert_eq!(
        local["task"]["maxAttempts"].as_i64(),
        before["task"]["maxAttempts"]
            .as_i64()
            .map(|value| value + 1)
    );
    let attempts = tasks::list_task_attempts(&paths, local_task["id"].as_str().unwrap())
        .expect("local attempts");
    assert_eq!(attempts["attempts"][0]["status"], "interrupted");
    assert_eq!(attempts["attempts"][0]["failureClass"], "retryable_channel");
    assert_eq!(attempts["attempts"][0]["error"]["code"], "studio_restart");
    let fenced = tasks::complete_task(
        &paths,
        local_task["id"].as_str().unwrap(),
        local_claim["leaseToken"].as_str().unwrap(),
        None,
    )
    .expect_err("restarted execution must be fenced");
    assert_eq!(fenced.code(), Some("execution_fenced"));
    let external =
        tasks::inspect_task(&paths, external_task["id"].as_str().unwrap()).expect("external task");
    assert!(matches!(
        external["task"]["status"].as_str(),
        Some("running" | "claimed" | "converting" | "indexing")
    ));
    let retried = tasks::claim_task(
        &paths,
        local_task["id"].as_str().unwrap(),
        local_target["target"]["id"].as_str().unwrap(),
    )
    .expect("immediate local retry");
    assert_eq!(retried["target"]["id"], local_target["target"]["id"]);
}

#[test]
fn preferred_task_dispatch_falls_back_to_other_channels() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let bootstrap = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let wiki_panel = bootstrap
        .panels
        .iter()
        .find(|panel| panel.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");
    let mut storage = Storage::open(&paths).expect("storage");
    crate::model_gateway::sync_builtin_local_cli_registry(&mut storage).expect("gateway registry");
    storage
        .connection()
        .execute(
            "UPDATE model_gateway_connections SET enabled = 1 WHERE id IN ('local-cli:codex', 'local-cli:hermes')",
            [],
        )
        .expect("enable test channels");
    let task = storage
        .insert_task(
            &bootstrap.project.id,
            &wiki_panel.panel.id,
            "wiki",
            "maintain_wiki",
            "wiki.maintain",
            "index.md",
            &json!({}),
            &json!({ "wikiSpaceId": "wiki:default" }),
        )
        .expect("task");
    let register = |name: &str, priority: i64, connection_id: &str| {
        tasks::register_target(
            &paths,
            tasks::TargetRegistration {
                name,
                host: Some("test"),
                transport: "poll",
                capabilities: vec!["wiki.maintain".to_owned()],
                priority,
                protocol_version: 3,
                max_concurrency: 1,
                model_gateway_connection_id: Some(connection_id),
            },
        )
        .expect("target")
    };
    let primary = register("automatic-primary", 100, "local-cli:codex");
    let pinned = register("pinned-channel", 50, "local-cli:hermes");
    let configured = tasks::set_task_dispatch(
        &paths,
        task["id"].as_str().unwrap(),
        "prefer",
        Some("local-cli:hermes"),
    )
    .expect("dispatch override");
    assert_eq!(configured["task"]["dispatchMode"], "prefer");
    assert_eq!(
        configured["task"]["requestedGatewayConnectionId"],
        "local-cli:hermes"
    );
    let rejected = tasks::claim_task(
        &paths,
        task["id"].as_str().unwrap(),
        primary["target"]["id"].as_str().unwrap(),
    )
    .expect_err("fallback must wait for the preferred channel");
    assert_eq!(rejected.code(), Some("task_not_claimable"));
    let claimed = tasks::claim_task(
        &paths,
        task["id"].as_str().unwrap(),
        pinned["target"]["id"].as_str().unwrap(),
    )
    .expect("preferred claim");
    assert_eq!(claimed["target"]["name"], "pinned-channel");
    let failed = tasks::fail_task_with_class(
        &paths,
        task["id"].as_str().unwrap(),
        claimed["leaseToken"].as_str().unwrap(),
        "preferred channel unavailable",
        None,
        tasks::TaskFailureClass::RetryableChannel,
    )
    .expect("preferred failure");
    assert_eq!(failed["task"]["ready"], true);
    let fallback = tasks::claim_task(
        &paths,
        task["id"].as_str().unwrap(),
        primary["target"]["id"].as_str().unwrap(),
    )
    .expect("fallback claim");
    assert_eq!(fallback["target"]["name"], "automatic-primary");

    let exclusive = tasks::set_task_dispatch(
        &paths,
        task["id"].as_str().unwrap(),
        "only",
        Some("local-cli:hermes"),
    )
    .expect_err("exclusive dispatch is unsupported");
    assert_eq!(exclusive.code(), Some("invalid_dispatch_mode"));
}

#[test]
fn zero_exit_without_a_conversion_artifact_is_invalid_output() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    wiki::add_raw_document(
        &paths,
        "invalid.pdf",
        None,
        Some("application/pdf"),
        "user",
        Some("wiki:default"),
        b"pdf fixture",
    )
    .expect("upload");
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let conversion = Storage::open(&paths)
        .expect("storage")
        .list_tasks(&bootstrap.project.id)
        .expect("tasks")
        .into_iter()
        .find(|task| task["type"] == "convert_document_to_markdown")
        .expect("conversion");
    Storage::open(&paths)
        .expect("storage")
        .connection()
        .execute(
            "UPDATE tasks SET max_attempts = 1 WHERE id = ?",
            [conversion["id"].as_str().unwrap()],
        )
        .expect("single attempt task");

    let (code, stdout, stderr) = run(&[
        "agent",
        "bridge",
        "run",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--once",
        "--capability",
        "wiki.convertDocument",
        "--command",
        "true",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let result = serde_json::from_str::<Value>(&stdout).expect("bridge result");
    assert_eq!(result["success"], false);
    assert_eq!(result["lifecycleError"]["code"], "invalid_output");
    let task = tasks::inspect_task(&paths, conversion["id"].as_str().unwrap()).expect("task");
    assert_eq!(task["task"]["status"], "failed");
    let attempts =
        tasks::list_task_attempts(&paths, conversion["id"].as_str().unwrap()).expect("attempts");
    assert_eq!(attempts["attempts"][0]["status"], "invalid_output");
    let retry = tasks::retry_task(&paths, conversion["id"].as_str().unwrap())
        .expect_err("terminal failure cannot retry in place");
    assert_eq!(retry.code(), Some("invalid_task_transition"));
}
