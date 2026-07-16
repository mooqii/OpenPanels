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
    pub(crate) workflow_id: String,
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
    pub(crate) execution_generation: i64,
    pub(crate) available_at: Option<String>,
    pub(crate) archived_at: Option<String>,
    pub(crate) terminal_reason: Value,
    pub(crate) required_protocol_version: i64,
    pub(crate) dispatch_mode: String,
    pub(crate) requested_gateway_connection_id: Option<String>,
    pub(crate) mutation_key: Option<String>,
    pub(crate) mutation_sequence: Option<i64>,
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
    pub(crate) transport: AgentTargetTransport,
    pub(crate) capabilities: Vec<String>,
    pub(crate) priority: i64,
    pub(crate) status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AgentTargetTransport {
    Poll,
    Command,
}
