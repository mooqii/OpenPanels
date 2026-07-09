use crate::control::{read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::OpenPanelsPaths;
use serde_json::{json, Value};

const BLOCKED_REASON_ATTEMPTS_EXCEEDED: &str = "attemptsExceeded";
const BLOCKED_REASON_LEASED: &str = "leased";
const BLOCKED_REASON_RETRY_LATER: &str = "retryLater";

#[derive(Debug, Clone, Default)]
pub struct TaskListFilter<'a> {
    pub pending: bool,
    pub queue: Option<&'a str>,
    pub status: Option<&'a str>,
}

pub fn list_tasks(paths: &OpenPanelsPaths, filter: TaskListFilter<'_>) -> Result<Value, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let tasks = sort_tasks_for_display(annotate_tasks(filter_tasks(bootstrap.tasks, &filter)));
    let pending_count = pending_task_count(&tasks);
    let ready_count = ready_task_count(&tasks);
    let blocked_count = blocked_task_count(&tasks);
    Ok(json!({
        "tasks": tasks,
        "pendingCount": pending_count,
        "readyCount": ready_count,
        "blockedCount": blocked_count,
    }))
}

pub fn next_task(paths: &OpenPanelsPaths, filter: TaskListFilter<'_>) -> Result<Value, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let tasks = annotate_tasks(filter_tasks(bootstrap.tasks, &filter));
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

pub fn inspect_task(paths: &OpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let task = annotate_tasks(bootstrap.tasks)
        .into_iter()
        .find(|task| task.get("id").and_then(Value::as_str) == Some(task_id))
        .ok_or_else(|| CliError::new(format!("Project task not found: {task_id}")))?;
    Ok(json!({ "task": task }))
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
