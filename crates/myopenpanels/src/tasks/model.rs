use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct TaskStatus(pub(crate) String);

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskLease {
    pub(crate) owner: Option<String>,
    pub(crate) expires_at: Option<String>,
    pub(crate) heartbeat_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskRecord {
    pub(crate) id: String,
    pub(crate) queue: String,
    pub(crate) project_id: String,
    pub(crate) panel_id: String,
    pub(crate) panel_kind: String,
    #[serde(rename = "type")]
    pub(crate) task_type: String,
    pub(crate) status: TaskStatus,
    pub(crate) target_id: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) attempt: i64,
    pub(crate) max_attempts: i64,
    pub(crate) lease: TaskLease,
    pub(crate) retry_after: Option<String>,
    pub(crate) capability: String,
    pub(crate) assigned_target_id: Option<String>,
    pub(crate) completed_at: Option<String>,
    pub(crate) input: Value,
    pub(crate) source: Value,
    pub(crate) result: Value,
    pub(crate) error: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentTarget {
    pub(crate) id: String,
    pub(crate) project_id: String,
    pub(crate) name: String,
    pub(crate) host: String,
    pub(crate) transport: String,
    pub(crate) endpoint: Option<String>,
    pub(crate) capabilities: Vec<String>,
    pub(crate) priority: i64,
    pub(crate) status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskDelivery {
    pub(crate) id: String,
    pub(crate) task_id: String,
    pub(crate) target_id: String,
    pub(crate) status: String,
    pub(crate) attempts: i64,
    pub(crate) next_attempt_at: Option<String>,
}
