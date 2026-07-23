use crate::control::{read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::MyOpenPanelsPaths;
use crate::storage::Storage;
use rand::Rng;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const BLOCKED_REASON_ATTEMPTS_EXCEEDED: &str = "attemptsExceeded";
const BLOCKED_REASON_LEASED: &str = "leased";
const BLOCKED_REASON_RETRY_LATER: &str = "retryLater";
const DEFAULT_LEASE_MINUTES: i64 = 15;

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
pub(crate) struct TargetRegistration<'a> {
    pub name: &'a str,
    pub host: Option<&'a str>,
    pub project_id: Option<&'a str>,
    pub capabilities: Vec<String>,
    pub priority: i64,
    pub max_concurrency: i64,
    pub model_gateway_connection_id: Option<&'a str>,
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
        Some("running")
    )
}

pub(crate) fn list_targets(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let targets = crate::model_gateway::worker_specs(paths)?
        .into_iter()
        .map(|spec| json!({
            "id": format!("agent-cli:{}", spec.provider_id),
            "name": spec.provider_name,
            "host": spec.host,
            "status": "online",
            "providerId": spec.provider_id,
            "modelGatewayConnectionId": spec.connection_id,
        }))
        .collect::<Vec<_>>();
    Ok(json!({ "onlineCount": targets.len(), "targets": targets }))
}

pub(crate) fn register_target(
    paths: &MyOpenPanelsPaths,
    registration: TargetRegistration<'_>,
) -> Result<Value, CliError> {
    if registration.max_concurrency < 1 {
        return Err(CliError::with_code(
            "invalid_target",
            "Target max concurrency must be positive.",
        ));
    }
    let capabilities = normalize_capabilities(registration.capabilities);
    if capabilities.is_empty() {
        return Err(CliError::with_code(
            "invalid_target",
            "Register at least one --capability.",
        ));
    }

    let storage = Storage::open(paths)?;
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
    let provider_id = registration
        .model_gateway_connection_id
        .and_then(|value| value.strip_prefix("local-cli:"))
        .unwrap_or(registration.name);
    let target_id = format!("agent-cli:{provider_id}");
    let target = json!({
        "id": target_id,
        "projectId": project_id,
        "name": registration.name,
        "host": registration.host.unwrap_or(registration.name),
        "capabilities": capabilities,
        "priority": registration.priority,
        "maxConcurrency": registration.max_concurrency,
        "modelGatewayConnectionId": registration.model_gateway_connection_id,
        "status": "online",
    });
    Ok(json!({ "target": target }))
}

pub(crate) fn heartbeat_target(
    paths: &MyOpenPanelsPaths,
    target_id: &str,
) -> Result<Value, CliError> {
    let project_id = read_project_bootstrap(paths, BootstrapRequest::new())?
        .project
        .id;
    heartbeat_target_in_session(paths, &project_id, target_id)
}

fn heartbeat_target_in_session(
    _paths: &MyOpenPanelsPaths,
    project_id: &str,
    target_id: &str,
) -> Result<Value, CliError> {
    Ok(json!({ "target": { "id": target_id, "projectId": project_id, "status": "online" } }))
}

pub(crate) fn deactivate_target(
    paths: &MyOpenPanelsPaths,
    target_id: &str,
    reason: &str,
) -> Result<Value, CliError> {
    let project_id = read_project_bootstrap(paths, BootstrapRequest::new())?
        .project
        .id;
    Ok(json!({ "target": { "id": target_id, "projectId": project_id, "status": "offline", "lastError": reason } }))
}

pub(crate) fn remove_target(
    paths: &MyOpenPanelsPaths,
    target_id: &str,
) -> Result<Value, CliError> {
    let _ = paths;
    Ok(json!({ "removed": true, "targetId": target_id }))
}
