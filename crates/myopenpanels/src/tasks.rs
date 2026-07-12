use crate::control::{read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::MyOpenPanelsPaths;
use crate::storage::Storage;
use hmac::{Hmac, Mac};
use rand::Rng;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::thread;
use std::time::{Duration, Instant};

const BLOCKED_REASON_ATTEMPTS_EXCEEDED: &str = "attemptsExceeded";
const BLOCKED_REASON_LEASED: &str = "leased";
const BLOCKED_REASON_RETRY_LATER: &str = "retryLater";
const DEFAULT_LEASE_MINUTES: i64 = 15;
const DEFAULT_LONG_POLL_MS: u64 = 25_000;
const TARGET_ONLINE_WINDOW_SECONDS: i64 = 90;
const DELIVERY_BACKOFF_SECONDS: [i64; 5] = [2, 10, 30, 120, 600];

#[derive(Debug, Clone)]
pub struct TargetRegistration<'a> {
    pub name: &'a str,
    pub host: Option<&'a str>,
    pub transport: &'a str,
    pub endpoint: Option<&'a str>,
    pub capabilities: Vec<String>,
    pub priority: i64,
}

#[derive(Debug, Clone)]
struct ReservedTask {
    id: String,
    previous_status: String,
    queue: String,
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
    import_legacy_targets(paths, &bootstrap.session.id)?;
    let tasks = annotate_dispatch_state(
        paths,
        &bootstrap.session.id,
        annotate_tasks(filter_tasks(bootstrap.tasks, &filter)),
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
    import_legacy_targets(paths, &bootstrap.session.id)?;
    let tasks = annotate_dispatch_state(
        paths,
        &bootstrap.session.id,
        annotate_tasks(filter_tasks(bootstrap.tasks, &filter)),
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
    let session_id = task_session_id(paths, task_id)?;
    recover_expired_tasks_in_session(paths, &session_id)?;
    inspect_task_in_session(paths, &session_id, task_id)
}

fn inspect_task_in_session(
    paths: &MyOpenPanelsPaths,
    session_id: &str,
    task_id: &str,
) -> Result<Value, CliError> {
    import_legacy_targets(paths, session_id)?;
    let tasks = Storage::open(paths)?.list_project_tasks(session_id)?;
    let task = annotate_dispatch_state(paths, session_id, annotate_tasks(tasks))?
        .into_iter()
        .find(|task| task.get("id").and_then(Value::as_str) == Some(task_id))
        .ok_or_else(|| CliError::new(format!("Project task not found: {task_id}")))?;
    Ok(json!({ "task": task }))
}

fn task_session_id(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<String, CliError> {
    Storage::open(paths)?
        .connection()
        .query_row(
            "SELECT session_id FROM project_tasks WHERE id = ? LIMIT 1",
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
    import_legacy_targets(paths, &bootstrap.session.id)?;
    let targets = read_targets(paths, &bootstrap.session.id)?;
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
    if !matches!(transport, "webhook" | "poll" | "command") {
        return Err(CliError::with_code(
            "invalid_target",
            "Expected target transport to be one of: webhook, poll, command.",
        ));
    }
    if transport == "webhook"
        && registration
            .endpoint
            .is_none_or(|endpoint| endpoint.trim().is_empty())
    {
        return Err(CliError::with_code(
            "invalid_target",
            "Webhook targets require --endpoint <url>.",
        ));
    }
    if transport == "webhook"
        && registration.endpoint.is_some_and(|endpoint| {
            !(endpoint.starts_with("http://") || endpoint.starts_with("https://"))
        })
    {
        return Err(CliError::with_code(
            "invalid_target",
            "Webhook target endpoint must use http:// or https://.",
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
    import_legacy_targets(paths, &bootstrap.session.id)?;
    let mut storage = Storage::open(paths)?;
    let now = crate::control::now_iso();
    let token = random_secret("opt");
    let token_hash = hash_secret(&token);
    let existing_id = storage
        .connection()
        .query_row(
            "SELECT id FROM agent_targets WHERE session_id = ? AND name = ? AND transport = ? ORDER BY updated_at DESC LIMIT 1",
            params![bootstrap.session.id, registration.name, transport],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(to_cli_error)?;
    let target_id = existing_id.unwrap_or_else(|| random_id("agent-target"));
    storage
        .connection_mut()
        .execute(
            r#"
            INSERT INTO agent_targets (
              id, session_id, name, host, transport, endpoint, capabilities_json,
              priority, status, token_hash, last_error, last_heartbeat_at,
              created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'online', ?, NULL, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
              name = excluded.name,
              host = excluded.host,
              transport = excluded.transport,
              endpoint = excluded.endpoint,
              capabilities_json = excluded.capabilities_json,
              priority = excluded.priority,
              status = 'online',
              token_hash = excluded.token_hash,
              last_error = NULL,
              last_heartbeat_at = excluded.last_heartbeat_at,
              updated_at = excluded.updated_at
            "#,
            params![
                target_id,
                bootstrap.session.id,
                registration.name,
                registration.host.unwrap_or(registration.name),
                transport,
                registration.endpoint,
                serde_json::to_string(&capabilities).map_err(to_cli_error)?,
                registration.priority,
                token_hash,
                now,
                now,
                now,
            ],
        )
        .map_err(to_cli_error)?;
    write_target_secret(paths, &target_id, &token)?;
    record_change(
        storage.connection(),
        "agent_target",
        &bootstrap.session.id,
        None,
    )?;
    let target = read_target_value(storage.connection(), &bootstrap.session.id, &target_id)?
        .ok_or_else(|| CliError::new("Registered target could not be read."))?;
    Ok(json!({ "target": target, "token": token }))
}

pub fn heartbeat_target(paths: &MyOpenPanelsPaths, target_id: &str) -> Result<Value, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    heartbeat_target_in_session(paths, &bootstrap.session.id, target_id)
}

fn heartbeat_target_in_session(
    paths: &MyOpenPanelsPaths,
    session_id: &str,
    target_id: &str,
) -> Result<Value, CliError> {
    let storage = Storage::open(paths)?;
    let now = crate::control::now_iso();
    let changed = storage
        .connection()
        .execute(
            "UPDATE agent_targets SET status = 'online', last_error = NULL, last_heartbeat_at = ?, updated_at = ? WHERE id = ? AND session_id = ?",
            params![now, now, target_id, session_id],
        )
        .map_err(to_cli_error)?;
    if changed == 0 {
        return Err(CliError::with_code(
            "target_not_found",
            format!("Agent target not found: {target_id}"),
        ));
    }
    record_change(storage.connection(), "agent_target", session_id, None)?;
    let target = read_target_value(storage.connection(), session_id, target_id)?
        .ok_or_else(|| CliError::new("Agent target could not be read."))?;
    Ok(json!({ "target": target }))
}

pub fn remove_target(paths: &MyOpenPanelsPaths, target_id: &str) -> Result<Value, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let storage = Storage::open(paths)?;
    let changed = storage
        .connection()
        .execute(
            "DELETE FROM agent_targets WHERE id = ? AND session_id = ?",
            params![target_id, bootstrap.session.id],
        )
        .map_err(to_cli_error)?;
    if changed == 0 {
        return Err(CliError::with_code(
            "target_not_found",
            format!("Agent target not found: {target_id}"),
        ));
    }
    let _ = fs::remove_file(target_secret_path(paths, target_id));
    record_change(
        storage.connection(),
        "agent_target",
        &bootstrap.session.id,
        None,
    )?;
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
            "SELECT token_hash FROM agent_targets WHERE id = ? AND session_id = ?",
            params![target_id, bootstrap.session.id],
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
    let session_id = read_project_bootstrap(paths, BootstrapRequest::new())?
        .session
        .id;
    loop {
        if let Some(payload) = claim_once(paths, &session_id, target_id, None, capability, queue)? {
            return Ok(payload);
        }
        if started.elapsed() >= Duration::from_millis(wait_ms) {
            return Ok(json!({ "task": Value::Null, "leaseToken": Value::Null }));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

pub fn claim_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    target_id: &str,
) -> Result<Value, CliError> {
    let session_id = task_session_id(paths, task_id)?;
    claim_once(paths, &session_id, target_id, Some(task_id), None, None)?.ok_or_else(|| {
        CliError::with_code(
            "task_not_claimable",
            format!("Project task is not claimable: {task_id}"),
        )
    })
}

fn claim_once(
    paths: &MyOpenPanelsPaths,
    session_id: &str,
    target_id: &str,
    task_id: Option<&str>,
    requested_capability: Option<&str>,
    requested_queue: Option<&str>,
) -> Result<Option<Value>, CliError> {
    recover_expired_tasks_in_session(paths, session_id)?;
    import_legacy_targets(paths, session_id)?;
    heartbeat_target_in_session(paths, session_id, target_id)?;
    let mut storage = Storage::open(paths)?;
    let target =
        read_target_value(storage.connection(), session_id, target_id)?.ok_or_else(|| {
            CliError::with_code(
                "target_not_found",
                format!("Agent target not found: {target_id}"),
            )
        })?;
    let reserved = reserve_task(
        &mut storage,
        session_id,
        &target,
        task_id,
        requested_capability,
        requested_queue,
    )?;
    let Some(reserved) = reserved else {
        return Ok(None);
    };

    let claim_result = match reserved.queue.as_str() {
        "wiki" => crate::wiki::claim_task(
            paths,
            &reserved.id,
            target.get("host").and_then(Value::as_str),
            Some(target_id),
        ),
        queue => Err(CliError::with_code(
            "queue_adapter_missing",
            format!("No task lifecycle adapter is available for queue: {queue}"),
        )),
    };
    let claimed = match claim_result {
        Ok(claimed) => claimed,
        Err(error) => {
            release_reservation(paths, session_id, &reserved)?;
            return Err(error);
        }
    };
    let claimed_status = claimed["task"]["status"].as_str().unwrap_or("running");
    let claimed_attempt = claimed["task"]["attempt"].as_i64().unwrap_or(1);

    let lease_token = random_secret("lease");
    let lease_expires_at = lease_expires_at();
    let storage = Storage::open(paths)?;
    storage
        .connection()
        .execute(
            r#"
            UPDATE project_tasks
            SET status = ?, attempts = ?, assigned_target_id = ?, lease_owner = ?, lease_token_hash = ?,
                lease_expires_at = ?, last_heartbeat_at = ?, retry_after = NULL,
                error_json = NULL, updated_at = ?
            WHERE id = ? AND session_id = ?
            "#,
            params![
                claimed_status,
                claimed_attempt,
                target_id,
                target_id,
                hash_secret(&lease_token),
                lease_expires_at,
                crate::control::now_iso(),
                crate::control::now_iso(),
                reserved.id,
                session_id,
            ],
        )
        .map_err(to_cli_error)?;
    storage
        .connection()
        .execute(
            "UPDATE task_deliveries SET status = 'acknowledged', acknowledged_at = ?, updated_at = ? WHERE task_id = ? AND target_id = ?",
            params![crate::control::now_iso(), crate::control::now_iso(), reserved.id, target_id],
        )
        .map_err(to_cli_error)?;
    record_change(storage.connection(), "task", session_id, None)?;
    let mut payload = inspect_task(paths, &reserved.id)?;
    payload["leaseToken"] = json!(lease_token);
    payload["target"] = target;
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
        queue => {
            return Err(CliError::with_code(
                "queue_adapter_missing",
                format!("No task lifecycle adapter is available for queue: {queue}"),
            ))
        }
    };
    let storage = Storage::open(paths)?;
    let now = crate::control::now_iso();
    storage
        .connection()
        .execute(
            "UPDATE project_tasks SET lease_expires_at = ?, last_heartbeat_at = ?, updated_at = ? WHERE id = ? AND lease_token_hash = ?",
            params![expires_at, now, now, task_id, hash_secret(lease_token)],
        )
        .map_err(to_cli_error)?;
    let session_id = lease["sessionId"].as_str().unwrap_or_default();
    record_change(storage.connection(), "task", session_id, None)?;
    inspect_task_in_session(paths, session_id, task_id)
}

pub fn complete_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
    result: Option<Value>,
) -> Result<Value, CliError> {
    let lease = verify_lease(paths, task_id, lease_token)?;
    match lease["queue"].as_str().unwrap_or("") {
        "wiki" => crate::wiki::complete_task(paths, task_id, result.clone())?,
        queue => {
            return Err(CliError::with_code(
                "queue_adapter_missing",
                format!("No task lifecycle adapter is available for queue: {queue}"),
            ))
        }
    };
    let session_id = lease["sessionId"].as_str().unwrap_or_default();
    finalize_task_runtime(paths, session_id, task_id, "succeeded", result, None, None)?;
    inspect_task_in_session(paths, session_id, task_id)
}

pub fn fail_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
    message: &str,
    retry_after: Option<&str>,
) -> Result<Value, CliError> {
    let lease = verify_lease(paths, task_id, lease_token)?;
    if retry_after.is_some_and(|value| chrono::DateTime::parse_from_rfc3339(value).is_err()) {
        return Err(CliError::with_code(
            "invalid_retry_after",
            "Expected --retry-after to be an RFC 3339 timestamp.",
        ));
    }
    let retry_after = retry_after
        .map(str::to_owned)
        .unwrap_or_else(|| execution_retry_after(lease["attempt"].as_i64().unwrap_or(1)));
    match lease["queue"].as_str().unwrap_or("") {
        "wiki" => crate::wiki::fail_task_with_retry(paths, task_id, message, Some(&retry_after))?,
        queue => {
            return Err(CliError::with_code(
                "queue_adapter_missing",
                format!("No task lifecycle adapter is available for queue: {queue}"),
            ))
        }
    };
    finalize_task_runtime(
        paths,
        lease["sessionId"].as_str().unwrap_or_default(),
        task_id,
        "failed",
        None,
        Some(json!(message)),
        Some(&retry_after),
    )?;
    reset_task_delivery(
        paths,
        task_id,
        lease["targetId"].as_str(),
        Some(&retry_after),
        Some(message),
    )?;
    inspect_task_in_session(
        paths,
        lease["sessionId"].as_str().unwrap_or_default(),
        task_id,
    )
}

pub fn release_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
) -> Result<Value, CliError> {
    let lease = verify_lease(paths, task_id, lease_token)?;
    match lease["queue"].as_str().unwrap_or("") {
        "wiki" => crate::wiki::release_task(paths, task_id)?,
        queue => {
            return Err(CliError::with_code(
                "queue_adapter_missing",
                format!("No task lifecycle adapter is available for queue: {queue}"),
            ))
        }
    };
    finalize_task_runtime(
        paths,
        lease["sessionId"].as_str().unwrap_or_default(),
        task_id,
        "queued",
        None,
        None,
        None,
    )?;
    reset_task_delivery(
        paths,
        task_id,
        lease["targetId"].as_str(),
        None,
        Some("Task released."),
    )?;
    inspect_task_in_session(
        paths,
        lease["sessionId"].as_str().unwrap_or_default(),
        task_id,
    )
}

pub fn retry_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let task = inspect_task(paths, task_id)?;
    let session_id = task["task"]["sessionId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    match task["task"]["queue"].as_str().unwrap_or("") {
        "wiki" => crate::wiki::retry_task(paths, task_id)?,
        queue => {
            return Err(CliError::with_code(
                "queue_adapter_missing",
                format!("No task lifecycle adapter is available for queue: {queue}"),
            ))
        }
    };
    finalize_task_runtime(paths, &session_id, task_id, "queued", None, None, None)?;
    reset_task_delivery(paths, task_id, None, None, None)?;
    inspect_task_in_session(paths, &session_id, task_id)
}

pub fn cancel_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let task = inspect_task(paths, task_id)?;
    let session_id = task["task"]["sessionId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    match task["task"]["queue"].as_str().unwrap_or("") {
        "wiki" => crate::wiki::cancel_task(paths, task_id)?,
        queue => {
            return Err(CliError::with_code(
                "queue_adapter_missing",
                format!("No task lifecycle adapter is available for queue: {queue}"),
            ))
        }
    };
    finalize_task_runtime(paths, &session_id, task_id, "cancelled", None, None, None)?;
    inspect_task_in_session(paths, &session_id, task_id)
}

pub fn list_deliveries(
    paths: &MyOpenPanelsPaths,
    task_id: Option<&str>,
) -> Result<Value, CliError> {
    let session_id = match task_id {
        Some(task_id) => task_session_id(paths, task_id)?,
        None => {
            read_project_bootstrap(paths, BootstrapRequest::new())?
                .session
                .id
        }
    };
    let storage = Storage::open(paths)?;
    let mut statement = storage
        .connection()
        .prepare(
            r#"
            SELECT d.id, d.task_id, d.target_id, d.status, d.attempts,
                   d.next_attempt_at, d.last_error, d.delivered_at,
                   d.acknowledged_at, d.created_at, d.updated_at
            FROM task_deliveries d
            JOIN project_tasks t ON t.id = d.task_id
            WHERE t.session_id = ? AND (? IS NULL OR d.task_id = ?)
            ORDER BY d.updated_at DESC
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map(params![session_id, task_id, task_id], delivery_from_row)
        .map_err(to_cli_error)?;
    let deliveries = rows
        .map(|row| row.map_err(to_cli_error))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({ "deliveries": deliveries }))
}

fn reserve_task(
    storage: &mut Storage,
    session_id: &str,
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
    let targets = read_targets_from_connection(&tx, session_id)?;
    tx.execute(
        r#"
        UPDATE project_tasks
        SET status = CASE WHEN status = 'reserved' THEN 'queued' ELSE status END,
            assigned_target_id = CASE WHEN status = 'reserved' THEN NULL ELSE assigned_target_id END
        WHERE session_id = ? AND status = 'reserved' AND updated_at < ?
        "#,
        params![
            session_id,
            (chrono::Utc::now() - chrono::Duration::seconds(30))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        ],
    )
    .map_err(to_cli_error)?;
    let candidates = {
        let mut statement = tx
            .prepare(
                r#"
                SELECT id, queue, status, capability
                FROM project_tasks
                WHERE session_id = ?
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
                    session_id,
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
                    ))
                },
            )
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    let target_id = target.get("id").and_then(Value::as_str);
    let candidate = candidates.into_iter().find(|(_, _, _, capability)| {
        capability_matches_any(&capabilities, capability)
            && matching_targets(&targets, capability)
                .first()
                .and_then(|preferred| preferred.get("id"))
                .and_then(Value::as_str)
                == target_id
    });
    let Some((id, queue, previous_status, _)) = candidate else {
        tx.commit().map_err(to_cli_error)?;
        return Ok(None);
    };
    let changed = tx
        .execute(
            "UPDATE project_tasks SET status = 'reserved', assigned_target_id = ?, updated_at = ? WHERE id = ? AND status = ?",
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
    }))
}

