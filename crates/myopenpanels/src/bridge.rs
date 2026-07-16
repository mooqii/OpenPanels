use crate::error::CliError;
use crate::paths::{sanitize_path_part, MyOpenPanelsPaths};
use crate::tasks;
use crate::trace::{self, TraceEventInput};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
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
        CliError::with_code(
            "invalid_argument",
            "Task bridge requires --command <command>. Register polling targets with agent target register.",
        )
        .with_param("--command")
    })?;
    let default_name = format!("command-bridge:{}", std::process::id());
    let registration = tasks::register_target(
        paths,
        tasks::TargetRegistration {
            name: options.name.unwrap_or(&default_name),
            host: options.host.or(Some("command-bridge")),
            transport: "command",
            capabilities: if options.capabilities.is_empty() {
                vec!["*".to_owned()]
            } else {
                options.capabilities.clone()
            },
            priority: options.priority,
            protocol_version: 3,
            max_concurrency: 1,
            model_gateway_connection_id: None,
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
            payload.get("attemptId").and_then(Value::as_str),
            payload.get("executionGeneration").and_then(Value::as_i64),
            payload.get("taskBrokerUrl").and_then(Value::as_str),
            payload.get("executionToken").and_then(Value::as_str),
        )?;
        result["targetId"] = json!(target_id);
        finalize_task_result(
            paths,
            &mut result,
            task,
            &target_id,
            lease_token,
            options.agent_prompt,
            options.host.unwrap_or("local-agent"),
            options.manual_lifecycle,
        )?;
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
    thread::spawn(move || run_model_gateway_loop(paths));
}

