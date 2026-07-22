#[test]
fn command_bridge_rejects_completion_without_staged_content() {
    let _broker = crate::content::enable_test_task_broker();
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let (code, _, stderr) = run(&[
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
        "Bridge Lifecycle",
        "--content",
        "# Bridge Lifecycle",
        "--space-id",
        "wiki:default",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");

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
        "--name",
        "lifecycle-bridge",
        "--capability",
        "wiki.ingestMarkdown",
        "--once",
        "--command",
        "printf '{\"result\":{\"executor\":\"command-test\"}}'",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let bridge = serde_json::from_str::<Value>(&stdout).expect("bridge json");
    assert_eq!(bridge["success"], false);
    assert_eq!(bridge["lifecycleError"]["code"], "invalid_output");
}

#[test]
fn target_registration_rejects_transport_flag_and_old_protocols() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let (code, _, _stderr) = run(&[
        "agent",
        "target",
        "register",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--name",
        "webhook-test",
        "--transport",
        "webhook",
        "--capability",
        "wiki.ingestMarkdown",
        "--format",
        "json",
    ]);
    assert_ne!(code, 0);

    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    for protocol_version in [1, 2] {
        let error = crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "old-protocol",
                host: None,
                project_id: None,
                capabilities: vec!["*".to_owned()],
                priority: 0,
                protocol_version,
                max_concurrency: 1,
                model_gateway_connection_id: None,
            },
        )
        .expect_err("old protocol must be rejected");
        assert_eq!(error.code(), Some("invalid_target"));
    }
}

#[test]
fn concurrent_worker_claim_assigns_a_task_once() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let (code, _, stderr) = run(&[
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
        "Concurrent Claim",
        "--content",
        "# Concurrent Claim",
        "--space-id",
        "wiki:default",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let registered = crate::tasks::register_target(
        &paths,
        crate::tasks::TargetRegistration {
            name: "concurrent-command-target",
            host: Some("test"),
            project_id: None,
            capabilities: vec!["wiki.ingestMarkdown".to_owned()],
            priority: 0,
            protocol_version: 3,
            max_concurrency: 1,
            model_gateway_connection_id: None,
        },
    )
    .expect("target");
    let target_id = registered["target"]["id"].as_str().unwrap().to_owned();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let paths = paths.clone();
        let target_id = target_id.clone();
        let barrier = barrier.clone();
        workers.push(thread::spawn(move || {
            barrier.wait();
            crate::tasks::claim_for_worker(&paths, &target_id, None, None).expect("claim")
        }));
    }
    barrier.wait();
    let results = workers
        .into_iter()
        .map(|worker| worker.join().expect("worker"))
        .collect::<Vec<_>>();
    assert_eq!(
        results
            .iter()
            .filter(|payload| !payload["task"].is_null())
            .count(),
        1
    );
}

