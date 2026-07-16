use crate::control::{read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::MyOpenPanelsPaths;
use crate::storage::Storage;
use rand::Rng;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::thread;
use std::time::{Duration, Instant};

pub(crate) mod model;

const BLOCKED_REASON_ATTEMPTS_EXCEEDED: &str = "attemptsExceeded";
const BLOCKED_REASON_LEASED: &str = "leased";
const BLOCKED_REASON_RETRY_LATER: &str = "retryLater";
const DEFAULT_LEASE_MINUTES: i64 = 15;
const DEFAULT_LONG_POLL_MS: u64 = 25_000;
const TARGET_ONLINE_WINDOW_SECONDS: i64 = 90;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TaskFailureClass {
    RetryableChannel,
    RetryableOutput,
    TerminalTask,
}

impl TaskFailureClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::RetryableChannel => "retryable_channel",
            Self::RetryableOutput => "retryable_output",
            Self::TerminalTask => "terminal_task",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "retryable_channel" => Some(Self::RetryableChannel),
            "retryable_output" => Some(Self::RetryableOutput),
            "terminal_task" => Some(Self::TerminalTask),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TargetRegistration<'a> {
    pub name: &'a str,
    pub host: Option<&'a str>,
    pub transport: &'a str,
    pub capabilities: Vec<String>,
    pub priority: i64,
    pub protocol_version: i64,
    pub max_concurrency: i64,
    pub model_gateway_connection_id: Option<&'a str>,
}

#[derive(Debug, Clone)]
struct ReservedTask {
    id: String,
    previous_status: String,
    queue: String,
    required_protocol_version: i64,
}

#[derive(Debug, Clone, Default)]
pub struct TaskListFilter<'a> {
    pub pending: bool,
    pub queue: Option<&'a str>,
    pub status: Option<&'a str>,
}

pub fn list_tasks(
    paths: &MyOpenPanelsPaths,
    filter: TaskListFilter<'_>,
) -> Result<Value, CliError> {
    recover_expired_tasks(paths)?;
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let tasks = annotate_dispatch_state(
        paths,
        &bootstrap.project.id,
        annotate_tasks(filter_tasks(
            bootstrap
                .tasks
                .into_iter()
                .filter(|task| task.get("archivedAt").is_none_or(Value::is_null))
                .collect(),
            &filter,
        )),
    )?;
    let tasks = sort_tasks_for_display(tasks);
    let pending_count = pending_task_count(&tasks);
    let ready_count = ready_task_count(&tasks);
    let blocked_count = blocked_task_count(&tasks);
    let unhandled_count = tasks
        .iter()
        .filter(|task| task.get("dispatchState").and_then(Value::as_str) == Some("noTarget"))
        .count();
    let running_count = tasks.iter().filter(|task| is_active_task(task)).count();
    Ok(json!({
        "tasks": tasks,
        "pendingCount": pending_count,
        "readyCount": ready_count,
        "blockedCount": blocked_count,
        "unhandledCount": unhandled_count,
        "runningCount": running_count,
    }))
}

pub fn next_task(paths: &MyOpenPanelsPaths, filter: TaskListFilter<'_>) -> Result<Value, CliError> {
    recover_expired_tasks(paths)?;
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let tasks = annotate_dispatch_state(
        paths,
        &bootstrap.project.id,
        annotate_tasks(filter_tasks(
            bootstrap
                .tasks
                .into_iter()
                .filter(|task| task.get("archivedAt").is_none_or(Value::is_null))
                .collect(),
            &filter,
        )),
    )?;
    let task = tasks
        .iter()
        .filter(|task| task.get("ready").and_then(Value::as_bool).unwrap_or(false))
        .find(|task| task.get("status").and_then(Value::as_str) == Some("queued"))
        .or_else(|| {
            tasks
                .iter()
                .filter(|task| task.get("ready").and_then(Value::as_bool).unwrap_or(false))
                .find(|task| task.get("status").and_then(Value::as_str) == Some("failed"))
        })
        .cloned();
    Ok(json!({ "task": task }))
}

pub fn inspect_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let project_id = task_project_id(paths, task_id)?;
    recover_expired_tasks_in_session(paths, &project_id)?;
    inspect_task_in_session(paths, &project_id, task_id)
}

fn inspect_task_in_session(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    task_id: &str,
) -> Result<Value, CliError> {
    let tasks = Storage::open(paths)?.list_tasks(project_id)?;
    let task = annotate_dispatch_state(paths, project_id, annotate_tasks(tasks))?
        .into_iter()
        .find(|task| task.get("id").and_then(Value::as_str) == Some(task_id))
        .ok_or_else(|| CliError::new(format!("Project task not found: {task_id}")))?;
    Ok(json!({ "task": task }))
}

