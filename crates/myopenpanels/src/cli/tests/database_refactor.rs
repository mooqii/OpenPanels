#[test]
fn task_retry_budget_is_three_across_agent_cli_fallbacks() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let wiki_panel = bootstrap
        .panels
        .iter()
        .find(|panel| panel.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");
    let mut settings = crate::model_gateway::read_settings(&paths).expect("settings");
    settings.local_cli.provider_id = Some("codex".to_owned());
    settings.local_cli.enabled_provider_ids = vec![
        "codex".to_owned(),
        "hermes".to_owned(),
        "claude".to_owned(),
    ];
    settings.local_cli.provider_order = settings.local_cli.enabled_provider_ids.clone();
    crate::model_gateway::write_settings(&paths, settings).expect("settings");
    let task = Storage::open(&paths)
        .expect("storage")
        .insert_task(
            &bootstrap.project.id,
            &wiki_panel.panel.id,
            "wiki",
            "maintain_wiki",
            "wiki.maintain",
            "wiki:default",
            &json!({ "changeEvents": [] }),
            &json!({ "wikiSpaceId": "wiki:default", "agentSkillId": "wiki-default" }),
        )
        .expect("task");
    let task_id = task["id"].as_str().expect("task id");
    let _broker = crate::content::enable_test_task_broker();

    for (index, provider) in ["codex", "hermes", "claude"].into_iter().enumerate() {
        let claim = tasks::claim_for_worker(
            &paths,
            &format!("agent-cli:{provider}"),
            Some("wiki.maintain"),
            Some("wiki"),
        )
        .expect("claim");
        assert_eq!(
            claim["task"]["id"],
            task_id,
            "provider={provider}, claim={claim}, task={}, settings={}",
            tasks::inspect_task(&paths, task_id).expect("inspect"),
            serde_json::to_value(crate::model_gateway::read_settings(&paths).expect("settings"))
                .expect("settings json")
        );
        assert_eq!(claim["task"]["attempt"], (index + 1) as i64);
        tasks::fail_task_with_class(
            &paths,
            task_id,
            claim["leaseToken"].as_str().expect("lease"),
            "channel unavailable",
            Some(&crate::control::now_iso()),
            tasks::TaskFailureClass::RetryableChannel,
        )
        .expect("failure");
    }
    let inspected = tasks::inspect_task(&paths, task_id).expect("task");
    assert_eq!(inspected["task"]["status"], "failed");
    assert_eq!(inspected["task"]["attempt"], 3);
    assert_eq!(inspected["task"]["attemptLimit"], 3);
    assert_eq!(inspected["task"]["attempts"].as_array().unwrap().len(), 3);
    assert!(tasks::claim_task(&paths, task_id, "agent-cli:codex").is_err());
}

#[test]
fn manual_retry_creates_a_fresh_linked_task() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let wiki_panel = bootstrap
        .panels
        .iter()
        .find(|panel| panel.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");
    let original = Storage::open(&paths)
        .expect("storage")
        .insert_task(
            &bootstrap.project.id,
            &wiki_panel.panel.id,
            "wiki",
            "maintain_wiki",
            "wiki.maintain",
            "wiki:default",
            &json!({ "changeEvents": [] }),
            &json!({ "wikiSpaceId": "wiki:default", "agentSkillId": "wiki-default" }),
        )
        .expect("task");
    let original_id = original["id"].as_str().expect("id");
    Storage::open(&paths)
        .expect("storage")
        .connection()
        .execute(
            "UPDATE tasks SET status = 'failed', attempt_count = 3, completed_at = ? WHERE id = ?",
            params![crate::control::now_iso(), original_id],
        )
        .expect("terminal task");

    let retried = tasks::retry_task(&paths, original_id).expect("retry");
    assert_ne!(retried["task"]["id"], original_id);
    assert_eq!(retried["task"]["retryOfTaskId"], original_id);
    assert_eq!(retried["task"]["attempt"], 0);
    assert_eq!(
        tasks::inspect_task(&paths, original_id).expect("original")["task"]["attempt"],
        3
    );
}

