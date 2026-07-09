use crate::error::CliError;
use crate::paths::{sanitize_path_part, OpenPanelsPaths};
use crate::tasks::{self, TaskListFilter};
use crate::trace::{self, TraceEventInput};
use crate::wiki;
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_OUTPUT_LIMIT_BYTES: usize = 64 * 1024;
const DEFAULT_WORKER_INTERVAL_MS: u64 = 2000;

#[derive(Debug, Clone)]
pub struct BridgeOptions<'a> {
    pub command: Option<&'a str>,
    pub interval_ms: u64,
    pub once: bool,
    pub queue: Option<&'a str>,
    pub timeout_ms: u64,
}

pub fn run_bridge(paths: &OpenPanelsPaths, options: BridgeOptions<'_>) -> Result<Value, CliError> {
    write_bridge_status(paths, "idle", None, None, None)?;
    let interval_ms = options.interval_ms.max(250);
    loop {
        write_bridge_status(paths, "idle", None, None, None)?;
        let payload = tasks::next_task(
            paths,
            TaskListFilter {
                pending: true,
                queue: options.queue,
                status: None,
            },
        )?;
        let Some(task) = payload.get("task").filter(|value| !value.is_null()) else {
            if options.once {
                return Ok(json!({ "ran": false, "task": null }));
            }
            std::thread::sleep(Duration::from_millis(interval_ms));
            continue;
        };

        write_bridge_status(paths, "running", Some(task), None, None)?;
        let result = if let Some(command) = options.command {
            run_task_command(paths, command, options.timeout_ms, task)?
        } else {
            run_builtin_task(paths, task)?
        };
        let status = if result.get("success").and_then(Value::as_bool) == Some(true) {
            "idle"
        } else {
            "error"
        };
        let error = result
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_owned);
        write_bridge_status(paths, status, None, Some(&result), error.as_deref())?;
        if options.once {
            return Ok(result);
        }
        std::thread::sleep(Duration::from_millis(interval_ms));
    }
}

pub fn start_builtin_worker_loop(paths: OpenPanelsPaths) {
    if std::env::var("OPENPANELS_DISABLE_BUILTIN_WORKER")
        .ok()
        .as_deref()
        == Some("1")
    {
        return;
    }
    thread::spawn(move || {
        let options = BridgeOptions {
            command: None,
            interval_ms: DEFAULT_WORKER_INTERVAL_MS,
            once: false,
            queue: None,
            timeout_ms: 600_000,
        };
        if let Err(error) = run_bridge(&paths, options) {
            let _ = write_bridge_status(&paths, "error", None, None, Some(error.message()));
            trace::record(TraceEventInput {
                audience: None,
                category: Some("error".to_owned()),
                detail: Some(json!({ "error": error.message() })),
                direction: Some("worker".to_owned()),
                release_summary: Some("Task worker stopped".to_owned()),
                run_id: None,
                source: Some("builtin-worker".to_owned()),
                summary: Some(format!("Task worker stopped: {}", error.message())),
                task_id: None,
            });
        }
    });
}

pub fn read_bridge_status(paths: &OpenPanelsPaths) -> Result<Value, CliError> {
    let path = bridge_status_path(paths);
    if !path.exists() {
        return Ok(json!({
            "status": "idle",
            "heartbeatAt": Value::Null,
            "currentTask": Value::Null,
            "lastTask": Value::Null,
            "lastError": Value::Null,
            "updatedAt": Value::Null,
        }));
    }
    let raw = fs::read_to_string(path).map_err(to_cli_error)?;
    serde_json::from_str(&raw).map_err(to_cli_error)
}

fn run_builtin_task(paths: &OpenPanelsPaths, task: &Value) -> Result<Value, CliError> {
    let task_id = task.get("id").and_then(Value::as_str).unwrap_or("");
    let queue = task.get("queue").and_then(Value::as_str).unwrap_or("");
    let capability = task.get("capability").and_then(Value::as_str).unwrap_or("");
    if queue == "wiki" && capability == "wiki.ingestMarkdown" {
        return run_builtin_wiki_ingest(paths, task);
    }
    let payload = json!({
        "ran": true,
        "success": false,
        "status": "no_executor",
        "error": format!("No built-in executor for {queue}:{capability}"),
        "task": task,
    });
    write_bridge_run(paths, task_id, &payload)?;
    Ok(payload)
}

