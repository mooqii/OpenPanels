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
    crate::ids::random_id(prefix)
}

fn random_secret(prefix: &str) -> String {
    let first: u128 = rand::rng().random();
    let second: u128 = rand::rng().random();
    format!("{prefix}_{first:032x}{second:032x}")
}

fn hash_secret(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
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
    task.get("status").and_then(Value::as_str) == Some("queued")
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
        "queued" => "ready",
        "running" => "leased",
        "failed" => "failed_terminal",
        "succeeded" => "succeeded",
        "cancelled" => "cancelled",
        "superseded" => "superseded",
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

    if task.get("attempt").and_then(Value::as_i64).unwrap_or(0)
        >= TASK_EXECUTION_LIMIT
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
        (true, "queued") => 0,
        (false, "queued") => 1,
        _ => 4,
    }
}
