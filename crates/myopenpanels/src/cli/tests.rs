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
    let pending = crate::agent_control::pending_entry_skill_update(&paths)
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

include!("tests/bootstrap_and_parsing.rs");
include!("tests/bootstrap_procedure_blockers.rs");
include!("tests/workflow_runs.rs");
include!("tests/task_routing.rs");
include!("tests/wiki_generation.rs");
include!("tests/project_and_selection.rs");
include!("tests/recovery_and_dispatch.rs");
include!("tests/content_broker.rs");
include!("tests/writing_and_wiki.rs");
include!("tests/task_scopes.rs");
