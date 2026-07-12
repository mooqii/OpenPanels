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
            assert_eq!(parsed.intent, action["intent"].as_str().unwrap());
        }
        outcome => panic!("action did not parse: {argv:?}: {outcome:?}"),
    }
}

fn run(args: &[&str]) -> (i32, String, String) {
    let canonical = canonical_test_args(args);
    let refs = canonical.iter().map(String::as_str).collect::<Vec<_>>();
    let (code, stdout, stderr) = run_raw(&refs);
    let Ok(envelope) = serde_json::from_str::<Value>(&stdout) else {
        return (code, stdout, stderr);
    };
    if envelope.get("schemaVersion").is_none() {
        return (code, stdout, stderr);
    }
    if envelope["ok"].as_bool() == Some(true) {
        return (
            code,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&envelope["data"]).expect("data")
            ),
            stderr,
        );
    }
    let legacy_error = json!({
        "code": envelope["error"]["code"],
        "error": envelope["error"]["message"],
        "retryable": envelope["error"]["retryable"],
        "recovery": envelope["error"]["recovery"]["instruction"],
    });
    (
        code,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&legacy_error).expect("error")
        ),
        stderr,
    )
}

fn canonical_test_args(args: &[&str]) -> Vec<String> {
    let mut values = args
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    if values.as_slice() == ["help"] {
        return vec!["--help".to_owned()];
    }
    if values.len() >= 2 && values[0] == "update" && values[1] == "help" {
        values[1] = "--help".to_owned();
        return values;
    }
    match values.first().map(String::as_str) {
        Some("tasks") => {
            values[0] = "task".to_owned();
            if values.get(1).map(String::as_str) == Some("inspect") {
                values[1] = "read".to_owned();
            } else if values.get(1).map(String::as_str) == Some("deliveries") {
                values[1] = "delivery".to_owned();
                values.insert(2, "list".to_owned());
            }
        }
        Some("project") if values.get(1).map(String::as_str) == Some("select") => {
            values[1] = "activate".to_owned();
            rename_flag(&mut values, "--id", "--project-id");
        }
        Some("panel") if values.get(1).map(String::as_str) == Some("switch") => {
            values[1] = "activate".to_owned();
            rename_flag(&mut values, "--kind", "--panel-kind");
        }
        Some("canvas") => match (
            values.get(1).map(String::as_str),
            values.get(2).map(String::as_str),
        ) {
            (Some("state"), _) => values
                .splice(
                    0..2,
                    ["panel".to_owned(), "state".to_owned(), "read".to_owned()],
                )
                .for_each(drop),
            (Some("selection"), Some("read")) => {
                values
                    .splice(
                        0..3,
                        [
                            "panel".to_owned(),
                            "selection".to_owned(),
                            "read".to_owned(),
                        ],
                    )
                    .for_each(drop);
                remove_flag(&mut values, "--include-image-base64", false);
            }
            (Some("selection"), Some("export")) => {
                rename_flag(&mut values, "--output", "--output-file");
                remove_flag(&mut values, "--allow-fallback", false);
            }
            (Some("image"), Some("insert")) => rename_flag(&mut values, "--image", "--image-file"),
            (Some("generation"), Some("complete")) => {
                values
                    .splice(0..3, ["operation".to_owned(), "complete".to_owned()])
                    .for_each(drop);
                rename_flag(&mut values, "--image", "--artifact-file");
            }
            (Some("generation"), Some("fail" | "cancel" | "inspect")) => {
                let action = if values[2] == "inspect" {
                    "read"
                } else {
                    values[2].as_str()
                }
                .to_owned();
                values
                    .splice(0..3, ["operation".to_owned(), action])
                    .for_each(drop);
            }
            _ => {}
        },
        Some("wiki") => canonicalize_wiki(&mut values),
        Some("agent") => canonicalize_agent(&mut values),
        _ => {}
    }
    maybe_add_focus_revision(&mut values);
    values
}

fn canonicalize_wiki(values: &mut Vec<String>) {
    match (
        values.get(1).map(String::as_str),
        values.get(2).map(String::as_str),
    ) {
        (Some("context"), _) => values
            .splice(
                0..2,
                ["panel".to_owned(), "context".to_owned(), "read".to_owned()],
            )
            .for_each(drop),
        (Some("selection"), Some("read")) => values
            .splice(
                0..3,
                [
                    "panel".to_owned(),
                    "selection".to_owned(),
                    "read".to_owned(),
                ],
            )
            .for_each(drop),
        (Some("documents"), _) => values[1] = "raw-document".to_owned(),
        (Some("markdown"), Some(action)) => {
            let action = action.to_owned();
            values
                .splice(
                    1..3,
                    ["raw-document".to_owned(), "markdown".to_owned(), action],
                )
                .for_each(drop);
            rename_flag(values, "--document-id", "--raw-document-id");
            rename_flag(values, "--file", "--content-file");
        }
        (Some("generated-documents"), _) => {
            values[1] = "generated-document".to_owned();
            rename_flag(values, "--document-id", "--generated-document-id");
            rename_flag(values, "--file", "--content-file");
        }
        (Some("spaces"), _) => {
            values[1] = "space".to_owned();
            if values.get(2).map(String::as_str) == Some("switch") {
                values[2] = "activate".to_owned();
            }
        }
        (Some("pages"), _) => {
            values[1] = "page".to_owned();
            rename_flag(values, "--file", "--content-file");
        }
        (Some("tasks"), _) => values.splice(0..2, ["task".to_owned()]).for_each(drop),
        (Some("generation"), Some("complete")) => {
            values
                .splice(0..3, ["operation".to_owned(), "complete".to_owned()])
                .for_each(drop);
            rename_flag(values, "--file", "--artifact-file");
        }
        (Some("generation"), Some("fail" | "cancel" | "inspect")) => {
            let action = if values[2] == "inspect" {
                "read"
            } else {
                values[2].as_str()
            }
            .to_owned();
            values
                .splice(0..3, ["operation".to_owned(), action])
                .for_each(drop);
        }
        _ => {}
    }
    if values.get(1).map(String::as_str) == Some("raw-document") {
        let file_flag = if values.get(2).map(String::as_str) == Some("add") {
            "--input-file"
        } else {
            "--content-file"
        };
        rename_flag(values, "--file", file_flag);
    }
    if let Some(index) = values.iter().position(|value| value == "--wiki-space-id") {
        if values.get(index + 1).is_none() {
            values.push("wiki:default".to_owned());
        }
    } else if matches!(values.get(0..3), Some([wiki, resource, action]) if wiki == "wiki" && resource == "raw-document" && matches!(action.as_str(), "add" | "create-markdown"))
    {
        values.push("--wiki-space-id".to_owned());
        values.push("wiki:default".to_owned());
    }
}