fn release_reservation(
    paths: &MyOpenPanelsPaths,
    session_id: &str,
    reserved: &ReservedTask,
) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    storage
        .connection()
        .execute(
            "UPDATE project_tasks SET status = ?, assigned_target_id = NULL, updated_at = ? WHERE id = ? AND session_id = ? AND status = 'reserved'",
            params![reserved.previous_status, crate::control::now_iso(), reserved.id, session_id],
        )
        .map_err(to_cli_error)?;
    Ok(())
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
            SELECT queue, attempts, assigned_target_id, lease_token_hash, lease_expires_at,
                   session_id, panel_id
            FROM project_tasks
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
                    "sessionId": row.get::<_, String>(5)?,
                    "panelId": row.get::<_, String>(6)?,
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

fn finalize_task_runtime(
    paths: &MyOpenPanelsPaths,
    session_id: &str,
    task_id: &str,
    status: &str,
    result: Option<Value>,
    error: Option<Value>,
    retry_after: Option<&str>,
) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    let now = crate::control::now_iso();
    let result_json = result
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(to_cli_error)?;
    let error_json = error
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(to_cli_error)?;
    storage
        .connection()
        .execute(
            r#"
            UPDATE project_tasks
            SET status = ?, assigned_target_id = NULL, lease_owner = NULL,
                lease_expires_at = NULL, last_heartbeat_at = NULL,
                lease_token_hash = NULL, retry_after = ?, result_json = ?,
                error_json = ?, completed_at = ?, updated_at = ?
            WHERE id = ? AND session_id = ?
            "#,
            params![
                status,
                retry_after,
                result_json,
                error_json,
                if matches!(status, "succeeded" | "cancelled") {
                    Some(now.clone())
                } else {
                    None
                },
                now,
                task_id,
                session_id,
            ],
        )
        .map_err(to_cli_error)?;
    record_change(storage.connection(), "task", session_id, None)?;
    Ok(())
}

