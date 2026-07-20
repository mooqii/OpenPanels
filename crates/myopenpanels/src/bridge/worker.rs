use crate::error::CliError;
use crate::paths::{sanitize_path_part, MyOpenPanelsPaths};
use crate::tasks;
use crate::trace::{self, TraceEventInput};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_OUTPUT_LIMIT_BYTES: usize = 64 * 1024;
const DEFAULT_WORKER_INTERVAL_MS: u64 = 2000;
const DEFAULT_LOCAL_AGENT_TIMEOUT_MS: u64 = 30 * 60 * 1000;
const MAX_AGENT_PROMPT_BYTES: usize = 256 * 1024;
const EXECUTION_RESULT_FILE: &str = "execution-result.json";

#[derive(Debug)]
struct WikiPromptSection {
    title: &'static str,
    inline_body: String,
    file_body: String,
    inline: bool,
}

#[derive(Debug)]
struct MaterializedSkillFile {
    relative_path: String,
    text: String,
}

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
            "Task bridge requires --command <command>.",
        )
        .with_param("--command")
    })?;
    let default_name = format!("command-bridge:{}", std::process::id());
    let registration = tasks::register_target(
        paths,
        tasks::TargetRegistration {
            name: options.name.unwrap_or(&default_name),
            host: options.host.or(Some("command-bridge")),
            project_id: None,
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
        let payload = tasks::claim_for_worker(paths, &target_id, None, options.queue)?;
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
            None,
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

#[derive(Clone)]
pub struct BuiltinWorkerHandle {
    shutdown: Arc<AtomicBool>,
    thread: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
}

impl BuiltinWorkerHandle {
    pub fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
    }

    pub fn shutdown_and_join(&self) {
        self.request_shutdown();
        if let Some(thread) = self.thread.lock().ok().and_then(|mut thread| thread.take()) {
            let _ = thread.join();
        }
    }
}

pub fn start_builtin_worker_loop(paths: MyOpenPanelsPaths) -> BuiltinWorkerHandle {
    let shutdown = Arc::new(AtomicBool::new(false));
    let handle = BuiltinWorkerHandle {
        shutdown: shutdown.clone(),
        thread: Arc::new(Mutex::new(None)),
    };
    if cfg!(test)
        || std::env::var("NODE_ENV").ok().as_deref() == Some("test")
        || std::env::var("MYOPENPANELS_DISABLE_LOCAL_AGENT")
            .ok()
            .as_deref()
            == Some("1")
    {
        return handle;
    }
    let worker = thread::spawn(move || run_model_gateway_loop(paths, shutdown));
    if let Ok(mut thread) = handle.thread.lock() {
        *thread = Some(worker);
    }
    handle
}

fn run_model_gateway_loop(paths: MyOpenPanelsPaths, shutdown: Arc<AtomicBool>) {
    let mut active: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut executions: Vec<thread::JoinHandle<()>> = Vec::new();
    while !shutdown.load(Ordering::Acquire) {
        let mut running_executions = Vec::with_capacity(executions.len());
        for execution in executions.drain(..) {
            if execution.is_finished() {
                let _ = execution.join();
            } else {
                running_executions.push(execution);
            }
        }
        executions = running_executions;
        let max_concurrency = match crate::model_gateway::read_settings(&paths) {
            Ok(settings) => settings.max_concurrency as usize,
            Err(error) => {
                record_gateway_error(&paths, error.message());
                wait_for_worker_interval(&shutdown);
                continue;
            }
        };
        let specs = match crate::model_gateway::worker_specs(&paths) {
            Ok(specs) => specs,
            Err(error) => {
                record_gateway_error(&paths, error.message());
                wait_for_worker_interval(&shutdown);
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
            wait_for_worker_interval(&shutdown);
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
            let desired_key = format!("{}:{project_id}:{max_concurrency}", spec.key);
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
                    project_id: None,
                    capabilities: vec!["*".to_owned()],
                    priority: 1000 - position as i64,
                    protocol_version: 3,
                    max_concurrency: max_concurrency as i64,
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
        let mut candidates = Vec::new();
        let mut offline_connections = Vec::new();
        for spec in &specs {
            let Some((_, target_id)) = active.get(&spec.connection_id) else {
                continue;
            };
            if tasks::heartbeat_target(&paths, target_id).is_err() {
                offline_connections.push(spec.connection_id.clone());
            } else {
                candidates.push((spec.clone(), target_id.clone()));
            }
        }
        for connection_id in offline_connections {
            active.remove(&connection_id);
        }
        if executions.len() >= max_concurrency {
            wait_for_worker_interval(&shutdown);
            continue;
        }
        let mut claimed = None;
        for (spec, target_id) in candidates {
            match tasks::claim_for_worker(&paths, &target_id, None, None) {
                Ok(payload) if payload.get("task").is_some_and(|task| !task.is_null()) => {
                    claimed = Some((spec, target_id, payload));
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
            wait_for_worker_interval(&shutdown);
            continue;
        };
        let Some(task) = payload.get("task").filter(|value| !value.is_null()) else {
            continue;
        };
        if payload.get("leaseToken").and_then(Value::as_str).is_none() {
            record_gateway_error(&paths, "Claimed task lease token is missing.");
            wait_for_worker_interval(&shutdown);
            continue;
        }
        let _ = write_bridge_status(&paths, "running", Some(task), None, None);
        let execution_paths = paths.clone();
        let execution_shutdown = shutdown.clone();
        executions.push(thread::spawn(move || {
            run_claimed_model_gateway_task(
                execution_paths,
                execution_shutdown,
                spec,
                target_id,
                payload,
            );
        }));
        wait_for_worker_interval(&shutdown);
    }
    for execution in executions {
        let _ = execution.join();
    }
}

fn run_claimed_model_gateway_task(
    paths: MyOpenPanelsPaths,
    shutdown: Arc<AtomicBool>,
    spec: crate::model_gateway::GatewayWorkerSpec,
    target_id: String,
    payload: Value,
) {
    let Some(task) = payload.get("task").filter(|value| !value.is_null()) else {
        return;
    };
    let Some(lease_token) = payload.get("leaseToken").and_then(Value::as_str) else {
        record_gateway_error(&paths, "Claimed task lease token is missing.");
        return;
    };
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
        Some(&shutdown),
    ) {
        Ok(result) => result,
        Err(error) => {
            let task_id = task.get("id").and_then(Value::as_str).unwrap_or("");
            if shutdown.load(Ordering::Acquire) {
                let _ = tasks::interrupt_task_for_studio_restart(&paths, task_id, lease_token);
                return;
            }
            let _ = tasks::fail_task_with_class(
                &paths,
                task_id,
                lease_token,
                error.message(),
                None,
                tasks::TaskFailureClass::RetryableChannel,
            );
            record_gateway_error(&paths, error.message());
            return;
        }
    };
    if result.get("interrupted").and_then(Value::as_bool) == Some(true) {
        let task_id = task.get("id").and_then(Value::as_str).unwrap_or("");
        if let Err(error) = tasks::interrupt_task_for_studio_restart(&paths, task_id, lease_token) {
            if error.code() != Some("execution_fenced") {
                record_gateway_error(&paths, error.message());
            }
        }
        let _ = write_bridge_status(&paths, "stopping", None, Some(&result), None);
        return;
    }
    result["targetId"] = json!(target_id);
    result["providerId"] = json!(spec.provider_id);
    result["providerName"] = json!(spec.provider_name);
    result["modelGatewayConnectionId"] = json!(spec.connection_id);
    if let Err(error) = finalize_task_result(
        &paths,
        &mut result,
        task,
        &target_id,
        lease_token,
        spec.agent_prompt,
        &spec.host,
        false,
    ) {
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
}

fn wait_for_worker_interval(shutdown: &AtomicBool) {
    let started = Instant::now();
    let interval = Duration::from_millis(DEFAULT_WORKER_INTERVAL_MS);
    while !shutdown.load(Ordering::Acquire) && started.elapsed() < interval {
        thread::sleep(Duration::from_millis(25));
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
            .get("commandResult")
            .filter(|value| !value.is_null())
            .cloned()
            .or_else(|| {
                result
                    .get("stdout")
                    .and_then(Value::as_str)
                    .and_then(|stdout| serde_json::from_str::<Value>(stdout.trim()).ok())
                    .map(|value| value.get("result").cloned().unwrap_or(value))
            })
            .or_else(|| {
                agent_prompt.then(|| {
                    json!({
                        "executor": host,
                        "targetId": target_id,
                    })
                })
            });
        tasks::complete_task(paths, task_id, lease_token, command_result)
    } else if result.get("errorCode").and_then(Value::as_str) == Some("invalid_output") {
        let message = result
            .get("error")
            .and_then(Value::as_str)
            .or_else(|| result.get("stderr").and_then(Value::as_str))
            .unwrap_or("Agent output did not satisfy the Wiki execution contract.");
        let lifecycle = tasks::fail_task(paths, task_id, lease_token, message, None)?;
        tasks::mark_latest_attempt_invalid_output(paths, task_id, message)?;
        Ok(lifecycle)
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