fn canonicalize_agent(values: &mut Vec<String>) {
    match values.get(1).map(String::as_str) {
        Some("capabilities") => values
            .splice(1..2, ["capability".to_owned(), "list".to_owned()])
            .for_each(drop),
        Some("guides") => values
            .splice(1..2, ["guide".to_owned(), "list".to_owned()])
            .for_each(drop),
        Some("guide")
            if values.get(2).is_some_and(|value| {
                !value.starts_with("--") && value != "list" && value != "read"
            }) =>
        {
            let id = values.remove(2);
            values.insert(2, "read".to_owned());
            values.insert(3, "--guide-id".to_owned());
            values.insert(4, id);
        }
        Some("skills") => values
            .splice(1..2, ["skill".to_owned(), "list".to_owned()])
            .for_each(drop),
        Some("skill")
            if values.get(2).is_some_and(|value| {
                !value.starts_with("--") && value != "list" && value != "read"
            }) =>
        {
            let id = values.remove(2);
            values.insert(2, "read".to_owned());
            values.insert(3, "--skill-id".to_owned());
            values.insert(4, id);
        }
        Some("bridge") if values.get(2).map(String::as_str) != Some("status") => {
            values.insert(2, "run".to_owned())
        }
        Some("targets") => values[1] = "target".to_owned(),
        Some("operations") => {
            values.remove(0);
            values[0] = "operation".to_owned();
            if values.get(1).map(String::as_str) == Some("inspect") {
                values[1] = "read".to_owned();
            }
        }
        _ => {}
    }
}

fn rename_flag(values: &mut [String], from: &str, to: &str) {
    if let Some(value) = values.iter_mut().find(|value| value.as_str() == from) {
        *value = to.to_owned();
    }
}

fn remove_flag(values: &mut Vec<String>, flag: &str, has_value: bool) {
    if let Some(index) = values.iter().position(|value| value == flag) {
        values.remove(index);
        if has_value && index < values.len() {
            values.remove(index);
        }
    }
}

fn maybe_add_focus_revision(values: &mut Vec<String>) {
    let mutating = matches!(
        values.get(0..3),
        Some([scope, resource, action]) if
            (scope == "canvas" && matches!((resource.as_str(), action.as_str()), ("image", "insert") | ("generation", "begin")))
            || (scope == "wiki" && matches!(action.as_str(), "add" | "create-markdown" | "write" | "create" | "rename" | "delete" | "publish" | "activate" | "begin"))
    );
    if !mutating
        || values
            .iter()
            .any(|value| value == "--expect-focus-revision")
        || values.iter().any(|value| value == "--task-id")
    {
        return;
    }
    let project_dir = flag_value(values, "--project-dir");
    let storage_dir = flag_value(values, "--storage-dir");
    let context_id = flag_value(values, "--context-id");
    let Ok(paths) = resolve_myopenpanels_paths(project_dir, storage_dir, context_id) else {
        return;
    };
    let Ok(revision) = read_focus_revision(&paths) else {
        return;
    };
    values.push("--expect-focus-revision".to_owned());
    values.push(revision.to_string());
}