#[test]
fn claim_transaction_enforces_global_concurrency_and_mutation_order() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let wiki_panel = bootstrap
        .panels
        .iter()
        .find(|panel| panel.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");
    let create_task = || {
        Storage::open(&paths)
            .expect("storage")
            .insert_task(
                &bootstrap.project.id,
                &wiki_panel.panel.id,
                "wiki",
                "maintain_wiki",
                "wiki.maintain",
                "wiki:default",
                &json!({ "changeEvents": [] }),
                &json!({ "wikiSpaceId": "wiki:default", "agentSkillId": "wiki-default" }),
            )
            .expect("task")["id"]
            .as_str()
            .expect("task id")
            .to_owned()
    };
    let _broker = crate::content::enable_test_task_broker();

    let mut settings = crate::model_gateway::read_settings(&paths).expect("settings");
    settings.max_concurrency = 1;
    crate::model_gateway::write_settings(&paths, settings).expect("settings");
    let first_pair = [create_task(), create_task()];
    let first_claim = tasks::claim_for_worker(&paths, "agent-cli:codex", None, Some("wiki"))
        .expect("first claim");
    let claimed_id = first_claim["task"]["id"].as_str().expect("claimed id");
    assert!(first_pair.iter().any(|task_id| task_id == claimed_id));
    let blocked = tasks::claim_for_worker(&paths, "agent-cli:codex", None, Some("wiki"))
        .expect("concurrency check");
    assert!(blocked["task"].is_null());
    tasks::fail_task_with_class(
        &paths,
        claimed_id,
        first_claim["leaseToken"].as_str().expect("lease"),
        "finish concurrency fixture",
        None,
        tasks::TaskFailureClass::TerminalTask,
    )
    .expect("terminal failure");
    let next_claim = tasks::claim_for_worker(&paths, "agent-cli:codex", None, Some("wiki"))
        .expect("next claim");
    assert_eq!(
        next_claim["task"]["id"],
        first_pair
            .iter()
            .find(|task_id| task_id.as_str() != claimed_id)
            .expect("other task")
            .as_str()
    );
    tasks::fail_task_with_class(
        &paths,
        next_claim["task"]["id"].as_str().expect("task id"),
        next_claim["leaseToken"].as_str().expect("lease"),
        "finish concurrency fixture",
        None,
        tasks::TaskFailureClass::TerminalTask,
    )
    .expect("terminal failure");

    let mut settings = crate::model_gateway::read_settings(&paths).expect("settings");
    settings.max_concurrency = 2;
    crate::model_gateway::write_settings(&paths, settings).expect("settings");
    let mutation_pair = [create_task(), create_task()];
    let storage = Storage::open(&paths).expect("storage");
    for task_id in &mutation_pair {
        storage
            .connection()
            .execute(
                "UPDATE tasks SET mutation_key = 'wiki:default' WHERE id = ?",
                [task_id],
            )
            .expect("mutation key");
    }
    drop(storage);
    let mutation_claim =
        tasks::claim_for_worker(&paths, "agent-cli:codex", None, Some("wiki"))
            .expect("mutation claim");
    let mutation_claimed_id = mutation_claim["task"]["id"].as_str().expect("task id");
    assert!(mutation_pair
        .iter()
        .any(|task_id| task_id == mutation_claimed_id));
    let serialized = tasks::claim_for_worker(&paths, "agent-cli:codex", None, Some("wiki"))
        .expect("mutation serialization");
    assert!(serialized["task"].is_null());
}

