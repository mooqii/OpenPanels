#[test]
fn cli_trace_url_falls_back_to_running_studio_session() {
    let _lock = TRACE_ENV_LOCK.lock().expect("trace env lock");
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
    let (port, server) = fake_studio_server(1);
    let server_url = format!("http://127.0.0.1:{port}");
    write_studio_session(
        &paths,
        &StudioSession {
            system_browser_url: Some(server_url.clone()),
            host: Some("127.0.0.1".to_owned()),
            lan_server_urls: Some(Vec::new()),
            local_server_url: Some(server_url.clone()),
            log_path: paths.context_dir.join("studio.log").display().to_string(),
            pid: std::process::id(),
            port,
            server_url: server_url.clone(),
            started_at: "2026-07-09T00:00:00.000Z".to_owned(),
            storage_dir: paths.storage_dir.display().to_string(),
        },
    )
    .expect("studio session");

    let argv = vec![
        "panel".to_owned(),
        "selection".to_owned(),
        "read".to_owned(),
        "--project-dir".to_owned(),
        project_dir.to_str().unwrap().to_owned(),
        "--storage-dir".to_owned(),
        storage_dir.to_str().unwrap().to_owned(),
        "--context-id".to_owned(),
        "ctx".to_owned(),
        "--format".to_owned(),
        "json".to_owned(),
    ];

    assert_eq!(
        trace_url_for_cli(&argv),
        Some(format!("{server_url}/api/trace/events"))
    );
    server.join().expect("server thread");
}