fn reset_task_delivery(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    target_id: Option<&str>,
    next_attempt_at: Option<&str>,
    error: Option<&str>,
) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    storage
        .connection()
        .execute(
            r#"
            UPDATE task_deliveries
            SET status = 'pending', attempts = 0, next_attempt_at = ?,
                last_error = ?, acknowledged_at = NULL, updated_at = ?
            WHERE task_id = ? AND (? IS NULL OR target_id = ?)
            "#,
            params![
                next_attempt_at,
                error,
                crate::control::now_iso(),
                task_id,
                target_id,
                target_id,
            ],
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
    recover_expired_tasks_in_session(paths, &bootstrap.session.id)
}

fn recover_expired_tasks_in_session(
    paths: &MyOpenPanelsPaths,
    session_id: &str,
) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    let now = crate::control::now_iso();
    storage
        .connection()
        .execute(
            "UPDATE project_tasks SET status = 'queued', assigned_target_id = NULL, updated_at = ? WHERE session_id = ? AND status = 'reserved' AND updated_at < ?",
            params![
                now,
                session_id,
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
                SELECT id, queue, attempts, assigned_target_id
                FROM project_tasks
                WHERE session_id = ?
                  AND status IN ('running', 'claimed', 'converting', 'indexing')
                  AND lease_expires_at IS NOT NULL
                  AND lease_expires_at <= ?
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![session_id, now], |row| {
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
        let retry_after = execution_retry_after(attempt.max(1));
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
            session_id,
            &task_id,
            "failed",
            None,
            Some(json!("Task lease expired.")),
            Some(&retry_after),
        )?;
        reset_task_delivery(
            paths,
            &task_id,
            target_id.as_deref(),
            Some(&retry_after),
            Some("Task lease expired."),
        )?;
    }
    Ok(())
}