#[test]
fn task_claim_and_heartbeat_do_not_write_domain_panel_state() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let wiki_space_id = active_wiki_space_id(&project_dir, &storage_dir);
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let uploaded = crate::wiki::add_raw_document(
        &paths,
        "source.png",
        Some("Source"),
        Some("image/png"),
        "user",
        Some(&wiki_space_id),
        b"binary source",
    )
    .expect("raw document");
    let task_id = uploaded["document"]["conversion"]["taskId"]
        .as_str()
        .expect("task id");
    let storage = Storage::open(&paths).expect("storage");
    let revision = storage
        .read_panel_state_revision(&bootstrap.project.id, &bootstrap.panel.id)
        .expect("revision");
    drop(storage);
    let _broker = crate::content::enable_test_task_broker();
    let claim = tasks::claim_task(&paths, task_id, "agent-cli:codex").expect("claim");
    assert_eq!(claim["task"]["status"], "running");
    tasks::heartbeat_task(
        &paths,
        task_id,
        claim["leaseToken"].as_str().expect("lease"),
    )
    .expect("heartbeat");
    assert_eq!(
        Storage::open(&paths)
            .expect("storage")
            .read_panel_state_revision(&bootstrap.project.id, &bootstrap.panel.id)
            .expect("revision"),
        revision
    );
}

#[test]
fn wiki_mutation_scope_skips_an_update_with_a_running_conversion() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let wiki_space_id = active_wiki_space_id(&project_dir, &storage_dir);
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let binary = crate::wiki::add_raw_document(
        &paths,
        "source.png",
        Some("Source"),
        Some("image/png"),
        "user",
        Some(&wiki_space_id),
        b"binary source",
    )
    .expect("binary document");
    std::thread::sleep(std::time::Duration::from_millis(2));
    let markdown = crate::wiki::add_raw_document(
        &paths,
        "ready.md",
        Some("Ready"),
        Some("text/markdown"),
        "user",
        Some(&wiki_space_id),
        b"# Ready",
    )
    .expect("markdown document");
    std::thread::sleep(std::time::Duration::from_millis(2));
    let second_markdown = crate::wiki::add_raw_document(
        &paths,
        "also-ready.md",
        Some("Also ready"),
        Some("text/markdown"),
        "user",
        Some(&wiki_space_id),
        b"# Also ready",
    )
    .expect("second markdown document");
    let conversion_id = binary["document"]["conversion"]["taskId"]
        .as_str()
        .expect("conversion task");
    let ready_ingestion_id = markdown["document"]["ingestionByWikiSpace"]
        [&wiki_space_id]["taskId"]
        .as_str()
        .expect("ready ingestion");
    let second_ready_ingestion_id = second_markdown["document"]["ingestionByWikiSpace"]
        [&wiki_space_id]["taskId"]
        .as_str()
        .expect("second ready ingestion");
    let listed = tasks::list_tasks(&paths, tasks::TaskListFilter::default()).expect("task list");
    for task_id in [ready_ingestion_id, second_ready_ingestion_id] {
        let task = listed["tasks"]
            .as_array()
            .expect("tasks")
            .iter()
            .find(|task| task["id"] == task_id)
            .expect("ready Wiki update");
        assert_eq!(task["ready"], true);
        assert_eq!(task["mutationBlocked"], false);
        assert!(task["blockedReason"].is_null());
    }
    let mutation_key = format!("wiki:{}", bootstrap.project.id);
    let scope = tasks::TaskExecutionScope::WikiMutationDrain {
        project_id: bootstrap.project.id,
        mutation_key,
    };
    let _broker = crate::content::enable_test_task_broker();

    let conversion_claim =
        tasks::claim_task_scope(&paths, &scope, "agent-cli:converter").expect("conversion claim");
    assert_eq!(conversion_claim["task"]["id"], conversion_id);

    let update_claim =
        tasks::claim_task_scope(&paths, &scope, "agent-cli:wiki").expect("update claim");
    assert!(
        [ready_ingestion_id, second_ready_ingestion_id]
            .iter()
            .any(|task_id| update_claim["task"]["id"] == **task_id)
    );
}

