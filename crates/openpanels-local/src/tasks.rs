use crate::control::{read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::OpenPanelsPaths;
use serde_json::{json, Value};

pub fn list_tasks(paths: &OpenPanelsPaths) -> Result<Value, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    Ok(json!({
        "tasks": bootstrap.tasks,
        "pendingCount": bootstrap.pending_task_count,
    }))
}

pub fn pending_task_count(tasks: &[Value]) -> usize {
    tasks.iter().filter(|task| is_pending_task(task)).count()
}

pub fn is_pending_task(task: &Value) -> bool {
    matches!(
        task.get("status").and_then(Value::as_str),
        Some("queued" | "failed")
    )
}