fn run_builtin_wiki_ingest(paths: &OpenPanelsPaths, task: &Value) -> Result<Value, CliError> {
    let task_id = task
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::new("Task id is required."))?;
    let document_id = task
        .get("input")
        .and_then(|input| input.get("documentId"))
        .and_then(Value::as_str)
        .or_else(|| {
            task.get("source")
                .and_then(|source| source.get("documentId"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            task.get("task")
                .and_then(|task| task.get("documentId"))
                .and_then(Value::as_str)
        })
        .ok_or_else(|| CliError::new("Wiki document id is required."))?;
    let wiki_space_id = task
        .get("input")
        .and_then(|input| input.get("wikiSpaceId"))
        .and_then(Value::as_str)
        .or_else(|| {
            task.get("source")
                .and_then(|source| source.get("wikiSpaceId"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            task.get("task")
                .and_then(|task| task.get("wikiSpaceId"))
                .and_then(Value::as_str)
        })
        .unwrap_or("wiki:default");

    let _claim = wiki::claim_task(paths, task_id, Some("builtin-wiki"), None)?;
    let source = wiki::read_markdown(paths, document_id)?;
    let markdown = source.get("markdown").and_then(Value::as_str).unwrap_or("");
    if markdown.trim().is_empty() {
        let message = "Source markdown is empty.";
        let failed = wiki::fail_task(paths, task_id, message)?;
        let payload = json!({
            "ran": true,
            "success": false,
            "status": "failed",
            "error": message,
            "task": failed["task"],
        });
        write_bridge_run(paths, task_id, &payload)?;
        return Ok(payload);
    }

    let document = source.get("document").cloned().unwrap_or(Value::Null);
    let title = document
        .get("title")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(document_id);
    let page_path = builtin_wiki_import_page_path(title, document_id);
    let content = builtin_wiki_page_content(title, document_id, markdown);
    let _page = wiki::write_page(
        paths,
        wiki_space_id,
        &page_path,
        &content,
        Some(title),
        Some(task_id),
    )?;
    let result = json!({
        "executor": "builtin-wiki",
        "pagePath": page_path,
        "documentId": document_id,
    });
    let completed = wiki::complete_task(paths, task_id, Some(result.clone()))?;
    let payload = json!({
        "ran": true,
        "success": true,
        "status": "completed",
        "result": result,
        "task": completed["task"],
    });
    write_bridge_run(paths, task_id, &payload)?;
    Ok(payload)
}

fn builtin_wiki_import_page_path(title: &str, document_id: &str) -> String {
    let raw = title.trim();
    let candidate = if raw.is_empty() { document_id } else { raw };
    let stem = candidate.strip_suffix(".md").unwrap_or(candidate);
    format!("imports/{}.md", sanitize_path_part(stem))
}

fn builtin_wiki_page_content(title: &str, document_id: &str, markdown: &str) -> String {
    let mut content = format!(
        "# {title}\n\nSource document: `{document_id}`\n\n---\n\n{}",
        markdown.trim()
    );
    content.push('\n');
    content
}

fn run_task_command(
    paths: &OpenPanelsPaths,
    command: &str,
    timeout_ms: u64,
    task: &Value,
) -> Result<Value, CliError> {
    let task_id = task.get("id").and_then(Value::as_str).unwrap_or("");
    let queue = task.get("queue").and_then(Value::as_str).unwrap_or("");
    let capability = task.get("capability").and_then(Value::as_str).unwrap_or("");
    let task_json = serde_json::to_string_pretty(task).map_err(to_cli_error)?;
    let mut child = shell_command(command)
        .env("OPENPANELS_PROJECT_DIR", &paths.project_dir)
        .env("OPENPANELS_STORAGE_DIR", &paths.storage_dir)
        .env("OPENPANELS_CONTEXT_ID", &paths.context_id)
        .env("OPENPANELS_TASK_ID", task_id)
        .env("OPENPANELS_TASK_QUEUE", queue)
        .env("OPENPANELS_TASK_CAPABILITY", capability)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(to_cli_error)?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(task_json.as_bytes())
            .map_err(to_cli_error)?;
        stdin.write_all(b"\n").map_err(to_cli_error)?;
    }
    drop(child.stdin.take());

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_reader = std::thread::spawn(move || read_pipe(stdout));
    let stderr_reader = std::thread::spawn(move || read_pipe(stderr));
    let timeout = Duration::from_millis(timeout_ms.max(1));
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(to_cli_error)? {
            break status;
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            let _ = child.kill();
            break child.wait().map_err(to_cli_error)?;
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| CliError::new("Bridge stdout reader failed."))?
        .map_err(to_cli_error)?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| CliError::new("Bridge stderr reader failed."))?
        .map_err(to_cli_error)?;
    let stdout = truncate_output(&stdout, DEFAULT_OUTPUT_LIMIT_BYTES);
    let stderr = truncate_output(&stderr, DEFAULT_OUTPUT_LIMIT_BYTES);
    let payload = json!({
        "ran": true,
        "success": status.success() && !timed_out,
        "timedOut": timed_out,
        "statusCode": status.code(),
        "stdout": stdout,
        "stderr": stderr,
        "task": task,
    });
    write_bridge_run(paths, task_id, &payload)?;
    Ok(payload)
}

fn read_pipe(mut pipe: Option<impl Read>) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    if let Some(pipe) = pipe.as_mut() {
        pipe.read_to_end(&mut bytes)?;
    }
    Ok(bytes)
}