fn run_model_gateway_loop(paths: MyOpenPanelsPaths) {
    let mut active: BTreeMap<String, (String, String)> = BTreeMap::new();
    loop {
        let specs = match crate::model_gateway::worker_specs(&paths) {
            Ok(specs) => specs,
            Err(error) => {
                record_gateway_error(&paths, error.message());
                thread::sleep(Duration::from_millis(DEFAULT_WORKER_INTERVAL_MS));
                continue;
            }
        };
        if specs.is_empty() {
            if let Ok(payload) = tasks::list_targets(&paths) {
                if let Some(targets) = payload.get("targets").and_then(Value::as_array) {
                    for target in targets.iter().filter(|target| {
                        target
                            .get("modelGatewayConnectionId")
                            .is_some_and(|value| !value.is_null())
                    }) {
                        if let Some(target_id) = target.get("id").and_then(Value::as_str) {
                            let _ = tasks::deactivate_target(
                                &paths,
                                target_id,
                                "Model gateway channel is unavailable.",
                            );
                        }
                    }
                }
            }
            for (_, target_id) in std::mem::take(&mut active).into_values() {
                let _ = tasks::deactivate_target(
                    &paths,
                    &target_id,
                    "Model gateway channel is unavailable.",
                );
            }
            let _ = write_bridge_status(&paths, "disabled", None, None, None);
            thread::sleep(Duration::from_millis(DEFAULT_WORKER_INTERVAL_MS));
            continue;
        }
        let project_id =
            crate::control::read_project_bootstrap(&paths, crate::control::BootstrapRequest::new())
                .ok()
                .map(|bootstrap| bootstrap.project.id)
                .unwrap_or_default();
        let desired_connections = specs
            .iter()
            .map(|spec| spec.connection_id.clone())
            .collect::<HashSet<_>>();
        if let Ok(payload) = tasks::list_targets(&paths) {
            if let Some(targets) = payload.get("targets").and_then(Value::as_array) {
                for target in targets {
                    let Some(connection_id) = target
                        .get("modelGatewayConnectionId")
                        .and_then(Value::as_str)
                    else {
                        continue;
                    };
                    if !desired_connections.contains(connection_id) {
                        if let Some(target_id) = target.get("id").and_then(Value::as_str) {
                            let _ = tasks::deactivate_target(
                                &paths,
                                target_id,
                                "Model gateway channel is unavailable.",
                            );
                        }
                    }
                }
            }
        }
        let stale_connections = active
            .keys()
            .filter(|connection_id| !desired_connections.contains(*connection_id))
            .cloned()
            .collect::<Vec<_>>();
        for connection_id in stale_connections {
            if let Some((_, target_id)) = active.remove(&connection_id) {
                let _ = tasks::deactivate_target(
                    &paths,
                    &target_id,
                    "Model gateway channel is unavailable.",
                );
            }
        }
        for (position, spec) in specs.iter().enumerate() {
            let desired_key = format!("{}:{project_id}", spec.key);
            if active
                .get(&spec.connection_id)
                .is_some_and(|(key, _)| key == &desired_key)
            {
                continue;
            }
            let registration = tasks::register_target(
                &paths,
                tasks::TargetRegistration {
                    name: &format!("model-gateway:{}:{}", paths.context_id, spec.connection_id),
                    host: Some(&spec.host),
                    transport: "command",
                    capabilities: vec!["*".to_owned()],
                    priority: 1000 - position as i64,
                    protocol_version: 3,
                    max_concurrency: 1,
                    model_gateway_connection_id: (spec.connection_id != "custom")
                        .then_some(spec.connection_id.as_str()),
                },
            );
            match registration {
                Ok(payload) => {
                    let Some(target_id) = payload.pointer("/target/id").and_then(Value::as_str)
                    else {
                        record_gateway_error(&paths, "Model gateway target id is missing.");
                        continue;
                    };
                    active.insert(
                        spec.connection_id.clone(),
                        (desired_key, target_id.to_owned()),
                    );
                }
                Err(error) => record_gateway_error(&paths, error.message()),
            }
        }
        let mut claimed = None;
        for spec in &specs {
            let Some((_, target_id)) = active.get(&spec.connection_id) else {
                continue;
            };
            if tasks::heartbeat_target(&paths, target_id).is_err() {
                active.remove(&spec.connection_id);
                continue;
            }
            match tasks::claim_next_filtered(&paths, target_id, None, None, Some(0)) {
                Ok(payload) if payload.get("task").is_some_and(|task| !task.is_null()) => {
                    claimed = Some((spec.clone(), target_id.clone(), payload));
                    break;
                }
                Ok(_) => {}
                Err(error) => {
                    record_gateway_error(&paths, error.message());
                }
            }
        }
        let Some((spec, target_id, payload)) = claimed else {
            let _ = write_bridge_status(&paths, "idle", None, None, None);
            thread::sleep(Duration::from_millis(DEFAULT_WORKER_INTERVAL_MS));
            continue;
        };
        let Some(task) = payload.get("task").filter(|value| !value.is_null()) else {
            continue;
        };
        let Some(lease_token) = payload.get("leaseToken").and_then(Value::as_str) else {
            record_gateway_error(&paths, "Claimed task lease token is missing.");
            thread::sleep(Duration::from_millis(DEFAULT_WORKER_INTERVAL_MS));
            continue;
        };
        let _ = write_bridge_status(&paths, "running", Some(task), None, None);
        let mut result = match run_task_command(
            &paths,
            &spec.command,
            DEFAULT_LOCAL_AGENT_TIMEOUT_MS,
            task,
            &target_id,
            lease_token,
            spec.agent_prompt,
            payload.get("attemptId").and_then(Value::as_str),
            payload.get("executionGeneration").and_then(Value::as_i64),
            payload.get("taskBrokerUrl").and_then(Value::as_str),
            payload.get("executionToken").and_then(Value::as_str),
        ) {
            Ok(result) => result,
            Err(error) => {
                let task_id = task.get("id").and_then(Value::as_str).unwrap_or("");
                let _ = tasks::fail_task_with_class(
                    &paths,
                    task_id,
                    lease_token,
                    error.message(),
                    None,
                    tasks::TaskFailureClass::RetryableChannel,
                );
                record_gateway_error(&paths, error.message());
                thread::sleep(Duration::from_millis(DEFAULT_WORKER_INTERVAL_MS));
                continue;
            }
        };
        result["targetId"] = json!(target_id);
        result["providerId"] = json!(spec.provider_id);
        result["providerName"] = json!(spec.provider_name);
        result["modelGatewayConnectionId"] = json!(spec.connection_id);
        let finalized = finalize_task_result(
            &paths,
            &mut result,
            task,
            &target_id,
            lease_token,
            spec.agent_prompt,
            &spec.host,
            false,
        );
        if let Err(error) = finalized {
            result["success"] = json!(false);
            result["error"] = json!(error.message());
        }
        let success = result.get("success").and_then(Value::as_bool) == Some(true);
        let error = result
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let _ = write_bridge_status(
            &paths,
            if success { "idle" } else { "error" },
            None,
            Some(&result),
            error.as_deref(),
        );
        thread::sleep(Duration::from_millis(DEFAULT_WORKER_INTERVAL_MS));
    }
}

