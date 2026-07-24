use super::*;

pub const MAX_TEXT_FILE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_STAGING_BYTES: i64 = 128 * 1024 * 1024;
pub const MAX_WIKI_FILES: usize = 10_000;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ResourceKind {
    WikiMarkdown,
    WikiSpace,
    MyDocument,
    WritingSkill,
}

impl ResourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WikiMarkdown => "wiki_markdown",
            Self::WikiSpace => "wiki_space",
            Self::MyDocument => "my_document",
            Self::WritingSkill => "writing_skill",
        }
    }

    pub(super) fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            "wiki_markdown" => Ok(Self::WikiMarkdown),
            "wiki_space" => Ok(Self::WikiSpace),
            "my_document" => Ok(Self::MyDocument),
            "writing_skill" => Ok(Self::WritingSkill),
            _ => Err(CliError::with_code(
                "invalid_content_resource",
                format!("Unsupported content resource kind: {value}"),
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageFileRequest {
    pub resource_kind: String,
    pub resource_key: String,
    pub logical_path: String,
    pub content_base64: String,
    pub mime_type: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileRequest {
    pub resource_kind: String,
    pub resource_key: String,
    pub logical_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareSkillRequest {
    pub skill_id: String,
    pub source: String,
    pub manifest: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskContextRequest {
    pub task_id: String,
    pub context_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillReadRequest {
    pub task_id: String,
    pub skill_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishingCheckpointRequest {
    pub task_id: String,
    pub phase: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ExecutionContext {
    pub(super) task_id: String,
    pub(super) project_id: String,
    pub(super) panel_id: String,
    pub(super) queue: String,
    pub(super) task_type: String,
    pub(super) task_capability: String,
    pub(super) generation: i64,
    pub(super) input: Value,
}

#[derive(Debug, Clone)]
pub struct ActiveResourceFile {
    pub logical_path: String,
    pub object_hash: String,
    pub size_bytes: i64,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ActiveResourceSnapshot {
    pub revision_id: String,
    pub content_version: i64,
    pub manifest_hash: String,
    pub files: Vec<ActiveResourceFile>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActivePointer {
    pub(crate) revision_id: String,
    pub(crate) content_version: i64,
    pub(crate) manifest_hash: String,
    #[serde(default)]
    pub(crate) archived: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RevisionFile {
    pub(super) logical_path: String,
    pub(super) content_hash: String,
    pub(super) size_bytes: i64,
    pub(super) mime_type: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RevisionManifest {
    pub(super) revision_id: String,
    pub(super) content_version: i64,
    pub(super) parent_revision_id: Option<String>,
    pub(super) created_at: String,
    pub(super) files: Vec<RevisionFile>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StagedResource {
    pub(crate) project_id: String,
    pub(crate) panel_id: String,
    pub(crate) resource_kind: String,
    pub(crate) resource_key: String,
    pub(crate) base_revision_id: Option<String>,
    pub(crate) base_content_version: i64,
    pub(crate) metadata: Value,
}

#[derive(Debug)]
pub(crate) struct PreparedTaskContent {
    pub(crate) commits: Vec<Value>,
    pub(super) activations: Vec<PreparedActivation>,
    pub(super) staging_root: Option<PathBuf>,
}

#[derive(Debug)]
pub(crate) struct PreparedDirectContent {
    pub(crate) commit: Value,
    pub(super) activation: PreparedActivation,
    pub(super) staging_root: PathBuf,
}

#[derive(Debug)]
pub(super) struct PreparedActivation {
    pub(super) active_path: PathBuf,
    pub(super) pointer: ActivePointer,
}
