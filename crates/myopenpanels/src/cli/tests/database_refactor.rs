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

    crate::wiki::claim_task(&paths, &task_id).expect("claim");
    crate::wiki::complete_task(
        &paths,
        &task_id,
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
