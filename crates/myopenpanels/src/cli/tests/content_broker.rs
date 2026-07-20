#[test]
fn panel_targeted_commands_do_not_change_focus_and_selection_remains_focus_bound() {
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
    let initial_revision = read_focus_revision(&paths).expect("focus revision");
    let initial_focus = crate::control::read_active_panel_value(&paths)
        .expect("active panel")
        .expect("focus");
    assert_eq!(initial_focus["kind"], "wiki");

    let (code, stdout, stderr) = run_raw(&[
        "canvas",
        "image",
        "generate",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}\n{stdout}");
    assert_eq!(read_focus_revision(&paths).unwrap(), initial_revision);
    assert_eq!(
        crate::control::read_active_panel_value(&paths)
            .unwrap()
            .unwrap()["kind"],
        "wiki"
    );

    let output_file = temp.path().join("selection.png");
    let (code, stdout, stderr) = run_raw(&[
        "canvas",
        "selection",
        "export",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--output-file",
        output_file.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 1, "{stderr}\n{stdout}");
    let error = serde_json::from_str::<Value>(&stderr).expect("selection error");
    assert_eq!(error["error"]["subtype"], "panel_kind_mismatch");

    let (code, stdout, stderr) = run_raw(&[
        "canvas",
        "image",
        "generate",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--use-selection",
        "--format",
        "json",
    ]);
    assert_eq!(code, 1, "{stderr}\n{stdout}");
    let error = serde_json::from_str::<Value>(&stderr).expect("selection error");
    assert_eq!(error["error"]["subtype"], "panel_kind_mismatch");

    let (code, stdout, stderr) = run_raw(&[
        "panel",
        "activate",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--panel-kind",
        "canvas",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}\n{stdout}");
    let canvas_revision = read_focus_revision(&paths).expect("canvas focus revision");

    let (code, stdout, stderr) = run_raw(&[
        "wiki",
        "space",
        "list",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}\n{stdout}");

    let (code, stdout, stderr) = run_raw(&[
        "wiki",
        "raw",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--title",
        "Background write",
        "--content",
        "# Background write",
        "--space-id",
        "wiki:default",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}\n{stdout}");
    assert_eq!(read_focus_revision(&paths).unwrap(), canvas_revision);
    assert_eq!(
        crate::control::read_active_panel_value(&paths)
            .unwrap()
            .unwrap()["kind"],
        "canvas"
    );

    let connection = Connection::open(storage_dir.join("main.sqlite3")).expect("database");
    let operation_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM agent_operations", [], |row| {
            row.get(0)
        })
        .expect("operation count");
    assert_eq!(operation_count, 1);
}

#[test]
fn entry_skill_requires_verified_open_and_refreshes_bootstrap_for_panel_work() {
    let skill = include_str!("../../../../../skills/myopenpanels/SKILL.md").replace("\r\n", "\n");
    let install = include_str!("../../../../../skills/myopenpanels/references/install.md");
    assert!(skill.contains("version: \"5.6\""));
    assert_eq!(crate::agent_control::ENTRY_SKILL_VERSION, "5.6");
    assert!(skill.lines().count() <= 150);
    assert!(skill.contains("references/install.md"));
    assert!(!skill.contains("curl -fsSL"));
    assert!(install.contains("failed `PATH` lookup does not"));
    assert!(install.contains("${MYOPENPANELS_INSTALL_DIR:-$HOME/.local/bin}/myopenpanels"));
    assert!(install.contains(".local\\bin\\myopenpanels.exe"));
    assert!(!skill.contains("myopenpanels-dev"));
    assert!(!skill.contains("checkout-local"));
    assert!(!install.contains("myopenpanels-dev"));
    assert!(!install.contains("checkout-local"));
    assert!(!skill.contains("minCliVersion"));
    assert!(!skill.contains("protocol-version"));
    assert!(skill.contains("MYOPENPANELS_TASK_BROKER_URL"));
    assert!(skill.contains("do not\nstart Studio or Bootstrap"));
    assert!(skill
        .contains("myopenpanels studio start --local-only --project-dir \"$PWD\" --format json"));
    assert!(skill.contains("myopenpanels agent bootstrap --format json"));
    assert!(skill.contains(
        "myopenpanels agent bootstrap --procedure <procedure-key> --format json"
    ));
    assert!(skill.contains("`panel.canvas.image.edit`"));
    assert!(skill.contains("`panel.wiki.knowledge.query`"));
    assert!(skill.contains("`panel.writing.context.read`"));
    assert!(skill.contains("`task.queue.inspect`"));
    assert!(skill.contains("Task Handoff"));
    assert!(!skill.contains("agent bootstrap --project-dir"));
    assert!(skill.contains("When intent is ambiguous"));
    assert!(!skill.contains("myopenpanels studio open-system-browser"));
    assert!(!skill.contains("myopenpanels agent capability"));
    assert!(!skill.contains("myopenpanels panel "));
    assert!(!skill.contains("myopenpanels wiki "));
    assert!(!skill.contains("myopenpanels canvas "));
    assert!(!skill.contains("myopenpanels task "));
    assert!(!skill.contains("myopenpanels operation "));
    assert!(skill.contains("`actions.required` in order"));
    assert!(skill.contains("`actions.suggested`"));
    assert!(!skill.contains("recommendedScopes"));
    assert!(!skill.contains("capabilityListActions"));
    assert!(!skill.contains("readAction"));
    assert!(!skill.contains("protocolVersion"));
    assert!(!skill.contains("commandCatalogVersion"));
    assert!(skill.contains("Success means Studio is ready, not visible"));
    assert!(skill.contains("Treat an `open-url` action as a host display action"));
    assert!(skill.contains("current Agent host's native\n   embedded browser"));
    assert!(skill.contains("Do not initialize browser automation"));
    assert!(skill.contains("is not an embedded-open success"));
    assert!(skill.contains("For an open-only request, stop after an opener succeeds"));
    assert!(skill.contains("run a fresh Procedure"));
    assert!(skill.contains("work clearly unrelated to MyOpenPanels"));
    assert!(skill.contains("Never reuse an earlier Bootstrap result"));
    assert!(skill.contains("`agent catalog --domain <domain>` discovery actions"));
    assert!(skill.contains("typed file, URL, Skill, or host"));
    assert!(skill.contains("If a required action updates the"));
    assert!(skill.contains("--local-only"));
    assert!(!skill.contains("--no-open"));
}

#[test]
fn task_and_operation_discovery_use_response_level_actions() {
    let task_list = with_task_actions(json!({ "tasks": [{ "id": "task:1" }] }), true);
    assert_eq!(task_list["actions"]["suggested"][0]["intent"], "task.read");
    assert_action_parses(&task_list["actions"]["suggested"][0]);

    let task = with_task_actions(
        json!({
            "task": {
                "id": "task:1",
                "status": "running",
                "capability": "wiki.page.search",
            }
        }),
        false,
    );
    for action in task["actions"]["suggested"].as_array().unwrap() {
        assert_action_parses(action);
        assert_eq!(action["intent"], "agent.catalog");
    }

    let operation_list =
        with_operation_actions(json!({ "operations": [{ "id": "operation:1" }] }), true);
    assert_eq!(
        operation_list["actions"]["suggested"][0]["intent"],
        "operation.read"
    );
    assert_action_parses(&operation_list["actions"]["suggested"][0]);

    let operation = with_operation_actions(
        json!({
            "id": "operation:1",
            "status": "active",
            "skillId": "myopenpanels-canvas-panel",
        }),
        false,
    );
    for action in operation["actions"]["suggested"].as_array().unwrap() {
        assert_action_parses(action);
    }
    assert!(operation["actions"]["suggested"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action["intent"] == "agent.skill.read"));
    assert!(operation["actions"]["suggested"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action["argv"]
            .as_array()
            .is_some_and(|argv| { argv.iter().any(|value| value == "operation") })));
}

fn seed_selection_database(
    storage_dir: &Path,
    project_id: &str,
    panel_id: &str,
    selection: Value,
    state: Option<Value>,
) {
    fs::create_dir_all(storage_dir).expect("storage dir");
    let project_dir = storage_dir.join("project");
    fs::create_dir_all(&project_dir).expect("project dir");
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let storage = Storage::open(&paths).expect("storage");
    storage
        .write_project(&crate::types::Project {
            id: project_id.to_owned(),
            title: "Project 1".to_owned(),
            created_at: "2026-07-08T00:00:00.000Z".to_owned(),
            updated_at: "2026-07-08T00:00:00.000Z".to_owned(),
            panel_ids: vec![panel_id.to_owned()],
        })
        .expect("project");
    storage
        .write_panel(&crate::types::Panel {
            id: panel_id.to_owned(),
            project_id: project_id.to_owned(),
            kind: crate::types::PanelKind::Canvas,
            title: "Design canvas".to_owned(),
            created_at: "2026-07-08T00:00:00.000Z".to_owned(),
            updated_at: "2026-07-08T00:00:00.000Z".to_owned(),
            state_ref: None,
        })
        .expect("panel");
    if let Some(state) = state {
        storage
            .write_panel_state(project_id, panel_id, &state)
            .expect("state");
    }
    storage
        .write_panel_selection(project_id, panel_id, &selection)
        .expect("selection");
}

fn tiny_png() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==")
            .expect("tiny png")
}

#[test]
fn non_text_upload_creates_a_workflow_run_dag_and_delete_fences_the_attempt() {
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

    let created = wiki::add_raw_document(
        &paths,
        "source.pdf",
        Some("Source"),
        Some("application/pdf"),
        "user",
        Some("wiki:default"),
        b"pdf fixture",
    )
    .expect("upload");
    let document_id = created["document"]["id"].as_str().unwrap();
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let stored_tasks = Storage::open(&paths)
        .expect("storage")
        .list_tasks(&bootstrap.project.id)
        .expect("tasks");
    let conversion = stored_tasks
        .iter()
        .find(|task| task["type"] == "convert_document_to_markdown")
        .cloned()
        .expect("conversion");
    let ingest = stored_tasks
        .iter()
        .find(|task| task["type"] == "ingest_markdown_into_wiki")
        .cloned()
        .expect("ingest");
    assert_eq!(conversion["status"], "queued");
    assert_eq!(ingest["status"], "waiting");
    assert_eq!(conversion["workflowRunId"], ingest["workflowRunId"]);
    let inspected_ingest =
        tasks::inspect_task(&paths, ingest["id"].as_str().unwrap()).expect("inspect ingest");
    assert_eq!(
        inspected_ingest["task"]["dependencies"][0]["prerequisiteTaskId"],
        conversion["id"]
    );

    let registered = tasks::register_target(
        &paths,
        tasks::TargetRegistration {
            name: "converter",
            host: Some("test"),
            project_id: None,
            capabilities: vec!["wiki.convertDocument".to_owned()],
            priority: 10,
            protocol_version: 3,
            max_concurrency: 1,
            model_gateway_connection_id: None,
        },
    )
    .expect("target");
    let claimed = tasks::claim_task(
        &paths,
        conversion["id"].as_str().unwrap(),
        registered["target"]["id"].as_str().unwrap(),
    )
    .expect("claim");
    let lease_token = claimed["leaseToken"].as_str().unwrap();
    assert_eq!(claimed["executionProtocolVersion"], 3);
    assert!(claimed["attemptId"].is_string());

    let deleted =
        wiki::delete_raw_document(&paths, document_id, Some("wiki:default")).expect("delete");
    assert_eq!(
        deleted["task"]["changeEvents"],
        json!([{
            "kind": "raw_document_deleted",
            "documentId": document_id,
            "title": "Source"
        }])
    );
    for task_id in [
        conversion["id"].as_str().unwrap(),
        ingest["id"].as_str().unwrap(),
    ] {
        let task = tasks::inspect_task(&paths, task_id).expect("task");
        assert_eq!(task["task"]["status"], "cancelled");
        assert_eq!(
            task["task"]["terminalReason"]["code"],
            "prerequisite_deleted"
        );
    }
    let fenced = tasks::complete_task(
        &paths,
        conversion["id"].as_str().unwrap(),
        lease_token,
        None,
    )
    .expect_err("cancelled attempt must be fenced");
    assert_eq!(fenced.code(), Some("execution_fenced"));
    let attempts =
        tasks::list_task_attempts(&paths, conversion["id"].as_str().unwrap()).expect("attempts");
    assert_eq!(attempts["attempts"][0]["status"], "cancelled");
    tasks::archive_task(&paths, conversion["id"].as_str().unwrap()).expect("archive");
    let listed = tasks::list_tasks(&paths, tasks::TaskListFilter::default()).expect("list");
    assert!(!listed["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| { task["id"] == conversion["id"] }));
    assert!(
        tasks::list_task_events(&paths, conversion["id"].as_str().unwrap()).expect("events")
            ["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["eventType"] == "archived")
    );
    tasks::archive_task(&paths, ingest["id"].as_str().unwrap()).expect("archive dependent");
    let workflow_runs = tasks::list_workflow_runs(&paths).expect("Workflow Runs");
    assert!(!workflow_runs["workflowRuns"]
        .as_array()
        .unwrap()
        .iter()
        .any(|workflow_run| workflow_run["workflowRunId"] == conversion["workflowRunId"]));
    let archived_workflow_run =
        tasks::read_workflow_run(&paths, conversion["workflowRunId"].as_str().unwrap())
            .expect("archived Workflow Run history");
    assert_eq!(archived_workflow_run["workflowRun"]["status"], "archived");
    assert_eq!(
        archived_workflow_run["tasks"].as_array().unwrap().len(),
        2
    );
}

#[test]
fn agent_routing_handles_saturated_targets() {
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
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let wiki_panel = bootstrap
        .panels
        .iter()
        .find(|panel| panel.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");
    let storage = Storage::open(&paths).expect("storage");
    let first = storage
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
        .expect("first task");
    let second = storage
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
        .expect("second task");
    let primary = tasks::register_target(
        &paths,
        tasks::TargetRegistration {
            name: "primary",
            host: Some("test"),
            project_id: None,
            capabilities: vec!["wiki.maintain".to_owned()],
            priority: 50,
            protocol_version: 3,
            max_concurrency: 1,
            model_gateway_connection_id: None,
        },
    )
    .expect("primary target");
    let fallback = tasks::register_target(
        &paths,
        tasks::TargetRegistration {
            name: "fallback",
            host: Some("test"),
            project_id: None,
            capabilities: vec!["wiki.maintain".to_owned()],
            priority: 10,
            protocol_version: 3,
            max_concurrency: 1,
            model_gateway_connection_id: None,
        },
    )
    .expect("fallback target");
    tasks::set_agent_route(
        &paths,
        "wiki.maintain",
        &[
            primary["target"]["id"].as_str().unwrap().to_owned(),
            fallback["target"]["id"].as_str().unwrap().to_owned(),
        ],
    )
    .expect("route");

    let first_claim = tasks::claim_task(
        &paths,
        first["id"].as_str().unwrap(),
        primary["target"]["id"].as_str().unwrap(),
    )
    .expect("primary claim");
    assert_eq!(first_claim["target"]["name"], "primary");
    let second_claim = tasks::claim_task(
        &paths,
        second["id"].as_str().unwrap(),
        fallback["target"]["id"].as_str().unwrap(),
    )
    .expect("fallback claim");
    assert_eq!(second_claim["target"]["name"], "fallback");

    tasks::remove_target(&paths, primary["target"]["id"].as_str().unwrap())
        .expect("remove active executor");
    let interrupted =
        tasks::inspect_task(&paths, first["id"].as_str().unwrap()).expect("interrupted task");
    assert_eq!(interrupted["task"]["status"], "failed");
    assert_eq!(interrupted["task"]["error"]["code"], "executor_removed");
    let interrupted_attempts =
        tasks::list_task_attempts(&paths, first["id"].as_str().unwrap()).expect("attempts");
    assert_eq!(interrupted_attempts["attempts"][0]["status"], "interrupted");
    let fenced = tasks::complete_task(
        &paths,
        first["id"].as_str().unwrap(),
        first_claim["leaseToken"].as_str().unwrap(),
        None,
    )
    .expect_err("removed executor is fenced");
    assert_eq!(fenced.code(), Some("execution_fenced"));
}