fn annotate_dispatch_state(
    paths: &MyOpenPanelsPaths,
    session_id: &str,
    tasks: Vec<Value>,
) -> Result<Vec<Value>, CliError> {
    let targets = read_targets(paths, session_id)?;
    let storage = Storage::open(paths)?;
    let mut output = Vec::with_capacity(tasks.len());
    for mut task in tasks {
        let capability = task.get("capability").and_then(Value::as_str).unwrap_or("");
        let matching = matching_targets(&targets, capability);
        let assigned_target = task
            .get("assignedTargetId")
            .and_then(Value::as_str)
            .and_then(|id| targets.iter().find(|target| target["id"] == id))
            .cloned();
        let last_delivery =
            read_last_delivery(storage.connection(), task["id"].as_str().unwrap_or(""))?;
        let dispatch_state = if is_active_task(&task) {
            "running"
        } else if !is_pending_task(&task) {
            "done"
        } else if matching.is_empty() {
            "noTarget"
        } else {
            match last_delivery
                .as_ref()
                .and_then(|delivery| delivery.get("status"))
                .and_then(Value::as_str)
            {
                Some("failed") => "retry",
                Some("sent") => "delivering",
                Some("exhausted") => "deliveryFailed",
                _ => "eligible",
            }
        };
        if let Some(object) = task.as_object_mut() {
            object.insert("dispatchState".to_owned(), json!(dispatch_state));
            object.insert("matchedTargetCount".to_owned(), json!(matching.len()));
            object.insert(
                "assignedTarget".to_owned(),
                assigned_target.unwrap_or(Value::Null),
            );
            object.insert(
                "lastDelivery".to_owned(),
                last_delivery.unwrap_or(Value::Null),
            );
        }
        output.push(task);
    }
    Ok(output)
}