#[test]
fn terminal_conversion_failure_fails_its_dependent_wiki_update() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let wiki_space_id = active_wiki_space_id(&project_dir, &storage_dir);
    let binary = crate::wiki::add_raw_document(
        &paths,
        "source.png",
        Some("Source"),
        Some("image/png"),
        "user",
        Some(&wiki_space_id),
        b"binary source",
    )
    .expect("binary document");
    let conversion_id = binary["document"]["conversion"]["taskId"]
        .as_str()
        .expect("conversion task");
    let ingestion_id = binary["document"]["ingestionByWikiSpace"][&wiki_space_id]["taskId"]
        .as_str()
        .expect("ingestion task");
    let _broker = crate::content::enable_test_task_broker();
    let claim = tasks::claim_task(&paths, conversion_id, "agent-cli:converter")
        .expect("conversion claim");

    tasks::fail_task_with_class(
        &paths,
        conversion_id,
        claim["leaseToken"].as_str().expect("lease token"),
        "unsupported document",
        None,
        tasks::TaskFailureClass::TerminalTask,
    )
    .expect("terminal conversion failure");

    let ingestion = tasks::inspect_task(&paths, ingestion_id).expect("dependent ingestion");
    assert_eq!(ingestion["task"]["status"], "failed");
    assert_eq!(ingestion["task"]["error"]["code"], "prerequisite_failed");
    assert_eq!(
        ingestion["task"]["error"]["prerequisiteTaskId"],
        conversion_id
    );
}

#[test]
fn terminal_task_state_propagates_through_all_dependency_descendants() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let wiki_panel = bootstrap
        .panels
        .iter()
        .find(|panel| panel.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");
    let storage = Storage::open(&paths).expect("storage");
    let mut task_ids = Vec::new();
    for target in ["root", "child", "grandchild"] {
        let task = storage
            .insert_task(
                &bootstrap.project.id,
                &wiki_panel.panel.id,
                "wiki",
                "maintain_wiki",
                "wiki.maintain",
                target,
                &json!({ "changeEvents": [] }),
                &json!({ "agentSkillId": "wiki-default" }),
            )
            .expect("task");
        task_ids.push(task["id"].as_str().expect("task id").to_owned());
    }
    storage
        .connection()
        .execute(
            "UPDATE tasks SET depends_on_task_id = ? WHERE project_id = ? AND id = ?",
            params![task_ids[0], bootstrap.project.id, task_ids[1]],
        )
        .expect("child dependency");
    storage
        .connection()
        .execute(
            "UPDATE tasks SET depends_on_task_id = ? WHERE project_id = ? AND id = ?",
            params![task_ids[1], bootstrap.project.id, task_ids[2]],
        )
        .expect("grandchild dependency");
    drop(storage);

    tasks::cancel_task(&paths, &task_ids[0]).expect("cancel root");

    for task_id in &task_ids[1..] {
        let dependent = tasks::inspect_task(&paths, task_id).expect("dependent");
        assert_eq!(dependent["task"]["status"], "cancelled");
        assert_eq!(
            dependent["task"]["error"]["code"],
            "prerequisite_failed"
        );
        assert_eq!(
            dependent["task"]["error"]["prerequisiteTaskId"],
            task_ids[0]
        );
    }
}

#[test]
fn task_output_plan_conflict_leaves_task_and_panel_unchanged() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let wiki_panel = bootstrap
        .panels
        .iter()
        .find(|panel| panel.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");
    let task = Storage::open(&paths)
        .expect("storage")
        .insert_task(
            &bootstrap.project.id,
            &wiki_panel.panel.id,
            "wiki",
            "maintain_wiki",
            "wiki.maintain",
            "wiki:default",
            &json!({ "changeEvents": [] }),
            &json!({ "wikiSpaceId": "wiki:default", "agentSkillId": "wiki-default" }),
        )
        .expect("task");
    let task_id = task["id"].as_str().expect("task id");
    let _broker = crate::content::enable_test_task_broker();
    let claim = tasks::claim_task(&paths, task_id, "agent-cli:codex").expect("claim");

    let storage = Storage::open(&paths).expect("storage");
    let (mut prepared_state, base_revision) = storage
        .read_panel_state_snapshot(&bootstrap.project.id, &wiki_panel.panel.id)
        .expect("snapshot")
        .expect("wiki state");
    prepared_state["outputPlanMarker"] = json!("prepared");
    let mut concurrent_state = prepared_state.clone();
    concurrent_state["outputPlanMarker"] = json!("concurrent");
    storage
        .write_panel_state(
            &bootstrap.project.id,
            &wiki_panel.panel.id,
            &concurrent_state,
        )
        .expect("concurrent write");
    drop(storage);

    let error = tasks::complete_task_with_prepared_panel_state_for_test(
        &paths,
        &bootstrap.project.id,
        task_id,
        claim["executionGeneration"]
            .as_i64()
            .expect("execution generation"),
        tasks::PreparedPanelState::new(&wiki_panel.panel.id, base_revision, prepared_state),
    )
    .expect_err("stale output plan must conflict");
    assert_eq!(error.code(), Some("content_conflict"));
    assert_eq!(
        tasks::inspect_task(&paths, task_id).expect("task")["task"]["status"],
        "running"
    );
    assert_eq!(
        Storage::open(&paths)
            .expect("storage")
            .read_panel_state(&bootstrap.project.id, &wiki_panel.panel.id)
            .expect("panel state")
            .expect("wiki state")["outputPlanMarker"],
        "concurrent"
    );
}

