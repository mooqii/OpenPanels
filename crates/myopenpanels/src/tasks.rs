include!("tasks/query_targets.rs");
include!("tasks/leases.rs");
include!("tasks/dispatch.rs");
include!("tasks/reservation.rs");
include!("tasks/workflow_runs.rs");
include!("tasks/routing.rs");
include!("tasks/projection.rs");
include!("tasks/scopes.rs");
include!("tasks/handoffs.rs");

#[derive(Clone, Copy, Eq, PartialEq)]
enum TaskQueueAdapter {
    Wiki,
    Writing,
    Typesetting,
    Publishing,
}

fn task_queue_adapter(queue: &str) -> Result<TaskQueueAdapter, CliError> {
    match queue {
        "wiki" => Ok(TaskQueueAdapter::Wiki),
        "writing" => Ok(TaskQueueAdapter::Writing),
        "typesetting" => Ok(TaskQueueAdapter::Typesetting),
        "publishing" => Ok(TaskQueueAdapter::Publishing),
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