fn read_targets(paths: &MyOpenPanelsPaths, session_id: &str) -> Result<Vec<Value>, CliError> {
    let storage = Storage::open(paths)?;
    read_targets_from_connection(storage.connection(), session_id)
}

fn read_targets_from_connection(
    connection: &rusqlite::Connection,
    session_id: &str,
) -> Result<Vec<Value>, CliError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT id, name, host, transport, endpoint, capabilities_json,
                   priority, status, last_error, last_heartbeat_at, created_at, updated_at
            FROM agent_targets
            WHERE session_id = ?
            ORDER BY priority DESC, last_heartbeat_at DESC, id ASC
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map(params![session_id], target_from_row)
        .map_err(to_cli_error)?;
    rows.map(|row| row.map(compute_target_status).map_err(to_cli_error))
        .collect()
}

fn read_target_value(
    connection: &rusqlite::Connection,
    session_id: &str,
    target_id: &str,
) -> Result<Option<Value>, CliError> {
    connection
        .query_row(
            r#"
            SELECT id, name, host, transport, endpoint, capabilities_json,
                   priority, status, last_error, last_heartbeat_at, created_at, updated_at
            FROM agent_targets
            WHERE session_id = ? AND id = ?
            "#,
            params![session_id, target_id],
            target_from_row,
        )
        .optional()
        .map(|target| target.map(compute_target_status))
        .map_err(to_cli_error)
}

