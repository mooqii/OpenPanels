include!("tasks/query_targets.rs");
include!("tasks/leases.rs");
include!("tasks/lifecycle.rs");
include!("tasks/reservation.rs");
include!("tasks/runtime.rs");
include!("tasks/routing.rs");
include!("tasks/projection.rs");
include!("tasks/scopes.rs");
include!("tasks/handoffs.rs");

pub(crate) const TASK_EXECUTION_LIMIT: i64 = 3;

#[derive(Clone, Copy, Eq, PartialEq)]
enum TaskQueueAdapter {
    Wiki,
    Writing,
    Publication,
    Release,
}

fn task_queue_adapter(queue: &str) -> Result<TaskQueueAdapter, CliError> {
    match queue {
        "wiki" => Ok(TaskQueueAdapter::Wiki),
        "writing" => Ok(TaskQueueAdapter::Writing),
        "publication" => Ok(TaskQueueAdapter::Publication),
        "release" => Ok(TaskQueueAdapter::Release),
        queue => Err(CliError::with_code(
            "queue_adapter_missing",
            format!("No task lifecycle adapter is available for queue: {queue}"),
        )),
    }
}

#[cfg(test)]
pub(crate) fn task_queue_has_lifecycle_adapter(queue: &str) -> bool {
    task_queue_adapter(queue).is_ok()
}