fn task_project_id(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<String, CliError> {
    Storage::open(paths)?
        .connection()
        .query_row(
            "SELECT project_id FROM tasks WHERE id = ? LIMIT 1",
            [task_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(to_cli_error)?
        .ok_or_else(|| {
            CliError::with_code(
                "task_not_found",
                format!("Project task not found: {task_id}"),
            )
        })
}

fn is_active_task(task: &Value) -> bool {
    matches!(
        task.get("status").and_then(Value::as_str),
        Some("reserved" | "running" | "claimed" | "converting" | "indexing")
    )
}

pub fn list_targets(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let targets = read_targets(paths, &bootstrap.project.id)?;
    let online_count = targets
        .iter()
        .filter(|target| target.get("status").and_then(Value::as_str) == Some("online"))
        .count();
    Ok(json!({ "targets": targets, "onlineCount": online_count }))
}

pub fn register_target(
    paths: &MyOpenPanelsPaths,
    registration: TargetRegistration<'_>,
) -> Result<Value, CliError> {
    let transport = registration.transport.trim();
    if !matches!(transport, "poll" | "command") {
        return Err(CliError::with_code(
            "invalid_target",
            "Expected target transport to be one of: poll, command.",
        ));
    }
    if !matches!(registration.protocol_version, 1 | 2 | 3) || registration.max_concurrency < 1 {
        return Err(CliError::with_code(
            "invalid_target",
            "Target protocol version must be 1, 2, or 3 and max concurrency must be positive.",
        ));
    }
    let capabilities = normalize_capabilities(registration.capabilities);
    if capabilities.is_empty() {
        return Err(CliError::with_code(
            "invalid_target",
            "Register at least one --capability.",
        ));
    }

    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let mut storage = Storage::open(paths)?;
    if let Some(connection_id) = registration.model_gateway_connection_id {
        let connection_exists = storage
            .connection()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM model_gateway_connections WHERE id = ? AND enabled = 1)",
                [connection_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(to_cli_error)?;
        if !connection_exists {
            return Err(CliError::with_code(
                "model_gateway_connection_not_found",
                format!("Model gateway connection not found: {connection_id}"),
            ));
        }
    }
    let now = crate::control::now_iso();
    let token = random_secret("opt");
    let token_hash = hash_secret(&token);
    let existing_id = storage
        .connection()
        .query_row(
            r#"
            SELECT id FROM agent_targets
            WHERE project_id = ? AND transport = ?
              AND (
                (? IS NOT NULL AND model_gateway_connection_id = ?)
                OR (? IS NULL AND name = ?)
              )
            ORDER BY updated_at DESC LIMIT 1
            "#,
            params![
                bootstrap.project.id,
                transport,
                registration.model_gateway_connection_id,
                registration.model_gateway_connection_id,
                registration.model_gateway_connection_id,
                registration.name,
            ],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(to_cli_error)?;
    let target_id = existing_id.unwrap_or_else(|| random_id("agent-target"));
    let tx = storage
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(to_cli_error)?;
    tx.execute(
        r#"
            INSERT INTO agent_targets (
              id, project_id, name, host, transport, endpoint, capabilities_json,
              priority, status, token_hash, last_error, last_heartbeat_at,
              created_at, updated_at, protocol_version, max_concurrency,
              model_gateway_connection_id
            )
            VALUES (?, ?, ?, ?, ?, NULL, ?, ?, 'online', ?, NULL, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
              name = excluded.name,
              host = excluded.host,
              transport = excluded.transport,
              capabilities_json = excluded.capabilities_json,
              priority = excluded.priority,
              status = 'online',
              token_hash = excluded.token_hash,
              last_error = NULL,
              last_heartbeat_at = excluded.last_heartbeat_at,
              updated_at = excluded.updated_at
              , protocol_version = excluded.protocol_version
              , max_concurrency = excluded.max_concurrency
              , model_gateway_connection_id = excluded.model_gateway_connection_id
            "#,
        params![
            target_id,
            bootstrap.project.id,
            registration.name,
            registration.host.unwrap_or(registration.name),
            transport,
            serde_json::to_string(&capabilities).map_err(to_cli_error)?,
            registration.priority,
            token_hash,
            now,
            now,
            now,
            registration.protocol_version,
            registration.max_concurrency,
            registration.model_gateway_connection_id,
        ],
    )
    .map_err(to_cli_error)?;
    write_target_secret(paths, &target_id, &token)?;
    crate::storage::record_scope(&tx, "agent_targets", Some(&bootstrap.project.id), None)?;
    let target = read_target_value(&tx, &bootstrap.project.id, &target_id)?
        .ok_or_else(|| CliError::new("Registered target could not be read."))?;
    tx.commit().map_err(to_cli_error)?;
    Ok(json!({ "target": target, "token": token }))
}

pub fn heartbeat_target(paths: &MyOpenPanelsPaths, target_id: &str) -> Result<Value, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    heartbeat_target_in_session(paths, &bootstrap.project.id, target_id)
}

pub fn deactivate_target(
    paths: &MyOpenPanelsPaths,
    target_id: &str,
    reason: &str,
) -> Result<Value, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let changed = tx
        .execute(
            "UPDATE agent_targets SET status = 'offline', last_error = ?, updated_at = ? WHERE id = ? AND project_id = ? AND transport IN ('poll', 'command') AND (status <> 'offline' OR last_error IS NOT ?)",
            params![reason, now, target_id, bootstrap.project.id, reason],
        )
        .map_err(to_cli_error)?;
    if changed > 0 {
        crate::storage::record_scope(&tx, "agent_targets", Some(&bootstrap.project.id), None)?;
    }
    let target = read_target_value(&tx, &bootstrap.project.id, target_id)?.ok_or_else(|| {
        CliError::with_code(
            "target_not_found",
            format!("Agent target not found: {target_id}"),
        )
    })?;
    tx.commit().map_err(to_cli_error)?;
    Ok(json!({ "target": target }))
}

fn heartbeat_target_in_session(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    target_id: &str,
) -> Result<Value, CliError> {
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let previous = tx.query_row(
        "SELECT status, last_error, last_heartbeat_at FROM agent_targets WHERE id = ? AND project_id = ? AND transport IN ('poll', 'command')",
        params![target_id, project_id],
        |row| Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
        )),
    ).optional().map_err(to_cli_error)?;
    let changed = tx.execute(
            "UPDATE agent_targets SET status = 'online', last_error = NULL, last_heartbeat_at = ?, updated_at = ? WHERE id = ? AND project_id = ? AND transport IN ('poll', 'command')",
            params![now, now, target_id, project_id],
        )
        .map_err(to_cli_error)?;
    if changed == 0 {
        return Err(CliError::with_code(
            "target_not_found",
            format!("Agent target not found: {target_id}"),
        ));
    }
    let should_notify = previous.is_none_or(|(status, error, heartbeat)| {
        status != "online"
            || error.is_some()
            || chrono::DateTime::parse_from_rfc3339(&heartbeat)
                .ok()
                .is_none_or(|heartbeat| {
                    heartbeat.with_timezone(&chrono::Utc)
                        <= chrono::Utc::now() - chrono::Duration::seconds(30)
                })
    });
    if should_notify {
        crate::storage::record_scope(&tx, "agent_targets", Some(project_id), None)?;
    }
    let target = read_target_value(&tx, project_id, target_id)?
        .ok_or_else(|| CliError::new("Agent target could not be read."))?;
    tx.commit().map_err(to_cli_error)?;
    Ok(json!({ "target": target }))
}

pub fn remove_target(paths: &MyOpenPanelsPaths, target_id: &str) -> Result<Value, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let interrupted = {
        let mut statement = tx.prepare(
            "SELECT id, queue, workflow_id, status FROM tasks WHERE project_id = ? AND assigned_agent_id = ? AND status IN ('reserved', 'running', 'claimed', 'converting', 'indexing')"
        ).map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![bootstrap.project.id, target_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    let now = crate::control::now_iso();
    let retry_after = now.clone();
    for (task_id, _, workflow_id, previous_status) in &interrupted {
        let reason = json!({ "code": "executor_removed", "targetId": target_id });
        tx.execute(
            r#"UPDATE tasks SET status = 'failed', assigned_agent_id = NULL, lease_owner = NULL,
               lease_token_hash = NULL, lease_expires_at = NULL, last_heartbeat_at = NULL,
               retry_after = ?, error_json = ?, execution_generation = execution_generation + 1,
               updated_at = ? WHERE id = ?"#,
            params![retry_after, reason.to_string(), now, task_id],
        )
        .map_err(to_cli_error)?;
        tx.execute("UPDATE task_attempts SET status = 'interrupted', finished_at = ?, error_json = ?, failure_class = 'retryable_channel' WHERE task_id = ? AND status = 'leased'", params![now, reason.to_string(), task_id]).map_err(to_cli_error)?;
        tx.execute("INSERT INTO task_events (task_id, workflow_id, event_type, from_status, to_status, reason_json, agent_target_id, created_at) VALUES (?, ?, 'executor_removed', ?, 'failed', ?, ?, ?)", params![task_id, workflow_id, previous_status, reason.to_string(), target_id, now]).map_err(to_cli_error)?;
    }
    let changed = tx
        .execute(
            "DELETE FROM agent_targets WHERE id = ? AND project_id = ? AND transport IN ('poll', 'command')",
            params![target_id, bootstrap.project.id],
        )
        .map_err(to_cli_error)?;
    if changed == 0 {
        return Err(CliError::with_code(
            "target_not_found",
            format!("Agent target not found: {target_id}"),
        ));
    }
    let _ = fs::remove_file(target_secret_path(paths, target_id));
    crate::storage::record_scope(&tx, "agent_targets", Some(&bootstrap.project.id), None)?;
    tx.commit().map_err(to_cli_error)?;
    for (task_id, queue, _, _) in &interrupted {
        if queue == "wiki" {
            let _ = crate::wiki::fail_task_with_retry(
                paths,
                task_id,
                "Executor removed.",
                Some(&retry_after),
            );
        } else if queue == "writing" {
            let _ = crate::writing::fail_task(paths, task_id, "Executor removed.");
        }
    }
    Ok(json!({ "removed": true, "targetId": target_id }))
}

pub fn verify_target_token(
    paths: &MyOpenPanelsPaths,
    target_id: &str,
    token: &str,
) -> Result<(), CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let storage = Storage::open(paths)?;
    let expected = storage
        .connection()
        .query_row(
            "SELECT token_hash FROM agent_targets WHERE id = ? AND project_id = ? AND transport IN ('poll', 'command')",
            params![target_id, bootstrap.project.id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(to_cli_error)?
        .ok_or_else(|| {
            CliError::with_code(
                "target_not_found",
                format!("Agent target not found: {target_id}"),
            )
        })?;
    if expected != hash_secret(token) {
        return Err(CliError::with_code(
            "unauthorized_target",
            "Agent target token is invalid.",
        ));
    }
    Ok(())
}

pub fn claim_next(
    paths: &MyOpenPanelsPaths,
    target_id: &str,
    capability: Option<&str>,
    wait_ms: Option<u64>,
) -> Result<Value, CliError> {
    claim_next_filtered(paths, target_id, capability, None, wait_ms)
}

pub fn claim_next_filtered(
    paths: &MyOpenPanelsPaths,
    target_id: &str,
    capability: Option<&str>,
    queue: Option<&str>,
    wait_ms: Option<u64>,
) -> Result<Value, CliError> {
    let wait_ms = wait_ms
        .unwrap_or(DEFAULT_LONG_POLL_MS)
        .min(DEFAULT_LONG_POLL_MS);
    let started = Instant::now();
    let project_id = read_project_bootstrap(paths, BootstrapRequest::new())?
        .project
        .id;
    loop {
        match claim_once(paths, &project_id, target_id, None, capability, queue) {
            Ok(Some(payload)) => return Ok(payload),
            Ok(None) => {}
            Err(error) if is_database_locked(&error) => {}
            Err(error) => return Err(error),
        }
        if started.elapsed() >= Duration::from_millis(wait_ms) {
            return Ok(json!({ "task": Value::Null, "leaseToken": Value::Null }));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn is_database_locked(error: &CliError) -> bool {
    error
        .message()
        .to_ascii_lowercase()
        .contains("database is locked")
}

pub fn claim_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    target_id: &str,
) -> Result<Value, CliError> {
    let project_id = task_project_id(paths, task_id)?;
    claim_once(paths, &project_id, target_id, Some(task_id), None, None)?.ok_or_else(|| {
        CliError::with_code(
            "task_not_claimable",
            format!("Project task is not claimable: {task_id}"),
        )
    })
}

fn claim_once(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    target_id: &str,
    task_id: Option<&str>,
    requested_capability: Option<&str>,
    requested_queue: Option<&str>,
) -> Result<Option<Value>, CliError> {
    recover_expired_tasks_in_session(paths, project_id)?;
    heartbeat_target_in_session(paths, project_id, target_id)?;
    let mut storage = Storage::open(paths)?;
    let target =
        read_target_value(storage.connection(), project_id, target_id)?.ok_or_else(|| {
            CliError::with_code(
                "target_not_found",
                format!("Agent target not found: {target_id}"),
            )
        })?;
    let reserved = reserve_task(
        &mut storage,
        project_id,
        &target,
        task_id,
        requested_capability,
        requested_queue,
    )?;
    let Some(reserved) = reserved else {
        return Ok(None);
    };

    let claim_result = match reserved.queue.as_str() {
        "wiki" => crate::wiki::claim_task(paths, &reserved.id),
        "writing" => crate::writing::claim_task(paths, &reserved.id),
        queue => Err(CliError::with_code(
            "queue_adapter_missing",
            format!("No task lifecycle adapter is available for queue: {queue}"),
        )),
    };
    let claimed = match claim_result {
        Ok(claimed) => claimed,
        Err(error) => {
            release_reservation(paths, project_id, &reserved)?;
            return Err(error);
        }
    };
    let claimed_status = claimed["task"]["status"].as_str().unwrap_or("running");
    let claimed_attempt = claimed["task"]["attempt"].as_i64().unwrap_or(1);

    let lease_token = random_secret("lease");
    let lease_expires_at = lease_expires_at();
    let attempt_id = random_id("task-attempt");
    let broker_url = std::env::var("MYOPENPANELS_TASK_BROKER_URL").ok();
    if reserved.required_protocol_version >= crate::content::EXECUTION_PROTOCOL_VERSION
        && broker_url.as_deref().is_none_or(str::is_empty)
        && !cfg!(test)
    {
        release_reservation(paths, project_id, &reserved)?;
        return Err(CliError::with_code(
            "broker_unavailable",
            "Execution protocol v3 requires a running Studio Task Broker.",
        ));
    }
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let execution_generation = tx.query_row(
            r#"
            UPDATE tasks
            SET status = ?, attempts = ?, assigned_agent_id = ?, lease_owner = ?, lease_token_hash = ?,
                lease_expires_at = ?, last_heartbeat_at = ?, retry_after = NULL,
                error_json = NULL, updated_at = ?, execution_generation = execution_generation + 1
            WHERE id = ? AND project_id = ?
            RETURNING execution_generation
            "#,
            params![
                claimed_status,
                claimed_attempt,
                target_id,
                target_id,
                hash_secret(&lease_token),
                lease_expires_at,
                now,
                now,
                reserved.id,
                project_id,
            ],
            |row| row.get::<_, i64>(0),
        )
        .map_err(to_cli_error)?;
    let model_gateway_connection_id = target
        .get("modelGatewayConnectionId")
        .and_then(Value::as_str);
    let executor_snapshot = json!({
        "targetId": target_id,
        "targetName": target.get("name"),
        "host": target.get("host"),
        "transport": target.get("transport"),
        "modelGatewayConnectionId": model_gateway_connection_id,
    });
    let workflow_id = tx
        .query_row(
            "SELECT workflow_id FROM tasks WHERE id = ?",
            [&reserved.id],
            |row| row.get::<_, String>(0),
        )
        .map_err(to_cli_error)?;
    tx.execute(
        r#"
        INSERT INTO task_attempts (
          id, task_id, attempt_number, execution_generation, agent_target_id,
          status, started_at, heartbeat_at, model_gateway_connection_id,
          executor_snapshot_json
        ) VALUES (?, ?, ?, ?, ?, 'leased', ?, ?, ?, ?)
        "#,
        params![
            attempt_id,
            reserved.id,
            claimed_attempt,
            execution_generation,
            target_id,
            now,
            now,
            model_gateway_connection_id,
            executor_snapshot.to_string(),
        ],
    )
    .map_err(to_cli_error)?;
    let execution = if reserved.required_protocol_version
        >= crate::content::EXECUTION_PROTOCOL_VERSION
        && broker_url.as_deref().is_some_and(|value| !value.is_empty())
    {
        Some(crate::content::create_execution_context_in_transaction(
            &tx,
            &reserved.id,
            &attempt_id,
            execution_generation,
            &lease_expires_at,
        )?)
    } else {
        None
    };
    tx.execute(
        "INSERT INTO task_events (task_id, workflow_id, event_type, from_status, to_status, attempt_id, agent_target_id, created_at) VALUES (?, ?, 'claimed', ?, ?, ?, ?, ?)",
        params![reserved.id, workflow_id, reserved.previous_status, claimed_status, attempt_id, target_id, now],
    )
    .map_err(to_cli_error)?;
    crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    let mut payload = inspect_task(paths, &reserved.id)?;
    payload["leaseToken"] = json!(lease_token);
    payload["target"] = target;
    payload["attemptId"] = json!(attempt_id);
    payload["executionGeneration"] = json!(execution_generation);
    payload["executionProtocolVersion"] = json!(reserved.required_protocol_version);
    payload["taskBrokerUrl"] = broker_url.map(Value::from).unwrap_or(Value::Null);
    payload["executionToken"] = execution
        .as_ref()
        .map(|value| Value::from(value.0.clone()))
        .unwrap_or(Value::Null);
    payload["executionTokenExpiresAt"] = execution
        .as_ref()
        .map(|_| Value::from(lease_expires_at))
        .unwrap_or(Value::Null);
    payload["inputManifest"] = json!(read_task_inputs(paths, &reserved.id)?);
    Ok(Some(payload))
}

pub fn heartbeat_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
) -> Result<Value, CliError> {
    let lease = verify_lease(paths, task_id, lease_token)?;
    let expires_at = lease_expires_at();
    match lease["queue"].as_str().unwrap_or("") {
        "wiki" => crate::wiki::heartbeat_task(paths, task_id, &expires_at)?,
        "writing" => crate::writing::heartbeat_task(paths, task_id)?,
        queue => {
            return Err(CliError::with_code(
                "queue_adapter_missing",
                format!("No task lifecycle adapter is available for queue: {queue}"),
            ))
        }
    };
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    tx.execute(
            "UPDATE tasks SET lease_expires_at = ?, last_heartbeat_at = ?, updated_at = ? WHERE id = ? AND lease_token_hash = ?",
            params![expires_at, now, now, task_id, hash_secret(lease_token)],
        )
        .map_err(to_cli_error)?;
    tx.execute(
        "UPDATE task_attempts SET heartbeat_at = ? WHERE task_id = ? AND status = 'leased'",
        params![now, task_id],
    )
    .map_err(to_cli_error)?;
    tx.execute(
        "UPDATE task_attempts SET execution_token_expires_at = ? WHERE task_id = ? AND status = 'leased' AND execution_token_hash IS NOT NULL",
        params![expires_at, task_id],
    )
    .map_err(to_cli_error)?;
    tx.execute(
        "UPDATE task_staging_sessions SET expires_at = ?, updated_at = ? WHERE task_id = ? AND status IN ('open', 'prepared')",
        params![expires_at, now, task_id],
    )
    .map_err(to_cli_error)?;
    let project_id = lease["projectId"].as_str().unwrap_or_default();
    crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    inspect_task_in_session(paths, project_id, task_id)
}

pub fn complete_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
    result: Option<Value>,
) -> Result<Value, CliError> {
    let lease = verify_lease(paths, task_id, lease_token)?;
    let prepared_panel_state: Option<(String, Value)> = match lease["queue"].as_str().unwrap_or("")
    {
        "wiki" => Some((
            lease["panelId"].as_str().unwrap_or_default().to_owned(),
            crate::wiki::prepare_task_completion(paths, task_id, result.clone())?["state"].clone(),
        )),
        "writing" => crate::writing::prepare_task_completion(paths, task_id)?,
        queue => {
            return Err(CliError::with_code(
                "queue_adapter_missing",
                format!("No task lifecycle adapter is available for queue: {queue}"),
            ))
        }
    };
    let project_id = lease["projectId"].as_str().unwrap_or_default();
    if let Err(error) = finalize_task_runtime(
        paths,
        project_id,
        task_id,
        "succeeded",
        result,
        None,
        None,
        None,
        prepared_panel_state
            .as_ref()
            .map(|(panel_id, state)| (panel_id.as_str(), state)),
        lease["executionGeneration"].as_i64(),
    ) {
        if error.code() == Some("content_conflict") {
            let _ = supersede_task_for_content_conflict(paths, task_id, "content-resource");
        }
        return Err(error);
    }
    inspect_task_in_session(paths, project_id, task_id)
}

pub fn fail_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
    message: &str,
    retry_after: Option<&str>,
) -> Result<Value, CliError> {
    fail_task_with_class(
        paths,
        task_id,
        lease_token,
        message,
        retry_after,
        TaskFailureClass::RetryableChannel,
    )
}

pub fn fail_task_with_class(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
    message: &str,
    retry_after: Option<&str>,
    failure_class: TaskFailureClass,
) -> Result<Value, CliError> {
    let lease = verify_lease(paths, task_id, lease_token)?;
    if retry_after.is_some_and(|value| chrono::DateTime::parse_from_rfc3339(value).is_err()) {
        return Err(CliError::with_code(
            "invalid_retry_after",
            "Expected --retry-after to be an RFC 3339 timestamp.",
        ));
    }
    let has_fallback = failure_class != TaskFailureClass::TerminalTask
        && has_untried_eligible_target(
            paths,
            lease["projectId"].as_str().unwrap_or_default(),
            task_id,
            lease["targetId"].as_str().unwrap_or_default(),
        )?;
    let retry_after = if failure_class == TaskFailureClass::TerminalTask {
        None
    } else if has_fallback {
        Some(crate::control::now_iso())
    } else if let Some(retry_after) = retry_after {
        Some(retry_after.to_owned())
    } else {
        Some(execution_retry_after(
            lease["attempt"].as_i64().unwrap_or(1),
        ))
    };
    match lease["queue"].as_str().unwrap_or("") {
        "wiki" => {
            crate::wiki::fail_task_with_retry(paths, task_id, message, retry_after.as_deref())?
        }
        "writing" => crate::writing::fail_task(paths, task_id, message)?,
        queue => {
            return Err(CliError::with_code(
                "queue_adapter_missing",
                format!("No task lifecycle adapter is available for queue: {queue}"),
            ))
        }
    };
    finalize_task_runtime(
        paths,
        lease["projectId"].as_str().unwrap_or_default(),
        task_id,
        "failed",
        None,
        Some(json!(message)),
        retry_after.as_deref(),
        Some(failure_class),
        None,
        lease["executionGeneration"].as_i64(),
    )?;
    inspect_task_in_session(
        paths,
        lease["projectId"].as_str().unwrap_or_default(),
        task_id,
    )
}

pub(crate) fn mark_latest_attempt_invalid_output(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    message: &str,
) -> Result<(), CliError> {
    let project_id = task_project_id(paths, task_id)?;
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let reason = json!({ "code": "invalid_output", "message": message });
    tx.execute(
        "UPDATE task_attempts SET status = 'invalid_output', error_json = ?, failure_class = 'retryable_output' WHERE id = (SELECT id FROM task_attempts WHERE task_id = ? ORDER BY execution_generation DESC LIMIT 1) AND status IN ('failed_retryable', 'failed_terminal')",
        params![reason.to_string(), task_id],
    ).map_err(to_cli_error)?;
    let workflow_id = tx
        .query_row(
            "SELECT workflow_id FROM tasks WHERE id = ?",
            [task_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(to_cli_error)?;
    tx.execute(
        "INSERT INTO task_events (task_id, workflow_id, event_type, from_status, to_status, reason_json, created_at) VALUES (?, ?, 'invalid_output', 'leased', 'failed', ?, ?)",
        params![task_id, workflow_id, reason.to_string(), now],
    ).map_err(to_cli_error)?;
    crate::storage::record_scope(&tx, "tasks", Some(&project_id), None)?;
    tx.commit().map_err(to_cli_error)
}

pub fn release_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
) -> Result<Value, CliError> {
    let lease = verify_lease(paths, task_id, lease_token)?;
    match lease["queue"].as_str().unwrap_or("") {
        "wiki" => crate::wiki::release_task(paths, task_id)?,
        "writing" => crate::writing::release_task(paths, task_id)?,
        queue => {
            return Err(CliError::with_code(
                "queue_adapter_missing",
                format!("No task lifecycle adapter is available for queue: {queue}"),
            ))
        }
    };
    finalize_task_runtime(
        paths,
        lease["projectId"].as_str().unwrap_or_default(),
        task_id,
        "queued",
        None,
        None,
        None,
        None,
        None,
        lease["executionGeneration"].as_i64(),
    )?;
    inspect_task_in_session(
        paths,
        lease["projectId"].as_str().unwrap_or_default(),
        task_id,
    )
}

pub fn retry_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let task = inspect_task(paths, task_id)?;
    if !matches!(task["task"]["status"].as_str(), Some("failed" | "queued")) {
        return Err(CliError::with_code(
            "invalid_task_transition",
            "Only queued or failed tasks can be retried. Succeeded and cancelled tasks require a new Workflow.",
        ));
    }
    if task["task"]["attempt"].as_i64().unwrap_or(0)
        >= task["task"]["maxAttempts"].as_i64().unwrap_or(8)
    {
        return Err(CliError::with_code(
            "invalid_task_transition",
            "The Task exhausted its Attempts. Start a new Workflow instead of retrying it in place.",
        ));
    }
    let project_id = task["task"]["projectId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    match task["task"]["queue"].as_str().unwrap_or("") {
        "wiki" => crate::wiki::retry_task(paths, task_id)?,
        "writing" => crate::writing::retry_task(paths, task_id)?,
        queue => {
            return Err(CliError::with_code(
                "queue_adapter_missing",
                format!("No task lifecycle adapter is available for queue: {queue}"),
            ))
        }
    };
    finalize_task_runtime(
        paths,
        &project_id,
        task_id,
        "queued",
        None,
        None,
        None,
        None,
        None,
        None,
    )?;
    inspect_task_in_session(paths, &project_id, task_id)
}

pub fn set_task_dispatch(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    mode: &str,
    requested_connection_id: Option<&str>,
) -> Result<Value, CliError> {
    if !matches!(mode, "auto" | "prefer" | "only") {
        return Err(CliError::with_code(
            "invalid_dispatch_mode",
            "Dispatch mode must be auto, prefer, or only.",
        ));
    }
    let requested_connection_id = requested_connection_id.filter(|value| !value.trim().is_empty());
    if mode == "auto" && requested_connection_id.is_some() {
        return Err(CliError::with_code(
            "invalid_dispatch_mode",
            "Automatic dispatch cannot pin a model gateway connection.",
        ));
    }
    if matches!(mode, "prefer" | "only") && requested_connection_id.is_none() {
        return Err(CliError::with_code(
            "invalid_dispatch_mode",
            "Preferred and exclusive dispatch require a model gateway connection.",
        ));
    }
    let project_id = task_project_id(paths, task_id)?;
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    if let Some(connection_id) = requested_connection_id {
        let exists = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM model_gateway_connections WHERE id = ? AND enabled = 1)",
                [connection_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(to_cli_error)?;
        if !exists {
            return Err(CliError::with_code(
                "model_gateway_connection_not_found",
                format!("Model gateway connection not found: {connection_id}"),
            ));
        }
    }
    let (status, workflow_id) = tx
        .query_row(
            "SELECT status, workflow_id FROM tasks WHERE id = ? AND project_id = ?",
            params![task_id, project_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(to_cli_error)?;
    if !matches!(status.as_str(), "waiting" | "queued" | "failed") {
        return Err(CliError::with_code(
            "invalid_task_transition",
            "Task dispatch can only change while the Task is waiting or ready.",
        ));
    }
    let now = crate::control::now_iso();
    tx.execute(
        "UPDATE tasks SET dispatch_mode = ?, requested_gateway_connection_id = ?, updated_at = ? WHERE id = ? AND project_id = ?",
        params![mode, requested_connection_id, now, task_id, project_id],
    )
    .map_err(to_cli_error)?;
    let reason = json!({
        "dispatchMode": mode,
        "requestedModelGatewayConnectionId": requested_connection_id,
    });
    tx.execute(
        "INSERT INTO task_events (task_id, workflow_id, event_type, from_status, to_status, reason_json, created_at) VALUES (?, ?, 'dispatch_updated', ?, ?, ?, ?)",
        params![task_id, workflow_id, status, status, reason.to_string(), now],
    )
    .map_err(to_cli_error)?;
    crate::storage::record_scope(&tx, "tasks", Some(&project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    inspect_task_in_session(paths, &project_id, task_id)
}

pub fn cancel_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let task = inspect_task(paths, task_id)?;
    if matches!(
        task["task"]["status"].as_str(),
        Some("succeeded" | "cancelled" | "stale" | "superseded")
    ) {
        return Err(CliError::with_code(
            "invalid_task_transition",
            "Terminal tasks cannot be cancelled.",
        ));
    }
    let project_id = task["task"]["projectId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    match task["task"]["queue"].as_str().unwrap_or("") {
        "wiki" => crate::wiki::cancel_task(paths, task_id)?,
        "writing" => crate::writing::cancel_task(paths, task_id)?,
        queue => {
            return Err(CliError::with_code(
                "queue_adapter_missing",
                format!("No task lifecycle adapter is available for queue: {queue}"),
            ))
        }
    };
    finalize_task_runtime(
        paths,
        &project_id,
        task_id,
        "cancelled",
        None,
        Some(json!({ "code": "user_cancelled" })),
        None,
        None,
        None,
        None,
    )?;
    inspect_task_in_session(paths, &project_id, task_id)
}

pub fn list_task_events(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let project_id = task_project_id(paths, task_id)?;
    let storage = Storage::open(paths)?;
    let mut statement = storage
        .connection()
        .prepare(
            r#"
            SELECT id, task_id, workflow_id, event_type, from_status, to_status,
                   reason_json, attempt_id, agent_target_id, created_at
            FROM task_events WHERE task_id = ? ORDER BY id ASC
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([task_id], task_event_from_row)
        .map_err(to_cli_error)?;
    let events = rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?;
    Ok(json!({ "projectId": project_id, "events": events }))
}

pub fn list_task_attempts(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let project_id = task_project_id(paths, task_id)?;
    let storage = Storage::open(paths)?;
    let mut statement = storage
        .connection()
        .prepare(
            r#"
            SELECT a.id, a.task_id, a.attempt_number, a.execution_generation, a.agent_target_id,
                   a.status, a.started_at, a.heartbeat_at, a.finished_at, a.result_json, a.error_json,
                   ss.status, ss.total_bytes, ss.updated_at,
                   a.model_gateway_connection_id, a.executor_snapshot_json, a.failure_class
            FROM task_attempts a
            LEFT JOIN task_staging_sessions ss ON ss.id = a.staging_session_id
            WHERE a.task_id = ? ORDER BY a.attempt_number ASC, a.started_at ASC
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([task_id], task_attempt_from_row)
        .map_err(to_cli_error)?;
    let attempts = rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?;
    Ok(json!({ "projectId": project_id, "attempts": attempts }))
}

pub fn list_workflows(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let project_id = read_project_bootstrap(paths, BootstrapRequest::new())?
        .project
        .id;
    let storage = Storage::open(paths)?;
    let mut statement = storage
        .connection()
        .prepare(
            r#"
            SELECT w.id, w.type, w.status, w.source_workflow_id, w.source_json,
                   w.created_at, w.updated_at, w.archived_at,
                   COUNT(t.id),
                   SUM(CASE WHEN t.status = 'succeeded' THEN 1 ELSE 0 END)
            FROM workflows w
            LEFT JOIN tasks t ON t.workflow_id = w.id
            WHERE w.project_id = ? AND w.archived_at IS NULL
            GROUP BY w.id
            ORDER BY w.updated_at DESC, w.id
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([&project_id], workflow_summary_from_row)
        .map_err(to_cli_error)?;
    let workflows = rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?;
    Ok(json!({ "projectId": project_id, "workflows": workflows }))
}

pub fn read_workflow(paths: &MyOpenPanelsPaths, workflow_id: &str) -> Result<Value, CliError> {
    let project_id = read_project_bootstrap(paths, BootstrapRequest::new())?
        .project
        .id;
    let storage = Storage::open(paths)?;
    let workflow = storage
        .connection()
        .query_row(
            r#"
            SELECT w.id, w.type, w.status, w.source_workflow_id, w.source_json,
                   w.created_at, w.updated_at, w.archived_at,
                   COUNT(t.id), SUM(CASE WHEN t.status = 'succeeded' THEN 1 ELSE 0 END)
            FROM workflows w LEFT JOIN tasks t ON t.workflow_id = w.id
            WHERE w.project_id = ? AND w.id = ? GROUP BY w.id
            "#,
            params![project_id, workflow_id],
            workflow_summary_from_row,
        )
        .optional()
        .map_err(to_cli_error)?
        .ok_or_else(|| {
            CliError::with_code(
                "workflow_not_found",
                format!("Workflow not found: {workflow_id}"),
            )
        })?;
    let tasks = storage
        .list_tasks(&project_id)?
        .into_iter()
        .filter(|task| task.get("workflowId").and_then(Value::as_str) == Some(workflow_id))
        .collect::<Vec<_>>();
    let mut dependencies = storage.connection().prepare(
        "SELECT task_id, prerequisite_task_id, success_condition, failure_policy, created_at FROM task_dependencies WHERE task_id IN (SELECT id FROM tasks WHERE workflow_id = ?) ORDER BY task_id, prerequisite_task_id"
    ).map_err(to_cli_error)?;
    let dependencies = dependencies
        .query_map([workflow_id], |row| {
            Ok(json!({
                "taskId": row.get::<_, String>(0)?,
                "prerequisiteTaskId": row.get::<_, String>(1)?,
                "successCondition": row.get::<_, String>(2)?,
                "failurePolicy": row.get::<_, String>(3)?,
                "createdAt": row.get::<_, String>(4)?,
            }))
        })
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    Ok(json!({ "workflow": workflow, "tasks": tasks, "dependencies": dependencies }))
}

pub fn archive_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let task = inspect_task(paths, task_id)?;
    let status = task["task"]["status"].as_str().unwrap_or_default();
    if !matches!(status, "succeeded" | "cancelled" | "stale" | "superseded")
        && !(status == "failed"
            && task["task"]["attempt"].as_i64().unwrap_or(0)
                >= task["task"]["maxAttempts"].as_i64().unwrap_or(8))
    {
        return Err(CliError::with_code(
            "invalid_task_transition",
            "Only terminal tasks can be archived.",
        ));
    }
    let project_id = task["task"]["projectId"].as_str().unwrap_or_default();
    let workflow_id = task["task"]["workflowId"].as_str().unwrap_or_default();
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    tx.execute(
        "UPDATE tasks SET archived_at = ?, updated_at = ? WHERE id = ? AND archived_at IS NULL",
        params![now, now, task_id],
    )
    .map_err(to_cli_error)?;
    tx.execute("INSERT INTO task_events (task_id, workflow_id, event_type, from_status, to_status, created_at) VALUES (?, ?, 'archived', ?, 'archived', ?)", params![task_id, workflow_id, status, now]).map_err(to_cli_error)?;
    tx.execute(
        r#"UPDATE workflows SET status = 'archived', archived_at = ?, updated_at = ?
           WHERE id = ? AND NOT EXISTS (
             SELECT 1 FROM tasks WHERE workflow_id = ? AND archived_at IS NULL
           )"#,
        params![now, now, workflow_id, workflow_id],
    )
    .map_err(to_cli_error)?;
    tx.execute(
        r#"DELETE FROM content_pins
           WHERE task_id IN (SELECT id FROM tasks WHERE workflow_id = ?)
             AND NOT EXISTS (SELECT 1 FROM tasks WHERE workflow_id = ? AND archived_at IS NULL)"#,
        params![workflow_id, workflow_id],
    )
    .map_err(to_cli_error)?;
    crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    inspect_task_in_session(paths, project_id, task_id)
}

pub fn list_agent_routes(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let project_id = read_project_bootstrap(paths, BootstrapRequest::new())?
        .project
        .id;
    let storage = Storage::open(paths)?;
    let mut statement = storage.connection().prepare(
        r#"SELECT r.capability, r.agent_target_id, r.position, t.name, t.status, t.protocol_version, t.max_concurrency
           FROM agent_routes r JOIN agent_targets t ON t.id = r.agent_target_id
           WHERE r.project_id = ? AND t.transport IN ('poll', 'command')
           ORDER BY r.capability, r.position"#,
    ).map_err(to_cli_error)?;
    let routes = statement
        .query_map([&project_id], |row| {
            Ok(json!({
                "capability": row.get::<_, String>(0)?,
                "targetId": row.get::<_, String>(1)?,
                "position": row.get::<_, i64>(2)?,
                "targetName": row.get::<_, String>(3)?,
                "targetStatus": row.get::<_, String>(4)?,
                "protocolVersion": row.get::<_, i64>(5)?,
                "maxConcurrency": row.get::<_, i64>(6)?,
            }))
        })
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    Ok(json!({ "projectId": project_id, "routes": routes }))
}

pub fn set_agent_route(
    paths: &MyOpenPanelsPaths,
    capability: &str,
    target_ids: &[String],
) -> Result<Value, CliError> {
    if capability.trim().is_empty() || target_ids.is_empty() {
        return Err(CliError::with_code(
            "invalid_route",
            "A capability and at least one target are required.",
        ));
    }
    let project_id = read_project_bootstrap(paths, BootstrapRequest::new())?
        .project
        .id;
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    tx.execute(
        "DELETE FROM agent_routes WHERE project_id = ? AND capability = ?",
        params![project_id, capability],
    )
    .map_err(to_cli_error)?;
    for (position, target_id) in target_ids.iter().enumerate() {
        let exists = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM agent_targets WHERE project_id = ? AND id = ? AND transport IN ('poll', 'command'))",
                params![project_id, target_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(to_cli_error)?;
        if !exists {
            return Err(CliError::with_code(
                "target_not_found",
                format!("Agent target not found: {target_id}"),
            ));
        }
        tx.execute("INSERT INTO agent_routes (project_id, capability, agent_target_id, position, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)", params![project_id, capability, target_id, position as i64, now, now]).map_err(to_cli_error)?;
    }
    crate::storage::record_scope(&tx, "agent_targets", Some(&project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    list_agent_routes(paths)
}

pub fn remove_agent_route(paths: &MyOpenPanelsPaths, capability: &str) -> Result<Value, CliError> {
    let project_id = read_project_bootstrap(paths, BootstrapRequest::new())?
        .project
        .id;
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    tx.execute(
        "DELETE FROM agent_routes WHERE project_id = ? AND capability = ?",
        params![project_id, capability],
    )
    .map_err(to_cli_error)?;
    crate::storage::record_scope(&tx, "agent_targets", Some(&project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    list_agent_routes(paths)
}

fn read_task_inputs(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Vec<Value>, CliError> {
    let storage = Storage::open(paths)?;
    let mut statement = storage
        .connection()
        .prepare(
            r#"
        SELECT ti.id, ti.resource_kind, ti.resource_id, ti.resource_version,
               ti.content_hash, ti.snapshot_ref, ti.missing_policy, ti.changed_policy,
               ti.created_at, cr.active_revision_id, cr.content_version, rev.manifest_hash
        FROM task_inputs ti JOIN tasks t ON t.id = ti.task_id
        LEFT JOIN content_resources cr ON cr.project_id = t.project_id
          AND cr.resource_key = ti.resource_id AND cr.archived_at IS NULL
          AND cr.resource_kind = CASE ti.resource_kind
            WHEN 'wiki.rawDocument' THEN 'wiki_markdown'
            WHEN 'wiki.generatedDocument' THEN 'generated_document'
            WHEN 'writing.targetDocument' THEN 'generated_document'
            WHEN 'writing.skill' THEN 'writing_skill'
            ELSE '' END
        LEFT JOIN content_revisions rev ON rev.id = cr.active_revision_id
        WHERE ti.task_id = ? ORDER BY ti.id
        "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([task_id], |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "resourceKind": row.get::<_, String>(1)?,
                "resourceId": row.get::<_, String>(2)?,
                "resourceVersion": row.get::<_, Option<String>>(3)?,
                "contentHash": row.get::<_, Option<String>>(4)?,
                "snapshotRef": row.get::<_, Option<String>>(5)?,
                "missingPolicy": row.get::<_, String>(6)?,
                "changedPolicy": row.get::<_, String>(7)?,
                "createdAt": row.get::<_, String>(8)?,
                "activeRevisionId": row.get::<_, Option<String>>(9)?,
                "activeContentVersion": row.get::<_, Option<i64>>(10)?,
                "activeManifestHash": row.get::<_, Option<String>>(11)?,
            }))
        })
        .map_err(to_cli_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)
}

fn reserve_task(
    storage: &mut Storage,
    project_id: &str,
    target: &Value,
    requested_task_id: Option<&str>,
    requested_capability: Option<&str>,
    requested_queue: Option<&str>,
) -> Result<Option<ReservedTask>, CliError> {
    let capabilities = target_capabilities(target);
    let now = crate::control::now_iso();
    let tx = storage
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(to_cli_error)?;
    let targets = read_targets_from_connection(&tx, project_id)?;
    tx.execute(
        r#"
        UPDATE tasks
        SET status = CASE WHEN status = 'reserved' THEN 'queued' ELSE status END,
            assigned_agent_id = CASE WHEN status = 'reserved' THEN NULL ELSE assigned_agent_id END
        WHERE project_id = ? AND status = 'reserved' AND updated_at < ?
        "#,
        params![
            project_id,
            (chrono::Utc::now() - chrono::Duration::seconds(30))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        ],
    )
    .map_err(to_cli_error)?;
    let candidates = {
        let mut statement = tx
            .prepare(
                r#"
                SELECT id, queue, status, capability, required_protocol_version
                FROM tasks
                WHERE project_id = ?
                  AND status IN ('queued', 'failed')
                  AND attempts < max_attempts
                  AND (retry_after IS NULL OR retry_after <= ?)
                  AND (lease_expires_at IS NULL OR lease_expires_at <= ?)
                  AND (? IS NULL OR id = ?)
                  AND (? IS NULL OR capability = ?)
                  AND (? IS NULL OR queue = ?)
                ORDER BY CASE status WHEN 'queued' THEN 0 ELSE 1 END,
                         updated_at ASC, id ASC
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map(
                params![
                    project_id,
                    now,
                    now,
                    requested_task_id,
                    requested_task_id,
                    requested_capability,
                    requested_capability,
                    requested_queue,
                    requested_queue,
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    let target_id = target.get("id").and_then(Value::as_str);
    let candidate = candidates
        .into_iter()
        .find(|(task_id, _, _, capability, protocol)| {
            capability_matches_any(&capabilities, capability)
                && preferred_target_id(&tx, project_id, &targets, task_id, capability, *protocol)
                    .ok()
                    .flatten()
                    .as_deref()
                    == target_id
        });
    let Some((id, queue, previous_status, _, required_protocol_version)) = candidate else {
        tx.commit().map_err(to_cli_error)?;
        return Ok(None);
    };
    let changed = tx
        .execute(
            "UPDATE tasks SET status = 'reserved', assigned_agent_id = ?, updated_at = ? WHERE id = ? AND status = ?",
            params![target["id"].as_str(), now, id, previous_status],
        )
        .map_err(to_cli_error)?;
    if changed != 1 {
        tx.commit().map_err(to_cli_error)?;
        return Ok(None);
    }
    tx.commit().map_err(to_cli_error)?;
    Ok(Some(ReservedTask {
        id,
        previous_status,
        queue,
        required_protocol_version,
    }))
}

fn release_reservation(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    reserved: &ReservedTask,
) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    tx.execute(
            "UPDATE tasks SET status = ?, assigned_agent_id = NULL, updated_at = ? WHERE id = ? AND project_id = ? AND status = 'reserved'",
            params![reserved.previous_status, crate::control::now_iso(), reserved.id, project_id],
        )
        .map_err(to_cli_error)?;
    tx.commit().map_err(to_cli_error)
}

fn verify_lease(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
) -> Result<Value, CliError> {
    recover_expired_tasks(paths)?;
    let storage = Storage::open(paths)?;
    let lease = storage
        .connection()
        .query_row(
            r#"
            SELECT queue, attempts, assigned_agent_id, lease_token_hash, lease_expires_at,
                   project_id, panel_id, status, execution_generation, required_protocol_version
            FROM tasks
            WHERE id = ?
            "#,
            params![task_id],
            |row| {
                Ok(json!({
                    "queue": row.get::<_, String>(0)?,
                    "attempt": row.get::<_, i64>(1)?,
                    "targetId": row.get::<_, Option<String>>(2)?,
                    "tokenHash": row.get::<_, Option<String>>(3)?,
                    "expiresAt": row.get::<_, Option<String>>(4)?,
                    "projectId": row.get::<_, String>(5)?,
                    "panelId": row.get::<_, String>(6)?,
                    "status": row.get::<_, String>(7)?,
                    "executionGeneration": row.get::<_, i64>(8)?,
                    "requiredProtocolVersion": row.get::<_, i64>(9)?,
                }))
            },
        )
        .optional()
        .map_err(to_cli_error)?
        .ok_or_else(|| {
            CliError::with_code(
                "task_not_found",
                format!("Project task not found: {task_id}"),
            )
        })?;
    if !matches!(
        lease["status"].as_str(),
        Some("running" | "claimed" | "converting" | "indexing")
    ) {
        return Err(CliError::with_code(
            "execution_fenced",
            "Task execution has been cancelled, replaced, or completed.",
        ));
    }
    if lease["tokenHash"].as_str() != Some(hash_secret(lease_token).as_str()) {
        return Err(CliError::with_code(
            "invalid_lease",
            "Task lease token is invalid.",
        ));
    }
    if lease["expiresAt"]
        .as_str()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .is_none_or(|value| value.with_timezone(&chrono::Utc) <= chrono::Utc::now())
    {
        return Err(CliError::with_code(
            "lease_expired",
            "Task lease has expired.",
        ));
    }
    Ok(lease)
}

pub(crate) fn verify_task_write_access(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
) -> Result<(), CliError> {
    let lease_token = match std::env::var("MYOPENPANELS_TASK_LEASE_TOKEN") {
        Ok(token) if !token.trim().is_empty() => token,
        _ if cfg!(test) => return Ok(()),
        _ => {
            return Err(CliError::with_code(
                "execution_fenced",
                "Task-scoped writes require the active execution lease token.",
            ))
        }
    };
    let lease = verify_lease(paths, task_id, &lease_token)?;
    if lease["requiredProtocolVersion"].as_i64().unwrap_or(1)
        >= crate::content::EXECUTION_PROTOCOL_VERSION
        && !crate::content::broker_execution_available()
        && !cfg!(test)
    {
        return Err(CliError::with_code(
            "broker_unavailable",
            "Execution protocol v3 does not permit direct content writes.",
        ));
    }
    if std::env::var("MYOPENPANELS_TASK_ID")
        .ok()
        .is_some_and(|expected| expected != task_id)
    {
        return Err(CliError::with_code(
            "execution_fenced",
            "The task-scoped token belongs to a different Task.",
        ));
    }
    if lease["status"].as_str().is_none() {
        return Err(CliError::with_code(
            "execution_fenced",
            "Task execution is no longer writable.",
        ));
    }
    Ok(())
}

pub(crate) fn cancel_tasks_for_resource(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    resource_kind: &str,
    resource_id: &str,
    reason_code: &str,
) -> Result<Vec<String>, CliError> {
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let tasks = {
        let mut statement = tx.prepare(
            r#"
            SELECT DISTINCT t.id, t.workflow_id, t.status
            FROM tasks t
            LEFT JOIN task_inputs i ON i.task_id = t.id
            WHERE t.project_id = ?
              AND t.status IN ('waiting', 'queued', 'failed', 'reserved', 'running', 'claimed', 'converting', 'indexing')
              AND ((i.resource_kind = ? AND i.resource_id = ? AND i.missing_policy = 'cancel')
                   OR json_extract(t.input_json, '$.documentId') = ?)
            "#,
        ).map_err(to_cli_error)?;
        let rows = statement
            .query_map(
                params![project_id, resource_kind, resource_id, resource_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    let reason =
        json!({ "code": reason_code, "resourceKind": resource_kind, "resourceId": resource_id });
    let mut cancelled = Vec::with_capacity(tasks.len());
    for (task_id, workflow_id, previous_status) in tasks {
        tx.execute(
            r#"UPDATE tasks SET status = 'cancelled', assigned_agent_id = NULL,
               lease_owner = NULL, lease_token_hash = NULL, lease_expires_at = NULL,
               last_heartbeat_at = NULL, terminal_reason_json = ?,
               execution_generation = execution_generation + 1,
               completed_at = ?, updated_at = ? WHERE id = ?"#,
            params![reason.to_string(), now, now, task_id],
        )
        .map_err(to_cli_error)?;
        tx.execute(
            "UPDATE task_attempts SET status = 'cancelled', finished_at = ?, error_json = ? WHERE task_id = ? AND status = 'leased'",
            params![now, reason.to_string(), task_id],
        ).map_err(to_cli_error)?;
        tx.execute(
            "INSERT INTO task_events (task_id, workflow_id, event_type, from_status, to_status, reason_json, created_at) VALUES (?, ?, 'input_missing', ?, 'cancelled', ?, ?)",
            params![task_id, workflow_id, previous_status, reason.to_string(), now],
        ).map_err(to_cli_error)?;
        propagate_prerequisite_failure(&tx, &task_id, "cancelled", &now)?;
        refresh_workflow_status(&tx, &workflow_id, &now)?;
        cancelled.push(task_id);
    }
    if !cancelled.is_empty() {
        crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    }
    tx.commit().map_err(to_cli_error)?;
    Ok(cancelled)
}

pub(crate) fn supersede_tasks_for_changed_resource(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    resource_id: &str,
) -> Result<Vec<String>, CliError> {
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let tasks = {
        let mut statement = tx.prepare(
            r#"SELECT id, workflow_id, status FROM tasks
               WHERE project_id = ? AND capability = 'wiki.ingestMarkdown'
                 AND json_extract(input_json, '$.documentId') = ?
                 AND status IN ('waiting', 'queued', 'failed', 'reserved', 'running', 'claimed', 'indexing')"#,
        ).map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![project_id, resource_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    let reason = json!({ "code": "content_conflict", "resourceId": resource_id });
    let mut superseded = Vec::new();
    for (task_id, workflow_id, previous_status) in tasks {
        tx.execute(
            r#"UPDATE tasks SET status = 'superseded', assigned_agent_id = NULL, lease_owner = NULL,
               lease_token_hash = NULL, lease_expires_at = NULL, last_heartbeat_at = NULL,
               execution_generation = execution_generation + 1, terminal_reason_json = ?,
               completed_at = ?, updated_at = ? WHERE id = ?"#,
            params![reason.to_string(), now, now, task_id],
        )
        .map_err(to_cli_error)?;
        tx.execute("UPDATE task_attempts SET status = 'cancelled', finished_at = ?, error_json = ? WHERE task_id = ? AND status = 'leased'", params![now, reason.to_string(), task_id]).map_err(to_cli_error)?;
        tx.execute("INSERT INTO task_events (task_id, workflow_id, event_type, from_status, to_status, reason_json, created_at) VALUES (?, ?, 'input_changed', ?, 'superseded', ?, ?)", params![task_id, workflow_id, previous_status, reason.to_string(), now]).map_err(to_cli_error)?;
        refresh_workflow_status(&tx, &workflow_id, &now)?;
        superseded.push(task_id);
    }
    if !superseded.is_empty() {
        crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    }
    tx.commit().map_err(to_cli_error)?;
    Ok(superseded)
}

pub(crate) fn supersede_task_for_content_conflict(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    resource_id: &str,
) -> Result<Value, CliError> {
    let task = inspect_task(paths, task_id)?;
    let project_id = task["task"]["projectId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let workflow_id = task["task"]["workflowId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let previous_status = task["task"]["status"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    if matches!(
        previous_status.as_str(),
        "succeeded" | "cancelled" | "stale" | "superseded"
    ) {
        return Ok(task);
    }
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let reason = json!({ "code": "content_conflict", "resourceId": resource_id });
    tx.execute(
        r#"UPDATE tasks SET status = 'superseded', assigned_agent_id = NULL,
           lease_owner = NULL, lease_token_hash = NULL, lease_expires_at = NULL,
           last_heartbeat_at = NULL, terminal_reason_json = ?,
           execution_generation = execution_generation + 1,
           completed_at = ?, updated_at = ? WHERE id = ?"#,
        params![reason.to_string(), now, now, task_id],
    )
    .map_err(to_cli_error)?;
    tx.execute(
        "UPDATE task_attempts SET status = 'cancelled', finished_at = ?, error_json = ? WHERE task_id = ? AND status = 'leased'",
        params![now, reason.to_string(), task_id],
    )
    .map_err(to_cli_error)?;
    tx.execute(
        "INSERT INTO task_events (task_id, workflow_id, event_type, from_status, to_status, reason_json, created_at) VALUES (?, ?, 'input_changed', ?, 'superseded', ?, ?)",
        params![task_id, workflow_id, previous_status, reason.to_string(), now],
    )
    .map_err(to_cli_error)?;
    propagate_prerequisite_failure(&tx, task_id, "superseded", &now)?;
    refresh_workflow_status(&tx, &workflow_id, &now)?;
    crate::storage::record_scope(&tx, "tasks", Some(&project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    inspect_task_in_session(paths, &project_id, task_id)
}

fn finalize_task_runtime(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    task_id: &str,
    status: &str,
    result: Option<Value>,
    error: Option<Value>,
    retry_after: Option<&str>,
    failure_class: Option<TaskFailureClass>,
    panel_state: Option<(&str, &Value)>,
    expected_generation: Option<i64>,
) -> Result<(), CliError> {
    let mut storage = Storage::open(paths)?;
    let tx = storage
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let mut result = result;
    let error_json = error
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(to_cli_error)?;
    let (previous_status, workflow_id, attempt_id, attempts, max_attempts, execution_generation): (
        String,
        String,
        Option<String>,
        i64,
        i64,
        i64,
    ) = tx
        .query_row(
            r#"
            SELECT t.status, t.workflow_id,
                   (SELECT id FROM task_attempts a
                    WHERE a.task_id = t.id AND a.execution_generation = t.execution_generation),
                   t.attempts, t.max_attempts, t.execution_generation
            FROM tasks t WHERE t.id = ? AND t.project_id = ?
            "#,
            params![task_id, project_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .map_err(to_cli_error)?;
    if expected_generation.is_some_and(|expected| expected != execution_generation) {
        return Err(CliError::with_code(
            "execution_fenced",
            "The Task is now owned by a newer execution generation.",
        ));
    }
    let has_staging = attempt_id.as_deref().is_some_and(|attempt_id| {
        tx.query_row(
            "SELECT staging_session_id IS NOT NULL FROM task_attempts WHERE id = ?",
            [attempt_id],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(false)
    });
    if status == "succeeded" && has_staging {
        let commits =
            crate::content::commit_task_staging_in_transaction(paths, &tx, task_id, &now)?;
        if !commits.is_empty() {
            let payload = result.get_or_insert_with(|| json!({}));
            if !payload.is_object() {
                *payload = json!({ "agentResult": payload.clone() });
            }
            payload["contentCommits"] = json!(commits);
        }
    } else if status != "succeeded" && has_staging {
        crate::content::abandon_task_staging_in_transaction(&tx, task_id, &now)?;
    }
    let result_json = result
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(to_cli_error)?;
    let terminal_reason_json = if matches!(status, "cancelled" | "stale" | "superseded") {
        error_json.clone()
    } else {
        None
    };
    tx.execute(
        r#"
            UPDATE tasks
            SET status = ?, assigned_agent_id = NULL, lease_owner = NULL,
                lease_expires_at = NULL, last_heartbeat_at = NULL,
                lease_token_hash = NULL, retry_after = ?, result_json = ?,
                error_json = ?, terminal_reason_json = COALESCE(?, terminal_reason_json),
                max_attempts = CASE WHEN ? = 'terminal_task' THEN attempts ELSE max_attempts END,
                execution_generation = execution_generation + CASE
                  WHEN ? IN ('failed', 'queued', 'cancelled') THEN 1 ELSE 0 END,
                completed_at = ?, updated_at = ?
            WHERE id = ? AND project_id = ?
            "#,
        params![
            status,
            retry_after,
            result_json,
            error_json,
            terminal_reason_json,
            failure_class.map(TaskFailureClass::as_str),
            status,
            if matches!(status, "succeeded" | "cancelled")
                || (status == "failed"
                    && (attempts >= max_attempts
                        || failure_class == Some(TaskFailureClass::TerminalTask)))
            {
                Some(now.clone())
            } else {
                None
            },
            now,
            task_id,
            project_id,
        ],
    )
    .map_err(to_cli_error)?;
    if let Some(attempt_id) = attempt_id.as_deref() {
        let attempt_status = match status {
            "succeeded" => "succeeded",
            "cancelled" => "cancelled",
            "failed" if failure_class == Some(TaskFailureClass::TerminalTask) => "failed_terminal",
            "failed" if attempts >= max_attempts => "failed_terminal",
            "failed" => "failed_retryable",
            _ => "interrupted",
        };
        tx.execute(
            "UPDATE task_attempts SET status = ?, heartbeat_at = ?, finished_at = ?, result_json = ?, error_json = ?, failure_class = ? WHERE id = ? AND status = 'leased'",
            params![attempt_status, now, now, result_json, error_json, failure_class.map(TaskFailureClass::as_str), attempt_id],
        )
        .map_err(to_cli_error)?;
    }
    tx.execute(
        "INSERT INTO task_events (task_id, workflow_id, event_type, from_status, to_status, reason_json, attempt_id, created_at) VALUES (?, ?, 'status_changed', ?, ?, ?, ?, ?)",
        params![task_id, workflow_id, previous_status, status, terminal_reason_json.or(error_json), attempt_id, now],
    )
    .map_err(to_cli_error)?;
    if status == "succeeded" {
        activate_ready_dependents(&tx, task_id, &now)?;
    } else if status == "failed"
        && (attempts >= max_attempts || failure_class == Some(TaskFailureClass::TerminalTask))
    {
        propagate_prerequisite_failure(&tx, task_id, "failed_terminal", &now)?;
    } else if matches!(status, "cancelled" | "stale" | "superseded") {
        propagate_prerequisite_failure(&tx, task_id, status, &now)?;
    }
    refresh_workflow_status(&tx, &workflow_id, &now)?;
    if let Some((panel_id, state)) = panel_state {
        Storage::write_panel_state_in_transaction(&tx, project_id, panel_id, state)?;
    }
    crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    tx.commit().map_err(to_cli_error)
}

fn activate_ready_dependents(
    connection: &rusqlite::Connection,
    prerequisite_task_id: &str,
    now: &str,
) -> Result<(), CliError> {
    let dependents = {
        let mut statement = connection
            .prepare(
                r#"
                SELECT t.id, t.workflow_id, t.capability
                FROM tasks t
                JOIN task_dependencies d ON d.task_id = t.id
                WHERE d.prerequisite_task_id = ? AND t.status = 'waiting'
                  AND NOT EXISTS (
                    SELECT 1 FROM task_dependencies remaining
                    JOIN tasks prerequisite ON prerequisite.id = remaining.prerequisite_task_id
                    WHERE remaining.task_id = t.id
                      AND prerequisite.status <> 'succeeded'
                  )
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map([prerequisite_task_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    for (task_id, workflow_id, _) in dependents {
        connection
            .execute(
                "UPDATE tasks SET status = 'queued', available_at = ?, updated_at = ? WHERE id = ? AND status = 'waiting'",
                params![now, now, task_id],
            )
            .map_err(to_cli_error)?;
        connection
            .execute(
                "INSERT INTO task_events (task_id, workflow_id, event_type, from_status, to_status, reason_json, created_at) VALUES (?, ?, 'dependency_satisfied', 'waiting', 'queued', ?, ?)",
                params![task_id, workflow_id, json!({ "prerequisiteTaskId": prerequisite_task_id }).to_string(), now],
            )
            .map_err(to_cli_error)?;
    }
    Ok(())
}

fn propagate_prerequisite_failure(
    connection: &rusqlite::Connection,
    prerequisite_task_id: &str,
    prerequisite_status: &str,
    now: &str,
) -> Result<(), CliError> {
    let dependents = {
        let mut statement = connection
            .prepare(
                r#"
                SELECT t.id, t.workflow_id, t.status, d.failure_policy
                FROM tasks t
                JOIN task_dependencies d ON d.task_id = t.id
                WHERE d.prerequisite_task_id = ?
                  AND t.status IN ('waiting', 'queued', 'failed', 'reserved', 'running', 'claimed', 'converting', 'indexing')
                  AND d.failure_policy <> 'continue_snapshot'
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map([prerequisite_task_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    for (task_id, workflow_id, previous_status, policy) in dependents {
        let next_status = if policy == "supersede" {
            "superseded"
        } else {
            "cancelled"
        };
        let reason = json!({
            "code": "prerequisite_failed",
            "prerequisiteTaskId": prerequisite_task_id,
            "prerequisiteStatus": prerequisite_status,
        });
        connection
            .execute(
                r#"
                UPDATE tasks SET status = ?, assigned_agent_id = NULL, lease_owner = NULL,
                  lease_token_hash = NULL, lease_expires_at = NULL, last_heartbeat_at = NULL,
                  execution_generation = execution_generation + 1,
                  terminal_reason_json = ?, completed_at = ?, updated_at = ?
                WHERE id = ?
                "#,
                params![next_status, reason.to_string(), now, now, task_id],
            )
            .map_err(to_cli_error)?;
        connection
            .execute(
                "UPDATE task_attempts SET status = 'cancelled', finished_at = ?, error_json = ? WHERE task_id = ? AND status = 'leased'",
                params![now, reason.to_string(), task_id],
            )
            .map_err(to_cli_error)?;
        connection
            .execute(
                "INSERT INTO task_events (task_id, workflow_id, event_type, from_status, to_status, reason_json, created_at) VALUES (?, ?, 'prerequisite_propagated', ?, ?, ?, ?)",
                params![task_id, workflow_id, previous_status, next_status, reason.to_string(), now],
            )
            .map_err(to_cli_error)?;
    }
    Ok(())
}

fn refresh_workflow_status(
    connection: &rusqlite::Connection,
    workflow_id: &str,
    now: &str,
) -> Result<(), CliError> {
    connection
        .execute(
            r#"
            UPDATE workflows
            SET status = CASE
                  WHEN EXISTS (SELECT 1 FROM tasks WHERE workflow_id = ?
                    AND status NOT IN ('succeeded', 'cancelled', 'stale', 'superseded')
                    AND NOT (status = 'failed' AND attempts >= max_attempts)) THEN 'active'
                  WHEN EXISTS (SELECT 1 FROM tasks WHERE workflow_id = ? AND status = 'succeeded')
                       AND NOT EXISTS (SELECT 1 FROM tasks WHERE workflow_id = ? AND status IN ('cancelled', 'stale', 'superseded')) THEN 'succeeded'
                  WHEN EXISTS (SELECT 1 FROM tasks WHERE workflow_id = ? AND status IN ('cancelled', 'stale', 'superseded')) THEN 'cancelled'
                  ELSE 'failed'
                END,
                updated_at = ?
            WHERE id = ?
            "#,
            params![workflow_id, workflow_id, workflow_id, workflow_id, now, workflow_id],
        )
        .map_err(to_cli_error)?;
    Ok(())
}

fn recover_expired_tasks(paths: &MyOpenPanelsPaths) -> Result<(), CliError> {
    let bootstrap = match read_project_bootstrap(paths, BootstrapRequest::new()) {
        Ok(bootstrap) => bootstrap,
        Err(error) if error.code() == Some("no_current_project") => return Ok(()),
        Err(error) => return Err(error),
    };
    recover_expired_tasks_in_session(paths, &bootstrap.project.id)
}

fn recover_expired_tasks_in_session(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    let now = crate::control::now_iso();
    storage
        .connection()
        .execute(
            "UPDATE tasks SET status = 'queued', assigned_agent_id = NULL, updated_at = ? WHERE project_id = ? AND status = 'reserved' AND updated_at < ?",
            params![
                now,
                project_id,
                (chrono::Utc::now() - chrono::Duration::seconds(30))
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
            ],
        )
        .map_err(to_cli_error)?;
    let expired = {
        let mut statement = storage
            .connection()
            .prepare(
                r#"
                SELECT id, queue, attempts, assigned_agent_id
                FROM tasks
                WHERE project_id = ?
                  AND status IN ('running', 'claimed', 'converting', 'indexing')
                  AND lease_expires_at IS NOT NULL
                  AND lease_expires_at <= ?
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![project_id, now], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    drop(storage);
    for (task_id, queue, attempt, target_id) in expired {
        let retry_after = if has_untried_eligible_target(
            paths,
            project_id,
            &task_id,
            target_id.as_deref().unwrap_or_default(),
        )? {
            crate::control::now_iso()
        } else {
            execution_retry_after(attempt.max(1))
        };
        if queue == "wiki" {
            crate::wiki::fail_task_with_retry(
                paths,
                &task_id,
                "Task lease expired.",
                Some(&retry_after),
            )?;
        }
        finalize_task_runtime(
            paths,
            project_id,
            &task_id,
            "failed",
            None,
            Some(json!("Task lease expired.")),
            Some(&retry_after),
            Some(TaskFailureClass::RetryableChannel),
            None,
            None,
        )?;
    }
    Ok(())
}

fn annotate_dispatch_state(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    tasks: Vec<Value>,
) -> Result<Vec<Value>, CliError> {
    let targets = read_targets(paths, project_id)?;
    let storage = Storage::open(paths)?;
    let mut output = Vec::with_capacity(tasks.len());
    for mut task in tasks {
        let capability = task.get("capability").and_then(Value::as_str).unwrap_or("");
        let mut matching = matching_targets(&targets, capability);
        if task.get("dispatchMode").and_then(Value::as_str) == Some("only") {
            if let Some(requested) = task
                .get("requestedGatewayConnectionId")
                .and_then(Value::as_str)
            {
                matching.retain(|target| target_channel_key(target) == requested);
            }
        }
        let assigned_target = task
            .get("assignedTargetId")
            .and_then(Value::as_str)
            .and_then(|id| targets.iter().find(|target| target["id"] == id))
            .cloned();
        let dependencies =
            read_task_dependency_values(storage.connection(), task["id"].as_str().unwrap_or(""))?;
        let required_protocol = task
            .get("requiredProtocolVersion")
            .and_then(Value::as_i64)
            .unwrap_or(1);
        let compatible_target_count = matching
            .iter()
            .filter(|target| {
                target
                    .get("protocolVersion")
                    .and_then(Value::as_i64)
                    .unwrap_or(1)
                    >= required_protocol
            })
            .count();
        let dispatch_state = if is_active_task(&task) {
            "running"
        } else if task.get("status").and_then(Value::as_str) == Some("waiting") {
            "waiting"
        } else if !is_pending_task(&task) {
            "done"
        } else if matching.is_empty() {
            "noTarget"
        } else if compatible_target_count == 0 {
            "incompatible"
        } else {
            "eligible"
        };
        if let Some(object) = task.as_object_mut() {
            object.insert("dispatchState".to_owned(), json!(dispatch_state));
            object.insert("matchedTargetCount".to_owned(), json!(matching.len()));
            object.insert(
                "compatibleTargetCount".to_owned(),
                json!(compatible_target_count),
            );
            object.insert("dependencies".to_owned(), json!(dependencies));
            object.insert(
                "assignedTarget".to_owned(),
                assigned_target.unwrap_or(Value::Null),
            );
        }
        output.push(task);
    }
    Ok(output)
}

fn read_targets(paths: &MyOpenPanelsPaths, project_id: &str) -> Result<Vec<Value>, CliError> {
    let storage = Storage::open(paths)?;
    read_targets_from_connection(storage.connection(), project_id)
}

fn read_targets_from_connection(
    connection: &rusqlite::Connection,
    project_id: &str,
) -> Result<Vec<Value>, CliError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT id, name, host, transport, capabilities_json,
                   priority, status, last_error, last_heartbeat_at, created_at, updated_at,
                   protocol_version, max_concurrency,
                   (SELECT COUNT(*) FROM tasks active
                    WHERE active.assigned_agent_id = agent_targets.id
                      AND active.status IN ('reserved', 'running', 'claimed', 'converting', 'indexing')),
                   model_gateway_connection_id
            FROM agent_targets
            WHERE project_id = ? AND transport IN ('poll', 'command')
            ORDER BY priority DESC, last_heartbeat_at DESC, id ASC
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map(params![project_id], target_from_row)
        .map_err(to_cli_error)?;
    rows.map(|row| row.map(compute_target_status).map_err(to_cli_error))
        .collect()
}

fn read_target_value(
    connection: &rusqlite::Connection,
    project_id: &str,
    target_id: &str,
) -> Result<Option<Value>, CliError> {
    connection
        .query_row(
            r#"
            SELECT id, name, host, transport, capabilities_json,
                   priority, status, last_error, last_heartbeat_at, created_at, updated_at,
                   protocol_version, max_concurrency,
                   (SELECT COUNT(*) FROM tasks active
                    WHERE active.assigned_agent_id = agent_targets.id
                      AND active.status IN ('reserved', 'running', 'claimed', 'converting', 'indexing')),
                   model_gateway_connection_id
            FROM agent_targets
            WHERE project_id = ? AND id = ? AND transport IN ('poll', 'command')
            "#,
            params![project_id, target_id],
            target_from_row,
        )
        .optional()
        .map(|target| target.map(compute_target_status))
        .map_err(to_cli_error)
}

fn target_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let capabilities_json = row.get::<_, String>(4)?;
    let capabilities =
        serde_json::from_str::<Value>(&capabilities_json).unwrap_or_else(|_| json!([]));
    Ok(json!({
        "id": row.get::<_, String>(0)?,
        "name": row.get::<_, String>(1)?,
        "host": row.get::<_, String>(2)?,
        "transport": row.get::<_, String>(3)?,
        "capabilities": capabilities,
        "priority": row.get::<_, i64>(5)?,
        "status": row.get::<_, String>(6)?,
        "lastError": row.get::<_, Option<String>>(7)?,
        "lastHeartbeatAt": row.get::<_, String>(8)?,
        "createdAt": row.get::<_, String>(9)?,
        "updatedAt": row.get::<_, String>(10)?,
        "protocolVersion": row.get::<_, i64>(11)?,
        "maxConcurrency": row.get::<_, i64>(12)?,
        "activeAttempts": row.get::<_, i64>(13)?,
        "modelGatewayConnectionId": row.get::<_, Option<String>>(14)?,
    }))
}

fn compute_target_status(mut target: Value) -> Value {
    let stale = target
        .get("lastHeartbeatAt")
        .and_then(Value::as_str)
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .is_none_or(|value| {
            value.with_timezone(&chrono::Utc)
                < chrono::Utc::now() - chrono::Duration::seconds(TARGET_ONLINE_WINDOW_SECONDS)
        });
    if stale && target.get("status").and_then(Value::as_str) == Some("online") {
        target["status"] = json!("offline");
    }
    target
}

fn matching_targets<'a>(targets: &'a [Value], capability: &str) -> Vec<&'a Value> {
    targets
        .iter()
        .filter(|target| target.get("status").and_then(Value::as_str) == Some("online"))
        .filter(|target| capability_matches_any(&target_capabilities(target), capability))
        .collect()
}

fn preferred_target_id(
    connection: &rusqlite::Connection,
    project_id: &str,
    targets: &[Value],
    task_id: &str,
    capability: &str,
    required_protocol_version: i64,
) -> Result<Option<String>, CliError> {
    let eligible = matching_targets(targets, capability)
        .into_iter()
        .filter(|target| {
            target
                .get("protocolVersion")
                .and_then(Value::as_i64)
                .unwrap_or(1)
                >= required_protocol_version
        })
        .filter(|target| {
            target
                .get("activeAttempts")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                < target
                    .get("maxConcurrency")
                    .and_then(Value::as_i64)
                    .unwrap_or(1)
        })
        .collect::<Vec<_>>();
    let (dispatch_mode, requested_connection_id) = connection
        .query_row(
            "SELECT dispatch_mode, requested_gateway_connection_id FROM tasks WHERE id = ?",
            [task_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .map_err(to_cli_error)?;
    let mut ordered = Vec::new();
    let routed_ids = route_target_ids(connection, project_id, capability)?;
    for routed_id in routed_ids {
        if let Some(target) = eligible
            .iter()
            .copied()
            .find(|target| target.get("id").and_then(Value::as_str) == Some(&routed_id))
        {
            ordered.push(target);
        }
    }
    for target in eligible {
        if !ordered
            .iter()
            .any(|candidate| candidate["id"] == target["id"])
        {
            ordered.push(target);
        }
    }
    if let Some(requested) = requested_connection_id.as_deref() {
        if dispatch_mode == "only" {
            ordered.retain(|target| target_channel_key(target) == requested);
        } else if dispatch_mode == "prefer" {
            ordered.sort_by_key(|target| usize::from(target_channel_key(target) != requested));
        }
    }
    let attempt_counts = channel_attempt_counts(connection, task_id)?;
    let minimum_attempts = ordered
        .iter()
        .map(|target| {
            attempt_counts
                .get(target_channel_key(target))
                .copied()
                .unwrap_or(0)
        })
        .min();
    Ok(ordered
        .iter()
        .find(|target| {
            Some(
                attempt_counts
                    .get(target_channel_key(target))
                    .copied()
                    .unwrap_or(0),
            ) == minimum_attempts
        })
        .and_then(|target| target.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned))
}

fn target_channel_key(target: &Value) -> &str {
    target
        .get("modelGatewayConnectionId")
        .and_then(Value::as_str)
        .or_else(|| target.get("id").and_then(Value::as_str))
        .unwrap_or("")
}

fn channel_attempt_counts(
    connection: &rusqlite::Connection,
    task_id: &str,
) -> Result<HashMap<String, i64>, CliError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT COALESCE(model_gateway_connection_id, agent_target_id), COUNT(*)
            FROM task_attempts
            WHERE task_id = ?
              AND status IN ('failed_retryable', 'invalid_output', 'interrupted')
            GROUP BY COALESCE(model_gateway_connection_id, agent_target_id)
            "#,
        )
        .map_err(to_cli_error)?;
    let counts = statement
        .query_map([task_id], |row| {
            Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(to_cli_error)?
        .filter_map(|row| match row {
            Ok((Some(key), count)) => Some(Ok((key, count))),
            Ok((None, _)) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(to_cli_error)?;
    Ok(counts)
}

fn has_untried_eligible_target(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    task_id: &str,
    current_target_id: &str,
) -> Result<bool, CliError> {
    let storage = Storage::open(paths)?;
    let connection = storage.connection();
    let (capability, required_protocol, dispatch_mode, requested_connection_id) = connection
        .query_row(
            r#"
            SELECT capability, required_protocol_version, dispatch_mode,
                   requested_gateway_connection_id
            FROM tasks WHERE id = ? AND project_id = ?
            "#,
            params![task_id, project_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .map_err(to_cli_error)?;
    let targets = read_targets_from_connection(connection, project_id)?;
    let mut attempt_counts = channel_attempt_counts(connection, task_id)?;
    let current_key = targets
        .iter()
        .find(|target| target.get("id").and_then(Value::as_str) == Some(current_target_id))
        .map(target_channel_key)
        .unwrap_or(current_target_id);
    *attempt_counts.entry(current_key.to_owned()).or_insert(0) += 1;
    let current_attempts = attempt_counts.get(current_key).copied().unwrap_or(1);
    Ok(matching_targets(&targets, &capability)
        .into_iter()
        .filter(|target| {
            target
                .get("protocolVersion")
                .and_then(Value::as_i64)
                .unwrap_or(1)
                >= required_protocol
        })
        .filter(|target| {
            dispatch_mode != "only"
                || requested_connection_id
                    .as_deref()
                    .is_some_and(|requested| target_channel_key(target) == requested)
        })
        .any(|target| {
            target_channel_key(target) != current_key
                && attempt_counts
                    .get(target_channel_key(target))
                    .copied()
                    .unwrap_or(0)
                    < current_attempts
        }))
}

fn route_target_ids(
    connection: &rusqlite::Connection,
    project_id: &str,
    capability: &str,
) -> Result<Vec<String>, CliError> {
    let mut routes = connection
        .prepare(
            "SELECT agent_target_id FROM agent_routes WHERE project_id = ? AND capability = ? ORDER BY position ASC",
        )
        .map_err(to_cli_error)?;
    let target_ids = routes
        .query_map(params![project_id, capability], |row| {
            row.get::<_, String>(0)
        })
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    Ok(target_ids)
}

fn target_capabilities(target: &Value) -> Vec<String> {
    target
        .get("capabilities")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn capability_matches_any(patterns: &[String], capability: &str) -> bool {
    patterns.iter().any(|pattern| {
        pattern == "*"
            || pattern == capability
            || pattern
                .strip_suffix(".*")
                .is_some_and(|prefix| capability.starts_with(&format!("{prefix}.")))
    })
}

fn normalize_capabilities(capabilities: Vec<String>) -> Vec<String> {
    let mut capabilities = capabilities
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    capabilities.sort();
    capabilities.dedup();
    capabilities
}

fn read_task_dependency_values(
    connection: &rusqlite::Connection,
    task_id: &str,
) -> Result<Vec<Value>, CliError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT d.prerequisite_task_id, p.status, d.success_condition, d.failure_policy
            FROM task_dependencies d JOIN tasks p ON p.id = d.prerequisite_task_id
            WHERE d.task_id = ? ORDER BY d.prerequisite_task_id
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([task_id], |row| {
            Ok(json!({
                "prerequisiteTaskId": row.get::<_, String>(0)?,
                "status": row.get::<_, String>(1)?,
                "successCondition": row.get::<_, String>(2)?,
                "failurePolicy": row.get::<_, String>(3)?,
            }))
        })
        .map_err(to_cli_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)
}

fn task_event_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let reason = row
        .get::<_, Option<String>>(6)?
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .unwrap_or(Value::Null);
    Ok(json!({
        "id": row.get::<_, i64>(0)?,
        "taskId": row.get::<_, String>(1)?,
        "workflowId": row.get::<_, String>(2)?,
        "eventType": row.get::<_, String>(3)?,
        "fromStatus": row.get::<_, Option<String>>(4)?,
        "toStatus": row.get::<_, Option<String>>(5)?,
        "reason": reason,
        "attemptId": row.get::<_, Option<String>>(7)?,
        "targetId": row.get::<_, Option<String>>(8)?,
        "createdAt": row.get::<_, String>(9)?,
    }))
}

fn task_attempt_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let parse = |index| -> rusqlite::Result<Value> {
        Ok(row
            .get::<_, Option<String>>(index)?
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .unwrap_or(Value::Null))
    };
    Ok(json!({
        "id": row.get::<_, String>(0)?,
        "taskId": row.get::<_, String>(1)?,
        "attemptNumber": row.get::<_, i64>(2)?,
        "executionGeneration": row.get::<_, i64>(3)?,
        "targetId": row.get::<_, Option<String>>(4)?,
        "status": row.get::<_, String>(5)?,
        "startedAt": row.get::<_, String>(6)?,
        "heartbeatAt": row.get::<_, Option<String>>(7)?,
        "finishedAt": row.get::<_, Option<String>>(8)?,
        "result": parse(9)?,
        "error": parse(10)?,
        "stagingStatus": row.get::<_, Option<String>>(11)?,
        "stagedBytes": row.get::<_, Option<i64>>(12)?.unwrap_or(0),
        "stagingUpdatedAt": row.get::<_, Option<String>>(13)?,
        "modelGatewayConnectionId": row.get::<_, Option<String>>(14)?,
        "executorSnapshot": parse(15)?,
        "failureClass": row.get::<_, Option<String>>(16)?,
    }))
}

fn workflow_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let source = row
        .get::<_, String>(4)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .unwrap_or_else(|| json!({}));
    let total = row.get::<_, i64>(8)?;
    let succeeded = row.get::<_, i64>(9)?;
    Ok(json!({
        "id": row.get::<_, String>(0)?,
        "type": row.get::<_, String>(1)?,
        "status": row.get::<_, String>(2)?,
        "sourceWorkflowId": row.get::<_, Option<String>>(3)?,
        "source": source,
        "createdAt": row.get::<_, String>(5)?,
        "updatedAt": row.get::<_, String>(6)?,
        "archivedAt": row.get::<_, Option<String>>(7)?,
        "taskCount": total,
        "succeededTaskCount": succeeded,
        "progress": if total == 0 { 0.0 } else { succeeded as f64 / total as f64 },
    }))
}

pub fn queue_status(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let targets_payload = list_targets(paths)?;
    let task_payload = list_tasks(paths, TaskListFilter::default())?;
    let targets = targets_payload["targets"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let running_count = task_payload["runningCount"].as_u64().unwrap_or(0);
    let unhandled_count = task_payload["unhandledCount"].as_u64().unwrap_or(0);
    let status = if running_count > 0 {
        "running"
    } else if unhandled_count > 0 {
        "noTarget"
    } else {
        "idle"
    };
    Ok(json!({
        "status": status,
        "onlineTargetCount": targets_payload["onlineCount"],
        "targetCount": targets.len(),
        "pendingCount": task_payload["pendingCount"],
        "runningCount": running_count,
        "unhandledCount": unhandled_count,
        "targets": targets,
        "updatedAt": crate::control::now_iso(),
    }))
}

fn random_id(prefix: &str) -> String {
    let random: u128 = rand::rng().random();
    format!("{prefix}:{random:032x}")
}

fn random_secret(prefix: &str) -> String {
    let first: u128 = rand::rng().random();
    let second: u128 = rand::rng().random();
    format!("{prefix}_{first:032x}{second:032x}")
}

fn hash_secret(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}

fn target_secret_path(paths: &MyOpenPanelsPaths, target_id: &str) -> std::path::PathBuf {
    paths.context_dir.join("agent-target-secrets").join(format!(
        "{}.token",
        crate::paths::sanitize_path_part(target_id)
    ))
}

fn write_target_secret(
    paths: &MyOpenPanelsPaths,
    target_id: &str,
    token: &str,
) -> Result<(), CliError> {
    let path = target_secret_path(paths, target_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(&path, format!("{token}\n")).map_err(to_cli_error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(to_cli_error)?;
    }
    Ok(())
}

fn lease_expires_at() -> String {
    (chrono::Utc::now() + chrono::Duration::minutes(DEFAULT_LEASE_MINUTES))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn execution_retry_after(attempt: i64) -> String {
    let seconds = match attempt {
        0 | 1 => 5,
        2 => 30,
        _ => 120,
    };
    (chrono::Utc::now() + chrono::Duration::seconds(seconds))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

pub fn pending_task_count(tasks: &[Value]) -> usize {
    tasks.iter().filter(|task| is_pending_task(task)).count()
}

pub fn ready_task_count(tasks: &[Value]) -> usize {
    tasks
        .iter()
        .filter(|task| task.get("ready").and_then(Value::as_bool) == Some(true))
        .count()
}

pub fn blocked_task_count(tasks: &[Value]) -> usize {
    tasks
        .iter()
        .filter(|task| is_pending_task(task))
        .filter(|task| task.get("ready").and_then(Value::as_bool) == Some(false))
        .count()
}

pub fn is_pending_task(task: &Value) -> bool {
    matches!(
        task.get("status").and_then(Value::as_str),
        Some("waiting" | "queued" | "failed")
    )
}

fn filter_tasks(tasks: Vec<Value>, filter: &TaskListFilter<'_>) -> Vec<Value> {
    tasks
        .into_iter()
        .filter(|task| {
            if filter.pending && !is_pending_task(task) {
                return false;
            }
            if let Some(queue) = filter.queue {
                if task.get("queue").and_then(Value::as_str) != Some(queue) {
                    return false;
                }
            }
            if let Some(status) = filter.status {
                if task.get("status").and_then(Value::as_str) != Some(status) {
                    return false;
                }
            }
            true
        })
        .collect()
}

pub fn annotate_tasks(tasks: Vec<Value>) -> Vec<Value> {
    tasks.into_iter().map(annotate_task).collect()
}

pub fn annotate_task(mut task: Value) -> Value {
    let (ready, blocked_reason, next_run_at) = task_execution_state(&task);
    let lifecycle_state = match task.get("status").and_then(Value::as_str).unwrap_or("") {
        "waiting" => "waiting",
        "queued" => "ready",
        "reserved" | "running" | "claimed" | "converting" | "indexing" => "leased",
        "failed"
            if task.get("attempt").and_then(Value::as_i64).unwrap_or(0)
                >= task.get("maxAttempts").and_then(Value::as_i64).unwrap_or(8) =>
        {
            "failed_terminal"
        }
        "failed" => "failed_retryable",
        "succeeded" => "succeeded",
        "cancelled" => "cancelled",
        "stale" | "superseded" => "superseded",
        status => status,
    }
    .to_owned();
    if let Some(object) = task.as_object_mut() {
        object.insert("ready".to_owned(), json!(ready));
        object.insert(
            "blockedReason".to_owned(),
            blocked_reason.map_or(Value::Null, Value::from),
        );
        object.insert(
            "nextRunAt".to_owned(),
            next_run_at.map_or(Value::Null, Value::from),
        );
        object.insert("lifecycleState".to_owned(), json!(lifecycle_state));
    }
    task
}

fn task_execution_state(task: &Value) -> (bool, Option<&'static str>, Option<String>) {
    if !is_pending_task(task) {
        return (false, None, None);
    }

    if task.get("status").and_then(Value::as_str) == Some("waiting") {
        return (false, Some("prerequisite"), None);
    }

    if task.get("status").and_then(Value::as_str) == Some("failed")
        && task.get("attempt").and_then(Value::as_i64).unwrap_or(0)
            >= task.get("maxAttempts").and_then(Value::as_i64).unwrap_or(8)
    {
        return (false, Some(BLOCKED_REASON_ATTEMPTS_EXCEEDED), None);
    }

    if let Some(retry_after) = future_time(task.get("retryAfter").and_then(Value::as_str)) {
        return (false, Some(BLOCKED_REASON_RETRY_LATER), Some(retry_after));
    }

    if task
        .get("lease")
        .and_then(|lease| lease.get("owner"))
        .and_then(Value::as_str)
        .filter(|owner| !owner.trim().is_empty())
        .is_some()
    {
        if let Some(expires_at) = future_time(
            task.get("lease")
                .and_then(|lease| lease.get("expiresAt"))
                .and_then(Value::as_str),
        ) {
            return (false, Some(BLOCKED_REASON_LEASED), Some(expires_at));
        }
    }

    (true, None, None)
}

fn future_time(value: Option<&str>) -> Option<String> {
    let value = value?;
    let parsed = chrono::DateTime::parse_from_rfc3339(value).ok()?;
    if parsed.with_timezone(&chrono::Utc) > chrono::Utc::now() {
        Some(value.to_owned())
    } else {
        None
    }
}

fn sort_tasks_for_display(mut tasks: Vec<Value>) -> Vec<Value> {
    tasks.sort_by(|left, right| {
        let left_rank = task_display_rank(left);
        let right_rank = task_display_rank(right);
        left_rank.cmp(&right_rank).then_with(|| {
            right
                .get("updatedAt")
                .and_then(Value::as_str)
                .unwrap_or("")
                .cmp(left.get("updatedAt").and_then(Value::as_str).unwrap_or(""))
        })
    });
    tasks
}

fn task_display_rank(task: &Value) -> u8 {
    let status = task.get("status").and_then(Value::as_str).unwrap_or("");
    let ready = task.get("ready").and_then(Value::as_bool).unwrap_or(false);
    match (ready, status) {
        (true, "failed") => 0,
        (true, "queued") => 1,
        (false, "failed") => 2,
        (false, "queued") => 3,
        _ => 4,
    }
}