#[test]
fn deleting_a_raw_document_cancels_and_fences_its_active_tasks() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let wiki_space_id = active_wiki_space_id(&project_dir, &storage_dir);
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let uploaded = crate::wiki::add_raw_document(
        &paths,
        "source.png",
        Some("Source"),
        Some("image/png"),
        "user",
        Some(&wiki_space_id),
        b"binary source",
    )
    .expect("raw document");
    let document_id = uploaded["document"]["id"].as_str().expect("document id");
    let tasks_before = Storage::open(&paths)
        .expect("storage")
        .list_tasks(&bootstrap.project.id)
        .expect("tasks");
    let conversion_id = tasks_before
        .iter()
        .find(|task| task["type"] == "convert_document_to_markdown")
        .and_then(|task| task["id"].as_str())
        .expect("conversion task")
        .to_owned();
    let ingestion_id = tasks_before
        .iter()
        .find(|task| task["type"] == "ingest_markdown_into_wiki")
        .and_then(|task| task["id"].as_str())
        .expect("ingestion task")
        .to_owned();
    let target = tasks::register_target(
        &paths,
        tasks::TargetRegistration {
            name: "deletion-fencing",
            host: Some("test"),
            project_id: Some(&bootstrap.project.id),
            capabilities: vec!["wiki.convertDocument".to_owned()],
            priority: 0,
            max_concurrency: 1,
            model_gateway_connection_id: None,
        },
    )
    .expect("target");
    let _broker = crate::content::enable_test_task_broker();
    let claim = tasks::claim_task(
        &paths,
        &conversion_id,
        target["target"]["id"].as_str().expect("target id"),
    )
    .expect("claim");

    crate::wiki::delete_raw_document(&paths, document_id, Some(&wiki_space_id))
        .expect("delete raw document");

    for task_id in [&conversion_id, &ingestion_id] {
        let task = tasks::inspect_task(&paths, task_id).expect("related task");
        assert_eq!(task["task"]["status"], "cancelled");
        assert_eq!(
            task["task"]["error"]["code"], "prerequisite_deleted",
            "{task}"
        );
        assert!(task["task"]["lease"]["owner"].is_null());
    }
    let storage = Storage::open(&paths).expect("storage");
    assert!(storage
        .connection()
        .query_row(
            "SELECT deleted_at FROM resources WHERE id = ?",
            [document_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .expect("resource lifecycle")
        .is_some());
    assert_eq!(
        storage
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM task_resources WHERE resource_id = ?",
                [document_id],
                |row| row.get::<_, i64>(0),
            )
            .expect("task links"),
        2
    );
    drop(storage);
    let stale_heartbeat = tasks::heartbeat_task(
        &paths,
        &conversion_id,
        claim["leaseToken"].as_str().expect("lease"),
    )
    .expect_err("deleted resource must fence the old execution");
    assert_eq!(stale_heartbeat.code(), Some("execution_fenced"));
    let wiki_state = Storage::open(&paths)
        .expect("storage")
        .read_panel_state(&bootstrap.project.id, &bootstrap.panel.id)
        .expect("panel state")
        .expect("Wiki state");
    assert!(wiki_state["rawDocuments"]
        .as_array()
        .is_some_and(Vec::is_empty));
}

