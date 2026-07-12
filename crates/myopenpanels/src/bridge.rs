use crate::error::CliError;
use crate::paths::{sanitize_path_part, MyOpenPanelsPaths};
use crate::tasks;
use crate::trace::{self, TraceEventInput};
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_OUTPUT_LIMIT_BYTES: usize = 64 * 1024;
const DEFAULT_WORKER_INTERVAL_MS: u64 = 2000;
const DEFAULT_LOCAL_AGENT_TIMEOUT_MS: u64 = 30 * 60 * 1000;

#[derive(Debug, Clone)]
pub struct BridgeOptions<'a> {
    pub agent_prompt: bool,
    pub capabilities: Vec<String>,
    pub command: Option<&'a str>,
    pub host: Option<&'a str>,
    pub interval_ms: u64,
    pub manual_lifecycle: bool,
    pub name: Option<&'a str>,
    pub once: bool,
    pub priority: i64,
    pub queue: Option<&'a str>,
    pub timeout_ms: u64,
}

pub fn run_bridge(
    paths: &MyOpenPanelsPaths,
    options: BridgeOptions<'_>,
) -> Result<Value, CliError> {
    let command = options.command.ok_or_else(|| {
        CliError::new("Task bridge requires --command <command>. Register webhook and polling targets with agent targets register.")
    })?;
    let default_name = format!("command-bridge:{}", std::process::id());
    let registration = tasks::register_target(
        paths,
        tasks::TargetRegistration {
            name: options.name.unwrap_or(&default_name),
            host: options.host.or(Some("command-bridge")),
            transport: "command",
            endpoint: None,
            capabilities: if options.capabilities.is_empty() {
                vec!["*".to_owned()]
            } else {
                options.capabilities.clone()
            },
            priority: options.priority,
        },
    )?;
    let target_id = registration["target"]["id"]
        .as_str()
        .ok_or_else(|| CliError::new("Command bridge target id is missing."))?
        .to_owned();
    write_bridge_status(paths, "idle", None, None, None)?;
    let interval_ms = options.interval_ms.max(250);
    loop {
        write_bridge_status(paths, "idle", None, None, None)?;
        tasks::heartbeat_target(paths, &target_id)?;
        let payload = tasks::claim_next_filtered(paths, &target_id, None, options.queue, Some(0))?;
        let Some(task) = payload.get("task").filter(|value| !value.is_null()) else {
            if options.once {
                return Ok(json!({ "ran": false, "task": null }));
            }
            std::thread::sleep(Duration::from_millis(interval_ms));
            continue;
        };

        write_bridge_status(paths, "running", Some(task), None, None)?;
        let lease_token = payload
            .get("leaseToken")
            .and_then(Value::as_str)
            .ok_or_else(|| CliError::new("Claimed task lease token is missing."))?;
        let mut result = run_task_command(
            paths,
            command,
            options.timeout_ms,
            task,
            &target_id,
            lease_token,
            options.agent_prompt,
        )?;
        result["targetId"] = json!(target_id);
        if options.manual_lifecycle {
            result["leaseToken"] = json!(lease_token);
        }
        if !options.manual_lifecycle {
            let task_id = task.get("id").and_then(Value::as_str).unwrap_or("");
            let lifecycle_result = if result.get("success").and_then(Value::as_bool) == Some(true) {
                let command_result = result
                    .get("stdout")
                    .and_then(Value::as_str)
                    .and_then(|stdout| serde_json::from_str::<Value>(stdout.trim()).ok())
                    .map(|value| value.get("result").cloned().unwrap_or(value))
                    .or_else(|| {
                        options.agent_prompt.then(|| {
                            json!({
                                "executor": options.host.unwrap_or("local-agent"),
                                "targetId": target_id,
                            })
                        })
                    });
                tasks::complete_task(paths, task_id, lease_token, command_result)
            } else {
                let message = if result.get("timedOut").and_then(Value::as_bool) == Some(true) {
                    "Bridge command timed out.".to_owned()
                } else {
                    result
                        .get("stderr")
                        .and_then(Value::as_str)
                        .filter(|value| !value.trim().is_empty())
                        .map(|value| value.trim().chars().take(500).collect())
                        .unwrap_or_else(|| "Bridge command failed.".to_owned())
                };
                tasks::fail_task(paths, task_id, lease_token, &message, None)
            };
            match lifecycle_result {
                Ok(lifecycle) => {
                    result["lifecycle"] = lifecycle;
                }
                Err(error) => {
                    let lifecycle = recover_lifecycle_after_error(paths, task_id, &error)?;
                    result["lifecycle"] = lifecycle;
                    result["lifecycleError"] = json!({
                        "code": error.code(),
                        "message": error.message(),
                    });
                    if !task_is_terminal(&result["lifecycle"]["task"]) {
                        result["success"] = json!(false);
                        result["error"] = json!(error.message());
                    }
                }
            }
        }
        let status = if result.get("success").and_then(Value::as_bool) == Some(true) {
            "idle"
        } else {
            "error"
        };
        let error = result
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let mut status_result = result.clone();
        if let Some(object) = status_result.as_object_mut() {
            object.remove("leaseToken");
        }
        write_bridge_status(paths, status, None, Some(&status_result), error.as_deref())?;
        if options.once {
            return Ok(result);
        }
        std::thread::sleep(Duration::from_millis(interval_ms));
    }
}

