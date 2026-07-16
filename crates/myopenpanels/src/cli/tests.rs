use super::*;
use base64::Engine;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::fs;
use std::io::Read;
use std::net::TcpListener;
use std::path::Path;
use std::sync::Mutex;
use std::thread;

static TRACE_ENV_LOCK: Mutex<()> = Mutex::new(());

fn run_raw(args: &[&str]) -> (i32, String, String) {
    let argv = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_cli_with_io(&argv, &mut stdout, &mut stderr);
    (
        code,
        String::from_utf8(stdout).expect("stdout should be utf8"),
        String::from_utf8(stderr).expect("stderr should be utf8"),
    )
}

fn assert_action_parses(action: &Value) {
    assert_eq!(action["executor"], "cli");
    let argv = action["argv"]
        .as_array()
        .expect("action argv")
        .iter()
        .map(|value| value.as_str().expect("argv string").to_owned())
        .collect::<Vec<_>>();
    match args::parse(&argv) {
        args::ParseOutcome::Invocation(parsed) => {
            let intent = action["intent"].as_str().unwrap();
            assert_eq!(parsed.intent(), intent);
            assert_eq!(
                parsed.command_id,
                registry::CommandId::from_intent(intent).expect("registered action intent")
            );
        }
        outcome => panic!("action did not parse: {argv:?}: {outcome:?}"),
    }
}

#[track_caller]
fn run(args: &[&str]) -> (i32, String, String) {
    let current = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    let refs = current.iter().map(String::as_str).collect::<Vec<_>>();
    let (code, stdout, stderr) = run_raw(&refs);
    let envelope_source = if stdout.trim().is_empty() {
        &stderr
    } else {
        &stdout
    };
    let Ok(envelope) = serde_json::from_str::<Value>(envelope_source) else {
        return (code, stdout, stderr);
    };
    if envelope.get("schemaVersion").is_none() {
        return (code, stdout, stderr);
    }
    if envelope["ok"].as_bool() == Some(true) {
        let mut data = envelope["data"].clone();
        if let Some(object) = data.as_object_mut() {
            object.insert("actions".to_owned(), envelope["actions"].clone());
        }
        return (
            code,
            format!("{}\n", serde_json::to_string_pretty(&data).expect("data")),
            stderr,
        );
    }
    let legacy_error = json!({
        "code": envelope["error"]["subtype"],
        "error": envelope["error"]["message"],
        "retryable": envelope["error"]["retryable"],
        "recovery": envelope["error"]["hint"],
    });
    (
        code,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&legacy_error).expect("error")
        ),
        String::new(),
    )
}

fn print_myopenpanels_task_id_command() -> &'static str {
    if cfg!(windows) {
        "<nul set /p dummy=%MYOPENPANELS_TASK_ID%"
    } else {
        "printf \"$MYOPENPANELS_TASK_ID\""
    }
}

fn update_task_in_panel_state(storage_dir: &Path, task_id: &str, fields: &[(&str, Value)]) {
    let connection = Connection::open(storage_dir.join("main.sqlite3")).expect("db");
    for (key, value) in fields {
        let column = match *key {
            "leaseOwner" => "lease_owner",
            "leaseExpiresAt" => "lease_expires_at",
            "retryAfter" => "retry_after",
            "status" => "status",
            "attempt" => "attempts",
            "maxAttempts" => "max_attempts",
            _ => panic!("unsupported task field: {key}"),
        };
        let sql = format!("UPDATE tasks SET {column} = ? WHERE id = ?");
        if matches!(*key, "attempt" | "maxAttempts") {
            connection
                .execute(&sql, params![value.as_i64(), task_id])
                .expect("update numeric task field");
        } else {
            connection
                .execute(&sql, params![value.as_str(), task_id])
                .expect("update text task field");
        }
    }
}