#[test]
fn studio_status_reports_missing_session() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");

    let (code, stdout, stderr) = run(&[
        "studio",
        "status",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "{stderr}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["server"], "missing");
    assert!(payload.get("contextId").is_none());
    assert_eq!(stderr, "");
}

#[test]
fn selection_reads_sqlite_panel_selection() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    let context_dir = storage_dir.join("contexts").join("ctx");
    fs::create_dir_all(&context_dir).expect("context dir");
    fs::create_dir_all(&project_dir).expect("project dir");
    seed_selection_database(
        &storage_dir,
        "session:1",
        "panel:canvas",
        serde_json::json!({
            "sessionId": "session:1",
            "panelId": "panel:canvas",
            "selectedShapeIds": ["shape:1"],
            "selectedShapes": [{
                "id": "shape:1",
                "type": "geo",
                "parentId": "page:main",
                "props": {},
                "bounds": { "x": 1, "y": 2, "width": 3, "height": 4 }
            }],
            "assetRef": null,
            "updatedAt": "2026-07-08T00:00:00.000Z"
        }),
        None,
    );
    fs::write(
        context_dir.join("active-session.json"),
        r#"{"sessionId":"session:1"}"#,
    )
    .expect("active session");

    let (code, stdout, stderr) = run(&[
        "panel",
        "selection",
        "read",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "{stderr}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(
        payload["value"]["selectedShapeIds"],
        serde_json::json!(["shape:1"])
    );
    assert_eq!(payload["value"]["isExplicitSelection"], true);
    assert_eq!(payload["focus"]["panelKind"], "canvas");
    assert_eq!(stderr, "");
}

#[test]
fn selection_fallback_is_not_explicit_and_asset_export_requires_opt_in() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    let context_dir = storage_dir.join("contexts").join("ctx");
    let asset_dir = storage_dir
        .join("sessions")
        .join(crate::paths::sanitize_path_part("session:1"))
        .join("panels")
        .join(crate::paths::sanitize_path_part("panel:canvas"))
        .join("assets");
    fs::create_dir_all(&context_dir).expect("context dir");
    fs::create_dir_all(&project_dir).expect("project dir");
    fs::create_dir_all(&asset_dir).expect("asset dir");
    fs::write(asset_dir.join("fallback.png"), tiny_png()).expect("asset");
    seed_selection_database(
        &storage_dir,
        "session:1",
        "panel:canvas",
        serde_json::json!({
            "sessionId": "session:1",
            "panelId": "panel:canvas",
            "selectedShapeIds": [],
            "selectedShapes": [],
            "assetRef": null,
            "updatedAt": "2026-07-08T00:00:00.000Z"
        }),
        Some(serde_json::json!({
            "schema": { "schemaVersion": 1, "recordVersions": {} },
            "currentPageId": "page:main",
            "selectedShapeIds": [],
            "store": {
                "page:main": { "id": "page:main", "typeName": "page", "name": "Page 1", "index": 1 },
                "asset:fallback": {
                    "id": "asset:fallback",
                    "typeName": "asset",
                    "type": "image",
                    "props": {
                        "name": "fallback.png",
                        "src": "/api/panels/session:1/panel:canvas/assets/fallback.png",
                        "w": 1,
                        "h": 1,
                        "mimeType": "image/png",
                        "isAnimated": false
                    },
                    "meta": {
                        "assetRef": "sessions/session:1/panels/panel:canvas/assets/fallback.png"
                    }
                },
                "shape:fallback": {
                    "id": "shape:fallback",
                    "typeName": "shape",
                    "type": "image",
                    "parentId": "page:main",
                    "index": 1,
                    "props": {
                        "assetId": "asset:fallback",
                        "x": 1,
                        "y": 2,
                        "width": 3,
                        "height": 4
                    }
                }
            }
        })),
    );
    fs::write(
        context_dir.join("active-session.json"),
        r#"{"sessionId":"session:1"}"#,
    )
    .expect("active session");

    let (code, stdout, stderr) = run(&[
        "panel",
        "selection",
        "read",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(payload["value"]["isExplicitSelection"], false);
    assert!(payload["value"].get("fallback").is_none());
    assert!(payload["value"]["assetRef"].is_null());

    let output_path = temp.path().join("out.png");
    let (code, _stdout, stderr) = run(&[
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
        output_path.to_str().unwrap(),
    ]);
    assert_eq!(code, 1);
    assert!(stderr.contains("No explicit Canvas selection asset is available"));

    let (code, stdout, _stderr) = run(&[
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
        output_path.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 1);
    let payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(payload["code"], "explicit_selection_required");
    assert!(!output_path.exists());
}

#[test]
fn version_prints_text() {
    let (code, stdout, stderr) = run(&["version"]);

    assert_eq!(code, 0);
    assert_eq!(stdout, format!("{VERSION}\n"));
    assert_eq!(stderr, "");
}

#[test]
fn long_version_flag_prints_bare_version() {
    let (code, stdout, stderr) = run_raw(&["--version"]);

    assert_eq!(code, 0);
    assert_eq!(stdout, format!("{VERSION}\n"));
    assert_eq!(stderr, "");
}

#[test]
fn version_prints_json() {
    let (code, stdout, stderr) = run_raw(&["--version", "--format", "json"]);

    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        format!(
            "{{\"ok\":true,\"schemaVersion\":3,\"intent\":\"cli.version.read\",\"data\":{{\"version\":\"{VERSION}\"}},\"actions\":{{\"required\":[],\"suggested\":[]}},\"meta\":{{\"cliVersion\":\"{VERSION}\"}}}}\n"
        )
    );
    assert_eq!(stderr, "");
}

#[test]
fn version_ignores_inherited_trace_url() {
    let _lock = TRACE_ENV_LOCK.lock().expect("trace env lock");
    let previous = std::env::var_os("MYOPENPANELS_TRACE_URL");
    std::env::set_var(
        "MYOPENPANELS_TRACE_URL",
        "http://127.0.0.1:9/api/trace/events",
    );
    let argv = vec!["--version".to_owned()];

    assert_eq!(trace_url_for_cli(&argv), None);

    if let Some(previous) = previous {
        std::env::set_var("MYOPENPANELS_TRACE_URL", previous);
    } else {
        std::env::remove_var("MYOPENPANELS_TRACE_URL");
    }
}

#[test]
fn help_prints_current_command_map() {
    let (code, stdout, stderr) = run(&[]);

    assert_eq!(code, 0);
    assert!(stdout.contains("Usage: myopenpanels"));
    assert!(stdout.contains("studio"));
    assert!(stdout.contains("canvas"));
    assert!(stdout.contains("typesetting"));
    assert!(stdout.contains("agent"));
    assert!(stdout.contains("update"));
    assert!(!stdout.contains("agent-context"));
    assert!(!stdout.contains("agent context"));
    assert!(!stdout.contains("active-panel"));
    assert!(!stdout.contains("insert-image"));
    assert_eq!(stderr, "");
}

#[test]
fn legacy_and_implicit_aliases_are_rejected() {
    let commands: &[&[&str]] = &[
        &["agent-context"],
        &["agent", "context"],
        &["panels"],
        &["active-panel"],
        &["panel-state"],
        &["canvas-state"],
        &["selection"],
        &["read-selection-asset"],
        &["insert-placeholder"],
        &["insert-image"],
        &["agent"],
        &["agent", "targets"],
        &["project"],
        &["panel"],
        &["canvas"],
        &["canvas", "selection"],
        &["wiki"],
        &["wiki", "selection"],
        &["wiki", "documents"],
        &["wiki", "tasks"],
        &["wiki", "spaces"],
        &["wiki", "pages"],
        &["tasks"],
        &["project", "current"],
        &["panel", "current"],
        &["panel", "context", "read"],
        &["panel", "state", "read"],
        &["canvas", "generation", "begin"],
        &["canvas", "image", "insert"],
        &["wiki", "raw-document", "list"],
        &["wiki", "generated-document", "list"],
        &["wiki", "generation", "begin"],
        &["wiki", "page", "write"],
        &["writing", "generation", "begin"],
        &["task", "event", "list"],
        &["task", "attempt", "list"],
        &["task", "delivery", "list"],
        &["agent", "capability", "list"],
    ];

    for command in commands {
        let mut args = command.to_vec();
        args.push("--format=json");
        let (code, stdout, stderr) = run_raw(&args);
        assert_eq!(code, 2, "command unexpectedly succeeded: {command:?}");
        assert_eq!(stdout, "", "unexpected stdout for {command:?}");
        assert!(
            serde_json::from_str::<Value>(&stderr).expect("json")["ok"] == false,
            "missing JSON error for {command:?}: {stderr}"
        );
    }

    let (code, stdout, stderr) = run_raw(&[
        "panel",
        "activate",
        "--panel-kind",
        "wiki",
        "--expect-focus-revision",
        "1",
        "--format=json",
    ]);
    assert_eq!(code, 2);
    assert_eq!(stdout, "");
    assert_eq!(
        serde_json::from_str::<Value>(&stderr).unwrap()["error"]["subtype"],
        "invalid_argument"
    );
}

#[test]
fn update_help_prints_manifest_controls() {
    let (code, stdout, stderr) = run(&["update", "--help"]);

    assert_eq!(code, 0);
    assert!(stdout.contains("myopenpanels update"));
    assert!(stdout.contains("MYOPENPANELS_UPDATE_MANIFEST_URL"));
    assert_eq!(stderr, "");
}

#[test]
fn unknown_command_prints_text_error() {
    let (code, stdout, stderr) = run_raw(&["nope"]);

    assert_eq!(code, 2);
    assert_eq!(stdout, "");
    assert!(stderr.contains("unrecognized subcommand 'nope'"));
}

#[test]
fn unknown_command_prints_json_error() {
    let (code, stdout, stderr) = run_raw(&["nope", "--format=json"]);

    assert_eq!(code, 2);
    assert_eq!(stdout, "");
    let payload = serde_json::from_str::<Value>(&stderr).expect("json error");
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["schemaVersion"], 3);
    assert_eq!(payload["intent"], "cli.parse");
    assert_eq!(payload["error"]["type"], "validation");
    assert_eq!(payload["error"]["subtype"], "invalid_argument");
    assert_eq!(payload["error"]["subtype"], "invalid_argument");
    assert!(payload["error"]["message"]
        .as_str()
        .unwrap()
        .contains("unrecognized subcommand"));
    assert_eq!(payload["error"]["retryable"], false);
    assert!(payload["error"]["hint"].as_str().is_some());
    assert_eq!(
        payload["actions"]["suggested"][0]["argv"],
        json!(["--help"])
    );
    assert!(!payload["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Usage:"));
}

#[test]
fn missing_flag_json_error_identifies_the_parameter() {
    let (code, stdout, stderr) = run_raw(&[
        "wiki",
        "page",
        "search",
        "--space-id",
        "space-1",
        "--format=json",
    ]);

    assert_eq!(code, 2);
    assert_eq!(stdout, "");
    let payload = serde_json::from_str::<Value>(&stderr).expect("json error");
    assert_eq!(payload["error"]["type"], "validation");
    assert_eq!(payload["error"]["subtype"], "invalid_argument");
    assert_eq!(payload["error"]["param"], "--query");
}

#[test]
fn bootstrap_writer_rejects_an_oversized_success_envelope() {
    let mut flags = BTreeMap::new();
    flags.insert("format".to_owned(), FlagValue::String("json".to_owned()));
    let invocation = Invocation {
        command_id: registry::CommandId::from_intent("agent.bootstrap.read")
            .expect("bootstrap command"),
        flags,
        positionals: vec!["agent".to_owned(), "bootstrap".to_owned()],
    };
    let mut stdout = Vec::new();
    let error = write_result(
        &invocation,
        &mut stdout,
        &json!({ "oversized": "x".repeat(crate::agent::MAX_BOOTSTRAP_ENVELOPE_BYTES) }),
        "bootstrap",
    )
    .expect_err("oversized Bootstrap must fail");
    assert_eq!(error.code(), Some("bootstrap_budget_exceeded"));
    assert!(stdout.is_empty());
}