pub fn start_builtin_worker_loop(paths: MyOpenPanelsPaths) {
    if cfg!(test)
        || std::env::var("NODE_ENV").ok().as_deref() == Some("test")
        || std::env::var("MYOPENPANELS_DISABLE_LOCAL_AGENT")
            .ok()
            .as_deref()
            == Some("1")
    {
        return;
    }
    let Some(worker) = resolve_local_agent_bridge(&paths) else {
        return;
    };
    thread::spawn(move || {
        let options = BridgeOptions {
            agent_prompt: worker.agent_prompt,
            capabilities: vec!["*".to_owned()],
            command: Some(&worker.command),
            host: Some(&worker.host),
            interval_ms: DEFAULT_WORKER_INTERVAL_MS,
            manual_lifecycle: worker.manual_lifecycle,
            name: Some(&worker.name),
            once: false,
            priority: -100,
            queue: None,
            timeout_ms: DEFAULT_LOCAL_AGENT_TIMEOUT_MS,
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

pub fn read_bridge_status(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let dispatcher = tasks::dispatcher_status(paths)?;
    let path = bridge_status_path(paths);
    if !path.exists() {
        return Ok(json!({
            "status": dispatcher["status"],
            "heartbeatAt": Value::Null,
            "currentTask": Value::Null,
            "lastTask": Value::Null,
            "lastError": Value::Null,
            "updatedAt": Value::Null,
            "dispatcher": dispatcher,
        }));
    }
    let raw = fs::read_to_string(path).map_err(to_cli_error)?;
    let mut status = serde_json::from_str::<Value>(&raw).map_err(to_cli_error)?;
    status["dispatcher"] = dispatcher.clone();
    if status.get("currentTask").is_none_or(Value::is_null) {
        status["status"] = dispatcher["status"].clone();
    }
    Ok(status)
}

fn run_task_command(
    paths: &MyOpenPanelsPaths,
    command: &str,
    timeout_ms: u64,
    task: &Value,
    target_id: &str,
    lease_token: &str,
    agent_prompt: bool,
) -> Result<Value, CliError> {
    let task_id = task.get("id").and_then(Value::as_str).unwrap_or("");
    let queue = task.get("queue").and_then(Value::as_str).unwrap_or("");
    let capability = task.get("capability").and_then(Value::as_str).unwrap_or("");
    let task_input = if agent_prompt {
        local_agent_task_prompt(paths, task)?
    } else {
        serde_json::to_string_pretty(task).map_err(to_cli_error)?
    };
    let mut child = shell_command(command)
        .env("MYOPENPANELS_PROJECT_DIR", &paths.project_dir)
        .env("MYOPENPANELS_STORAGE_DIR", &paths.storage_dir)
        .env("MYOPENPANELS_CONTEXT_ID", &paths.context_id)
        .env("MYOPENPANELS_TASK_ID", task_id)
        .env("MYOPENPANELS_TASK_QUEUE", queue)
        .env("MYOPENPANELS_TASK_CAPABILITY", capability)
        .env("MYOPENPANELS_TARGET_ID", target_id)
        .env("MYOPENPANELS_TASK_LEASE_TOKEN", lease_token)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(to_cli_error)?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(task_input.as_bytes())
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
    let heartbeat_paths = paths.clone();
    let heartbeat_task_id = task_id.to_owned();
    let heartbeat_token = lease_token.to_owned();
    let heartbeat_target_id = target_id.to_owned();
    let heartbeat_running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let heartbeat_flag = heartbeat_running.clone();
    let heartbeat = std::thread::spawn(move || {
        let mut last_heartbeat = Instant::now();
        while heartbeat_flag.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(250));
            if heartbeat_flag.load(std::sync::atomic::Ordering::Relaxed)
                && last_heartbeat.elapsed() >= Duration::from_secs(30)
            {
                let _ =
                    tasks::heartbeat_task(&heartbeat_paths, &heartbeat_task_id, &heartbeat_token);
                let _ = tasks::heartbeat_target(&heartbeat_paths, &heartbeat_target_id);
                last_heartbeat = Instant::now();
            }
        }
    });
    let status = loop {
        if let Some(status) = child.try_wait().map_err(to_cli_error)? {
            break status;
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            terminate_child_process(&mut child);
            break child.wait().map_err(to_cli_error)?;
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    heartbeat_running.store(false, std::sync::atomic::Ordering::Relaxed);
    let _ = heartbeat.join();
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

#[derive(Debug)]
struct LocalAgentBridge {
    agent_prompt: bool,
    command: String,
    host: String,
    manual_lifecycle: bool,
    name: String,
}

fn resolve_local_agent_bridge(paths: &MyOpenPanelsPaths) -> Option<LocalAgentBridge> {
    if let Ok(command) = std::env::var("MYOPENPANELS_AGENT_COMMAND") {
        if !command.trim().is_empty() {
            return Some(LocalAgentBridge {
                agent_prompt: false,
                command,
                host: "configured-agent".to_owned(),
                manual_lifecycle: false,
                name: format!("local-agent:{}", paths.context_id),
            });
        }
    }
    if has_codex_environment() {
        let executable =
            find_executable("codex", std::env::var("MYOPENPANELS_CODEX_EXECUTABLE").ok())?;
        return Some(LocalAgentBridge {
            agent_prompt: true,
            command: format!(
                "{} exec --ignore-user-config --cd {} --add-dir {} --dangerously-bypass-approvals-and-sandbox -",
                shell_quote(&executable),
                shell_quote(&paths.project_dir.display().to_string()),
                shell_quote(&paths.storage_dir.display().to_string()),
            ),
            host: "codex-cli".to_owned(),
            manual_lifecycle: false,
            name: format!("local-codex:{}", paths.context_id),
        });
    }
    None
}

fn local_agent_task_prompt(paths: &MyOpenPanelsPaths, task: &Value) -> Result<String, CliError> {
    let task_json = serde_json::to_string_pretty(task).map_err(to_cli_error)?;
    let task_id = task.get("id").and_then(Value::as_str).unwrap_or("");
    let capability = task.get("capability").and_then(Value::as_str).unwrap_or("");
    let task_type = task.get("type").and_then(Value::as_str).unwrap_or("");
    let document_id = task
        .get("input")
        .and_then(|input| input.get("documentId"))
        .and_then(Value::as_str)
        .or_else(|| {
            task.get("source")
                .and_then(|source| source.get("documentId"))
                .and_then(Value::as_str)
        })
        .unwrap_or("");
    let wiki_space_id = task
        .get("input")
        .and_then(|input| input.get("wikiSpaceId"))
        .and_then(Value::as_str)
        .unwrap_or("wiki:default");
    let cli = std::env::var("MYOPENPANELS_CLI")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .map(|path| path.display().to_string())
        })
        .unwrap_or_else(|| "myopenpanels".to_owned());
    let wiki_skill = if task.get("queue").and_then(Value::as_str) == Some("wiki") {
        task.pointer("/task/agentSkillId")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                crate::wiki::wiki_context(paths).ok().map(|context| {
                    crate::wiki::selected_agent_skill_id(&context["state"]).to_owned()
                })
            })
            .unwrap_or_else(|| "karpathy-llm-wiki".to_owned())
    } else {
        String::new()
    };
    let wiki_steps = match task_type {
        "ingest_markdown_into_wiki" => format!(
            "Load the Wiki panel contract with `{cli} agent skill wiki-panel --task-id {task_id} --format json`, then read its authoring routing reference. Read the source with `{cli} wiki markdown read --document-id {document_id} --format json`. Load the selected authoring skill with `{cli} agent skill {wiki_skill} --task-id {task_id} --format json`, read its SKILL.md, then update useful Wiki pages in `{wiki_space_id}` using `wiki pages write --task-id {task_id}`."
        ),
        "convert_document_to_markdown" => format!(
            "Load the Wiki panel contract with `{cli} agent skill wiki-panel --task-id {task_id} --format json`, then load the selected authoring skill with `{cli} agent skill {wiki_skill} --task-id {task_id} --format json`. Inspect the raw document metadata, follow the selected skill's conversion workflow, and save it with `{cli} wiki markdown write --document-id {document_id} --file <markdown-file> --task-id {task_id} --format json`."
        ),
        "rebuild_wiki_index" => format!(
            "Load the Wiki panel contract with `{cli} agent skill wiki-panel --task-id {task_id} --format json`, then load the selected authoring skill with `{cli} agent skill {wiki_skill} --task-id {task_id} --format json`. Follow the selected skill while maintaining `{wiki_space_id}` with `wiki pages write --task-id {task_id}`."
        ),
        _ if task.get("queue").and_then(Value::as_str) == Some("wiki") => format!(
            "Load the task skill or guide exposed by `{cli} agent capabilities --format json`, then perform the requested Wiki writes using `--task-id {task_id}`."
        ),
        _ => format!(
            "Use `{cli} agent capabilities --format json` to find the commands for capability `{capability}`, then perform the requested panel writes."
        ),
    };
    Ok(format!(
        "You are the local MyOpenPanels agent target. Process exactly one already-claimed task, then stop.\n\nDo not modify MyOpenPanels application source code. Use the agent-facing CLI commands only. The bridge owns claim, heartbeat, complete, fail, and retry; do not call task lifecycle commands yourself. Exit nonzero if you cannot perform the requested panel writes reliably.\n\nCapability: {capability}\nTask id: {task_id}\n\n{wiki_steps}\n\nVerify the requested writes before exiting. Keep the final response brief.\n\nTask JSON:\n{task_json}\n\nProject: {}\nContext: {}",
        paths.project_dir.display(),
        paths.context_id,
    ))
}

fn has_codex_environment() -> bool {
    std::env::var("CODEX_THREAD_ID").is_ok()
        || std::env::var("CODEX_SHELL").is_ok()
        || std::env::var("CODEX_INTERNAL_ORIGINATOR_OVERRIDE").is_ok()
}

fn find_executable(name: &str, override_path: Option<String>) -> Option<String> {
    if let Some(path) = override_path.filter(|path| std::path::Path::new(path).is_file()) {
        return Some(path);
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .map(|directory| directory.join(name))
        .find(|path| path.is_file())
        .map(|path| path.display().to_string())
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':'))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn read_pipe(mut pipe: Option<impl Read>) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    if let Some(pipe) = pipe.as_mut() {
        pipe.read_to_end(&mut bytes)?;
    }
    Ok(bytes)
}