fn update_wiki_state_field(storage_dir: &Path, key: &str, value: Value) {
    let connection = Connection::open(storage_dir.join("main.sqlite3")).expect("db");
    let raw: String = connection
        .query_row(
            "SELECT state_json FROM panel_states WHERE state_json LIKE '%\"wikiSpaces\"%' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("wiki panel state");
    let mut state = serde_json::from_str::<Value>(&raw).expect("state json");
    state
        .as_object_mut()
        .expect("wiki state object")
        .insert(key.to_owned(), value);
    connection
        .execute(
            "UPDATE panel_states SET state_json = ? WHERE state_json LIKE '%\"wikiSpaces\"%'",
            [serde_json::to_string(&state).expect("state string")],
        )
        .expect("update wiki panel state");
}

fn fake_studio_server(request_count: usize) -> (u16, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
    let port = listener.local_addr().expect("local addr").port();
    let server = thread::spawn(move || {
        let body = format!(
            "{{\"ok\":true,\"version\":\"{}\"}}",
            env!("CARGO_PKG_VERSION")
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        );
        for _ in 0..request_count {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buffer = [0_u8; 1024];
            let _ = stream.read(&mut buffer);
            stream.write_all(response.as_bytes()).expect("response");
        }
    });
    (port, server)
}

fn create_cli_project(project_dir: &Path, storage_dir: &Path) {
    create_cli_project_unacknowledged(project_dir, storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let pending = crate::agent_control::pending_entry_skill_update(&paths, VERSION)
        .expect("entry skill requirement")
        .expect("pending entry skill update");
    crate::agent_control::acknowledge_entry_skill_update(
        &paths,
        &pending.event_id,
        crate::agent_control::ENTRY_SKILL_VERSION,
    )
    .expect("acknowledge entry skill");
}

fn create_cli_project_unacknowledged(project_dir: &Path, storage_dir: &Path) {
    let (code, stdout, stderr) = run(&[
        "project",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
}

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
    let pending = crate::agent_control::pending_entry_skill_update(&caller_paths, VERSION)
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
    assert_eq!(session.pid, std::process::id());
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
    assert_eq!(bootstrap["protocolVersion"], 6);
    assert_eq!(bootstrap["commandCatalogVersion"], 1);
    assert_eq!(bootstrap["panel"]["context"]["panelKind"], "canvas");
    assert_eq!(bootstrap["panel"]["selection"]["supported"], true);
    assert!(bootstrap.get("capabilities").is_none());
    let required_skill = &bootstrap["skills"][0];
    assert_eq!(required_skill["id"], "canvas-panel");
    assert!(Path::new(required_skill["contextPath"].as_str().unwrap()).is_file());
    assert!(Path::new(required_skill["localPath"].as_str().unwrap()).is_file());

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
    assert_eq!(bootstrap["skills"][0]["id"], "writing-panel");
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
fn cli_rejects_legacy_unknown_and_inapplicable_project_arguments() {
    for args in [
        vec!["project", "create", "--path", ".", "--format", "json"],
        vec!["project", "list", "--title", "Nope", "--format", "json"],
        vec!["studio", "start", "--no-open", "--format", "json"],
        vec!["studio", "start", "--project", ".", "--format", "json"],
    ] {
        let (code, stdout, stderr) = run(&args);
        assert_eq!(code, 2, "unexpected success for {args:?}");
        assert_eq!(stderr, "");
        let payload = serde_json::from_str::<Value>(&stdout).expect("error");
        assert_eq!(payload["code"], "invalid_argument");
        assert_eq!(payload["retryable"], false);
        assert!(payload["recovery"].as_str().is_some());
    }

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
    assert_eq!(payload["protocolVersion"], 6);
    assert!(payload.get("supportedProtocolVersions").is_none());
    assert_eq!(payload["cliVersion"], VERSION);
    assert_eq!(payload["commandCatalogVersion"], 1);
    assert_eq!(payload["bootstrapBudget"]["maxBytes"], 8192);
    assert!(payload.get("entrySkill").is_none());
    assert!(payload.get("entrySkillUpdate").is_none());
    assert_eq!(payload["focus"]["panelKind"], "wiki");
    assert_eq!(payload["panel"]["context"]["panelKind"], "wiki");
    assert_eq!(payload["panel"]["contextTruncated"], false);
    assert_eq!(payload["panel"]["selection"]["supported"], true);
    assert_eq!(payload["operations"]["activeCount"], 0);
    assert_eq!(payload["operations"]["items"], json!([]));
    assert!(actions["required"].as_array().unwrap().len() >= 2);
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
    assert_eq!(required_skill["id"], "wiki-panel");
    assert!(Path::new(required_skill["contextPath"].as_str().unwrap()).is_file());
    assert!(Path::new(required_skill["localPath"].as_str().unwrap()).is_file());
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
    assert_eq!(index["catalogVersion"], 1);
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
fn agent_bootstrap_delivers_entry_skill_update_until_the_context_acknowledges_it() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project_unacknowledged(&project_dir, &storage_dir);

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
    let (code, stdout, stderr) = run(&bootstrap_args);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let pending = serde_json::from_str::<Value>(&stdout).expect("pending bootstrap");
    assert_eq!(
        pending["entrySkillUpdate"]["requiredVersion"],
        crate::agent_control::ENTRY_SKILL_VERSION
    );
    let required_steps = pending["actions"]["required"].as_array().unwrap();
    assert_eq!(required_steps[0]["executor"], "agent-host");
    assert_eq!(
        required_steps[0]["intent"],
        "agent-host.skill.update-required"
    );
    assert_action_parses(&required_steps[1]);
    assert_eq!(pending["actions"]["suggested"], json!([]));

    let event_id = pending["entrySkillUpdate"]["eventId"]
        .as_str()
        .expect("event id");
    let (code, repeated, stderr) = run(&bootstrap_args);
    assert_eq!(code, 0, "{stderr}{repeated}");
    assert_eq!(
        serde_json::from_str::<Value>(&repeated).unwrap()["entrySkillUpdate"]["eventId"],
        event_id
    );

    let (code, stdout, stderr) = run(&[
        "agent",
        "entry-skill",
        "acknowledge",
        "--event-id",
        event_id,
        "--installed-version",
        "0.0",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 1, "{stderr}{stdout}");
    assert_eq!(
        serde_json::from_str::<Value>(&stdout).unwrap()["code"],
        "entry_skill_version_too_old"
    );

    let (code, stdout, stderr) = run(&[
        "agent",
        "entry-skill",
        "acknowledge",
        "--event-id",
        event_id,
        "--installed-version",
        crate::agent_control::ENTRY_SKILL_VERSION,
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    assert_eq!(
        serde_json::from_str::<Value>(&stdout).unwrap()["acknowledged"],
        true
    );

    let (code, stdout, stderr) = run(&bootstrap_args);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let normal = serde_json::from_str::<Value>(&stdout).expect("normal bootstrap");
    assert!(normal.get("entrySkillUpdate").is_none());
    assert_eq!(normal["skills"][0]["id"], "wiki-panel");
}

#[test]
fn agent_bootstrap_prepares_panel_and_wiki_task_authoring_skills() {
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
    let task = crate::wiki::reindex_wiki_space(&paths, None).expect("reindex task");
    let task_id = task["task"]["id"].as_str().expect("task id");

    let (code, stdout, stderr) = run_raw(&[
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
    assert_eq!(code, 0, "{stderr}{stdout}");
    assert!(
        stdout.len() <= crate::agent::MAX_BOOTSTRAP_ENVELOPE_BYTES,
        "Bootstrap was {} bytes",
        stdout.len()
    );
    let envelope = serde_json::from_str::<Value>(&stdout).expect("bootstrap");
    let payload = &envelope["data"];
    let required_skills = payload["skills"].as_array().unwrap();
    assert_eq!(required_skills.len(), 2);
    assert_eq!(required_skills[0]["id"], "wiki-panel");
    assert_eq!(required_skills[1]["id"], "karpathy-llm-wiki");
    assert_eq!(required_skills[1]["taskId"], task_id);
    for skill in required_skills {
        let context_path = Path::new(skill["contextPath"].as_str().unwrap());
        let local_path = Path::new(skill["localPath"].as_str().unwrap());
        assert!(context_path.is_file());
        assert!(local_path.is_file());
    }
    let authoring_context = fs::read_to_string(required_skills[1]["contextPath"].as_str().unwrap())
        .expect("authoring loader context");
    assert!(authoring_context.contains(task_id));
}

#[test]
fn agent_bootstrap_rejects_protocol_selection_without_target_side_effects() {
    let temp = tempfile::tempdir().expect("temp dir");
    let untouched_project = temp.path().join("project");
    let untouched_storage = temp.path().join("storage");
    let (code, stdout, stderr) = run_raw(&[
        "agent",
        "bootstrap",
        "--protocol-version",
        "99",
        "--project-dir",
        untouched_project.to_str().unwrap(),
        "--storage-dir",
        untouched_storage.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 2, "{stderr}{stdout}");
    assert_eq!(stdout, "");
    let error = serde_json::from_str::<Value>(&stderr).unwrap();
    assert_eq!(error["error"]["type"], "validation");
    assert_eq!(error["error"]["subtype"], "invalid_argument");
    assert_eq!(error["error"]["subtype"], "invalid_argument");
    assert!(error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("unexpected argument '--protocol-version'"));
    assert!(!untouched_project.exists());
    assert!(!untouched_storage.exists());
}

#[test]
fn canvas_write_commands_insert_and_replace_shapes() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    let image_path = project_dir.join("image.png");
    let metadata_path = project_dir.join("image-metadata.json");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let (code, _, stderr) = run(&[
        "panel",
        "activate",
        "--panel-kind",
        "canvas",
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
    fs::write(&image_path, tiny_png()).expect("image");
    fs::write(
        &metadata_path,
        serde_json::to_string_pretty(&json!({
            "generateOptions": {
                "prompt": "Soft editorial product photo",
                "model": "test-image-model",
                "referenceImages": [
                    {
                        "source": "local_path",
                        "path": image_path.to_str().unwrap(),
                        "role": "reference"
                    }
                ]
            },
            "generatedBy": "agent"
        }))
        .unwrap(),
    )
    .expect("metadata");

    let (code, stdout, stderr) = run(&[
        "canvas",
        "image",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--image-file",
        image_path.to_str().unwrap(),
        "--display-width",
        "512",
        "--display-height",
        "512",
        "--metadata-file",
        metadata_path.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let inserted = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(
        inserted["bounds"],
        json!({ "x": 160.0, "y": 160.0, "width": 512.0, "height": 512.0 })
    );

    let (code, stdout, stderr) = run(&[
        "canvas",
        "image",
        "generate",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--display-width",
        "512",
        "--display-height",
        "512",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let generation = serde_json::from_str::<Value>(&stdout).expect("json");
    let placeholder = json!({
        "shapeId": generation["operation"]["target"]["placeholderShapeId"],
        "bounds": generation["operation"]["target"]["bounds"],
    });
    assert_eq!(
        placeholder["bounds"],
        json!({ "x": 160.0, "y": 752.0, "width": 512.0, "height": 512.0 })
    );

    let (code, stdout, stderr) = run(&[
        "canvas",
        "image",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--image-file",
        image_path.to_str().unwrap(),
        "--replace-shape-id",
        placeholder["shapeId"].as_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let replaced = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(replaced["replacedShapeId"], placeholder["shapeId"]);
    assert_eq!(replaced["bounds"], placeholder["bounds"]);

    let (code, stdout, stderr) = run(&[
        "panel",
        "read",
        "--detail",
        "full",
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
    let state = serde_json::from_str::<Value>(&stdout).expect("json")["state"].clone();
    assert_eq!(state["selectedShapeIds"], json!([replaced["shapeId"]]));
    assert!(state["store"][placeholder["shapeId"].as_str().unwrap()].is_null());
    let inserted_asset_id = inserted["assetId"].as_str().unwrap();
    assert_eq!(
        state["store"][inserted_asset_id]["meta"]["generateOptions"]["prompt"],
        "Soft editorial product photo"
    );
    assert_eq!(
        state["store"][inserted_asset_id]["meta"]["generateOptions"]["referenceImages"][0]["path"],
        image_path.to_str().unwrap()
    );
    assert!(state["store"][inserted_asset_id]["meta"]["assetRef"]
        .as_str()
        .is_some_and(|value| value.starts_with("projects/")));

    let (code, stdout, stderr) = run(&[
        "canvas",
        "image",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--image-file",
        image_path.to_str().unwrap(),
        "--replace-shape-id",
        "shape:missing-placeholder",
        "--display-width",
        "256",
        "--display-height",
        "128",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let fallback_insert = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(fallback_insert["replacedShapeId"].is_null());
    assert_eq!(
        fallback_insert["bounds"],
        json!({ "x": 160.0, "y": 1344.0, "width": 256.0, "height": 128.0 })
    );

    let (code, stdout, stderr) = run(&[
        "panel",
        "read",
        "--detail",
        "full",
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
    let state = serde_json::from_str::<Value>(&stdout).expect("json")["state"].clone();
    assert_eq!(
        state["selectedShapeIds"],
        json!([fallback_insert["shapeId"]])
    );
    assert!(state["store"][fallback_insert["shapeId"].as_str().unwrap()].is_object());
}

#[test]
fn wiki_commands_create_markdown_tasks_and_pages() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);

    let (code, stdout, stderr) = run(&[
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
        "Research note",
        "--file-name",
        "research-note.md",
        "--content",
        "# Research note\n\nA useful source.",
        "--space-id",
        "wiki:default",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let raw = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(raw["document"]["conversion"]["status"], "not_required");
    assert_eq!(
        raw["document"]["ingestionByWikiSpace"]["wiki:default"]["status"],
        "queued"
    );

    let (code, stdout, stderr) = run(&[
        "task",
        "next",
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
    let next = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(next["task"]["type"], "ingest_markdown_into_wiki");
    assert_eq!(next["task"]["source"]["agentSkillId"], "karpathy-llm-wiki");
    let task_id = next["task"]["id"].as_str().unwrap();

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
    assert_eq!(code, 0, "{stderr}");
    let context = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(context["tasks"]["next"]["taskId"], task_id);
    assert!(context["tasks"]["next"].get("readCommand").is_none());
    assert!(context["tasks"]["next"].get("readAction").is_none());
    let task_action = context["actions"]["suggested"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["intent"] == "task.read")
        .expect("Task read action");
    assert_action_parses(task_action);

    update_wiki_state_field(
        &storage_dir,
        "wikiAgentSkillId",
        json!("karpathy-llm-wiki-zh"),
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
    assert_eq!(code, 0, "{stderr}");
    let context = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(context["tasks"]["next"]["taskId"], task_id);
    assert!(context.get("state").is_none());
    let task_queue_action = context["actions"]["suggested"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["skillId"] == "task-queue")
        .expect("Task queue Skill action");
    assert_action_parses(task_queue_action);

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "list",
        "--task-type",
        "ingest_markdown_into_wiki",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let authoring_skills = serde_json::from_str::<Value>(&stdout).expect("authoring skills");
    assert_eq!(authoring_skills["skills"].as_array().unwrap().len(), 2);

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "read",
        "--skill-id",
        "wiki-panel",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let panel_skill = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(panel_skill["markdown"]
        .as_str()
        .unwrap_or("")
        .contains("`agent.skill.read`"));

    let (code, stdout, stderr) = run(&[
        "task",
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
    assert_eq!(code, 0, "{stderr}");
    let project_tasks = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(project_tasks["pendingCount"], 1);
    assert_eq!(project_tasks["readyCount"], 1);
    assert_eq!(project_tasks["blockedCount"], 0);
    assert_eq!(project_tasks["tasks"][0]["queue"], "wiki");
    assert_eq!(project_tasks["tasks"][0]["id"], task_id);
    assert_eq!(project_tasks["tasks"][0]["ready"], true);
    assert_eq!(
        project_tasks["tasks"][0]["type"],
        "ingest_markdown_into_wiki"
    );
    assert_eq!(
        project_tasks["tasks"][0]["capability"],
        "wiki.ingestMarkdown"
    );
    assert_eq!(
        project_tasks["tasks"][0]["input"]["documentId"],
        raw["document"]["id"]
    );
    assert_eq!(
        project_tasks["tasks"][0]["source"]["wikiSpaceId"],
        "wiki:default"
    );
    assert_eq!(project_tasks["tasks"][0]["attempt"], 0);
    assert_eq!(project_tasks["tasks"][0]["maxAttempts"], 8);
    assert!(project_tasks["tasks"][0]["lease"]["owner"].is_null());
    assert!(project_tasks["tasks"][0]["retryAfter"].is_null());
    let (code, stdout, stderr) = run(&[
        "task",
        "list",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--status",
        "queued",
        "--pending",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let filtered_project_tasks = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(filtered_project_tasks["pendingCount"], 1);
    assert_eq!(filtered_project_tasks["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(filtered_project_tasks["tasks"][0]["id"], task_id);
    let (code, stdout, stderr) = run(&[
        "task",
        "next",
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
    let project_next = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(project_next["task"]["id"], task_id);
    assert_eq!(project_next["task"]["ready"], true);

    let future = (chrono::Utc::now() + chrono::Duration::minutes(10))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let past = (chrono::Utc::now() - chrono::Duration::minutes(10))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    update_task_in_panel_state(
        &storage_dir,
        task_id,
        &[
            ("leaseOwner", json!("agent:test")),
            ("leaseExpiresAt", json!(future)),
        ],
    );
    let (code, stdout, stderr) = run(&[
        "task",
        "next",
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
    let leased_next = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(leased_next["task"].is_null());
    let (code, stdout, stderr) = run(&[
        "task",
        "list",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--pending",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let leased_list = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(leased_list["readyCount"], 0);
    assert_eq!(leased_list["blockedCount"], 1);
    assert_eq!(leased_list["tasks"][0]["blockedReason"], "leased");
    update_task_in_panel_state(&storage_dir, task_id, &[("leaseExpiresAt", json!(past))]);
    let (code, stdout, stderr) = run(&[
        "task",
        "next",
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
    let expired_lease_next = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(expired_lease_next["task"]["id"], task_id);
    update_task_in_panel_state(
        &storage_dir,
        task_id,
        &[
            ("leaseOwner", Value::Null),
            ("leaseExpiresAt", Value::Null),
            ("retryAfter", json!(future)),
        ],
    );
    let (code, stdout, stderr) = run(&[
        "task",
        "next",
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
    let retry_next = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(retry_next["task"].is_null());
    update_task_in_panel_state(
        &storage_dir,
        task_id,
        &[
            ("retryAfter", Value::Null),
            ("status", json!("failed")),
            ("attempt", json!(3)),
            ("maxAttempts", json!(3)),
        ],
    );
    let (code, stdout, stderr) = run(&[
        "task",
        "next",
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
    let attempts_next = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(attempts_next["task"].is_null());
    update_task_in_panel_state(
        &storage_dir,
        task_id,
        &[
            ("status", json!("queued")),
            ("attempt", json!(0)),
            ("maxAttempts", json!(3)),
        ],
    );

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
        "--command",
        print_myopenpanels_task_id_command(),
        "--manual-lifecycle",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let bridge = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(bridge["ran"], true);
    assert_eq!(bridge["task"]["id"], task_id);
    assert_eq!(bridge["stdout"], task_id);
    let first_lease_token = bridge["leaseToken"].as_str().unwrap();
    let (code, _, stderr) = run(&[
        "task",
        "release",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
        "--lease-token",
        first_lease_token,
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
        "--once",
        "--timeout-ms",
        "50",
        "--command",
        "sleep 1",
        "--manual-lifecycle",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let timed_out_bridge = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(timed_out_bridge["timedOut"], true);
    assert_eq!(timed_out_bridge["success"], false);
    let timeout_lease_token = timed_out_bridge["leaseToken"].as_str().unwrap();
    let (code, _, stderr) = run(&[
        "task",
        "release",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
        "--lease-token",
        timeout_lease_token,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");

    let (code, _, stderr) = run(&[
        "task",
        "retry",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
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
        "--once",
        "--command",
        "yes x | head -c 70000",
        "--manual-lifecycle",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let truncated_bridge = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(truncated_bridge["stdout"]
        .as_str()
        .unwrap()
        .contains("output truncated"));

    let (code, stdout, stderr) = run(&[
        "task",
        "read",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let inspected_task = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(inspected_task["task"]["id"], task_id);
    assert_eq!(inspected_task["task"]["dispatchState"], "running");
    assert!(inspected_task["task"]["assignedTarget"].is_object());
    assert!(!storage_dir
        .join("contexts")
        .join("ctx")
        .join("wakeups")
        .join(format!(
            "{}.json",
            crate::paths::sanitize_path_part(task_id)
        ))
        .exists());

    let truncated_lease_token = truncated_bridge["leaseToken"].as_str().unwrap();

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "read",
        "--skill-id",
        "karpathy-llm-wiki",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains(&format!("- task id: {task_id}")));
    assert!(stdout.contains("`task.claim`"));
    assert!(stdout.contains("Read `SKILL.md` directly from the local path above"));
    assert!(stdout.contains("# Skill: karpathy-llm-wiki"));

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "read",
        "--skill-id",
        "karpathy-llm-wiki",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let skill_payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(skill_payload["skill"]["id"], "karpathy-llm-wiki");
    assert!(
        Path::new(skill_payload["localPath"].as_str().unwrap_or("")).ends_with(
            Path::new(".myopenpanels")
                .join("skills")
                .join("karpathy-llm-wiki")
                .join("SKILL.md")
        )
    );
    assert!(skill_payload["markdown"]
        .as_str()
        .unwrap_or("")
        .contains(&format!("- task id: {task_id}")));

    let page_file = project_dir.join("topic.md");
    fs::write(&page_file, "# Topic\n\nStructured page.").expect("page");
    let (code, stdout, stderr) = run(&[
        "wiki",
        "page",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--space-id",
        "wiki:default",
        "--path",
        "topics/topic.md",
        "--content-file",
        page_file.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let page = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(page["task"]["type"], "rebuild_wiki_index");
    assert_eq!(page["task"]["wikiSpaceId"], "wiki:default");
    let page_index_item = page["wikiSpace"]["pageIndex"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["path"] == "topics/topic.md"))
        .expect("wiki page index item");
    assert_eq!(page_index_item["wordCount"], 21);

    let (code, stdout, stderr) = run(&[
        "task",
        "complete",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
        "--lease-token",
        truncated_lease_token,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let complete = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(complete["task"]["status"], "succeeded");
    let (code, stdout, stderr) = run(&[
        "task",
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
    assert_eq!(code, 0, "{stderr}");
    let project_tasks = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(project_tasks["pendingCount"], 1);

    let db = Connection::open(storage_dir.join("main.sqlite3")).expect("db");
    let stored_status: String = db
        .query_row(
            "SELECT status FROM tasks WHERE id = ? AND queue = 'wiki'",
            params![task_id],
            |row| row.get(0),
        )
        .expect("task row");
    assert_eq!(stored_status, "succeeded");

    let binary_path = project_dir.join("archive.bin");
    fs::write(&binary_path, [1_u8, 2, 3]).expect("binary");
    let (code, stdout, stderr) = run(&[
        "wiki",
        "raw",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--source-file",
        binary_path.to_str().unwrap(),
        "--mime-type",
        "application/octet-stream",
        "--space-id",
        "wiki:default",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let binary = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(binary["document"]["conversion"]["status"], "queued");

    let convert_task_id = binary["document"]["conversion"]["taskId"]
        .as_str()
        .expect("task id");
    let (code, stdout, stderr) = run(&[
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
        "wiki-converter",
        "--transport",
        "poll",
        "--capability",
        "wiki.*",
        "--priority",
        "100",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let target = serde_json::from_str::<Value>(&stdout).expect("json");
    let target_id = target["target"]["id"].as_str().expect("target id");
    let (code, stdout, stderr) = run(&[
        "task",
        "claim",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        convert_task_id,
        "--target-id",
        target_id,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}\n{stdout}");
    let conversion_claim = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(conversion_claim["task"]["status"], "running");
    let conversion_lease = conversion_claim["leaseToken"]
        .as_str()
        .expect("lease token");

    let converted_file = project_dir.join("converted.md");
    fs::write(&converted_file, "# Archive\n\nConverted.").expect("converted");
    let (code, stdout, stderr) = run(&[
        "wiki",
        "raw",
        "update",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--raw-document-id",
        binary["document"]["id"].as_str().unwrap(),
        "--content-file",
        converted_file.to_str().unwrap(),
        "--task-id",
        convert_task_id,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let markdown = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(markdown["task"].is_null());

    let (code, stdout, stderr) = run(&[
        "task",
        "complete",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        convert_task_id,
        "--lease-token",
        conversion_lease,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let conversion_complete = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(conversion_complete["task"]["status"], "succeeded");

    let (code, stdout, stderr) = run(&[
        "panel",
        "read",
        "--detail",
        "full",
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
    let conversion_state = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(
        conversion_state["state"]["rawDocuments"][0]["conversion"]["status"],
        "ready"
    );
    assert_eq!(
        conversion_state["state"]["rawDocuments"][0]["ingestionByWikiSpace"]["wiki:default"]
            ["status"],
        "queued"
    );
}

#[test]
fn wiki_selection_and_query_context_are_agent_facing_without_panel_state_churn() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);

    let (code, stdout, stderr) = run(&[
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
        "Product brief",
        "--content",
        "# Product brief\n\nMyOpenPanels keeps project knowledge local.",
        "--space-id",
        "wiki:default",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let document = serde_json::from_str::<Value>(&stdout).expect("json")["document"].clone();

    let page_file = project_dir.join("product.md");
    fs::write(
        &page_file,
        "# MyOpenPanels\n\nMyOpenPanels provides a local indexed Wiki for project knowledge.\n",
    )
    .expect("page file");
    let (code, _stdout, stderr) = run(&[
        "wiki",
        "page",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--space-id",
        "wiki:default",
        "--path",
        "concepts/myopenpanels.md",
        "--content-file",
        page_file.to_str().unwrap(),
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
    let storage = Storage::open(&paths).expect("storage");
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let wiki_panel = bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");
    let revision_before = storage
        .read_panel_state_revision(&bootstrap.project.id, &wiki_panel.panel.id)
        .expect("panel revision");
    wiki::write_agent_selection(
        &paths,
        true,
        &[document["id"].as_str().unwrap().to_owned()],
        &[],
    )
    .expect("selection");
    let revision_after = Storage::open(&paths)
        .expect("storage")
        .read_panel_state_revision(&bootstrap.project.id, &wiki_panel.panel.id)
        .expect("panel revision");
    assert_eq!(revision_after, revision_before);

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
    let selection = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(selection["value"]["selection"]["isWikiSelected"], true);
    assert_eq!(
        selection["value"]["selectedRawDocuments"][0]["id"],
        document["id"]
    );
    assert!(
        selection["value"]["selectedRawDocuments"][0]["originalFilePath"]
            .as_str()
            .is_some_and(|path| Path::new(path).is_file())
    );
    assert!(selection["value"]["wiki"].get("loadAction").is_none());
    assert_action_parses(&selection["actions"]["suggested"][0]);
    assert!(selection["value"].get("actions").is_none());

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
    assert_eq!(code, 0, "{stderr}");
    let context = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(context["panel"]["selection"]["isExplicit"], true);
    assert_eq!(
        context["panel"]["selection"]["summary"]["rawDocumentCount"],
        1
    );
    assert!(context.get("knowledgeContext").is_none());

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "read",
        "--skill-id",
        "wiki-panel",
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
    let skill = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(skill["markdown"]
        .as_str()
        .unwrap_or("")
        .contains("`wiki.page.search`"));

    let (code, stdout, stderr) = run(&[
        "wiki",
        "page",
        "search",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--space-id",
        "wiki:default",
        "--query",
        "local indexed Wiki",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let search = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(search["matches"][0]["path"], "concepts/myopenpanels.md");
}

#[test]
fn generated_documents_support_versions_selection_publication_and_deletion() {
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

    let created = wiki::create_generated_document(
        &paths,
        "report.md",
        Some("Report"),
        Some("text/markdown"),
        None,
        Some("thread:1"),
        b"# Report\n\nVersion one.",
    )
    .expect("create generated document");
    let document_id = created["document"]["id"]
        .as_str()
        .expect("document id")
        .to_owned();
    assert_eq!(created["document"]["contentVersion"], 1);
    assert_eq!(created["document"]["wordCount"], 18);
    assert!(wiki::create_generated_document(
        &paths,
        "report.pdf",
        None,
        Some("application/pdf"),
        None,
        None,
        b"not a pdf",
    )
    .is_err());

    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let wiki_panel = bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");
    let revision_before = Storage::open(&paths)
        .expect("storage")
        .read_panel_state_revision(&bootstrap.project.id, &wiki_panel.panel.id)
        .expect("revision");
    let selection =
        wiki::write_agent_selection(&paths, false, &[], &[document_id.clone()]).expect("selection");
    assert_eq!(
        selection["selectedGeneratedDocuments"][0]["id"],
        document_id
    );
    let revision_after = Storage::open(&paths)
        .expect("storage")
        .read_panel_state_revision(&bootstrap.project.id, &wiki_panel.panel.id)
        .expect("revision");
    assert_eq!(revision_after, revision_before);

    let read = wiki::read_generated_document(&paths, &document_id).expect("read");
    assert_eq!(read["content"], "# Report\n\nVersion one.");
    let first_publish =
        wiki::publish_generated_document(&paths, &document_id, None).expect("first publish");
    assert_eq!(first_publish["rawDocument"]["source"], "agent");
    assert!(wiki::publish_generated_document(&paths, &document_id, None).is_err());

    let updated = wiki::write_generated_document(
        &paths,
        &document_id,
        "report.md",
        Some("text/markdown"),
        b"# Report\n\nVersion two.",
    )
    .expect("update");
    assert_eq!(updated["document"]["contentVersion"], 2);
    assert_eq!(updated["document"]["wordCount"], 18);
    let second_publish =
        wiki::publish_generated_document(&paths, &document_id, None).expect("second publish");
    assert_eq!(
        second_publish["document"]["publishHistory"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );

    wiki::delete_generated_document(&paths, &document_id).expect("delete");
    let context = wiki::wiki_context(&paths).expect("context");
    assert_eq!(
        context["state"]["rawDocuments"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        wiki::read_agent_selection(&paths).expect("selection")["selectedGeneratedDocuments"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
}

#[test]
fn wiki_document_file_names_can_be_renamed_without_changing_extensions() {
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

    let raw = wiki::add_raw_document(
        &paths,
        "draft.md",
        None,
        Some("text/markdown"),
        "user",
        Some("wiki:default"),
        b"# Draft",
    )
    .expect("raw document");
    let raw_id = raw["document"]["id"].as_str().expect("raw id");
    let renamed_raw =
        wiki::rename_raw_document(&paths, raw_id, "final.md").expect("rename raw document");
    assert_eq!(renamed_raw["document"]["originalFileName"], "final.md");
    assert_eq!(renamed_raw["document"]["title"], "final");
    assert!(wiki::raw_document_original(&paths, raw_id)
        .expect("raw original")
        .file_path
        .ends_with("final.md"));

    let generated = wiki::create_generated_document(
        &paths,
        "generated.md",
        None,
        Some("text/markdown"),
        None,
        None,
        b"# Generated",
    )
    .expect("generated document");
    let generated_id = generated["document"]["id"].as_str().expect("generated id");
    let renamed_generated =
        wiki::rename_generated_document_file(&paths, generated_id, "article.md")
            .expect("rename generated document");
    assert_eq!(
        renamed_generated["document"]["originalFileName"],
        "article.md"
    );
    assert_eq!(
        wiki::read_generated_document(&paths, generated_id).expect("generated content")["content"],
        "# Generated"
    );

    wiki::write_page(
        &paths,
        "wiki:default",
        "notes/draft.md",
        "# Page",
        None,
        None,
    )
    .expect("write page");
    let renamed_page =
        wiki::rename_page(&paths, "wiki:default", "notes/draft.md", "notes/final.md")
            .expect("rename page");
    assert_eq!(renamed_page["pagePath"], "notes/final.md");
    assert_eq!(
        wiki::read_page(&paths, "wiki:default", "notes/final.md").expect("renamed page")
            ["markdown"],
        "# Page"
    );
}

#[test]
fn wiki_mdx_upload_skips_conversion_and_queues_ingest() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    update_wiki_state_field(
        &storage_dir,
        "wikiAgentSkillId",
        json!("karpathy-llm-wiki-zh"),
    );
    let mdx_path = project_dir.join("component.mdx");
    let mdx_content = "# Component\n\n<ComponentPreview name=\"Button\" />\n";
    fs::write(&mdx_path, mdx_content).expect("mdx file");

    let (code, stdout, stderr) = run(&[
        "wiki",
        "raw",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--source-file",
        mdx_path.to_str().unwrap(),
        "--mime-type",
        "application/octet-stream",
        "--space-id",
        "wiki:default",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "{stderr}");
    let result = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(result["document"]["conversion"]["status"], "not_required");
    assert_eq!(result["document"]["markdownVersion"], 1);
    assert_eq!(
        result["document"]["wordCount"],
        mdx_content
            .chars()
            .filter(|character| !character.is_whitespace())
            .count()
    );
    assert_eq!(
        result["document"]["ingestionByWikiSpace"]["wiki:default"]["status"],
        "queued"
    );
    assert!(result["state"].get("tasks").is_none());
    let (code, stdout, stderr) = run(&[
        "task",
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
    assert_eq!(code, 0, "{stderr}");
    let tasks = serde_json::from_str::<Value>(&stdout).expect("tasks");
    assert_eq!(tasks["tasks"][0]["type"], "ingest_markdown_into_wiki");
    assert_eq!(
        tasks["tasks"][0]["source"]["agentSkillId"],
        "karpathy-llm-wiki-zh"
    );
}

#[test]
fn agent_bridge_without_command_does_not_process_wiki_tasks() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);

    let (code, stdout, stderr) = run(&[
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
        "Bridge Source",
        "--file-name",
        "bridge-source.md",
        "--content",
        "# Bridge Source\n\nContent imported by the built-in worker.",
        "--space-id",
        "wiki:default",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let created = serde_json::from_str::<Value>(&stdout).expect("json");
    let _task_id = created["document"]["ingestionByWikiSpace"]["wiki:default"]["taskId"]
        .as_str()
        .unwrap();

    let (code, stdout, _stderr) = run(&[
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
        "--format",
        "json",
    ]);
    assert_eq!(code, 2);
    let bridge = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(bridge["error"]
        .as_str()
        .is_some_and(|message| message.contains("requires --command")));

    let (code, stdout, stderr) = run(&[
        "task",
        "list",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--pending",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let pending = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(pending["pendingCount"], 1);
    assert_eq!(pending["tasks"][0]["status"], "queued");

    let (code, stdout, stderr) = run(&[
        "agent",
        "bridge",
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
    let status = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(status["status"], "noTarget");
    assert_eq!(status["queue"]["unhandledCount"], 1);
}

#[test]
fn generic_targets_claim_and_complete_project_tasks() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);

    let (code, stdout, stderr) = run(&[
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
        "Generic Task",
        "--content",
        "# Generic Task\n\nQueue protocol coverage.",
        "--space-id",
        "wiki:default",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let created = serde_json::from_str::<Value>(&stdout).expect("created json");
    let task_id = created["document"]["ingestionByWikiSpace"]["wiki:default"]["taskId"]
        .as_str()
        .unwrap();

    let (code, stdout, stderr) = run(&[
        "task",
        "list",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--pending",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let unhandled = serde_json::from_str::<Value>(&stdout).expect("tasks json");
    assert_eq!(unhandled["unhandledCount"], 1);
    assert_eq!(unhandled["tasks"][0]["dispatchState"], "noTarget");

    let (code, stdout, stderr) = run(&[
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
        "test-poller",
        "--transport",
        "poll",
        "--capability",
        "wiki.ingestMarkdown",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let registered = serde_json::from_str::<Value>(&stdout).expect("target json");
    let target_id = registered["target"]["id"].as_str().unwrap();
    assert!(registered["token"].is_string());

    let (code, _, stderr) = run(&[
        "project",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--title",
        "Different active project",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");

    let (code, stdout, stderr) = run(&[
        "task",
        "claim",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--target-id",
        target_id,
        "--task-id",
        task_id,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let claimed = serde_json::from_str::<Value>(&stdout).expect("claim json");
    assert_eq!(claimed["task"]["id"], task_id);
    assert_eq!(claimed["task"]["attempt"], 1);
    let lease_token = claimed["leaseToken"].as_str().unwrap();

    let (code, _, _) = run(&[
        "task",
        "heartbeat",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
        "--lease-token",
        "wrong-token",
        "--format",
        "json",
    ]);
    assert_eq!(code, 3);

    let result_file = temp.path().join("result.json");
    fs::write(&result_file, r#"{"executor":"test-poller"}"#).expect("result file");
    let (code, stdout, stderr) = run(&[
        "task",
        "complete",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
        "--lease-token",
        lease_token,
        "--result-file",
        result_file.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let completed = serde_json::from_str::<Value>(&stdout).expect("complete json");
    assert_eq!(completed["task"]["status"], "succeeded");
    assert_eq!(completed["task"]["result"]["executor"], "test-poller");

    let (code, stdout, stderr) = run(&[
        "task",
        "list",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--pending",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let pending = serde_json::from_str::<Value>(&stdout).expect("pending json");
    assert_eq!(pending["pendingCount"], 0);
}

#[test]
fn claim_next_respects_target_priority() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let common = [
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ];
    let (code, _, stderr) = run(&[
        "wiki",
        "raw",
        "create",
        "--title",
        "Priority",
        "--content",
        "# Priority",
        "--space-id",
        "wiki:default",
        common[0],
        common[1],
        common[2],
        common[3],
        common[4],
        common[5],
        common[6],
        common[7],
    ]);
    assert_eq!(code, 0, "{stderr}");

    let register = |name: &str, priority: &str| {
        run(&[
            "agent",
            "target",
            "register",
            "--name",
            name,
            "--transport",
            "poll",
            "--capability",
            "wiki.ingestMarkdown",
            "--priority",
            priority,
            common[0],
            common[1],
            common[2],
            common[3],
            common[4],
            common[5],
            common[6],
            common[7],
        ])
    };
    let (code, stdout, stderr) = register("fallback", "-100");
    assert_eq!(code, 0, "{stderr}");
    let fallback = serde_json::from_str::<Value>(&stdout).expect("fallback target");
    let fallback_id = fallback["target"]["id"].as_str().unwrap();
    let (code, stdout, stderr) = register("preferred", "10");
    assert_eq!(code, 0, "{stderr}");
    let preferred = serde_json::from_str::<Value>(&stdout).expect("preferred target");
    let preferred_id = preferred["target"]["id"].as_str().unwrap();

    let claim = |target_id: &str| {
        run(&[
            "task",
            "claim-next",
            "--target-id",
            target_id,
            "--wait-ms",
            "0",
            common[0],
            common[1],
            common[2],
            common[3],
            common[4],
            common[5],
            common[6],
            common[7],
        ])
    };
    let (code, stdout, stderr) = claim(fallback_id);
    assert_eq!(code, 0, "{stderr}");
    let fallback_claim = serde_json::from_str::<Value>(&stdout).expect("fallback claim");
    assert!(fallback_claim["task"].is_null());

    let (code, stdout, stderr) = claim(preferred_id);
    assert_eq!(code, 0, "{stderr}");
    let preferred_claim = serde_json::from_str::<Value>(&stdout).expect("preferred claim");
    assert_eq!(preferred_claim["task"]["capability"], "wiki.ingestMarkdown");
}

#[test]
fn command_bridge_owns_task_lifecycle_by_default() {
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
    assert_eq!(bridge["success"], true);
    assert_eq!(bridge["lifecycle"]["task"]["status"], "succeeded");
    assert_eq!(
        bridge["lifecycle"]["task"]["result"]["executor"],
        "command-test"
    );
}

#[test]
fn target_registration_rejects_removed_webhook_transport() {
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
    let error = crate::tasks::register_target(
        &paths,
        crate::tasks::TargetRegistration {
            name: "legacy-webhook",
            host: None,
            transport: "webhook",
            capabilities: vec!["*".to_owned()],
            priority: 0,
            protocol_version: 2,
            max_concurrency: 1,
            model_gateway_connection_id: None,
        },
    )
    .expect_err("webhook transport must be rejected");
    assert_eq!(error.code(), Some("invalid_target"));

    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let now = crate::control::now_iso();
    Storage::open(&paths)
        .expect("storage")
        .connection()
        .execute(
            r#"INSERT INTO agent_targets (
              id, project_id, name, host, transport, endpoint, capabilities_json,
              priority, status, token_hash, last_error, last_heartbeat_at,
              created_at, updated_at, protocol_version, max_concurrency
            ) VALUES (
              'target:legacy-webhook', ?, 'Legacy Webhook', 'legacy', 'webhook',
              'http://localhost/wake', '["*"]', 100, 'online', 'hash', NULL,
              ?, ?, ?, 3, 4
            )"#,
            params![bootstrap.project.id, now, now, now],
        )
        .expect("seed historical webhook target");
    assert_eq!(
        crate::tasks::list_targets(&paths).expect("targets")["targets"]
            .as_array()
            .expect("target array")
            .len(),
        0
    );
    let claim = crate::tasks::claim_next(&paths, "target:legacy-webhook", None, None)
        .expect_err("historical webhook target cannot claim");
    assert_eq!(claim.code(), Some("target_not_found"));
}

#[test]
fn concurrent_claim_next_assigns_a_task_once() {
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
            name: "concurrent-poller",
            host: Some("test"),
            transport: "poll",
            capabilities: vec!["wiki.ingestMarkdown".to_owned()],
            priority: 0,
            protocol_version: 2,
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
            crate::tasks::claim_next(&paths, &target_id, None, Some(0)).expect("claim")
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
    let skill = include_str!("../../../../skills/myopenpanels/SKILL.md");
    let install = include_str!("../../../../skills/myopenpanels/references/install.md");
    assert!(skill.contains("version: \"5.0\""));
    assert_eq!(crate::agent_control::ENTRY_SKILL_VERSION, "5.0");
    assert!(skill.lines().count() <= 60);
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
    assert!(!skill.contains("agent bootstrap --project-dir"));
    assert!(skill.contains("Before every request that may read, use, or modify"));
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
    assert!(skill.contains("For an open-only request, stop here without Bootstrap"));
    assert!(skill.contains("run a fresh `myopenpanels agent bootstrap"));
    assert!(skill.contains("work clearly unrelated to MyOpenPanels"));
    assert!(skill.contains("Never reuse an earlier Bootstrap result"));
    assert!(skill.contains("`agent catalog --domain <domain>` actions"));
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
            "skillId": "canvas-panel",
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
fn non_text_upload_creates_a_workflow_dag_and_delete_fences_the_attempt() {
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
    assert_eq!(conversion["workflowId"], ingest["workflowId"]);
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
            transport: "poll",
            capabilities: vec!["wiki.convertDocument".to_owned()],
            priority: 10,
            protocol_version: 2,
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
    assert_eq!(claimed["executionProtocolVersion"], 2);
    assert!(claimed["attemptId"].is_string());

    wiki::delete_raw_document(&paths, document_id, Some("wiki:default")).expect("delete");
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
    let workflows = tasks::list_workflows(&paths).expect("workflows");
    assert!(!workflows["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|workflow| workflow["id"] == conversion["workflowId"]));
    let archived_workflow =
        tasks::read_workflow(&paths, conversion["workflowId"].as_str().unwrap())
            .expect("archived workflow history");
    assert_eq!(archived_workflow["workflow"]["status"], "archived");
    assert_eq!(archived_workflow["tasks"].as_array().unwrap().len(), 2);
}

#[test]
fn agent_routing_skips_incompatible_and_saturated_targets() {
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
            "rebuild_wiki_index",
            "wiki.rebuildIndex",
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
            "rebuild_wiki_index",
            "wiki.rebuildIndex",
            "index.md",
            &json!({}),
            &json!({ "wikiSpaceId": "wiki:default" }),
        )
        .expect("second task");
    let legacy = tasks::register_target(
        &paths,
        tasks::TargetRegistration {
            name: "legacy",
            host: Some("test"),
            transport: "poll",
            capabilities: vec!["wiki.rebuildIndex".to_owned()],
            priority: 100,
            protocol_version: 1,
            max_concurrency: 1,
            model_gateway_connection_id: None,
        },
    )
    .expect("legacy target");
    let primary = tasks::register_target(
        &paths,
        tasks::TargetRegistration {
            name: "primary",
            host: Some("test"),
            transport: "poll",
            capabilities: vec!["wiki.rebuildIndex".to_owned()],
            priority: 50,
            protocol_version: 2,
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
            transport: "poll",
            capabilities: vec!["wiki.rebuildIndex".to_owned()],
            priority: 10,
            protocol_version: 2,
            max_concurrency: 1,
            model_gateway_connection_id: None,
        },
    )
    .expect("fallback target");
    tasks::set_agent_route(
        &paths,
        "wiki.rebuildIndex",
        &[
            legacy["target"]["id"].as_str().unwrap().to_owned(),
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
    let storage = Storage::open(&paths).expect("storage");
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
            "rebuild_wiki_index",
            "wiki.rebuildIndex",
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
                capabilities: vec!["wiki.rebuildIndex".to_owned()],
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
        "wiki.rebuildIndex",
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
    let storage = Storage::open(&paths).expect("storage");
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
            "rebuild_wiki_index",
            "wiki.rebuildIndex",
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
                capabilities: vec!["wiki.rebuildIndex".to_owned()],
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
    let pending_outbox: i64 = Storage::open(&paths)
        .expect("storage")
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM dispatch_outbox WHERE task_id = ? AND status = 'pending'",
            [conversion["id"].as_str().unwrap()],
            |row| row.get(0),
        )
        .expect("outbox");
    assert_eq!(pending_outbox, 0);
}