#[test]
fn cancelling_and_archiving_a_document_task_keeps_its_projection_consistent() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let wiki_space_id = active_wiki_space_id(&project_dir, &storage_dir);
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let uploaded = crate::wiki::add_raw_document(
        &paths,
        "source.md",
        Some("Source"),
        Some("text/markdown"),
        "user",
        Some(&wiki_space_id),
        b"# Source",
    )
    .expect("raw document");
    let task_id = uploaded["document"]["ingestionByWikiSpace"][wiki_space_id.as_str()]["taskId"]
        .as_str()
        .expect("task id");

    let cancelled = tasks::cancel_task(&paths, task_id).expect("cancel");
    assert_eq!(cancelled["task"]["status"], "cancelled");
    let state = Storage::open(&paths)
        .expect("storage")
        .read_panel_state(&bootstrap.project.id, &bootstrap.panel.id)
        .expect("panel state")
        .expect("Wiki state");
    assert_eq!(
        state["rawDocuments"][0]["ingestionByWikiSpace"][wiki_space_id.as_str()]["status"],
        "cancelled"
    );

    let archived = tasks::archive_task(&paths, task_id).expect("archive");
    assert!(archived["task"]["archivedAt"].is_string());
    let state_after_archive = Storage::open(&paths)
        .expect("storage")
        .read_panel_state(&bootstrap.project.id, &bootstrap.panel.id)
        .expect("panel state")
        .expect("Wiki state");
    assert_eq!(
        state_after_archive["rawDocuments"][0]["ingestionByWikiSpace"][wiki_space_id.as_str()]
            ["status"],
        "cancelled"
    );
}

#[test]
fn deleting_a_queued_task_archives_its_dependents_and_updates_resource_status() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let wiki_space_id = active_wiki_space_id(&project_dir, &storage_dir);
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let uploaded = crate::wiki::add_raw_document(
        &paths,
        "source.png",
        Some("Source"),
        Some("image/png"),
        "user",
        Some(&wiki_space_id),
        b"binary source",
    )
    .expect("raw document");
    let conversion_id = uploaded["document"]["conversion"]["taskId"]
        .as_str()
        .expect("conversion task")
        .to_owned();
    let ingestion_id = uploaded["document"]["ingestionByWikiSpace"][wiki_space_id.as_str()]
        ["taskId"]
        .as_str()
        .expect("ingestion task")
        .to_owned();

    let deleted = tasks::delete_task(&paths, &conversion_id).expect("delete task");

    let deleted_ids = deleted["deletedTaskIds"]
        .as_array()
        .expect("deleted task ids");
    assert_eq!(deleted_ids.len(), 2);
    assert!(deleted_ids.iter().any(|value| value == &conversion_id));
    assert!(deleted_ids.iter().any(|value| value == &ingestion_id));
    for task_id in [&conversion_id, &ingestion_id] {
        let task = tasks::inspect_task(&paths, task_id).expect("deleted task");
        assert_eq!(task["task"]["status"], "cancelled");
        assert!(task["task"]["archivedAt"].is_string());
    }
    assert!(tasks::list_tasks(&paths, tasks::TaskListFilter::default())
        .expect("visible tasks")["tasks"]
        .as_array()
        .is_some_and(Vec::is_empty));

    let state = Storage::open(&paths)
        .expect("storage")
        .read_panel_state(&bootstrap.project.id, &bootstrap.panel.id)
        .expect("panel state")
        .expect("Wiki state");
    assert_eq!(state["rawDocuments"][0]["conversion"]["status"], "cancelled");
    assert_eq!(
        state["rawDocuments"][0]["ingestionByWikiSpace"][wiki_space_id.as_str()]["status"],
        "cancelled"
    );
}