fn flag_value<'a>(values: &'a [String], flag: &str) -> Option<&'a str> {
    values
        .iter()
        .position(|value| value == flag)
        .and_then(|index| values.get(index + 1))
        .map(String::as_str)
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
    let raw: String = connection
        .query_row(
            "SELECT state_json FROM panel_states WHERE state_json LIKE '%\"tasks\"%' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("panel state");
    let mut state = serde_json::from_str::<Value>(&raw).expect("state json");
    let task = state
        .get_mut("tasks")
        .and_then(Value::as_array_mut)
        .and_then(|tasks| {
            tasks
                .iter_mut()
                .find(|task| task.get("id").and_then(Value::as_str) == Some(task_id))
        })
        .expect("task");
    let object = task.as_object_mut().expect("task object");
    for (key, value) in fields {
        object.insert((*key).to_owned(), value.clone());
    }
    connection
        .execute(
            "UPDATE panel_states SET state_json = ? WHERE state_json LIKE '%\"tasks\"%'",
            [serde_json::to_string(&state).expect("state string")],
        )
        .expect("update panel state");
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
            context_dir: paths.context_dir.display().to_string(),
            context_id: paths.context_id.clone(),
            context_id_source: paths.context_id_source.clone(),
            host: Some("127.0.0.1".to_owned()),
            lan_server_urls: Some(Vec::new()),
            local_server_url: Some(server_url.clone()),
            log_path: paths.context_dir.join("studio.log").display().to_string(),
            pid: std::process::id(),
            port,
            project_dir: paths.project_dir.display().to_string(),
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
fn studio_start_json_reuses_same_project_studio() {
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
            context_dir: owner_paths.context_dir.display().to_string(),
            context_id: owner_paths.context_id.clone(),
            context_id_source: owner_paths.context_id_source.clone(),
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
            project_dir: owner_paths.project_dir.display().to_string(),
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
    assert_eq!(payload["context"]["id"], "owner");
    assert_eq!(payload["projectReady"], true);
    assert_eq!(payload["reusedExisting"], true);
    assert_eq!(payload["serverVersion"], VERSION);
    assert_eq!(payload["lifecycle"], "reused");
    assert_eq!(payload["previousVersion"], Value::Null);
    assert_eq!(payload["browserRefreshRequired"], false);
    assert_eq!(payload["nextRequiredAction"]["intent"], "studio.open");
    assert_eq!(payload["nextRequiredAction"]["required"], true);
    assert_eq!(payload["nextRequiredAction"]["url"], server_url);
    assert_eq!(
        payload["nextRequiredAction"]["preferredTarget"],
        "in_app_browser"
    );
    assert_eq!(
        payload["nextRequiredAction"]["fallback"]["intent"],
        "studio.open-system-browser"
    );
    assert_eq!(
        payload["nextRequiredAction"]["fallback"]["command"],
        "myopenpanels studio open-system-browser"
    );
    assert_eq!(
        payload["nextRequiredAction"]["fallback"]["args"],
        json!([
            "--local-only",
            "--project-dir",
            project_dir,
            "--format",
            "json"
        ])
    );
    assert_eq!(
        payload["nextRequiredAction"]["fallback"]["argv"],
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
    assert_action_parses(&payload["nextRequiredAction"]["fallback"]);
    assert!(payload["nextRequiredAction"]["completionCriterion"]
        .as_str()
        .unwrap()
        .contains("accepted"));
    assert!(storage_dir
        .join("contexts")
        .join("borrower")
        .join("studio-session.json")
        .exists());

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
fn panel_commands_follow_borrowed_running_studio_context() {
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
    let server_url = format!("http://127.0.0.1:{port}");
    write_studio_session(
        &borrower_paths,
        &StudioSession {
            system_browser_url: Some(server_url.clone()),
            context_dir: owner_paths.context_dir.display().to_string(),
            context_id: owner_paths.context_id.clone(),
            context_id_source: owner_paths.context_id_source.clone(),
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
            project_dir: owner_paths.project_dir.display().to_string(),
            server_url,
            started_at: "2026-07-09T00:00:00.000Z".to_owned(),
            storage_dir: owner_paths.storage_dir.display().to_string(),
        },
    )
    .expect("borrowed studio session");

    let resolved =
        resolve_panel_paths_for_active_studio(borrower_paths.clone(), false).expect("resolved");
    assert_eq!(resolved.context_id, "owner");
    assert_eq!(resolved.context_dir, owner_paths.context_dir);

    let explicit =
        resolve_panel_paths_for_active_studio(borrower_paths.clone(), true).expect("explicit");
    assert_eq!(explicit.context_id, "borrower");
    assert_eq!(explicit.context_dir, borrower_paths.context_dir);
    server.join().expect("server thread");
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
            context_dir: paths.context_dir.display().to_string(),
            context_id: paths.context_id.clone(),
            context_id_source: paths.context_id_source.clone(),
            host: Some("127.0.0.1".to_owned()),
            lan_server_urls: Some(Vec::new()),
            local_server_url: Some(server_url.clone()),
            log_path: paths.context_dir.join("studio.log").display().to_string(),
            pid: std::process::id(),
            port: 43_217,
            project_dir: paths.project_dir.display().to_string(),
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

    let payload = studio_system_browser_payload(&result, &bootstrap, |url| {
        assert_eq!(url, server_url);
        Ok(())
    })
    .expect("browser payload");
    assert_eq!(payload["opened"], true);
    assert_eq!(payload["openTarget"], "system_browser");
    assert!(payload.get("nextRequiredAction").is_none());

    let error = studio_system_browser_payload(&result, &bootstrap, |_| {
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
            context_dir: owner_paths.context_dir.display().to_string(),
            context_id: owner_paths.context_id.clone(),
            context_id_source: owner_paths.context_id_source.clone(),
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
            project_dir: owner_paths.project_dir.display().to_string(),
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
    assert_eq!(payload["nextRequiredAction"]["intent"], "studio.open");
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
        context_dir: owner_paths.context_dir.display().to_string(),
        context_id: owner_paths.context_id.clone(),
        context_id_source: owner_paths.context_id_source.clone(),
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
        project_dir: owner_paths.project_dir.display().to_string(),
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

    assert_eq!(code, 1, "{stderr}{stdout}");
    assert!(owner_paths.context_dir.join("studio-session.json").exists());
    assert_eq!(session.pid, std::process::id());
    server.join().expect("server thread");
}

#[test]
fn studio_binding_failure_is_structured() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_file = temp.path().join("storage-file");
    fs::create_dir_all(&project_dir).expect("project dir");
    fs::write(&storage_file, "not a directory").expect("storage file");
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_file.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let session = StudioSession {
        system_browser_url: Some("http://127.0.0.1:1".to_owned()),
        context_dir: paths.context_dir.display().to_string(),
        context_id: paths.context_id.clone(),
        context_id_source: paths.context_id_source.clone(),
        host: Some("127.0.0.1".to_owned()),
        lan_server_urls: Some(Vec::new()),
        local_server_url: Some("http://127.0.0.1:1".to_owned()),
        log_path: paths.context_dir.join("studio.log").display().to_string(),
        pid: std::process::id(),
        port: 1,
        project_dir: paths.project_dir.display().to_string(),
        server_url: "http://127.0.0.1:1".to_owned(),
        started_at: "2026-07-11T00:00:00.000Z".to_owned(),
        storage_dir: paths.storage_dir.display().to_string(),
    };

    let error = bind_and_bootstrap_studio(&paths, &session).expect_err("binding should fail");
    assert_eq!(error.code(), Some("studio_binding_failed"));
    assert!(error.retryable());
    assert!(error.recovery().is_some());
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
        vec!["wiki", "canvas"]
    );
    assert_eq!(stderr, "");

    let (code, stdout, stderr) = run(&[
        "panel",
        "switch",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--kind",
        "canvas",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
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
    assert_eq!(code, 0, "{stderr}");
    let bootstrap = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(bootstrap["protocolVersion"], 3);
    assert_eq!(bootstrap["commandCatalogVersion"], 3);
    assert_eq!(bootstrap["panel"]["context"]["panelKind"], "canvas");
    assert_eq!(bootstrap["panel"]["selection"]["supported"], true);
    assert!(bootstrap.get("capabilities").is_none());
    assert_eq!(
        bootstrap["discovery"]["activePanelSkill"]["id"],
        "canvas-panel"
    );
    assert_eq!(
        bootstrap["discovery"]["activePanelSkill"]["readCommand"],
        "myopenpanels agent skill read --skill-id canvas-panel --format json"
    );
    assert_eq!(
        bootstrap["nextRequiredAction"]["intent"],
        "load-active-panel-skill"
    );
    assert_eq!(bootstrap["nextActions"][0]["skillId"], "canvas-panel");
    assert_eq!(bootstrap["nextActions"][0]["required"], true);
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
        "select",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--id",
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
        assert_eq!(code, 1, "unexpected success for {args:?}");
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
    assert_eq!(code, 1);
    assert_eq!(stderr, "");
    assert_eq!(
        serde_json::from_str::<Value>(&stdout).expect("missing")["code"],
        "project_directory_not_found"
    );
}

#[test]
fn agent_bootstrap_emits_focus_guides_and_capabilities() {
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
    assert_eq!(payload["protocolVersion"], 3);
    assert!(payload.get("supportedProtocolVersions").is_none());
    assert_eq!(payload["cliVersion"], VERSION);
    assert_eq!(payload["commandCatalogVersion"], 3);
    assert_eq!(payload["bootstrapBudget"]["maxBytes"], 8192);
    assert_eq!(payload["entrySkill"]["id"], "myopenpanels");
    assert!(payload["entrySkill"].get("requiredVersion").is_none());
    assert_eq!(payload["focus"]["panelKind"], "wiki");
    assert_eq!(payload["panel"]["context"]["panelKind"], "wiki");
    assert_eq!(payload["panel"]["contextTruncated"], false);
    assert_eq!(payload["panel"]["selection"]["supported"], true);
    assert_eq!(payload["operations"]["activeCount"], 0);
    assert_eq!(payload["operations"]["items"], json!([]));
    assert_eq!(
        payload["nextRequiredAction"]["intent"],
        "load-active-panel-skill"
    );
    assert_eq!(payload["nextRequiredAction"]["required"], true);
    assert_eq!(
        payload["nextRequiredAction"]["actionIntent"],
        "agent.skill.read"
    );
    assert_eq!(payload["discovery"]["activePanelSkill"]["id"], "wiki-panel");
    assert!(payload["discovery"]["recommendedScopes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|scope| scope == "wiki"));
    for action in payload["nextActions"].as_array().unwrap() {
        assert_action_parses(action);
        assert!(action["loadWhen"].as_str().is_some());
    }
    let required_skill_action = &payload["nextActions"][0];
    assert_eq!(required_skill_action["intent"], "agent.skill.read");
    assert_eq!(required_skill_action["required"], true);
    assert_eq!(required_skill_action["skillId"], "wiki-panel");
    assert!(required_skill_action["argv"]
        .as_array()
        .is_some_and(|argv| argv.windows(2).any(|pair| {
            pair[0] == "--project-dir" && pair[1] == project_dir.to_string_lossy().as_ref()
        })));
    assert!(payload["discovery"].get("capabilityIndexAction").is_none());
    assert!(payload["discovery"].get("capabilityListActions").is_none());
    assert!(payload["discovery"].get("guideListAction").is_none());
    assert!(payload["discovery"].get("skillListAction").is_none());
    assert!(payload["discovery"]["activePanelSkill"]
        .get("readAction")
        .is_none());
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

    let (code, stdout, stderr) = run(&["agent", "capability", "list", "--format", "json"]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let index = serde_json::from_str::<Value>(&stdout).expect("scope index");
    assert_eq!(index["catalogVersion"], 3);
    for action in index["nextActions"].as_array().unwrap() {
        assert_action_parses(action);
    }
    assert!(index["scopes"]
        .as_array()
        .expect("scopes")
        .iter()
        .any(|scope| scope["scope"] == "wiki"));

    let (code, stdout, stderr) = run(&[
        "agent",
        "capability",
        "list",
        "--scope",
        "wiki",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let wiki_capabilities = serde_json::from_str::<Value>(&stdout).expect("wiki capabilities");
    assert_eq!(wiki_capabilities["catalogVersion"], 3);
    assert_eq!(wiki_capabilities["scope"], "wiki");
    let page_search = wiki_capabilities["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|capability| capability["intent"] == "wiki.page.search")
        .expect("page search summary");
    assert!(page_search.get("args").is_none());
    assert_eq!(page_search["requiredPanelKind"], "wiki");
    assert_eq!(page_search["command"], "myopenpanels wiki page search");
    assert_eq!(
        page_search["readCommand"],
        "myopenpanels agent capability read --intent wiki.page.search --format json"
    );
    assert!(page_search.get("readAction").is_none());
    for action in wiki_capabilities["nextActions"].as_array().unwrap() {
        assert_action_parses(action);
    }
    assert!(wiki_capabilities["nextActions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action["argv"]
            .as_array()
            .is_some_and(|argv| { argv.iter().any(|value| value == "wiki.page.search") })));

    let (code, stdout, stderr) = run(&[
        "agent",
        "capability",
        "read",
        "--intent",
        "wiki.page.search",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let page_search = serde_json::from_str::<Value>(&stdout).expect("page search descriptor");
    assert_eq!(page_search["catalogVersion"], 3);
    assert_eq!(page_search["capability"]["intent"], "wiki.page.search");
    assert!(page_search["capability"]["args"].as_array().is_some());
    assert!(page_search["capability"].get("argv").is_none());
    assert_eq!(page_search["nextActions"], json!([]));
    assert_eq!(
        page_search["nextRequiredAction"]["intent"],
        "execute-capability"
    );

    let (code, stdout, stderr) = run_raw(&[
        "agent",
        "capability",
        "list",
        "--scope",
        "missing",
        "--format",
        "json",
    ]);
    assert_eq!(code, 1, "{stderr}{stdout}");
    let error = serde_json::from_str::<Value>(&stdout).unwrap();
    assert_eq!(error["error"]["code"], "capability_scope_not_found");
    assert_eq!(
        error["error"]["recovery"]["command"],
        "myopenpanels agent capability list --format json"
    );

    let (code, stdout, stderr) = run(&[
        "agent",
        "guides",
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
    let guides = serde_json::from_str::<Value>(&stdout).expect("guide summaries");
    assert!(guides["guides"]
        .as_array()
        .unwrap()
        .iter()
        .any(|guide| guide["id"] == "tasks.queue"));
    assert!(guides["guides"]
        .as_array()
        .unwrap()
        .iter()
        .all(|guide| guide.get("markdown").is_none()));
    for guide in guides["guides"].as_array().unwrap() {
        assert!(guide.get("readAction").is_none());
    }
    for action in guides["nextActions"].as_array().unwrap() {
        assert_action_parses(action);
    }

    let (code, stdout, stderr) = run(&[
        "agent",
        "skills",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout.trim(), "4 skills");
    assert!(!stdout.contains(".myopenpanels"));
    assert!(!stdout.contains("SKILL.md"));

    let (code, stdout, stderr) = run(&[
        "agent",
        "skills",
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
    let skills_payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(skills_payload["skills"].as_array().unwrap().len(), 4);
    assert_eq!(skills_payload["skills"][0]["id"], "canvas-panel");
    assert!(skills_payload["skills"][0].get("localPath").is_none());
    assert!(skills_payload["skills"][0]
        .get("requiresCapabilities")
        .is_none());
    assert_eq!(skills_payload["skills"][1]["id"], "karpathy-llm-wiki");
    assert_eq!(skills_payload["skills"][2]["id"], "karpathy-llm-wiki-zh");
    assert_eq!(skills_payload["skills"][3]["id"], "wiki-panel");
    assert!(skills_payload["skills"][0]["taskTypes"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(skills_payload["skills"][3]["taskTypes"]
        .as_array()
        .unwrap()
        .is_empty());
    for skill in skills_payload["skills"].as_array().unwrap() {
        assert!(skill.get("readAction").is_none());
    }
    for action in skills_payload["nextActions"].as_array().unwrap() {
        assert_action_parses(action);
    }
    assert_action_parses(
        &registry::command_action(
            "operation.read",
            vec![
                "--operation-id".to_owned(),
                "operation:test".to_owned(),
                "--format".to_owned(),
                "json".to_owned(),
            ],
        )
        .unwrap(),
    );

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "list",
        "--panel-kind",
        "canvas",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let filtered = serde_json::from_str::<Value>(&stdout).expect("filtered skills");
    assert_eq!(filtered["skills"].as_array().unwrap().len(), 1);
    assert_eq!(filtered["skills"][0]["id"], "canvas-panel");

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "list",
        "--panel-kind",
        "canvas",
        "--task-type",
        "ingest_markdown_into_wiki",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    assert!(serde_json::from_str::<Value>(&stdout).unwrap()["skills"]
        .as_array()
        .unwrap()
        .is_empty());

    let (code, stdout, stderr) = run(&[
        "agent",
        "guide",
        "list",
        "--task-type",
        "not-a-task-type",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    assert!(serde_json::from_str::<Value>(&stdout).unwrap()["guides"]
        .as_array()
        .unwrap()
        .is_empty());

    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    for skill in crate::agent::list_agent_skills(&paths).expect("skills") {
        for intent in skill.skill.requires_capabilities {
            assert!(
                crate::cli::registry::capability_payload(&intent).is_some(),
                "skill {} requires missing capability {intent}",
                skill.skill.id
            );
        }
    }
    for guide in crate::agent::list_agent_guides().expect("guides") {
        for intent in guide.requires_capabilities {
            assert!(
                crate::cli::registry::capability_payload(&intent).is_some(),
                "guide {} requires missing capability {intent}",
                guide.id
            );
        }
    }

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "wiki-panel",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("# Skill: wiki-panel"));
    assert!(stdout.contains("Read `SKILL.md` directly"));
    assert!(stdout.contains("myopenpanels panel selection read"));

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
    let skill_payload = serde_json::from_str::<Value>(&stdout).expect("skill payload");
    assert_eq!(
        skill_payload["nextRequiredAction"]["intent"],
        "read-skill-file"
    );
    assert_eq!(skill_payload["nextRequiredAction"]["required"], true);
    assert_eq!(
        skill_payload["nextRequiredAction"]["localPath"],
        skill_payload["localPath"]
    );

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "canvas-panel",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("# Skill: canvas-panel"));
    assert!(stdout.contains("myopenpanels canvas generation begin"));

    let (code, stdout, stderr) = run(&[
        "wiki",
        "context",
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
    assert_eq!(
        serde_json::from_str::<Value>(&stdout).expect("json")["focus"]["panelKind"],
        "wiki"
    );
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
    assert_eq!(code, 1, "{stderr}{stdout}");
    let error = serde_json::from_str::<Value>(&stdout).unwrap();
    assert_eq!(error["error"]["code"], "invalid_argument");
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
        "insert",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--image",
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
        "generation",
        "begin",
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
        "insert",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--image",
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
        "canvas",
        "state",
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
        .is_some_and(|value| value.starts_with("sessions/")));

    let (code, stdout, stderr) = run(&[
        "canvas",
        "image",
        "insert",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--image",
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
        "canvas",
        "state",
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
        "documents",
        "create-markdown",
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
        "wiki",
        "tasks",
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
    assert_eq!(next["task"]["task"]["agentSkillId"], "karpathy-llm-wiki");
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
    assert!(context["tasks"]["next"]["readCommand"]
        .as_str()
        .unwrap_or("")
        .contains(&format!("task read --task-id {task_id}")));
    assert!(context["tasks"]["next"].get("readAction").is_none());
    let task_action = context["nextActions"]
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

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "list",
        "--task-type",
        "ingest_markdown_into_wiki",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let authoring_skills = serde_json::from_str::<Value>(&stdout).expect("authoring skills");
    assert_eq!(authoring_skills["skills"].as_array().unwrap().len(), 2);

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
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
    assert_eq!(code, 0, "{stderr}");
    let panel_skill = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(panel_skill["markdown"]
        .as_str()
        .unwrap_or("")
        .contains(&format!(
            "agent skill read --skill-id karpathy-llm-wiki --task-id {task_id}"
        )));

    let (code, stdout, stderr) = run(&[
        "tasks",
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
    assert_eq!(project_tasks["tasks"][0]["maxAttempts"], 3);
    assert!(project_tasks["tasks"][0]["lease"]["owner"].is_null());
    assert!(project_tasks["tasks"][0]["retryAfter"].is_null());
    let (code, stdout, stderr) = run(&[
        "tasks",
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
        "tasks",
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
        "tasks",
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
        "tasks",
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
        "tasks",
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
        "tasks",
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
        "tasks",
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
        "tasks",
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
        "tasks",
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
        "tasks",
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
        "tasks",
        "inspect",
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
    assert_eq!(inspected_task["task"]["task"]["id"], task_id);

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
    assert!(stdout.contains("myopenpanels task claim"));
    assert!(stdout.contains("Read `SKILL.md` directly from the local path above"));
    assert!(stdout.contains("# Skill: karpathy-llm-wiki"));

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
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
        "pages",
        "write",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--wiki-space-id",
        "wiki:default",
        "--path",
        "topics/topic.md",
        "--file",
        page_file.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let page = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(page["task"]["type"], "rebuild_wiki_index");
    assert_eq!(page["task"]["wikiSpaceId"], "wiki:default");

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
        "tasks",
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
            "SELECT status FROM wiki_tasks WHERE id = ?",
            params![task_id],
            |row| row.get(0),
        )
        .expect("wiki task row");
    assert_eq!(stored_status, "succeeded");
    let stored_project_status: String = db
        .query_row(
            "SELECT status FROM project_tasks WHERE id = ? AND queue = 'wiki'",
            params![task_id],
            |row| row.get(0),
        )
        .expect("project task row");
    assert_eq!(stored_project_status, "succeeded");

    let binary_path = project_dir.join("archive.bin");
    fs::write(&binary_path, [1_u8, 2, 3]).expect("binary");
    let (code, stdout, stderr) = run(&[
        "wiki",
        "documents",
        "add",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--file",
        binary_path.to_str().unwrap(),
        "--mime-type",
        "application/octet-stream",
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
        "markdown",
        "write",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--document-id",
        binary["document"]["id"].as_str().unwrap(),
        "--file",
        converted_file.to_str().unwrap(),
        "--task-id",
        convert_task_id,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
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
        "state",
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
        "documents",
        "create-markdown",
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
        "pages",
        "write",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--wiki-space-id",
        "wiki:default",
        "--path",
        "concepts/myopenpanels.md",
        "--file",
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
        .read_panel_state_revision(&bootstrap.session.id, &wiki_panel.panel.id)
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
        .read_panel_state_revision(&bootstrap.session.id, &wiki_panel.panel.id)
        .expect("panel revision");
    assert_eq!(revision_after, revision_before);

    let (code, stdout, stderr) = run(&[
        "wiki",
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
    assert_action_parses(&selection["nextActions"][0]);
    assert!(selection["value"].get("nextActions").is_none());

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
        .contains("wiki page search --wiki-space-id wiki:default"));

    let (code, stdout, stderr) = run(&[
        "wiki",
        "pages",
        "search",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--wiki-space-id",
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
        .read_panel_state_revision(&bootstrap.session.id, &wiki_panel.panel.id)
        .expect("revision");
    let selection =
        wiki::write_agent_selection(&paths, false, &[], &[document_id.clone()]).expect("selection");
    assert_eq!(
        selection["selectedGeneratedDocuments"][0]["id"],
        document_id
    );
    let revision_after = Storage::open(&paths)
        .expect("storage")
        .read_panel_state_revision(&bootstrap.session.id, &wiki_panel.panel.id)
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
    fs::write(
        &mdx_path,
        "# Component\n\n<ComponentPreview name=\"Button\" />\n",
    )
    .expect("mdx file");

    let (code, stdout, stderr) = run(&[
        "wiki",
        "documents",
        "add",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--file",
        mdx_path.to_str().unwrap(),
        "--mime-type",
        "application/octet-stream",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "{stderr}");
    let result = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(result["document"]["conversion"]["status"], "not_required");
    assert_eq!(result["document"]["markdownVersion"], 1);
    assert_eq!(
        result["document"]["ingestionByWikiSpace"]["wiki:default"]["status"],
        "queued"
    );
    assert_eq!(
        result["state"]["tasks"][0]["type"],
        "ingest_markdown_into_wiki"
    );
    assert_eq!(
        result["state"]["tasks"][0]["agentSkillId"],
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
        "documents",
        "create-markdown",
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
    assert_eq!(code, 1);
    let bridge = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(bridge["error"]
        .as_str()
        .is_some_and(|message| message.contains("requires --command")));

    let (code, stdout, stderr) = run(&[
        "tasks",
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
    assert_eq!(status["dispatcher"]["unhandledCount"], 1);
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
        "documents",
        "create-markdown",
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
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let created = serde_json::from_str::<Value>(&stdout).expect("created json");
    let task_id = created["document"]["ingestionByWikiSpace"]["wiki:default"]["taskId"]
        .as_str()
        .unwrap();

    let (code, stdout, stderr) = run(&[
        "tasks",
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
        "targets",
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
    assert_eq!(code, 0, "{stderr}");
    let claimed = serde_json::from_str::<Value>(&stdout).expect("claim json");
    assert_eq!(claimed["task"]["id"], task_id);
    assert_eq!(claimed["task"]["attempt"], 1);
    let lease_token = claimed["leaseToken"].as_str().unwrap();

    let (code, _, _) = run(&[
        "tasks",
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
    assert_eq!(code, 1);

    let result_file = temp.path().join("result.json");
    fs::write(&result_file, r#"{"executor":"test-poller"}"#).expect("result file");
    let (code, stdout, stderr) = run(&[
        "tasks",
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
        "tasks",
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
        "documents",
        "create-markdown",
        "--title",
        "Priority",
        "--content",
        "# Priority",
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
            "targets",
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
            "tasks",
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
        "documents",
        "create-markdown",
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
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");

    let (code, stdout, stderr) = run(&[
        "agent",
        "bridge",
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
fn webhook_delivery_is_signed_and_does_not_claim_the_task() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let (code, _, stderr) = run(&[
        "wiki",
        "documents",
        "create-markdown",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--title",
        "Webhook Task",
        "--content",
        "# Webhook Task",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");

    let listener = TcpListener::bind("127.0.0.1:0").expect("webhook listener");
    let port = listener.local_addr().expect("webhook addr").port();
    let (sender, receiver) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("webhook accept");
        let mut request = Vec::new();
        loop {
            let mut buffer = [0_u8; 2048];
            let read = stream.read(&mut buffer).expect("webhook request");
            request.extend_from_slice(&buffer[..read]);
            let Some(header_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
                continue;
            };
            let header_text = String::from_utf8_lossy(&request[..header_end]);
            let content_length = header_text
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            if request.len() >= header_end + 4 + content_length {
                break;
            }
        }
        sender
            .send(String::from_utf8_lossy(&request).to_string())
            .expect("captured request");
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\ncontent-length: 0\r\n\r\n")
            .expect("webhook response");
    });
    let endpoint = format!("http://127.0.0.1:{port}/wake");
    let (code, _, stderr) = run(&[
        "agent",
        "targets",
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
        "--endpoint",
        &endpoint,
        "--capability",
        "wiki.ingestMarkdown",
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
    let dispatched =
        crate::tasks::dispatch_webhooks_once(&paths, "http://127.0.0.1:9999").expect("dispatch");
    assert_eq!(dispatched["delivered"], 1);
    let request = receiver.recv().expect("webhook request capture");
    assert!(request
        .to_ascii_lowercase()
        .contains("x-myopenpanels-signature: sha256="));
    assert!(request.contains("wiki.ingestMarkdown"));
    server.join().expect("webhook server");

    let (code, stdout, stderr) = run(&[
        "tasks",
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
    let tasks = serde_json::from_str::<Value>(&stdout).expect("tasks json");
    assert_eq!(tasks["tasks"][0]["attempt"], 0);
    assert_eq!(tasks["tasks"][0]["lastDelivery"]["status"], "sent");
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
        "documents",
        "create-markdown",
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
            endpoint: None,
            capabilities: vec!["wiki.ingestMarkdown".to_owned()],
            priority: 0,
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
    assert_eq!(payload["contextId"], "ctx");
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
        "canvas",
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
        "canvas",
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
        "--output",
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
        "--output",
        output_path.to_str().unwrap(),
        "--allow-fallback",
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
            "{{\"ok\":true,\"schemaVersion\":1,\"intent\":\"cli.version.read\",\"data\":{{\"version\":\"{VERSION}\"}},\"meta\":{{\"cliVersion\":\"{VERSION}\"}}}}\n"
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
    ];

    for command in commands {
        let mut args = command.to_vec();
        args.push("--format=json");
        let (code, stdout, stderr) = run_raw(&args);
        assert_eq!(code, 1, "command unexpectedly succeeded: {command:?}");
        assert_eq!(stderr, "", "unexpected stderr for {command:?}");
        assert!(
            serde_json::from_str::<Value>(&stdout).expect("json")["ok"] == false,
            "missing JSON error for {command:?}: {stdout}"
        );
    }
}

#[test]
fn update_help_prints_manifest_controls() {
    let (code, stdout, stderr) = run(&["update", "help"]);

    assert_eq!(code, 0);
    assert!(stdout.contains("myopenpanels update"));
    assert!(stdout.contains("MYOPENPANELS_UPDATE_MANIFEST_URL"));
    assert_eq!(stderr, "");
}

#[test]
fn unknown_command_prints_text_error() {
    let (code, stdout, stderr) = run_raw(&["nope"]);

    assert_eq!(code, 1);
    assert_eq!(stdout, "");
    assert!(stderr.contains("unrecognized subcommand 'nope'"));
}

#[test]
fn unknown_command_prints_json_error() {
    let (code, stdout, stderr) = run_raw(&["nope", "--format=json"]);

    assert_eq!(code, 1);
    let payload = serde_json::from_str::<Value>(&stdout).expect("json error");
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["intent"], "cli.parse");
    assert_eq!(payload["error"]["code"], "invalid_argument");
    assert!(payload["error"]["message"]
        .as_str()
        .unwrap()
        .contains("unrecognized subcommand"));
    assert_eq!(payload["error"]["retryable"], false);
    assert!(payload["error"]["recovery"]["instruction"]
        .as_str()
        .is_some());
    assert_eq!(stderr, "");
}

#[test]
fn bootstrap_writer_rejects_an_oversized_success_envelope() {
    let mut flags = BTreeMap::new();
    flags.insert("format".to_owned(), FlagValue::String("json".to_owned()));
    let invocation = Invocation {
        flags,
        intent: "agent.bootstrap.read".to_owned(),
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
fn focus_bound_mutations_reject_wrong_panel_and_stale_revision_without_writing() {
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
    let initial_revision = read_focus_revision(&paths)
        .expect("focus revision")
        .to_string();

    let (code, stdout, stderr) = run_raw(&[
        "canvas",
        "generation",
        "begin",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--expect-focus-revision",
        &initial_revision,
        "--format",
        "json",
    ]);
    assert_eq!(code, 1, "{stderr}\n{stdout}");
    let mismatch = serde_json::from_str::<Value>(&stdout).expect("mismatch error");
    assert_eq!(mismatch["error"]["code"], "panel_kind_mismatch");

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

    let (code, stdout, stderr) = run_raw(&[
        "canvas",
        "generation",
        "begin",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--expect-focus-revision",
        &initial_revision,
        "--format",
        "json",
    ]);
    assert_eq!(code, 1, "{stderr}\n{stdout}");
    let stale = serde_json::from_str::<Value>(&stdout).expect("stale error");
    assert_eq!(stale["error"]["code"], "focus_changed");

    let connection = Connection::open(storage_dir.join("main.sqlite3")).expect("database");
    let operation_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM agent_operations", [], |row| {
            row.get(0)
        })
        .expect("operation count");
    assert_eq!(operation_count, 0);
}

#[test]
fn entry_skill_requires_verified_open_and_refreshes_bootstrap_for_panel_work() {
    let skill = include_str!("../../../../skills/myopenpanels/SKILL.md");
    assert!(skill.contains("version: \"3.2\""));
    assert!(!skill.contains("minCliVersion"));
    assert!(!skill.contains("protocol-version"));
    assert!(skill
        .contains("myopenpanels studio start --local-only --project-dir \"$PWD\" --format json"));
    assert!(skill.contains("myopenpanels agent bootstrap --project-dir \"$PWD\" --format json"));
    assert!(!skill.contains("myopenpanels studio open-system-browser"));
    assert!(!skill.contains("myopenpanels agent capability"));
    assert!(!skill.contains("myopenpanels panel "));
    assert!(!skill.contains("myopenpanels wiki "));
    assert!(!skill.contains("myopenpanels canvas "));
    assert!(!skill.contains("myopenpanels task "));
    assert!(!skill.contains("myopenpanels operation "));
    assert!(skill.contains("data.nextRequiredAction.url"));
    assert!(skill.contains("data.nextRequiredAction.fallback.argv"));
    assert!(skill.contains("data.nextActions"));
    assert!(!skill.contains("recommendedScopes"));
    assert!(!skill.contains("capabilityListActions"));
    assert!(!skill.contains("readAction"));
    assert!(!skill.contains("protocolVersion"));
    assert!(!skill.contains("commandCatalogVersion"));
    assert!(skill.contains("Studio readiness only"));
    assert!(skill.contains("If the user only asked to open the panel"));
    assert!(skill.contains("Do not request Agent Bootstrap merely to verify"));
    assert!(skill.contains("run a fresh Agent Bootstrap"));
    assert!(skill.contains("Do not bootstrap for requests clearly unrelated"));
    assert!(skill.contains("do not reuse a previous Bootstrap result"));
    assert!(skill.contains("The active Panel Skill is mandatory"));
    assert!(skill.contains("before evaluating any other"));
    assert!(skill.contains("data.opened: true"));
    assert!(skill.contains("--local-only"));
    assert!(!skill.contains("--no-open"));
}

#[test]
fn task_and_operation_discovery_use_response_level_next_actions() {
    let task_list = with_task_next_actions(json!({ "tasks": [{ "id": "task:1" }] }), true);
    assert_eq!(task_list["nextActions"][0]["intent"], "task.read");
    assert_action_parses(&task_list["nextActions"][0]);

    let task = with_task_next_actions(
        json!({
            "task": {
                "id": "task:1",
                "status": "running",
                "capability": "wiki.page.search",
            }
        }),
        false,
    );
    for action in task["nextActions"].as_array().unwrap() {
        assert_action_parses(action);
        assert_eq!(action["intent"], "agent.capability.read");
    }

    let operation_list =
        with_operation_next_actions(json!({ "operations": [{ "id": "operation:1" }] }), true);
    assert_eq!(operation_list["nextActions"][0]["intent"], "operation.read");
    assert_action_parses(&operation_list["nextActions"][0]);

    let operation = with_operation_next_actions(
        json!({
            "id": "operation:1",
            "status": "active",
            "skillId": "canvas-panel",
            "guideId": "tasks.queue",
        }),
        false,
    );
    for action in operation["nextActions"].as_array().unwrap() {
        assert_action_parses(action);
    }
    assert!(operation["nextActions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action["intent"] == "agent.skill.read"));
    assert!(operation["nextActions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action["argv"]
            .as_array()
            .is_some_and(|argv| { argv.iter().any(|value| value == "operation.complete") })));
}

fn seed_selection_database(
    storage_dir: &Path,
    session_id: &str,
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
    let connection = storage.connection();
    connection
            .execute(
                "INSERT INTO sessions (id, title, created_at, updated_at, panel_ids_json, session_json) VALUES (?, 'Project 1', '2026-07-08T00:00:00.000Z', '2026-07-08T00:00:00.000Z', ?, ?)",
                params![
                    session_id,
                    serde_json::json!([panel_id]).to_string(),
                    serde_json::json!({
                        "id": session_id,
                        "title": "Project 1",
                        "panelIds": [panel_id],
                        "createdAt": "2026-07-08T00:00:00.000Z",
                        "updatedAt": "2026-07-08T00:00:00.000Z"
                    }).to_string()
                ],
            )
            .expect("session");
    connection
            .execute(
                "INSERT INTO panels (id, session_id, kind, title, created_at, updated_at, state_ref, panel_json) VALUES (?, ?, 'canvas', 'Design canvas', '2026-07-08T00:00:00.000Z', '2026-07-08T00:00:00.000Z', NULL, ?)",
                params![
                    panel_id,
                    session_id,
                    serde_json::json!({
                        "id": panel_id,
                        "sessionId": session_id,
                        "kind": "canvas",
                        "title": "Design canvas",
                        "createdAt": "2026-07-08T00:00:00.000Z",
                        "updatedAt": "2026-07-08T00:00:00.000Z"
                    }).to_string()
                ],
            )
            .expect("panel");
    if let Some(state) = state {
        connection
                .execute(
                    "INSERT INTO panel_states (session_id, panel_id, schema_version, state_json, updated_at) VALUES (?, ?, 1, ?, '2026-07-08T00:00:00.000Z')",
                    params![session_id, panel_id, state.to_string()],
                )
                .expect("state");
    }
    connection
            .execute(
                "INSERT INTO panel_selections (session_id, panel_id, asset_ref, selected_shape_ids_json, selection_json, updated_at) VALUES (?, ?, NULL, ?, ?, '2026-07-08T00:00:00.000Z')",
                params![
                    session_id,
                    panel_id,
                    selection["selectedShapeIds"].to_string(),
                    selection.to_string()
                ],
            )
            .expect("selection");
}

fn tiny_png() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==")
            .expect("tiny png")
}