fn truncate_output(bytes: &[u8], limit: usize) -> String {
    if bytes.len() <= limit {
        return String::from_utf8_lossy(bytes).to_string();
    }
    let mut text = String::from_utf8_lossy(&bytes[..limit]).to_string();
    text.push_str("\n[openpanels: output truncated]");
    text
}

fn write_bridge_run(
    paths: &OpenPanelsPaths,
    task_id: &str,
    payload: &Value,
) -> Result<(), CliError> {
    let runs_dir = paths.context_dir.join("bridge-runs");
    fs::create_dir_all(&runs_dir).map_err(to_cli_error)?;
    let timestamp = crate::control::now_iso();
    let file_name = format!(
        "{}-{}.json",
        sanitize_path_part(task_id),
        sanitize_path_part(&timestamp)
    );
    let content = serde_json::to_string_pretty(&json!({
        "createdAt": timestamp,
        "run": payload,
    }))
    .map_err(to_cli_error)?;
    fs::write(runs_dir.join(file_name), content).map_err(to_cli_error)
}

fn write_bridge_status(
    paths: &OpenPanelsPaths,
    status: &str,
    current_task: Option<&Value>,
    last_task: Option<&Value>,
    last_error: Option<&str>,
) -> Result<(), CliError> {
    let now = crate::control::now_iso();
    let mut existing = read_bridge_status(paths).unwrap_or_else(|_| json!({}));
    if !existing.is_object() {
        existing = json!({});
    }
    let object = existing
        .as_object_mut()
        .ok_or_else(|| CliError::new("Bridge status is invalid."))?;
    object.insert("status".to_owned(), json!(status));
    object.insert("heartbeatAt".to_owned(), json!(now));
    object.insert("updatedAt".to_owned(), json!(now));
    object.insert(
        "currentTask".to_owned(),
        current_task.cloned().unwrap_or(Value::Null),
    );
    if let Some(last_task) = last_task {
        object.insert("lastTask".to_owned(), last_task.clone());
    } else {
        object.entry("lastTask").or_insert_with(|| Value::Null);
    }
    if let Some(last_error) = last_error {
        object.insert("lastError".to_owned(), json!(last_error));
    } else if status != "error" {
        object.insert("lastError".to_owned(), Value::Null);
    } else {
        object.entry("lastError").or_insert_with(|| Value::Null);
    }
    let path = bridge_status_path(paths);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(
        path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&existing).map_err(to_cli_error)?
        ),
    )
    .map_err(to_cli_error)
}

fn bridge_status_path(paths: &OpenPanelsPaths) -> PathBuf {
    paths.context_dir.join("agent-bridge-status.json")
}

#[cfg(unix)]
fn shell_command(command: &str) -> Command {
    let mut child = Command::new("sh");
    child.arg("-c").arg(command);
    child
}

#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    let mut child = Command::new("cmd");
    child.arg("/C").arg(command);
    child
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