fn record_gateway_error(paths: &MyOpenPanelsPaths, message: &str) {
    let _ = write_bridge_status(paths, "error", None, None, Some(message));
    trace::record(TraceEventInput {
        audience: None,
        category: Some("error".to_owned()),
        detail: Some(json!({ "error": message })),
        direction: Some("worker".to_owned()),
        release_summary: Some("Model gateway error".to_owned()),
        run_id: None,
        source: Some("model-gateway".to_owned()),
        summary: Some(format!("Model gateway error: {message}")),
        task_id: None,
    });
}

fn finalize_task_result(
    paths: &MyOpenPanelsPaths,
    result: &mut Value,
    task: &Value,
    target_id: &str,
    lease_token: &str,
    agent_prompt: bool,
    host: &str,
    manual_lifecycle: bool,
) -> Result<(), CliError> {
    if manual_lifecycle {
        result["leaseToken"] = json!(lease_token);
        return Ok(());
    }
    let task_id = task.get("id").and_then(Value::as_str).unwrap_or("");
    let lifecycle_result = if result.get("success").and_then(Value::as_bool) == Some(true) {
        let command_result = result
            .get("stdout")
            .and_then(Value::as_str)
            .and_then(|stdout| serde_json::from_str::<Value>(stdout.trim()).ok())
            .map(|value| value.get("result").cloned().unwrap_or(value))
            .or_else(|| {
                agent_prompt.then(|| {
                    json!({
                        "executor": host,
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
            let lifecycle = if matches!(
                error.code(),
                Some("invalid_output" | "writing_operation_active" | "writing_skill_not_installed")
            ) {
                let lifecycle =
                    tasks::fail_task(paths, task_id, lease_token, error.message(), None)?;
                tasks::mark_latest_attempt_invalid_output(paths, task_id, error.message())?;
                lifecycle
            } else {
                recover_lifecycle_after_error(paths, task_id, &error)?
            };
            result["lifecycle"] = lifecycle;
            result["lifecycleError"] = json!({
                "code": error.code(),
                "message": error.message(),
            });
            if error.code() == Some("invalid_output")
                || !task_is_terminal(&result["lifecycle"]["task"])
            {
                result["success"] = json!(false);
                result["error"] = json!(error.message());
            }
        }
    }
    Ok(())
}

pub fn read_bridge_status(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let queue = tasks::queue_status(paths)?;
    let path = bridge_status_path(paths);
    if !path.exists() {
        return Ok(json!({
            "status": queue["status"],
            "heartbeatAt": Value::Null,
            "currentTask": Value::Null,
            "lastTask": Value::Null,
            "lastError": Value::Null,
            "updatedAt": Value::Null,
            "queue": queue,
        }));
    }
    let raw = fs::read_to_string(path).map_err(to_cli_error)?;
    let mut status = serde_json::from_str::<Value>(&raw).map_err(to_cli_error)?;
    status["queue"] = queue.clone();
    if status.get("currentTask").is_none_or(Value::is_null) {
        status["status"] = queue["status"].clone();
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
    attempt_id: Option<&str>,
    execution_generation: Option<i64>,
    task_broker_url: Option<&str>,
    execution_token: Option<&str>,
) -> Result<Value, CliError> {
    let task_id = task.get("id").and_then(Value::as_str).unwrap_or("");
    let queue = task.get("queue").and_then(Value::as_str).unwrap_or("");
    let capability = task.get("capability").and_then(Value::as_str).unwrap_or("");
    let execution_workspace = paths
        .storage_dir
        .join("executions")
        .join(sanitize_path_part(task_id))
        .join(format!(
            "{}-{}",
            execution_generation.unwrap_or_default(),
            sanitize_path_part(attempt_id.unwrap_or("attempt"))
        ));
    if execution_workspace.exists() {
        fs::remove_dir_all(&execution_workspace).map_err(to_cli_error)?;
    }
    fs::create_dir_all(&execution_workspace).map_err(to_cli_error)?;
    let _workspace_cleanup = ExecutionWorkspaceCleanup(execution_workspace.clone());
    let execution_task = materialize_task_inputs(paths, task, &execution_workspace)?;
    let task_input = if agent_prompt {
        local_agent_task_prompt(paths, &execution_task)?
    } else {
        serde_json::to_string_pretty(&execution_task).map_err(to_cli_error)?
    };
    let mut child = shell_command(command)
        .current_dir(&execution_workspace)
        .env_remove("MYOPENPANELS_STORAGE_DIR")
        .env("MYOPENPANELS_PROJECT_DIR", &execution_workspace)
        .env("MYOPENPANELS_CONTEXT_ID", &paths.context_id)
        .env("MYOPENPANELS_TASK_ID", task_id)
        .env("MYOPENPANELS_TASK_QUEUE", queue)
        .env("MYOPENPANELS_TASK_CAPABILITY", capability)
        .env("MYOPENPANELS_TARGET_ID", target_id)
        .env("MYOPENPANELS_TASK_LEASE_TOKEN", lease_token)
        .env(
            "MYOPENPANELS_TASK_ATTEMPT_ID",
            attempt_id.unwrap_or_default(),
        )
        .env(
            "MYOPENPANELS_TASK_EXECUTION_GENERATION",
            execution_generation.unwrap_or_default().to_string(),
        )
        .env(
            "MYOPENPANELS_EXECUTION_GENERATION",
            execution_generation.unwrap_or_default().to_string(),
        )
        .env("MYOPENPANELS_TASK_WORKSPACE", &execution_workspace)
        .env("MYOPENPANELS_EXECUTION_WORKSPACE", &execution_workspace)
        .env(
            "MYOPENPANELS_TASK_BROKER_URL",
            task_broker_url.unwrap_or_default(),
        )
        .env(
            "MYOPENPANELS_TASK_TOKEN",
            execution_token.unwrap_or_default(),
        )
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
    let heartbeat_failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let heartbeat_flag = heartbeat_running.clone();
    let heartbeat_failed_flag = heartbeat_failed.clone();
    let heartbeat = std::thread::spawn(move || {
        let mut last_heartbeat = Instant::now();
        while heartbeat_flag.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(250));
            if heartbeat_flag.load(std::sync::atomic::Ordering::Relaxed)
                && last_heartbeat.elapsed() >= Duration::from_secs(30)
            {
                if tasks::heartbeat_task(&heartbeat_paths, &heartbeat_task_id, &heartbeat_token)
                    .is_err()
                {
                    heartbeat_failed_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    break;
                }
                let _ = tasks::heartbeat_target(&heartbeat_paths, &heartbeat_target_id);
                last_heartbeat = Instant::now();
            }
        }
    });
    let status = loop {
        if let Some(status) = child.try_wait().map_err(to_cli_error)? {
            break status;
        }
        if heartbeat_failed.load(std::sync::atomic::Ordering::Relaxed) {
            terminate_child_process(&mut child);
            break child.wait().map_err(to_cli_error)?;
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

fn materialize_task_inputs(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
) -> Result<Value, CliError> {
    let mut materialized = task.clone();
    if task.get("type").and_then(Value::as_str) == Some("convert_document_to_markdown") {
        let document_id = task
            .pointer("/input/documentId")
            .and_then(Value::as_str)
            .or_else(|| task.get("documentId").and_then(Value::as_str));
        if let Some(document_id) = document_id {
            let original = crate::wiki::raw_document_original(paths, document_id)?;
            let inputs_dir = workspace.join("inputs");
            fs::create_dir_all(&inputs_dir).map_err(to_cli_error)?;
            let file_name = original
                .document
                .get("originalFileName")
                .and_then(Value::as_str)
                .unwrap_or("source.bin");
            let destination = inputs_dir.join(crate::paths::sanitize_path_part(file_name));
            fs::copy(&original.file_path, &destination).map_err(to_cli_error)?;
            materialized["executionInputs"]["originalDocument"] = json!({
                "documentId": document_id,
                "fileName": file_name,
                "filePath": destination,
                "mimeType": original.mime_type,
                "sizeBytes": original.size_bytes,
            });
        }
    }
    Ok(materialized)
}

struct ExecutionWorkspaceCleanup(PathBuf);

impl Drop for ExecutionWorkspaceCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
        if let Some(task_dir) = self.0.parent() {
            let _ = fs::remove_dir(task_dir);
        }
    }
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
        .get("source")
        .and_then(|source| source.get("wikiSpaceId"))
        .or_else(|| task.get("input").and_then(|input| input.get("wikiSpaceId")))
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
        task.pointer("/source/agentSkillId")
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
    let writing_skill = task
        .pointer("/input/writingSkillId")
        .and_then(Value::as_str)
        .unwrap_or("<writing-skill-id>");
    let wiki_steps = match task_type {
        "generate_document" if task.get("queue").and_then(Value::as_str) == Some("writing") => format!(
            "Load `{cli} agent skill read --skill-id writing-panel --task-id {task_id} --format json` and read the returned SKILL.md. Read the immutable request with `{cli} writing request read --task-id {task_id} --format json`, then load `{cli} agent skill read --skill-id {writing_skill} --task-id {task_id} --format json` and follow that Writing Skill. Load only the captured sources, begin with `{cli} writing generate --task-id {task_id} --title <title> --document-format markdown --format json`, then complete the returned Operation with the generated UTF-8 document."
        ),
        "refine_writing_skill" if task.get("queue").and_then(Value::as_str) == Some("writing") => format!(
            "Load `{cli} agent skill read --skill-id writing-panel --task-id {task_id} --format json` and `{cli} agent skill read --skill-id writing-skill-refiner --task-id {task_id} --format json`. Read the immutable request with `{cli} writing refinement read --task-id {task_id} --format json`. Read every captured raw or generated document, never read Wiki pages, produce the required self-contained SKILL.md, then install it with `{cli} writing skill install --task-id {task_id} --skill-file <SKILL.md> --format json`."
        ),
        "ingest_markdown_into_wiki" => format!(
            "Load the Wiki panel contract with `{cli} agent skill read --skill-id wiki-panel --task-id {task_id} --format json`, then read its authoring routing reference. Read the source with `{cli} wiki raw read --raw-document-id {document_id} --format json`. Load the selected authoring skill with `{cli} agent skill read --skill-id {wiki_skill} --task-id {task_id} --format json`, read its SKILL.md and routed references, then create useful Wiki pages with `{cli} wiki page create --space-id {wiki_space_id} --path <path.md> --content-file <file> --task-id {task_id} --format json`; use `wiki page update` with the same arguments when revising a page."
        ),
        "convert_document_to_markdown" => format!(
            "Load the Wiki panel contract with `{cli} agent skill read --skill-id wiki-panel --task-id {task_id} --format json`, then load the selected authoring skill with `{cli} agent skill read --skill-id {wiki_skill} --task-id {task_id} --format json`. Convert the immutable file at `executionInputs.originalDocument.filePath` from the Task JSON, and save it with `{cli} wiki raw update --raw-document-id {document_id} --content-file <markdown-file> --task-id {task_id} --format json`."
        ),
        "rebuild_wiki_index" => format!(
            "Load the Wiki panel contract with `{cli} agent skill read --skill-id wiki-panel --task-id {task_id} --format json`, then load the selected authoring skill with `{cli} agent skill read --skill-id {wiki_skill} --task-id {task_id} --format json`. Follow the selected skill while maintaining `{wiki_space_id}` with task-bound `wiki page create` and `wiki page update` commands."
        ),
        _ if task.get("queue").and_then(Value::as_str) == Some("wiki") => format!(
            "Load the task Skill and `{cli} agent catalog --domain wiki --format json`, then perform the requested Wiki writes using `--task-id {task_id}`."
        ),
        _ => format!(
            "Use `{cli} agent catalog --format json` to find the domain for task capability `{capability}`, load that complete domain catalog, then perform the requested panel writes."
        ),
    };
    Ok(format!(
        "You are the local MyOpenPanels agent target. Process exactly one already-claimed task, then stop.\n\nThis is an isolated protocol-v3 Task execution. Do not run Agent Bootstrap or start Studio; the Task Broker provides the captured context. Do not modify MyOpenPanels application source code. Use the agent-facing CLI commands only, always passing the supplied task id on writes. The bridge owns claim, heartbeat, complete, fail, and retry; do not call task lifecycle commands yourself. Exit nonzero if you cannot perform the requested panel writes reliably.\n\nCapability: {capability}\nTask id: {task_id}\n\n{wiki_steps}\n\nVerify the requested writes before exiting. Keep the final response brief.\n\nTask JSON:\n{task_json}\n\nContext: {}",
        paths.context_id,
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writing_bridge_prompt_loads_the_task_selected_skill() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("bridge-writing-test"),
        )
        .expect("paths");
        let prompt = local_agent_task_prompt(
            &paths,
            &json!({
                "id": "task:writing",
                "queue": "writing",
                "type": "generate_document",
                "capability": "writing.generateDocument",
                "input": { "writingSkillId": "writing-xiaohongshu-note" },
                "source": {},
            }),
        )
        .expect("prompt");

        assert!(prompt.contains("--skill-id writing-xiaohongshu-note"));
        assert!(prompt.contains("follow that Writing Skill"));
    }

    #[test]
    fn wiki_ingestion_prompt_uses_current_broker_backed_commands() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("bridge-wiki-test"),
        )
        .expect("paths");
        let prompt = local_agent_task_prompt(
            &paths,
            &json!({
                "id": "task:wiki",
                "queue": "wiki",
                "type": "ingest_markdown_into_wiki",
                "capability": "wiki.ingestMarkdown",
                "input": { "documentId": "raw:source" },
                "source": {
                    "agentSkillId": "karpathy-llm-wiki-zh",
                    "wikiSpaceId": "wiki:research"
                },
            }),
        )
        .expect("prompt");

        assert!(prompt.contains("agent skill read --skill-id wiki-panel"));
        assert!(prompt.contains("wiki raw read --raw-document-id raw:source"));
        assert!(prompt.contains("wiki page create"));
        assert!(prompt.contains("--space-id wiki:research"));
        assert!(prompt.contains("--task-id task:wiki"));
        assert!(!prompt.contains("agent skill wiki-panel"));
        assert!(!prompt.contains("wiki markdown"));
        assert!(!prompt.contains("wiki pages"));
    }

    #[cfg(unix)]
    #[test]
    fn task_command_uses_attempt_workspace_and_cleans_it_after_exit() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("bridge-workspace-test"),
        )
        .expect("paths");

        let result = run_task_command(
            &paths,
            "printf '%s|%s|%s|%s' \"$MYOPENPANELS_TASK_ATTEMPT_ID\" \"$MYOPENPANELS_EXECUTION_GENERATION\" \"$PWD\" \"${MYOPENPANELS_STORAGE_DIR-unset}\"; touch \"$MYOPENPANELS_EXECUTION_WORKSPACE/output.md\"",
            1_000,
            &json!({
                "id": "task:workspace",
                "queue": "wiki",
                "capability": "wiki.ingestMarkdown",
            }),
            "target:test",
            "lease:test",
            false,
            Some("attempt:test"),
            Some(7),
            None,
            None,
        )
        .expect("command");

        let stdout = result["stdout"].as_str().expect("stdout");
        assert!(stdout.starts_with("attempt:test|7|"));
        assert!(stdout.ends_with("|unset"));
        assert!(stdout.contains(
            &storage
                .join("executions/task:workspace/7-attempt:test")
                .display()
                .to_string()
        ));
        assert!(!storage.join("executions").join("task-workspace").exists());
    }
}