fn target_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let capabilities_json = row.get::<_, String>(5)?;
    let capabilities =
        serde_json::from_str::<Value>(&capabilities_json).unwrap_or_else(|_| json!([]));
    Ok(json!({
        "id": row.get::<_, String>(0)?,
        "name": row.get::<_, String>(1)?,
        "host": row.get::<_, String>(2)?,
        "transport": row.get::<_, String>(3)?,
        "endpoint": row.get::<_, Option<String>>(4)?,
        "capabilities": capabilities,
        "priority": row.get::<_, i64>(6)?,
        "status": row.get::<_, String>(7)?,
        "lastError": row.get::<_, Option<String>>(8)?,
        "lastHeartbeatAt": row.get::<_, String>(9)?,
        "createdAt": row.get::<_, String>(10)?,
        "updatedAt": row.get::<_, String>(11)?,
    }))
}

fn compute_target_status(mut target: Value) -> Value {
    let transport = target
        .get("transport")
        .and_then(Value::as_str)
        .unwrap_or("");
    let stale = matches!(transport, "poll" | "command")
        && target
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

fn read_last_delivery(
    connection: &rusqlite::Connection,
    task_id: &str,
) -> Result<Option<Value>, CliError> {
    connection
        .query_row(
            r#"
            SELECT id, task_id, target_id, status, attempts, next_attempt_at,
                   last_error, delivered_at, acknowledged_at, created_at, updated_at
            FROM task_deliveries
            WHERE task_id = ?
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
            params![task_id],
            delivery_from_row,
        )
        .optional()
        .map_err(to_cli_error)
}

fn delivery_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": row.get::<_, String>(0)?,
        "taskId": row.get::<_, String>(1)?,
        "targetId": row.get::<_, String>(2)?,
        "status": row.get::<_, String>(3)?,
        "attempts": row.get::<_, i64>(4)?,
        "nextAttemptAt": row.get::<_, Option<String>>(5)?,
        "lastError": row.get::<_, Option<String>>(6)?,
        "deliveredAt": row.get::<_, Option<String>>(7)?,
        "acknowledgedAt": row.get::<_, Option<String>>(8)?,
        "createdAt": row.get::<_, String>(9)?,
        "updatedAt": row.get::<_, String>(10)?,
    }))
}

pub fn start_dispatcher_loop(paths: MyOpenPanelsPaths, server_url: String) {
    thread::spawn(move || loop {
        if let Err(error) = dispatch_webhooks_once(&paths, &server_url) {
            crate::trace::record(crate::trace::TraceEventInput {
                audience: None,
                category: Some("error".to_owned()),
                detail: Some(json!({ "error": error.message() })),
                direction: Some("dispatch".to_owned()),
                release_summary: Some("Task dispatcher error".to_owned()),
                run_id: None,
                source: Some("task-dispatcher".to_owned()),
                summary: Some(format!("Task dispatcher error: {}", error.message())),
                task_id: None,
            });
        }
        thread::sleep(Duration::from_secs(2));
    });
}

pub fn dispatch_webhooks_once(
    paths: &MyOpenPanelsPaths,
    server_url: &str,
) -> Result<Value, CliError> {
    recover_expired_tasks(paths)?;
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    import_legacy_targets(paths, &bootstrap.session.id)?;
    let targets = read_targets(paths, &bootstrap.session.id)?;
    let tasks = annotate_tasks(bootstrap.tasks);
    let mut delivered = 0usize;
    for task in tasks
        .iter()
        .filter(|task| task.get("ready").and_then(Value::as_bool) == Some(true))
    {
        let capability = task.get("capability").and_then(Value::as_str).unwrap_or("");
        let candidates = matching_targets(&targets, capability)
            .into_iter()
            .filter(|target| target.get("transport").and_then(Value::as_str) == Some("webhook"))
            .collect::<Vec<_>>();
        for target in candidates {
            let (due, exhausted) = delivery_schedule(paths, task, target)?;
            if exhausted {
                continue;
            }
            if due {
                deliver_webhook(paths, &bootstrap.session.id, server_url, task, target)?;
                delivered += 1;
            }
            break;
        }
    }
    Ok(json!({ "delivered": delivered }))
}