fn recover_lifecycle_after_error(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    error: &CliError,
) -> Result<Value, CliError> {
    let task = tasks::inspect_task(paths, task_id)?;
    if matches!(error.code(), Some("invalid_lease" | "lease_expired"))
        && task_is_terminal(&task["task"])
    {
        return Ok(task);
    }
    Ok(task)
}

fn task_is_terminal(task: &Value) -> bool {
    matches!(
        task.get("status").and_then(Value::as_str),
        Some("succeeded" | "failed" | "cancelled")
    )
}

fn truncate_output(bytes: &[u8], limit: usize) -> String {
    if bytes.len() <= limit {
        return String::from_utf8_lossy(bytes).to_string();
    }
    let mut text = String::from_utf8_lossy(&bytes[..limit]).to_string();
    text.push_str("\n[myopenpanels: output truncated]");
    text
}

fn write_bridge_run(
    paths: &MyOpenPanelsPaths,
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
    paths: &MyOpenPanelsPaths,
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

fn bridge_status_path(paths: &MyOpenPanelsPaths) -> PathBuf {
    paths.context_dir.join("agent-bridge-status.json")
}

#[cfg(unix)]
fn shell_command(command: &str) -> Command {
    use std::os::unix::process::CommandExt;

    let mut child = Command::new("sh");
    child.arg("-c").arg(command);
    child.process_group(0);
    child
}

#[cfg(unix)]
fn terminate_child_process(child: &mut std::process::Child) {
    let process_group = format!("-{}", child.id());
    let _ = Command::new("kill")
        .arg("-TERM")
        .arg(&process_group)
        .status();
    std::thread::sleep(Duration::from_millis(250));
    if matches!(child.try_wait(), Ok(None)) {
        let _ = Command::new("kill")
            .arg("-KILL")
            .arg(&process_group)
            .status();
    }
}

#[cfg(windows)]
fn terminate_child_process(child: &mut std::process::Child) {
    let _ = child.kill();
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
