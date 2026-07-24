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

#[derive(Clone, Debug)]
pub(crate) struct PreparedPanelState {
    pub(crate) panel_id: String,
    pub(crate) base_revision: i64,
    pub(crate) state: Value,
}

impl PreparedPanelState {
    pub(crate) fn new(panel_id: &str, base_revision: i64, state: Value) -> Self {
        Self {
            panel_id: panel_id.to_owned(),
            base_revision,
            state,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedMyDocumentContent {
    pub(crate) expected_content_version: u64,
    pub(crate) document: Value,
}

impl PreparedMyDocumentContent {
    pub(crate) fn new(expected_content_version: u64, document: Value) -> Self {
        Self {
            expected_content_version,
            document,
        }
    }
}

pub(crate) struct PreparedMyDocumentDeletion {
    pub(crate) panel_id: String,
    pub(crate) document_id: String,
}

struct TaskOutputPlan {
    result: Option<Value>,
    panel_state: Option<PreparedPanelState>,
    my_document_content: Option<PreparedMyDocumentContent>,
    my_document_deletion: Option<PreparedMyDocumentDeletion>,
}

impl TaskOutputPlan {
    fn empty() -> Self {
        Self {
            result: None,
            panel_state: None,
            my_document_content: None,
            my_document_deletion: None,
        }
    }

    fn completed(
        result: Option<Value>,
        panel_state: Option<PreparedPanelState>,
        my_document_content: Option<PreparedMyDocumentContent>,
        my_document_deletion: Option<PreparedMyDocumentDeletion>,
    ) -> Self {
        Self {
            result,
            panel_state,
            my_document_content,
            my_document_deletion,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum TaskDomain {
    Wiki,
    Writing,
    Publication,
    Release,
}

fn task_domain(queue: &str) -> Result<TaskDomain, CliError> {
    match queue {
        "wiki" => Ok(TaskDomain::Wiki),
        "writing" => Ok(TaskDomain::Writing),
        "publication" => Ok(TaskDomain::Publication),
        "release" => Ok(TaskDomain::Release),
        queue => Err(CliError::with_code(
            "task_domain_missing",
            format!("No Task domain is available for queue: {queue}"),
        )),
    }
}

#[cfg(test)]
pub(crate) fn task_queue_has_domain(queue: &str) -> bool {
    task_domain(queue).is_ok()
}