pub fn dispatcher_status(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let targets_payload = list_targets(paths)?;
    let task_payload = list_tasks(paths, TaskListFilter::default())?;
    let targets = targets_payload["targets"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let tasks = task_payload["tasks"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let running_count = task_payload["runningCount"].as_u64().unwrap_or(0);
    let unhandled_count = task_payload["unhandledCount"].as_u64().unwrap_or(0);
    let retry_count = tasks
        .iter()
        .filter(|task| task.get("dispatchState").and_then(Value::as_str) == Some("retry"))
        .count();
    let delivery_error = tasks.iter().find_map(|task| {
        task.get("lastDelivery")
            .filter(|delivery| {
                matches!(
                    delivery.get("status").and_then(Value::as_str),
                    Some("failed" | "exhausted")
                )
            })
            .cloned()
    });
    let status = if running_count > 0 {
        "running"
    } else if retry_count > 0 {
        "retry"
    } else if unhandled_count > 0 {
        "noTarget"
    } else if delivery_error.is_some() {
        "error"
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
        "retryCount": retry_count,
        "lastDelivery": delivery_error,
        "targets": targets,
        "updatedAt": crate::control::now_iso(),
    }))
}

fn delivery_schedule(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    target: &Value,
) -> Result<(bool, bool), CliError> {
    let storage = Storage::open(paths)?;
    let delivery = storage
        .connection()
        .query_row(
            "SELECT status, attempts, next_attempt_at FROM task_deliveries WHERE task_id = ? AND target_id = ?",
            params![task["id"].as_str(), target["id"].as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(to_cli_error)?;
    let Some((status, attempts, next_attempt_at)) = delivery else {
        return Ok((true, false));
    };
    if status == "acknowledged" {
        return Ok((false, false));
    }
    if status == "exhausted" || attempts >= DELIVERY_BACKOFF_SECONDS.len() as i64 {
        return Ok((false, true));
    }
    Ok((
        next_attempt_at
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .is_none_or(|value| value.with_timezone(&chrono::Utc) <= chrono::Utc::now()),
        false,
    ))
}

fn deliver_webhook(
    paths: &MyOpenPanelsPaths,
    session_id: &str,
    server_url: &str,
    task: &Value,
    target: &Value,
) -> Result<(), CliError> {
    let task_id = task.get("id").and_then(Value::as_str).unwrap_or("");
    let target_id = target.get("id").and_then(Value::as_str).unwrap_or("");
    let endpoint = target
        .get("endpoint")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::new("Webhook target endpoint is missing."))?;
    let body = json!({
        "protocolVersion": 1,
        "taskId": task_id,
        "queue": task["queue"],
        "capability": task["capability"],
        "claimUrl": format!("{}/api/tasks/{}/claim", server_url.trim_end_matches('/'), task_id),
        "targetId": target_id,
    });
    let body_text = serde_json::to_string(&body).map_err(to_cli_error)?;
    let secret = read_target_secret(paths, target_id)?;
    let signature = sign_payload(&secret, body_text.as_bytes())?;
    let response = ureq::post(endpoint)
        .set("content-type", "application/json")
        .set("x-myopenpanels-target-id", target_id)
        .set("x-myopenpanels-signature", &format!("sha256={signature}"))
        .timeout(Duration::from_secs(10))
        .send_string(&body_text);
    let (success, error) = match response {
        Ok(response) if (200..300).contains(&response.status()) => (true, None),
        Ok(response) => (
            false,
            Some(format!("Webhook returned HTTP {}.", response.status())),
        ),
        Err(error) => (false, Some(error.to_string())),
    };
    record_delivery_attempt(
        paths,
        session_id,
        task_id,
        target_id,
        success,
        error.as_deref(),
    )?;
    crate::trace::record(crate::trace::TraceEventInput {
        audience: None,
        category: Some(if success { "task" } else { "error" }.to_owned()),
        detail: Some(json!({
            "targetId": target_id,
            "endpoint": endpoint,
            "success": success,
            "error": error,
        })),
        direction: Some("push".to_owned()),
        release_summary: Some(if success {
            "Task notification sent".to_owned()
        } else {
            "Task notification failed".to_owned()
        }),
        run_id: None,
        source: Some("task-dispatcher".to_owned()),
        summary: Some(if success {
            format!("Sent task {task_id} to {target_id}")
        } else {
            format!("Failed to send task {task_id} to {target_id}")
        }),
        task_id: Some(task_id.to_owned()),
    });
    Ok(())
}

fn record_delivery_attempt(
    paths: &MyOpenPanelsPaths,
    session_id: &str,
    task_id: &str,
    target_id: &str,
    success: bool,
    error: Option<&str>,
) -> Result<(), CliError> {
    let mut storage = Storage::open(paths)?;
    let now = crate::control::now_iso();
    let tx = storage
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(to_cli_error)?;
    let existing = tx
        .query_row(
            "SELECT id, attempts FROM task_deliveries WHERE task_id = ? AND target_id = ?",
            params![task_id, target_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(to_cli_error)?;
    let (delivery_id, previous_attempts) = existing.unwrap_or_else(|| (random_id("delivery"), 0));
    let attempts = previous_attempts + 1;
    let exhausted = attempts >= DELIVERY_BACKOFF_SECONDS.len() as i64;
    let status = if exhausted {
        "exhausted"
    } else if success {
        "sent"
    } else {
        "failed"
    };
    let next_attempt_at = if success {
        Some(
            (chrono::Utc::now() + chrono::Duration::seconds(30))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        )
    } else if exhausted {
        None
    } else {
        let seconds = DELIVERY_BACKOFF_SECONDS[(attempts - 1) as usize];
        Some(
            (chrono::Utc::now() + chrono::Duration::seconds(seconds))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        )
    };
    tx.execute(
        r#"
        INSERT INTO task_deliveries (
          id, task_id, target_id, status, attempts, next_attempt_at,
          last_error, delivered_at, acknowledged_at, created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?)
        ON CONFLICT(task_id, target_id) DO UPDATE SET
          status = excluded.status,
          attempts = excluded.attempts,
          next_attempt_at = excluded.next_attempt_at,
          last_error = excluded.last_error,
          delivered_at = excluded.delivered_at,
          updated_at = excluded.updated_at
        "#,
        params![
            delivery_id,
            task_id,
            target_id,
            status,
            attempts,
            next_attempt_at,
            error,
            if success { Some(now.clone()) } else { None },
            now,
            now,
        ],
    )
    .map_err(to_cli_error)?;
    tx.execute(
        "INSERT INTO task_delivery_attempts (delivery_id, attempt, status, error, created_at) VALUES (?, ?, ?, ?, ?)",
        params![delivery_id, attempts, if success { "sent" } else { "failed" }, error, now],
    )
    .map_err(to_cli_error)?;
    tx.execute(
        "INSERT INTO storage_changes (kind, session_id, panel_id, created_at) VALUES ('task_delivery', ?, NULL, ?)",
        params![session_id, now],
    )
    .map_err(to_cli_error)?;
    tx.commit().map_err(to_cli_error)
}

fn import_legacy_targets(paths: &MyOpenPanelsPaths, session_id: &str) -> Result<(), CliError> {
    let path = paths.context_dir.join("agent-targets.json");
    if !path.exists() {
        return Ok(());
    }
    let raw = fs::read_to_string(path).map_err(to_cli_error)?;
    let targets = serde_json::from_str::<Value>(&raw)
        .ok()
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    if targets.is_empty() {
        return Ok(());
    }
    let storage = Storage::open(paths)?;
    for target in targets {
        let target_id = target
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| random_id("agent-target"));
        let exists = storage
            .connection()
            .query_row(
                "SELECT 1 FROM agent_targets WHERE id = ?",
                params![target_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(to_cli_error)?
            .is_some();
        if exists {
            continue;
        }
        let wake_url = target.get("wakeUrl").and_then(Value::as_str);
        let now = target
            .get("updatedAt")
            .and_then(Value::as_str)
            .unwrap_or_else(|| {
                target
                    .get("createdAt")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            });
        let now = if now.is_empty() {
            crate::control::now_iso()
        } else {
            now.to_owned()
        };
        let token = random_secret("opt");
        storage
            .connection()
            .execute(
                r#"
                INSERT OR IGNORE INTO agent_targets (
                  id, session_id, name, host, transport, endpoint, capabilities_json,
                  priority, status, token_hash, last_error, last_heartbeat_at,
                  created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, 0, 'online', ?, NULL, ?, ?, ?)
                "#,
                params![
                    target_id,
                    session_id,
                    target
                        .get("threadId")
                        .and_then(Value::as_str)
                        .unwrap_or("legacy-agent"),
                    target
                        .get("host")
                        .and_then(Value::as_str)
                        .unwrap_or("legacy"),
                    if wake_url.is_some() {
                        "webhook"
                    } else {
                        "poll"
                    },
                    wake_url,
                    serde_json::to_string(&vec![
                        "wiki.convertDocument",
                        "wiki.ingestMarkdown",
                        "wiki.rebuildIndex",
                    ])
                    .map_err(to_cli_error)?,
                    hash_secret(&token),
                    now,
                    now,
                    now,
                ],
            )
            .map_err(to_cli_error)?;
        write_target_secret(paths, &target_id, &token)?;
    }
    Ok(())
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

fn sign_payload(secret: &str, payload: &[u8]) -> Result<String, CliError> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(to_cli_error)?;
    mac.update(payload);
    Ok(format!("{:x}", mac.finalize().into_bytes()))
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

fn read_target_secret(paths: &MyOpenPanelsPaths, target_id: &str) -> Result<String, CliError> {
    fs::read_to_string(target_secret_path(paths, target_id))
        .map(|value| value.trim().to_owned())
        .map_err(to_cli_error)
}

fn record_change(
    connection: &rusqlite::Connection,
    kind: &str,
    session_id: &str,
    panel_id: Option<&str>,
) -> Result<(), CliError> {
    connection
        .execute(
            "INSERT INTO storage_changes (kind, session_id, panel_id, created_at) VALUES (?, ?, ?, ?)",
            params![kind, session_id, panel_id, crate::control::now_iso()],
        )
        .map_err(to_cli_error)?;
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
        Some("queued" | "failed")
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
    }
    task
}

fn task_execution_state(task: &Value) -> (bool, Option<&'static str>, Option<String>) {
    if !is_pending_task(task) {
        return (false, None, None);
    }

    if task.get("status").and_then(Value::as_str) == Some("failed")
        && task.get("attempt").and_then(Value::as_i64).unwrap_or(0)
            >= task.get("maxAttempts").and_then(Value::as_i64).unwrap_or(3)
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