#[test]
fn studio_start_json_reuses_the_storage_singleton() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    let owner_paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("owner"),
    )
    .expect("owner paths");
    let (port, server) = fake_studio_server(3);
    let server_url = format!("http://127.0.0.1:{port}");
    write_studio_session(
        &owner_paths,
        &StudioSession {
            system_browser_url: Some(server_url.clone()),
            host: Some("127.0.0.1".to_owned()),
            lan_server_urls: Some(Vec::new()),
            local_server_url: Some(server_url.clone()),
            log_path: owner_paths
                .context_dir
                .join("studio.log")
                .display()
                .to_string(),
            pid: std::process::id(),
            port,
            server_url: server_url.clone(),
            started_at: "2026-07-09T00:00:00.000Z".to_owned(),
            storage_dir: owner_paths.storage_dir.display().to_string(),
        },
    )
    .expect("studio session");

    let (code, stdout, stderr) = run(&[
        "studio",
        "start",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "borrower",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "{stderr}{stdout}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(payload["embeddedBrowserUrl"], server_url);
    assert_eq!(payload["systemBrowserUrl"], server_url);
    assert_eq!(payload["context"]["id"], "studio");
    assert_eq!(payload["projectReady"], true);
    assert_eq!(payload["reusedExisting"], true);
    assert_eq!(payload["serverVersion"], VERSION);
    assert_eq!(payload["lifecycle"], "reused");
    assert_eq!(payload["previousVersion"], Value::Null);
    assert_eq!(payload["browserRefreshRequired"], false);
    assert_eq!(payload["actions"]["required"][0]["intent"], "studio.open");
    assert_eq!(payload["actions"]["required"][0]["url"], server_url);
    assert_eq!(
        payload["actions"]["required"][1]["intent"],
        "studio.open-system-browser"
    );
    assert_eq!(
        payload["actions"]["required"][1]["argv"],
        json!([
            "studio",
            "open-system-browser",
            "--local-only",
            "--project-dir",
            project_dir,
            "--format",
            "json"
        ])
    );
    assert_action_parses(&payload["actions"]["required"][1]);
    assert_eq!(
        payload["actions"]["required"][1]["condition"]["actionId"],
        "studio.open.in-app"
    );
    assert!(storage_dir.join("studio").join("instance.json").exists());

    let (code, stdout, stderr) = run(&[
        "agent",
        "bootstrap",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    assert_eq!(
        serde_json::from_str::<Value>(&stdout).expect("bootstrap")["focus"]["projectId"],
        payload["project"]["id"]
    );
    server.join().expect("server thread");
}

#[test]
fn agent_bootstrap_without_project_dir_uses_user_visible_studio() {
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
    let (port, server) = fake_studio_server(1);
    let server_url = format!("http://127.0.0.1:{port}");
    let session = StudioSession {
        system_browser_url: Some(server_url.clone()),
        host: Some("127.0.0.1".to_owned()),
        lan_server_urls: Some(Vec::new()),
        local_server_url: Some(server_url.clone()),
        log_path: paths.context_dir.join("studio.log").display().to_string(),
        pid: std::process::id(),
        port,
        server_url,
        started_at: "2026-07-12T00:00:00.000Z".to_owned(),
        storage_dir: paths.storage_dir.display().to_string(),
    };
    write_studio_session(&paths, &session).expect("current studio");
    let caller_paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        None,
    )
    .expect("caller paths");
    let pending = crate::agent_control::pending_entry_skill_update(&caller_paths)
        .expect("entry skill requirement")
        .expect("pending update");
    crate::agent_control::acknowledge_entry_skill_update(
        &caller_paths,
        &pending.event_id,
        crate::agent_control::ENTRY_SKILL_VERSION,
    )
    .expect("acknowledge caller skill");

    let (code, stdout, stderr) = run(&[
        "agent",
        "bootstrap",
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "{stderr}{stdout}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("bootstrap");
    assert_eq!(payload["focus"]["projectId"].as_str().is_some(), true);
    assert!(!payload["actions"]["required"]
        .as_array()
        .unwrap()
        .is_empty());
    let loader_path = payload["skills"][0]["contextPath"]
        .as_str()
        .expect("loader path");
    assert!(Path::new(loader_path).starts_with(&caller_paths.context_dir));
    assert!(!Path::new(loader_path).starts_with(&paths.context_dir));
    server.join().expect("server thread");
}

#[test]
fn agent_bootstrap_without_visible_studio_has_direct_recovery() {
    let temp = tempfile::tempdir().expect("temp dir");
    let storage_dir = temp.path().join(".myopenpanels");

    let (code, stdout, stderr) = run(&[
        "agent",
        "bootstrap",
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--format",
        "json",
    ]);

    assert_eq!(code, 1, "{stderr}{stdout}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("error");
    assert_eq!(payload["code"], "no_current_studio");
    assert!(payload["recovery"]
        .as_str()
        .unwrap()
        .contains("agent bootstrap --format json"));
}

#[test]
fn system_browser_payload_requires_a_successful_launcher() {
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
    let server_url = "http://127.0.0.1:43217".to_owned();
    let result = StudioStartResult {
        session: StudioSession {
            system_browser_url: Some(server_url.clone()),
            host: Some("127.0.0.1".to_owned()),
            lan_server_urls: Some(Vec::new()),
            local_server_url: Some(server_url.clone()),
            log_path: paths.context_dir.join("studio.log").display().to_string(),
            pid: std::process::id(),
            port: 43_217,
            server_url: server_url.clone(),
            started_at: "2026-07-11T00:00:00.000Z".to_owned(),
            storage_dir: paths.storage_dir.display().to_string(),
        },
        reused_existing: false,
        server_version: VERSION.to_owned(),
        lifecycle: crate::studio::StudioLifecycle::Started,
        previous_version: None,
        browser_refresh_required: false,
    };

    let payload = studio_system_browser_payload(&paths, &result, &bootstrap, |url| {
        assert_eq!(url, server_url);
        Ok(())
    })
    .expect("browser payload");
    assert_eq!(payload["opened"], true);
    assert_eq!(payload["openTarget"], "system_browser");
    assert!(payload.get("nextRequiredAction").is_none());

    let error = studio_system_browser_payload(&paths, &result, &bootstrap, |_| {
        Err(CliError::with_recovery(
            "browser_open_failed",
            "launcher failed",
            true,
            format!("Open {server_url} manually."),
        ))
    })
    .expect_err("launcher failure");
    assert_eq!(error.code(), Some("browser_open_failed"));
    assert!(error.retryable());
    assert!(error.recovery().unwrap().contains(&server_url));
}

#[test]
fn studio_serve_reuses_existing_studio_without_foreground_server() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    let owner_paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("owner"),
    )
    .expect("owner paths");
    let (port, server) = fake_studio_server(2);
    let server_url = format!("http://127.0.0.1:{port}");
    write_studio_session(
        &owner_paths,
        &StudioSession {
            system_browser_url: Some(server_url.clone()),
            host: Some("127.0.0.1".to_owned()),
            lan_server_urls: Some(Vec::new()),
            local_server_url: Some(server_url.clone()),
            log_path: owner_paths
                .context_dir
                .join("studio.log")
                .display()
                .to_string(),
            pid: std::process::id(),
            port,
            server_url: server_url.clone(),
            started_at: "2026-07-09T00:00:00.000Z".to_owned(),
            storage_dir: owner_paths.storage_dir.display().to_string(),
        },
    )
    .expect("studio session");

    let (code, stdout, stderr) = run(&[
        "studio",
        "serve",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "borrower",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "{stderr}{stdout}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(payload["embeddedBrowserUrl"], server_url);
    assert_eq!(payload["foreground"], false);
    assert_eq!(payload["reusedExisting"], true);
    assert_eq!(payload["actions"]["required"][0]["intent"], "studio.open");
    server.join().expect("server thread");
}

#[test]
fn failed_project_prepare_does_not_stop_a_reused_studio() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    let owner_paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("owner"),
    )
    .expect("owner paths");
    let borrower_paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("borrower"),
    )
    .expect("borrower paths");
    let (port, server) = fake_studio_server(1);
    let session = StudioSession {
        system_browser_url: Some(format!("http://127.0.0.1:{port}")),
        host: Some("127.0.0.1".to_owned()),
        lan_server_urls: Some(Vec::new()),
        local_server_url: Some(format!("http://127.0.0.1:{port}")),
        log_path: owner_paths
            .context_dir
            .join("studio.log")
            .display()
            .to_string(),
        pid: std::process::id(),
        port,
        server_url: format!("http://127.0.0.1:{port}"),
        started_at: "2026-07-11T00:00:00.000Z".to_owned(),
        storage_dir: owner_paths.storage_dir.display().to_string(),
    };
    write_studio_session(&owner_paths, &session).expect("owner session");
    fs::create_dir_all(&borrower_paths.context_dir).expect("borrower context");
    fs::write(storage_dir.join("main.sqlite3"), "not sqlite").expect("invalid storage");

    let (code, stdout, stderr) = run(&[
        "studio",
        "start",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "borrower",
        "--format",
        "json",
    ]);

    assert_eq!(code, 5, "{stderr}{stdout}");
    assert!(owner_paths.studio_dir.join("instance.json").exists());
    let _ = TcpStream::connect(("127.0.0.1", port));
    server.join().expect("server thread");
}

#[test]
fn project_read_commands_bootstrap_project() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let (code, stdout, stderr) = run(&[
        "panel",
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
    let payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(payload["activePanelKind"], "wiki");
    assert_eq!(
        payload["panels"]
            .as_array()
            .expect("panels")
            .iter()
            .map(|panel| panel["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["wiki", "writing", "canvas", "typesetting", "publishing"]
    );
    assert_eq!(stderr, "");

    let (code, stdout, stderr) = run(&[
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
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    assert_eq!(
        serde_json::from_str::<Value>(&stdout).expect("json")["focus"]["panelKind"],
        "canvas"
    );
    let (code, stdout, stderr) = run(&[
        "agent",
        "bootstrap",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let bootstrap = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(bootstrap.get("protocolVersion").is_none());
    assert!(bootstrap.get("commandCatalogVersion").is_none());
    assert_eq!(bootstrap["panel"]["context"]["panelKind"], "canvas");
    assert_eq!(bootstrap["panel"]["selection"]["supported"], true);
    assert!(bootstrap.get("capabilities").is_none());
    let required_skill = &bootstrap["skills"][0];
    assert_eq!(required_skill["id"], "myopenpanels-panels");
    assert!(Path::new(required_skill["contextPath"].as_str().unwrap()).is_file());
    assert!(Path::new(required_skill["localPath"].as_str().unwrap()).is_file());
    assert_eq!(required_skill["referencePaths"].as_array().unwrap().len(), 1);
    assert!(required_skill["referencePaths"][0]
        .as_str()
        .unwrap()
        .ends_with("references/canvas-contract.md"));
    let canvas_loader = fs::read_to_string(required_skill["contextPath"].as_str().unwrap())
        .expect("Canvas loader");
    assert!(canvas_loader.contains("`canvas.image.generate`"));
    assert!(!canvas_loader.contains("`wiki.page.search`"));
    assert!(!canvas_loader.contains("`writing.write`"));
    assert!(storage_dir
        .join("skills/myopenpanels-panels/SKILL.md")
        .is_file());

    let (code, stdout, stderr) = run(&[
        "panel",
        "activate",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--panel-kind",
        "writing",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");

    let (code, stdout, stderr) = run(&[
        "agent",
        "bootstrap",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let bootstrap = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(bootstrap["panel"]["context"]["panelKind"], "writing");
    assert_eq!(
        bootstrap["focus"]["availablePanelKinds"],
        json!(["wiki", "writing", "canvas", "typesetting", "publishing"])
    );
    assert_eq!(
        bootstrap["skills"][0]["id"],
        "myopenpanels-panels"
    );

    let (code, stdout, stderr) = run(&[
        "panel",
        "activate",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--panel-kind",
        "typesetting",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");

    let (code, stdout, stderr) = run(&[
        "agent",
        "bootstrap",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let bootstrap = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(bootstrap["panel"]["context"]["panelKind"], "typesetting");
    assert_eq!(bootstrap["skills"][0]["id"], "myopenpanels-panels");
    assert!(bootstrap["skills"][0]["referencePaths"][0]
        .as_str()
        .unwrap()
        .ends_with("references/publication-contract.md"));

    let (code, stdout, stderr) = run(&[
        "panel",
        "activate",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--panel-kind",
        "publishing",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");

    let (code, stdout, stderr) = run(&[
        "agent",
        "bootstrap",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let bootstrap = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(bootstrap["panel"]["context"]["panelKind"], "publishing");
    assert_eq!(bootstrap["skills"][0]["id"], "myopenpanels-panels");
    assert!(bootstrap["skills"][0]["referencePaths"][0]
        .as_str()
        .unwrap()
        .ends_with("references/release-contract.md"));
}

#[test]
fn project_list_marks_current_and_select_switches_focus() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");

    let create = |title: &str| {
        let (code, stdout, stderr) = run(&[
            "project",
            "create",
            "--project-dir",
            project_dir.to_str().unwrap(),
            "--storage-dir",
            storage_dir.to_str().unwrap(),
            "--context-id",
            "ctx",
            "--title",
            title,
            "--format",
            "json",
        ]);
        assert_eq!(code, 0, "{stderr}{stdout}");
        serde_json::from_str::<Value>(&stdout).expect("project")
    };
    let first = create("First");
    let second = create("Second");

    let list_args = [
        "project",
        "list",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ];
    let (code, stdout, stderr) = run(&list_args);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let projects = serde_json::from_str::<Value>(&stdout).expect("projects");
    assert!(projects["projects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|project| project["id"] == second["project"]["id"] && project["current"] == true));

    let first_id = first["project"]["id"].as_str().unwrap();
    let (code, stdout, stderr) = run(&[
        "project",
        "activate",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--project-id",
        first_id,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let selected = serde_json::from_str::<Value>(&stdout).expect("selected");
    assert_eq!(selected["project"]["id"], first_id);
    assert!(selected["focusRevision"].as_u64().unwrap() > 0);

    let (code, stdout, stderr) = run(&list_args);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let projects = serde_json::from_str::<Value>(&stdout).expect("projects");
    assert!(projects["projects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|project| project["id"] == first_id && project["current"] == true));
}

#[test]
fn studio_start_rejects_a_missing_project_directory() {
    let missing = tempfile::tempdir().expect("temp").path().join("missing");
    let (code, stdout, stderr) = run(&[
        "studio",
        "start",
        "--project-dir",
        missing.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 2);
    assert_eq!(stderr, "");
    assert_eq!(
        serde_json::from_str::<Value>(&stdout).expect("missing")["code"],
        "project_directory_not_found"
    );
}

#[test]
fn agent_bootstrap_emits_focus_skills_and_capabilities() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);

    let bootstrap_args = [
        "agent",
        "bootstrap",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ];
    let (code, raw_stdout, stderr) = run_raw(&bootstrap_args);
    assert_eq!(code, 0, "{stderr}{raw_stdout}");
    assert!(raw_stdout.ends_with('\n'));
    assert_eq!(raw_stdout.lines().count(), 1);
    assert!(
        raw_stdout.len() <= crate::agent::MAX_BOOTSTRAP_ENVELOPE_BYTES,
        "Bootstrap was {} bytes",
        raw_stdout.len()
    );
    let envelope = serde_json::from_str::<Value>(&raw_stdout).expect("envelope");
    assert_eq!(envelope["intent"], "agent.bootstrap.read");
    let payload = &envelope["data"];
    let actions = &envelope["actions"];
    assert!(payload.get("protocolVersion").is_none());
    assert!(payload.get("supportedProtocolVersions").is_none());
    assert!(payload.get("cliVersion").is_none());
    assert_eq!(envelope["meta"]["cliVersion"], VERSION);
    assert!(payload.get("commandCatalogVersion").is_none());
    assert_eq!(
        payload["bootstrapBudget"]["maxBytes"],
        crate::agent::MAX_BOOTSTRAP_ENVELOPE_BYTES
    );
    assert!(payload.get("entrySkill").is_none());
    assert!(payload.get("entrySkillUpdate").is_none());
    assert_eq!(payload["focus"]["panelKind"], "wiki");
    assert_eq!(payload["panel"]["context"]["panelKind"], "wiki");
    assert_eq!(payload["panel"]["contextTruncated"], false);
    assert_eq!(payload["panel"]["selection"]["supported"], true);
    assert_eq!(payload["operations"]["activeCount"], 0);
    assert_eq!(payload["operations"]["items"], json!([]));
    assert!(actions["required"].as_array().unwrap().len() >= 3);
    assert!(payload["discovery"]["recommendedDomains"]
        .as_array()
        .unwrap()
        .iter()
        .any(|scope| scope == "wiki"));
    for action in actions["suggested"].as_array().unwrap() {
        assert_action_parses(action);
        assert!(action["condition"].is_object());
    }
    let required_skill = &payload["skills"][0];
    assert_eq!(required_skill["id"], "myopenpanels-panels");
    assert!(Path::new(required_skill["contextPath"].as_str().unwrap()).is_file());
    assert!(Path::new(required_skill["localPath"].as_str().unwrap()).is_file());
    assert!(required_skill["referencePaths"][0]
        .as_str()
        .unwrap()
        .ends_with("references/wiki-contract.md"));
    let wiki_loader = fs::read_to_string(required_skill["contextPath"].as_str().unwrap())
        .expect("Wiki loader");
    assert!(wiki_loader.contains("`wiki.page.search`"));
    assert!(!wiki_loader.contains("`canvas.image.generate`"));
    assert!(!wiki_loader.contains("`writing.write`"));
    assert!(payload["discovery"].get("capabilityIndexAction").is_none());
    assert!(payload["discovery"].get("capabilityListActions").is_none());
    assert!(payload["discovery"].get("guideListAction").is_none());
    assert!(payload["discovery"].get("skillListAction").is_none());
    for removed in [
        "capabilities",
        "availableGuides",
        "availableSkills",
        "knowledgeContext",
        "suggestedCommands",
        "activeOperations",
        "studioBinding",
        "state",
    ] {
        assert!(
            payload.get(removed).is_none(),
            "v2 field remained: {removed}"
        );
    }

    let (code, stdout, stderr) = run(&["agent", "catalog", "--format", "json"]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let index = serde_json::from_str::<Value>(&stdout).expect("catalog index");
    assert!(index.get("catalogVersion").is_none());
    assert!(index["domains"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["domain"] == "wiki"));

    let (code, stdout, stderr) = run(&["agent", "catalog", "--domain", "wiki", "--format", "json"]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let wiki_catalog = serde_json::from_str::<Value>(&stdout).expect("wiki catalog");
    let page_search = wiki_catalog["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["intent"] == "wiki.page.search")
        .unwrap();
    assert!(page_search["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg["flag"] == "--query"));

    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .unwrap();
    for skill in crate::agent::list_agent_skills(&paths).unwrap() {
        for intent in skill.skill.requires_commands {
            assert!(
                crate::cli::registry::catalog_domain_for_intent(&intent).is_some(),
                "skill {} requires uncataloged command {intent}",
                skill.skill.id
            );
        }
    }
}

#[test]
fn procedure_bootstrap_targets_without_changing_focus_and_returns_scoped_commands() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);

    let args = [
        "agent",
        "bootstrap",
        "--procedure",
        "canvas.image.insert",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ];
    let (code, stdout, stderr) = run_raw(&args);
    assert_eq!(code, 0, "{stderr}{stdout}");
    assert!(stdout.len() <= crate::agent::MAX_BOOTSTRAP_ENVELOPE_BYTES);
    let envelope = serde_json::from_str::<Value>(&stdout).expect("procedure bootstrap");
    let payload = &envelope["data"];
    assert!(payload.get("protocolVersion").is_none());
    assert!(payload.get("procedureCatalogVersion").is_none());
    assert_eq!(payload["agentProcedure"]["key"], "canvas.image.insert");
    assert!(payload["agentProcedure"].get("executionMode").is_none());
    assert!(payload.get("workflowCatalogVersion").is_none());
    assert!(payload.get("agentWorkflow").is_none());
    assert_eq!(payload["focus"]["panelKind"], "wiki");
    assert_eq!(payload["target"]["panelKind"], "canvas");
    assert_eq!(payload["readiness"], "ready");
    assert_eq!(payload["commands"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        payload["commands"]["items"][0]["intent"],
        "canvas.image.create"
    );
    assert_eq!(envelope["actions"]["required"], json!([]));
    assert_eq!(payload["agentProcedure"]["skillId"], "myopenpanels-panels");
    assert_canvas_insert_procedure_contract(payload);
    let reference_paths = payload["agentProcedure"]["referencePaths"]
        .as_array()
        .unwrap();
    assert_eq!(reference_paths.len(), 2);
    assert_eq!(
        payload["agentProcedure"]["referencePath"],
        reference_paths.last().unwrap().clone()
    );
    assert!(reference_paths[0]
        .as_str()
        .unwrap()
        .ends_with("references/canvas-contract.md"));
    assert!(reference_paths[1]
        .as_str()
        .unwrap()
        .ends_with("references/canvas-image-insert.md"));
    assert!(envelope["actions"]["suggested"]
        .as_array()
        .unwrap()
        .iter()
        .all(|action| action["intent"] != "agent.catalog"));

    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    assert_eq!(
        crate::control::read_active_panel_value(&paths)
            .expect("active panel")
            .expect("active panel value")["kind"],
        "wiki"
    );

    for procedure in [
        "panel.canvas.selection.read",
        "panel.canvas.selection.export",
        "canvas.image.insert",
        "canvas.image.generate",
        "canvas.image.edit",
        "wiki-space.query",
        "wiki-source.import",
        "my-document.read",
        "my-document.create",
        "my-document.revise",
        "wiki-source.create-from-my-document",
        "my-document.delete",
        "wiki-space.manage",
        "writing.context.read",
        "publication.title.request",
        "task.queue.inspect",
        "task.queue.retry",
        "task.queue.cancel",
        "task.queue.archive",
    ] {
        let (code, stdout, stderr) = run_raw(&[
            "agent",
            "bootstrap",
            "--procedure",
            procedure,
            "--project-dir",
            project_dir.to_str().unwrap(),
            "--storage-dir",
            storage_dir.to_str().unwrap(),
            "--context-id",
            "ctx",
            "--format",
            "json",
        ]);
        assert_eq!(code, 0, "{procedure}: {stderr}{stdout}");
        assert!(
            stdout.len() <= crate::agent::MAX_BOOTSTRAP_ENVELOPE_BYTES,
            "{procedure} Bootstrap was {} bytes",
            stdout.len()
        );
        let envelope = serde_json::from_str::<Value>(&stdout).expect("procedure envelope");
        assert_eq!(envelope["data"]["agentProcedure"]["key"], procedure);
        let reference_paths = envelope["data"]["agentProcedure"]["referencePaths"]
            .as_array()
            .expect("Procedure references");
        assert_eq!(
            envelope["data"]["agentProcedure"]["referencePath"],
            reference_paths.last().unwrap().clone()
        );
        if procedure.starts_with("task.queue.") {
            assert_eq!(
                envelope["data"]["agentProcedure"]["skillId"],
                "myopenpanels-task-queue"
            );
            assert_eq!(reference_paths.len(), 1);
        } else {
            assert_eq!(
                envelope["data"]["agentProcedure"]["skillId"],
                "myopenpanels-panels"
            );
            assert!(!reference_paths.is_empty());
        }
        assert_complete_procedure_package(procedure, &envelope);
    }
}
