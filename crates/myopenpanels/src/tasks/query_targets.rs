use crate::control::{read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::MyOpenPanelsPaths;
use crate::storage::Storage;
use rand::Rng;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub(crate) mod model;

const BLOCKED_REASON_ATTEMPTS_EXCEEDED: &str = "attemptsExceeded";
const BLOCKED_REASON_LEASED: &str = "leased";
const BLOCKED_REASON_RETRY_LATER: &str = "retryLater";
const DEFAULT_LEASE_MINUTES: i64 = 15;
const TARGET_ONLINE_WINDOW_SECONDS: i64 = 90;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TaskFailureClass {
    RetryableChannel,
    RetryableInterruption,
    RetryableOutput,
    TerminalTask,
}

impl TaskFailureClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::RetryableChannel => "retryable_channel",
            Self::RetryableInterruption => "retryable_channel",
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
    pub project_id: Option<&'a str>,
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
    if registration.protocol_version != crate::content::EXECUTION_PROTOCOL_VERSION
        || registration.max_concurrency < 1
    {
        return Err(CliError::with_code(
            "invalid_target",
            "Target protocol version must be 3 and max concurrency must be positive.",
        ));
    }
    let capabilities = normalize_capabilities(registration.capabilities);
    if capabilities.is_empty() {
        return Err(CliError::with_code(
            "invalid_target",
            "Register at least one --capability.",
        ));
    }

    let mut storage = Storage::open(paths)?;
    let project_id = match registration.project_id {
        Some(project_id) => {
            if storage.read_project(project_id)?.is_none() {
                return Err(CliError::with_code(
                    "project_not_found",
                    format!("Project not found: {project_id}"),
                ));
            }
            project_id.to_owned()
        }
        None => read_project_bootstrap(paths, BootstrapRequest::new())?
            .project
            .id,
    };
    if let Some(connection_id) = registration.model_gateway_connection_id {
        crate::model_gateway::sync_builtin_local_cli_registry(&mut storage)?;
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
    let existing_id = storage
        .connection()
        .query_row(
            r#"
            SELECT id FROM agent_targets
            WHERE project_id = ? AND transport = 'command'
              AND (
                (? IS NOT NULL AND model_gateway_connection_id = ?)
                OR (? IS NULL AND name = ?)
              )
            ORDER BY updated_at DESC LIMIT 1
            "#,
            params![
                project_id,
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
              id, project_id, name, host, transport, capabilities_json,
              priority, status, token_hash, last_error, last_heartbeat_at,
              created_at, updated_at, protocol_version, max_concurrency,
              model_gateway_connection_id
            )
            VALUES (?, ?, ?, ?, 'command', ?, ?, 'online', 'command-only', NULL, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
              name = excluded.name,
              host = excluded.host,
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
            project_id,
            registration.name,
            registration.host.unwrap_or(registration.name),
            serde_json::to_string(&capabilities).map_err(to_cli_error)?,
            registration.priority,
            now,
            now,
            now,
            registration.protocol_version,
            registration.max_concurrency,
            registration.model_gateway_connection_id,
        ],
    )
    .map_err(to_cli_error)?;
    crate::storage::record_scope(&tx, "agent_targets", Some(&project_id), None)?;
    let target = read_target_value(&tx, &project_id, &target_id)?
        .ok_or_else(|| CliError::new("Registered target could not be read."))?;
    tx.commit().map_err(to_cli_error)?;
    Ok(json!({ "target": target }))
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
            "UPDATE agent_targets SET status = 'offline', last_error = ?, updated_at = ? WHERE id = ? AND project_id = ? AND transport = 'command' AND (status <> 'offline' OR last_error IS NOT ?)",
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
        "SELECT status, last_error, last_heartbeat_at FROM agent_targets WHERE id = ? AND project_id = ? AND transport = 'command'",
        params![target_id, project_id],
        |row| Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
        )),
    ).optional().map_err(to_cli_error)?;
    let changed = tx.execute(
            "UPDATE agent_targets SET status = 'online', last_error = NULL, last_heartbeat_at = ?, updated_at = ? WHERE id = ? AND project_id = ? AND transport = 'command'",
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
    let storage = Storage::open(paths)?;
    let project_id = storage
        .connection()
        .query_row(
            "SELECT project_id FROM agent_targets WHERE id = ? AND transport = 'command'",
            [target_id],
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
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let interrupted = {
        let mut statement = tx.prepare(
            "SELECT id, queue, workflow_id, status FROM tasks WHERE project_id = ? AND assigned_agent_id = ? AND status IN ('reserved', 'running', 'claimed', 'converting', 'indexing')"
        ).map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![project_id, target_id], |row| {
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
            "DELETE FROM agent_targets WHERE id = ? AND project_id = ? AND transport = 'command'",
            params![target_id, project_id],
        )
        .map_err(to_cli_error)?;
    if changed == 0 {
        return Err(CliError::with_code(
            "target_not_found",
            format!("Agent target not found: {target_id}"),
        ));
    }
    crate::storage::record_scope(&tx, "agent_targets", Some(&project_id), None)?;
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