#[test]
fn wiki_status_is_derived_even_when_a_stale_projection_is_written() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let wiki_space_id = active_wiki_space_id(&project_dir, &storage_dir);
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let uploaded = crate::wiki::add_raw_document(
        &paths,
        "source.md",
        Some("Source"),
        Some("text/markdown"),
        "user",
        Some(&wiki_space_id),
        b"# Source",
    )
    .expect("raw document");
    let original_task_id =
        uploaded["document"]["ingestionByWikiSpace"][wiki_space_id.as_str()]["taskId"]
        .as_str()
        .expect("task id");
    let storage = Storage::open(&paths).expect("storage");
    storage
        .connection()
        .execute(
            "UPDATE tasks SET status = 'succeeded', completed_at = updated_at WHERE id = ?",
            [original_task_id],
        )
        .expect("complete stale task");
    let mut state = storage
        .read_panel_state(&bootstrap.project.id, &bootstrap.panel.id)
        .expect("panel state")
        .expect("Wiki state");
    state["rawDocuments"][0]["ingestionByWikiSpace"][wiki_space_id.as_str()]["status"] =
        json!("queued");
    state["rawDocuments"][0]["ingestionByWikiSpace"][wiki_space_id.as_str()]["taskId"] =
        json!("task:missing");
    storage
        .write_panel_state(&bootstrap.project.id, &bootstrap.panel.id, &state)
        .expect("stale projection");
    drop(storage);

    let repaired = crate::wiki::wiki_context(&paths).expect("reconciled Wiki");
    let repaired_projection = &repaired["state"]["rawDocuments"][0]["ingestionByWikiSpace"]
        [wiki_space_id.as_str()];
    let repaired_task_id = repaired_projection["taskId"].as_str().expect("repaired task id");
    assert_ne!(repaired_task_id, "task:missing");
    assert_eq!(repaired_task_id, original_task_id);
    assert_eq!(repaired_projection["status"], "unrecorded");
    assert_eq!(
        repaired_projection["error"]["code"],
        "ingestion_result_missing"
    );
    assert_eq!(
        Storage::open(&paths)
            .expect("storage")
            .connection()
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get::<_, i64>(0))
            .expect("task count"),
        1
    );
}

#[test]
fn filtered_wiki_ingestion_is_persisted_as_a_successful_terminal_result() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let wiki_space_id = active_wiki_space_id(&project_dir, &storage_dir);
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let uploaded = crate::wiki::add_raw_document(
        &paths,
        "unrelated.md",
        Some("Unrelated"),
        Some("text/markdown"),
        "user",
        Some(&wiki_space_id),
        b"# Unrelated source",
    )
    .expect("raw document");
    let task_id = uploaded["document"]["ingestionByWikiSpace"][wiki_space_id.as_str()]["taskId"]
        .as_str()
        .expect("task id")
        .to_owned();

    let claim =
        crate::tasks::claim_task(&paths, &task_id, "agent-cli:codex").expect("claim");
    crate::tasks::complete_task(
        &paths,
        &task_id,
        claim["leaseToken"].as_str().expect("lease"),
        Some(json!({
            "outcome": "no_change",
            "disposition": "excluded",
            "reasonCode": "not_relevant",
            "summary": "The source is outside this Wiki's scope.",
            "changedPaths": [],
            "artifacts": []
        })),
    )
    .expect("complete filtered ingestion");

    let state = Storage::open(&paths)
        .expect("storage")
        .read_panel_state(&bootstrap.project.id, &bootstrap.panel.id)
        .expect("panel state")
        .expect("Wiki state");
    let ingestion =
        &state["rawDocuments"][0]["ingestionByWikiSpace"][wiki_space_id.as_str()];
    assert_eq!(ingestion["status"], "filtered");
    assert_eq!(ingestion["reasonCode"], "not_relevant");
    assert_eq!(ingestion["taskId"], task_id);

    let storage = Storage::open(&paths).expect("storage");
    let record = storage
        .connection()
        .query_row(
            r#"
            SELECT disposition, reason_code, processed_document_version, task_id
            FROM wiki_source_ingestions
            WHERE project_id = ? AND wiki_space_id = ?
            "#,
            params![bootstrap.project.id, wiki_space_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .expect("ingestion record");
    assert_eq!(
        record,
        (
            "excluded".to_owned(),
            Some("not_relevant".to_owned()),
            1,
            task_id
        )
    );
}
