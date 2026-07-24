use crate::control::now_iso;
use crate::error::CliError;
use crate::paths::{sanitize_path_part, MyOpenPanelsPaths};
use crate::storage::Storage;
use base64::Engine;
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

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

    fn parse(value: &str) -> Result<Self, CliError> {
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
struct ExecutionContext {
    task_id: String,
    project_id: String,
    panel_id: String,
    queue: String,
    task_type: String,
    task_capability: String,
    generation: i64,
    input: Value,
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
struct ActivePointer {
    revision_id: String,
    content_version: i64,
    manifest_hash: String,
    #[serde(default)]
    archived: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RevisionFile {
    logical_path: String,
    content_hash: String,
    size_bytes: i64,
    mime_type: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RevisionManifest {
    revision_id: String,
    content_version: i64,
    parent_revision_id: Option<String>,
    created_at: String,
    files: Vec<RevisionFile>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StagedResource {
    project_id: String,
    panel_id: String,
    resource_kind: String,
    resource_key: String,
    base_revision_id: Option<String>,
    base_content_version: i64,
    metadata: Value,
}

#[derive(Debug)]
pub(crate) struct PreparedTaskContent {
    pub(crate) commits: Vec<Value>,
    activations: Vec<PreparedActivation>,
    staging_root: Option<PathBuf>,
}

#[derive(Debug)]
pub(crate) struct PreparedDirectContent {
    pub(crate) commit: Value,
    activation: PreparedActivation,
    staging_root: PathBuf,
}

#[derive(Debug)]
struct PreparedActivation {
    active_path: PathBuf,
    pointer: ActivePointer,
}

pub fn hash_secret(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}

pub fn authorize_agent_broker_capability(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    capability: &str,
) -> Result<(), CliError> {
    let context = authorize(paths, execution_token)?;
    if crate::bridge::task_handler_allows_agent_broker_capability(
        &context.queue,
        &context.task_type,
        &context.task_capability,
        capability,
    ) {
        Ok(())
    } else {
        Err(CliError::with_code(
            "task_handler_command_not_allowed",
            format!("Task Handler does not allow Agent-side Broker capability {capability}."),
        ))
    }
}

pub fn create_execution_context_in_transaction(
    tx: &Transaction<'_>,
    task_id: &str,
    _attempt_id: &str,
    generation: i64,
    _expires_at: &str,
) -> Result<(String, String), CliError> {
    let token = format!(
        "execution:{:032x}{:032x}",
        rand::random::<u128>(),
        rand::random::<u128>()
    );
    let changed = tx
        .execute(
            "UPDATE tasks SET execution_token_hash = ? WHERE id = ? AND execution_generation = ? AND status = 'running'",
            params![hash_secret(&token), task_id, generation],
        )
        .map_err(to_cli_error)?;
    if changed != 1 {
        return Err(CliError::with_code(
            "execution_fenced",
            "The active Task execution no longer exists.",
        ));
    }
    Ok((token, format!("task:{task_id}:{generation}")))
}

pub fn abandon_task_staging_in_transaction(
    tx: &Transaction<'_>,
    task_id: &str,
    _now: &str,
) -> Result<(), CliError> {
    tx.execute(
        "UPDATE tasks SET execution_token_hash = NULL WHERE id = ?",
        [task_id],
    )
    .map_err(to_cli_error)?;
    Ok(())
}

pub fn stage_file(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &StageFileRequest,
) -> Result<Value, CliError> {
    stage_file_internal(paths, execution_token, request, false)
}

pub(crate) fn stage_runtime_validated_file(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &StageFileRequest,
) -> Result<Value, CliError> {
    stage_file_internal(paths, execution_token, request, true)
}

fn stage_file_internal(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &StageFileRequest,
    domain_validated: bool,
) -> Result<Value, CliError> {
    let kind = ResourceKind::parse(&request.resource_kind)?;
    if !domain_validated
        && matches!(kind, ResourceKind::MyDocument | ResourceKind::WritingSkill)
    {
        return Err(CliError::with_code(
            "invalid_broker_route",
            "My Documents and Writing Skills must use their domain preparation endpoint.",
        ));
    }
    validate_logical_path(&request.logical_path)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&request.content_base64)
        .map_err(|_| CliError::with_code("invalid_output", "Staged content is not valid base64."))?;
    if bytes.len() > MAX_TEXT_FILE_BYTES || std::str::from_utf8(&bytes).is_err() {
        return Err(CliError::with_code(
            "content_too_large",
            "Staged content must be bounded UTF-8 text.",
        ));
    }
    let context = authorize(paths, execution_token)?;
    let resource_dir = staging_resource_dir(
        paths,
        &context,
        kind,
        &request.resource_key,
    );
    fs::create_dir_all(resource_dir.join("files")).map_err(to_cli_error)?;
    let metadata_path = resource_dir.join("resource.json");
    if !metadata_path.exists() {
        let active = read_active_pointer(paths, &context.project_id, kind, &request.resource_key)?;
        write_json_atomic(
            &metadata_path,
            &StagedResource {
                project_id: context.project_id.clone(),
                panel_id: context.panel_id.clone(),
                resource_kind: kind.as_str().to_owned(),
                resource_key: request.resource_key.clone(),
                base_revision_id: active.as_ref().map(|value| value.revision_id.clone()),
                base_content_version: active.as_ref().map_or(0, |value| value.content_version),
                metadata: request.metadata.clone(),
            },
        )?;
    }
    let destination = resource_dir
        .join("files")
        .join(logical_path_buf(&request.logical_path)?);
    write_materialized_file(&destination, &bytes)?;
    let file_metadata = destination.with_extension(format!(
        "{}mopmeta",
        destination
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!("{value}."))
            .unwrap_or_default()
    ));
    write_json_atomic(
        &file_metadata,
        &json!({ "mimeType": request.mime_type, "metadata": request.metadata }),
    )?;
    let total = directory_size(&staging_task_dir(paths, &context))?;
    if total > MAX_STAGING_BYTES as u64 {
        let _ = fs::remove_file(&destination);
        return Err(CliError::with_code(
            "content_too_large",
            format!("An Attempt cannot stage more than {MAX_STAGING_BYTES} bytes."),
        ));
    }
    let content_hash = hash_bytes(&bytes);
    Ok(json!({
        "staged": true,
        "taskId": context.task_id,
        "executionGeneration": context.generation,
        "resourceKind": kind.as_str(),
        "resourceKey": request.resource_key,
        "logicalPath": request.logical_path,
        "contentHash": content_hash,
        "sizeBytes": bytes.len(),
    }))
}

pub fn read_file(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &ReadFileRequest,
) -> Result<Value, CliError> {
    let kind = ResourceKind::parse(&request.resource_kind)?;
    validate_logical_path(&request.logical_path)?;
    let context = authorize(paths, execution_token)?;
    let staged = staging_resource_dir(paths, &context, kind, &request.resource_key)
        .join("files")
        .join(logical_path_buf(&request.logical_path)?);
    let (bytes, mime_type) = if staged.is_file() {
        (fs::read(&staged).map_err(to_cli_error)?, mime_for_path(&staged))
    } else {
        let snapshot = resource_snapshot_for_task(
            paths,
            &context.project_id,
            kind,
            &request.resource_key,
            &context.input,
        )?
            .ok_or_else(|| CliError::with_code("content_not_found", "Content resource not found."))?;
        let file = snapshot.files.into_iter().find(|file| file.logical_path == request.logical_path)
            .ok_or_else(|| CliError::with_code("content_not_found", "Content file not found."))?;
        (file.bytes, file.mime_type)
    };
    Ok(json!({
        "resourceKind": kind.as_str(),
        "resourceKey": request.resource_key,
        "logicalPath": request.logical_path,
        "mimeType": mime_type,
        "contentBase64": base64::engine::general_purpose::STANDARD.encode(&bytes),
        "contentHash": hash_bytes(&bytes),
    }))
}

pub fn publishing_checkpoint(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &PublishingCheckpointRequest,
) -> Result<Value, CliError> {
    let context = authorize(paths, execution_token)?;
    if context.task_id != request.task_id
        || !crate::release::is_publishing_task_type(&context.task_type)
    {
        return Err(CliError::with_code("execution_fenced", "Execution cannot checkpoint this Task."));
    }
    crate::release::checkpoint_attempt_for_broker(paths, &request.task_id, &request.phase)
}

pub fn read_task_context(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &TaskContextRequest,
) -> Result<Value, CliError> {
    let context = authorize(paths, execution_token)?;
    if context.task_id != request.task_id {
        return Err(CliError::with_code("execution_fenced", "Execution token belongs to another Task."));
    }
    match request.context_kind.as_str() {
        "writing_request" => crate::writing::read_request(paths, &request.task_id),
        "writing_distillation" => crate::writing::read_distillation(paths, &request.task_id),
        _ => Err(CliError::with_code("invalid_task_context", "Unsupported Task context request.")),
    }
}

pub fn read_skill(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &SkillReadRequest,
) -> Result<Value, CliError> {
    let context = authorize(paths, execution_token)?;
    if context.task_id != request.task_id {
        return Err(CliError::with_code("execution_fenced", "Execution token belongs to another Task."));
    }
    serde_json::to_value(crate::agent::read_agent_skill(
        paths,
        &request.skill_id,
        Some(&request.task_id),
    )?)
    .map_err(to_cli_error)
}

pub fn prepare_skill(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &PrepareSkillRequest,
) -> Result<Value, CliError> {
    let context = authorize(paths, execution_token)?;
    let expected_id = context.input.get("skillId").and_then(Value::as_str).unwrap_or_default();
    if request.skill_id != expected_id {
        return Err(CliError::with_code(
            "writing_skill_file_invalid",
            "Generated Writing Skill does not match the claimed Task.",
        ));
    }
    crate::agent::validate_portable_writing_skill(&request.source, "SKILL.md", expected_id)?;
    let mut manifest = request.manifest.clone();
    manifest["source"] = json!("custom");
    manifest["skillId"] = json!(expected_id);
    manifest["name"] = context
        .input
        .get("name")
        .cloned()
        .unwrap_or_else(|| json!(expected_id));
    manifest["binding"] = json!({ "moduleKinds": ["writing"] });
    manifest["originProjectId"] = json!(context.project_id);
    manifest["taskId"] = json!(context.task_id);
    let first = stage_file_internal(
        paths,
        execution_token,
        &StageFileRequest {
            resource_kind: ResourceKind::WritingSkill.as_str().to_owned(),
            resource_key: request.skill_id.clone(),
            logical_path: "SKILL.md".to_owned(),
            content_base64: base64::engine::general_purpose::STANDARD.encode(request.source.as_bytes()),
            mime_type: "text/markdown".to_owned(),
            metadata: manifest.clone(),
        },
        true,
    )?;
    stage_file_internal(
        paths,
        execution_token,
        &StageFileRequest {
            resource_kind: ResourceKind::WritingSkill.as_str().to_owned(),
            resource_key: request.skill_id.clone(),
            logical_path: "manifest.json".to_owned(),
            content_base64: base64::engine::general_purpose::STANDARD
                .encode(serde_json::to_vec_pretty(&manifest).map_err(to_cli_error)?),
            mime_type: "application/json".to_owned(),
            metadata: manifest,
        },
        true,
    )?;
    Ok(json!({ "skillId": request.skill_id, "staged": true, "contentHash": first["contentHash"] }))
}

pub fn task_has_staged_resource(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    kind: ResourceKind,
) -> Result<bool, CliError> {
    let Some(root) = staging_root_for_task(paths, task_id)? else {
        return Ok(false);
    };
    Ok(root.join(kind.as_str()).is_dir())
}

pub fn staged_files_for_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    kind: ResourceKind,
) -> Result<Vec<(String, String, Vec<u8>, Value)>, CliError> {
    let Some(root) = staging_root_for_task(paths, task_id)? else {
        return Ok(Vec::new());
    };
    let kind_dir = root.join(kind.as_str());
    if !kind_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for resource_dir in read_dirs(&kind_dir)? {
        let metadata: StagedResource = read_json(&resource_dir.join("resource.json"))?;
        for file in revision_files(&resource_dir.join("files"))? {
            if file.0.ends_with(".mopmeta") {
                continue;
            }
            result.push((
                metadata.resource_key.clone(),
                file.0,
                fs::read(file.1).map_err(to_cli_error)?,
                metadata.metadata.clone(),
            ));
        }
    }
    result.sort_by(|left, right| (&left.0, &left.1).cmp(&(&right.0, &right.1)));
    Ok(result)
}

pub(crate) fn prepare_task_staging_in_transaction(
    paths: &MyOpenPanelsPaths,
    tx: &Transaction<'_>,
    task_id: &str,
    _now: &str,
    allow_empty: bool,
) -> Result<PreparedTaskContent, CliError> {
    let (project_id, generation, handler_key) = tx
        .query_row(
            "SELECT project_id, execution_generation, handler_key FROM tasks WHERE id = ? AND status = 'running'",
            [task_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?)),
        )
        .map_err(to_cli_error)?;
    let root = staging_root(paths, &project_id, task_id, generation);
    if !root.is_dir() {
        let route = crate::capabilities::task_route_for_handler(&handler_key)?;
        if allow_empty || route.is_none() {
            return Ok(PreparedTaskContent {
                commits: Vec::new(),
                activations: Vec::new(),
                staging_root: None,
            });
        }
        return Err(CliError::with_code("invalid_output", "Task completed without staged content."));
    }
    let mut staged_resources = Vec::new();
    for kind_dir in read_dirs(&root)? {
        for resource_dir in read_dirs(&kind_dir)? {
            let staged: StagedResource = read_json(&resource_dir.join("resource.json"))?;
            let kind = ResourceKind::parse(&staged.resource_kind)?;
            let current = read_active_pointer(paths, &project_id, kind, &staged.resource_key)?;
            if current.as_ref().map(|value| value.revision_id.as_str())
                != staged.base_revision_id.as_deref()
                || current.as_ref().map_or(0, |value| value.content_version)
                    != staged.base_content_version
            {
                return Err(CliError::with_code(
                    "content_conflict",
                    format!("Content changed while the Task was running: {}", staged.resource_key),
                ));
            }
            staged_resources.push((staged, resource_dir));
        }
    }
    let mut commits = Vec::new();
    let mut activations = Vec::new();
    for (staged, resource_dir) in staged_resources {
        let kind = ResourceKind::parse(&staged.resource_kind)?;
        let (active_path, revision) = prepare_staged_resource(paths, &staged, &resource_dir, None)?;
        commits.push(json!({
            "resourceKind": kind.as_str(),
            "resourceKey": staged.resource_key,
            "revisionId": revision.revision_id,
            "contentVersion": revision.content_version,
            "manifestHash": revision.manifest_hash,
        }));
        activations.push(PreparedActivation {
            active_path,
            pointer: revision,
        });
    }
    tx.execute("UPDATE tasks SET execution_token_hash = NULL WHERE id = ?", [task_id])
        .map_err(to_cli_error)?;
    Ok(PreparedTaskContent {
        commits,
        activations,
        staging_root: Some(root),
    })
}

pub(crate) fn publish_prepared_task_content(
    _paths: &MyOpenPanelsPaths,
    prepared: PreparedTaskContent,
) -> Result<(), CliError> {
    for activation in &prepared.activations {
        write_json_atomic(&activation.active_path, &activation.pointer)?;
    }
    if let Some(root) = prepared.staging_root {
        let _ = fs::remove_dir_all(root);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_direct_text_content(
    paths: &MyOpenPanelsPaths,
    operation_id: &str,
    project_id: &str,
    panel_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
    bytes: &[u8],
    mime_type: &str,
    base_content_version: u64,
) -> Result<PreparedDirectContent, CliError> {
    validate_logical_path(logical_path)?;
    if bytes.len() > MAX_TEXT_FILE_BYTES || std::str::from_utf8(bytes).is_err() {
        return Err(CliError::with_code(
            "invalid_my_document",
            "My Document content must be bounded UTF-8 text.",
        ));
    }
    let current = read_active_pointer(paths, project_id, kind, resource_key)?;
    let current_version = current.as_ref().map_or(0, |value| value.content_version);
    if current_version != base_content_version as i64 {
        return Err(CliError::with_code(
            "content_conflict",
            format!(
                "Content changed from version {base_content_version} to {current_version}."
            ),
        ));
    }
    let staging_root = paths
        .storage_dir
        .join("operations")
        .join(sanitize_path_part(operation_id))
        .join("content-staging");
    if staging_root.exists() {
        fs::remove_dir_all(&staging_root).map_err(to_cli_error)?;
    }
    let destination = staging_root
        .join("files")
        .join(logical_path_buf(logical_path)?);
    write_materialized_file(&destination, bytes)?;
    let staged = StagedResource {
        project_id: project_id.to_owned(),
        panel_id: panel_id.to_owned(),
        resource_kind: kind.as_str().to_owned(),
        resource_key: resource_key.to_owned(),
        base_revision_id: current.as_ref().map(|value| value.revision_id.clone()),
        base_content_version: current_version,
        metadata: json!({ "replaceAll": true }),
    };
    let (active_path, pointer) = prepare_staged_resource(
        paths,
        &staged,
        &staging_root,
        Some((logical_path, mime_type)),
    )?;
    let commit = json!({
        "resourceKind": kind.as_str(),
        "resourceKey": resource_key,
        "revisionId": pointer.revision_id,
        "contentVersion": pointer.content_version,
        "manifestHash": pointer.manifest_hash,
    });
    Ok(PreparedDirectContent {
        commit,
        activation: PreparedActivation {
            active_path,
            pointer,
        },
        staging_root,
    })
}

pub(crate) fn publish_prepared_direct_content(
    prepared: PreparedDirectContent,
) -> Result<(), CliError> {
    write_json_atomic(
        &prepared.activation.active_path,
        &prepared.activation.pointer,
    )?;
    let _ = fs::remove_dir_all(prepared.staging_root);
    Ok(())
}

pub fn recover_filesystem(paths: &MyOpenPanelsPaths) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    let completed = {
        let mut statement = storage
            .connection()
            .prepare(
                r#"
                SELECT project_id, result_json, completed_at, id
                FROM tasks
                WHERE status = 'succeeded' AND result_json IS NOT NULL
                UNION ALL
                SELECT project_id, json_extract(operation_json, '$.result'), completed_at, id
                FROM direct_operations
                WHERE status = 'completed' AND json_type(operation_json, '$.result') = 'object'
                ORDER BY completed_at, id
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(to_cli_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_cli_error)?;
        rows
    };
    drop(storage);

    let mut retained_revisions = BTreeSet::new();
    for (project_id, result_json) in completed {
        let result: Value = serde_json::from_str(&result_json).map_err(to_cli_error)?;
        for commit in result
            .get("contentCommits")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(kind) = commit
                .get("resourceKind")
                .and_then(Value::as_str)
                .map(ResourceKind::parse)
                .transpose()?
            else {
                continue;
            };
            let Some(resource_key) = commit.get("resourceKey").and_then(Value::as_str) else {
                continue;
            };
            let Some(revision_id) = commit.get("revisionId").and_then(Value::as_str) else {
                continue;
            };
            let resource = resource_dir(paths, &project_id, kind, resource_key);
            let revision_dir = resource.join(revision_id);
            let manifest_path = revision_dir.join("manifest.json");
            if !manifest_path.is_file() {
                continue;
            }
            let manifest_bytes = fs::read(&manifest_path).map_err(to_cli_error)?;
            let manifest: RevisionManifest =
                serde_json::from_slice(&manifest_bytes).map_err(to_cli_error)?;
            let pointer = ActivePointer {
                revision_id: manifest.revision_id,
                content_version: manifest.content_version,
                manifest_hash: hash_bytes(&manifest_bytes),
                archived: false,
            };
            let current = read_active_pointer(paths, &project_id, kind, resource_key)?;
            if current
                .as_ref()
                .is_none_or(|value| value.content_version < pointer.content_version)
            {
                write_json_atomic(&resource.join("active.json"), &pointer)?;
            }
            retained_revisions.insert(revision_dir);
        }
    }

    let projects_root = paths.storage_dir.join("projects");
    for project_dir in read_dirs(&projects_root)? {
        let content_dir = project_dir.join("content");
        let _ = fs::remove_dir_all(content_dir.join(".staging"));
        for kind_dir in read_dirs(&content_dir)? {
            let kind_name = kind_dir
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if ResourceKind::parse(kind_name).is_err() {
                continue;
            }
            for resource in read_dirs(&kind_dir)? {
                retain_active_revision_chain(&resource, &mut retained_revisions)?;
                for revision in read_dirs(&resource)? {
                    let name = revision.file_name().and_then(|value| value.to_str()).unwrap_or("");
                    if name.starts_with('.') || !retained_revisions.contains(&revision) {
                        fs::remove_dir_all(revision).map_err(to_cli_error)?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn retain_active_revision_chain(
    resource: &Path,
    retained: &mut BTreeSet<PathBuf>,
) -> Result<(), CliError> {
    let active_path = resource.join("active.json");
    if !active_path.is_file() {
        return Ok(());
    }
    let active: ActivePointer = read_json(&active_path)?;
    let mut revision_id = Some(active.revision_id);
    while let Some(current) = revision_id {
        let revision = resource.join(&current);
        if !retained.insert(revision.clone()) {
            break;
        }
        let manifest_path = revision.join("manifest.json");
        if !manifest_path.is_file() {
            break;
        }
        revision_id = read_json::<RevisionManifest>(&manifest_path)?.parent_revision_id;
    }
    Ok(())
}

pub fn active_resource_descriptor(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<Option<Value>, CliError> {
    Ok(read_active_pointer(paths, project_id, kind, resource_key)?.map(|active| {
        json!({
            "revisionId": active.revision_id,
            "contentVersion": active.content_version,
            "manifestHash": active.manifest_hash,
        })
    }))
}

pub fn active_resource_snapshot(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<Option<ActiveResourceSnapshot>, CliError> {
    let Some(active) = read_active_pointer(paths, project_id, kind, resource_key)? else {
        return Ok(None);
    };
    resource_snapshot_at_revision(
        paths,
        project_id,
        kind,
        resource_key,
        &active.revision_id,
    )
}

fn resource_snapshot_at_revision(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    revision_id: &str,
) -> Result<Option<ActiveResourceSnapshot>, CliError> {
    let resource = resource_dir(paths, project_id, kind, resource_key);
    let manifest_path = resource.join(revision_id).join("manifest.json");
    if !manifest_path.is_file() {
        return Ok(None);
    }
    let manifest: RevisionManifest = read_json(&manifest_path)?;
    let files = manifest
        .files
        .into_iter()
        .map(|file| {
            let bytes = fs::read(
                resource
                    .join(revision_id)
                    .join("files")
                    .join(logical_path_buf(&file.logical_path)?),
            )
            .map_err(to_cli_error)?;
            Ok(ActiveResourceFile {
                logical_path: file.logical_path,
                object_hash: file.content_hash,
                size_bytes: file.size_bytes,
                mime_type: file.mime_type,
                bytes,
            })
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    Ok(Some(ActiveResourceSnapshot {
        revision_id: manifest.revision_id,
        content_version: manifest.content_version,
        manifest_hash: hash_bytes(&fs::read(manifest_path).map_err(to_cli_error)?),
        files,
    }))
}

fn resource_snapshot_for_task(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    input: &Value,
) -> Result<Option<ActiveResourceSnapshot>, CliError> {
    if let Some(revision_id) = pinned_revision_id(input, kind, resource_key) {
        return resource_snapshot_at_revision(
            paths,
            project_id,
            kind,
            resource_key,
            revision_id,
        );
    }
    active_resource_snapshot(paths, project_id, kind, resource_key)
}

fn pinned_revision_id<'a>(
    input: &'a Value,
    kind: ResourceKind,
    resource_key: &str,
) -> Option<&'a str> {
    match kind {
        ResourceKind::WikiSpace
            if input
                .pointer("/contextSnapshot/wikiSelection/wikiSpaceId")
                .and_then(Value::as_str)
                == Some(resource_key) => input
                .pointer("/contextSnapshot/wikiSelection/contentRevisionId")
                .and_then(Value::as_str),
        _ => None,
    }
}

pub fn read_active_text(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
) -> Result<Option<String>, CliError> {
    let Some(snapshot) = active_resource_snapshot(paths, project_id, kind, resource_key)? else {
        return Ok(None);
    };
    snapshot
        .files
        .into_iter()
        .find(|file| file.logical_path == logical_path)
        .map(|file| String::from_utf8(file.bytes).map_err(|_| CliError::with_code("invalid_content", "Stored content is not UTF-8.")))
        .transpose()
}

pub fn rename_active_file(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    current_path: &str,
    next_path: &str,
) -> Result<Option<Value>, CliError> {
    let Some(snapshot) = active_resource_snapshot(paths, project_id, kind, resource_key)? else {
        return Ok(None);
    };
    if snapshot.files.iter().any(|file| file.logical_path == next_path) {
        return Err(CliError::with_code("content_conflict", "Destination content already exists."));
    }
    let resource = resource_dir(paths, project_id, kind, resource_key);
    let stage = tempfile::tempdir_in(&paths.storage_dir).map_err(to_cli_error)?;
    copy_tree(
        &resource.join(&snapshot.revision_id).join("files"),
        &stage.path().join("files"),
    )?;
    let current = stage.path().join("files").join(logical_path_buf(current_path)?);
    let next = stage.path().join("files").join(logical_path_buf(next_path)?);
    if !current.exists() {
        return Err(CliError::with_code("content_unavailable", "Source content does not exist."));
    }
    if let Some(parent) = next.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::rename(current, next).map_err(to_cli_error)?;
    let staged = StagedResource {
        project_id: project_id.to_owned(),
        panel_id: String::new(),
        resource_kind: kind.as_str().to_owned(),
        resource_key: resource_key.to_owned(),
        base_revision_id: Some(snapshot.revision_id),
        base_content_version: snapshot.content_version,
        metadata: json!({ "replaceAll": true }),
    };
    let pointer = commit_staged_resource(paths, &staged, stage.path())?;
    Ok(Some(json!({
        "revisionId": pointer.revision_id,
        "contentVersion": pointer.content_version,
        "manifestHash": pointer.manifest_hash,
    })))
}

pub fn materialize_active_file(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
    destination: &Path,
) -> Result<Option<Value>, CliError> {
    let Some(snapshot) = active_resource_snapshot(paths, project_id, kind, resource_key)? else {
        return Ok(None);
    };
    let file = snapshot.files.iter().find(|file| file.logical_path == logical_path)
        .ok_or_else(|| CliError::with_code("content_unavailable", "Active revision does not contain the file."))?;
    write_materialized_file(destination, &file.bytes)?;
    Ok(Some(json!({
        "revisionId": snapshot.revision_id,
        "contentVersion": snapshot.content_version,
        "manifestHash": snapshot.manifest_hash,
        "logicalPath": file.logical_path,
        "objectHash": file.object_hash,
        "sizeBytes": file.size_bytes,
        "mimeType": file.mime_type,
        "localPath": destination,
    })))
}

pub(crate) fn write_materialized_file(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let parent = path.parent().ok_or_else(|| CliError::new("Content path has no parent."))?;
    fs::create_dir_all(parent).map_err(to_cli_error)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(to_cli_error)?;
    temporary.write_all(bytes).map_err(to_cli_error)?;
    temporary.as_file().sync_all().map_err(to_cli_error)?;
    temporary.persist(path).map_err(|error| to_cli_error(error.error))?;
    Ok(())
}

pub fn active_writing_skill_sources(
    paths: &MyOpenPanelsPaths,
) -> Result<Vec<(String, String, Value, String)>, CliError> {
    let mut result = Vec::new();
    let projects = paths.storage_dir.join("projects");
    for project in read_dirs(&projects)? {
        let skills = project.join("content").join(ResourceKind::WritingSkill.as_str());
        for skill in read_dirs(&skills)? {
            let skill_id = read_resource_key(&skill).unwrap_or_else(|| {
                skill.file_name().unwrap_or_default().to_string_lossy().into_owned()
            });
            let project_id = project.file_name().unwrap_or_default().to_string_lossy();
            let Some(snapshot) = active_resource_snapshot(
                paths,
                &project_id,
                ResourceKind::WritingSkill,
                &skill_id,
            )? else { continue };
            let source = snapshot.files.iter().find(|file| file.logical_path == "SKILL.md")
                .and_then(|file| String::from_utf8(file.bytes.clone()).ok());
            let manifest = snapshot.files.iter().find(|file| file.logical_path == "manifest.json")
                .and_then(|file| serde_json::from_slice::<Value>(&file.bytes).ok());
            if let (Some(source), Some(manifest)) = (source, manifest) {
                let dir = skill.join(&snapshot.revision_id).join("files");
                result.push((skill_id, source, manifest, dir.display().to_string()));
            }
        }
    }
    result.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(result)
}

pub fn active_writing_skill_dir(paths: &MyOpenPanelsPaths, skill_id: &str) -> Option<PathBuf> {
    active_writing_skill_sources(paths)
        .ok()?
        .into_iter()
        .find(|value| value.0 == skill_id)
        .map(|value| PathBuf::from(value.3))
}

pub fn writing_skill_project_id(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
) -> Result<Option<String>, CliError> {
    for project in read_dirs(&paths.storage_dir.join("projects"))? {
        let project_id = project.file_name().unwrap_or_default().to_string_lossy().into_owned();
        if read_active_pointer(paths, &project_id, ResourceKind::WritingSkill, skill_id)?.is_some() {
            return Ok(Some(project_id));
        }
    }
    Ok(None)
}

pub fn archive_resource(
    paths: &MyOpenPanelsPaths,
    project_id: Option<&str>,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<(), CliError> {
    let projects = if let Some(project_id) = project_id {
        vec![project_id.to_owned()]
    } else {
        read_dirs(&paths.storage_dir.join("projects"))?
            .into_iter()
            .map(|path| path.file_name().unwrap_or_default().to_string_lossy().into_owned())
            .collect()
    };
    for project_id in projects {
        let pointer_path = resource_dir(paths, &project_id, kind, resource_key).join("active.json");
        if pointer_path.is_file() {
            let mut pointer: ActivePointer = read_json(&pointer_path)?;
            pointer.archived = true;
            write_json_atomic(&pointer_path, &pointer)?;
        }
    }
    Ok(())
}

pub fn start_gc_loop(paths: MyOpenPanelsPaths) {
    if cfg!(test) {
        return;
    }
    std::thread::spawn(move || loop {
        let _ = gc_content(&paths);
        std::thread::sleep(std::time::Duration::from_secs(60 * 60));
    });
}

pub fn gc_content(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let storage = Storage::open(paths)?;
    let mut statement = storage
        .connection()
        .prepare("SELECT id, project_id, execution_generation FROM tasks WHERE status = 'running'")
        .map_err(to_cli_error)?;
    let active = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)))
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    let mut removed = 0;
    for project in read_dirs(&paths.storage_dir.join("projects"))? {
        let staging = project.join("content/.staging");
        for task_dir in read_dirs(&staging)? {
            let task_id = task_dir.file_name().unwrap_or_default().to_string_lossy();
            let keep = active.iter().any(|(id, _, _)| sanitize_path_part(id) == task_id);
            if !keep {
                fs::remove_dir_all(task_dir).map_err(to_cli_error)?;
                removed += 1;
            }
        }
    }
    Ok(json!({ "removedStagingDirectories": removed, "prunedRevisions": 0 }))
}

pub(crate) fn pinned_task_input_text(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
) -> Result<Option<String>, CliError> {
    let (project_id, input) = task_input_from_storage(paths, task_id)?;
    let Some(snapshot) = resource_snapshot_for_task(
        paths,
        &project_id,
        kind,
        resource_key,
        &input,
    )? else {
        return Ok(None);
    };
    snapshot
        .files
        .into_iter()
        .find(|file| file.logical_path == logical_path)
        .map(|file| {
            String::from_utf8(file.bytes).map_err(|_| {
                CliError::with_code("invalid_content", "Stored content is not UTF-8.")
            })
        })
        .transpose()
}

pub(crate) fn pinned_task_input_paths(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<Vec<String>, CliError> {
    let (project_id, input) = task_input_from_storage(paths, task_id)?;
    Ok(resource_snapshot_for_task(paths, &project_id, kind, resource_key, &input)?
        .map(|snapshot| snapshot.files.into_iter().map(|file| file.logical_path).collect())
        .unwrap_or_default())
}

pub(crate) fn task_wiki_base_paths(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    wiki_space_id: &str,
) -> Result<Vec<String>, CliError> {
    pinned_task_input_paths(paths, task_id, ResourceKind::WikiSpace, wiki_space_id)
}

fn authorize(paths: &MyOpenPanelsPaths, token: &str) -> Result<ExecutionContext, CliError> {
    let storage = Storage::open(paths)?;
    let token_hash = hash_secret(token);
    let row = storage
        .connection()
        .query_row(
            r#"
            SELECT t.id, t.project_id, COALESCE(t.origin_panel_id, ''), t.handler_key,
                   t.execution_generation, t.input_json
            FROM tasks t
            WHERE t.execution_token_hash = ? AND t.status = 'running'
              AND (t.lease_expires_at IS NULL OR t.lease_expires_at > ?)
            "#,
            params![token_hash, now_iso()],
            |row| Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
            )),
        )
        .optional()
        .map_err(to_cli_error)?
        .ok_or_else(|| CliError::with_code("execution_fenced", "Execution token is invalid or expired."))?;
    let route = crate::capabilities::task_route_for_handler(&row.3)?
        .ok_or_else(|| CliError::with_code("task_handler_not_found", "Task handler is unavailable."))?;
    Ok(ExecutionContext {
        task_id: row.0,
        project_id: row.1,
        panel_id: row.2,
        queue: route.queue.clone(),
        task_type: route.task_type.clone(),
        task_capability: route.capability.clone(),
        generation: row.4,
        input: serde_json::from_str(&row.5).unwrap_or_else(|_| json!({})),
    })
}

fn commit_staged_resource(
    paths: &MyOpenPanelsPaths,
    staged: &StagedResource,
    stage_dir: &Path,
) -> Result<ActivePointer, CliError> {
    commit_staged_resource_with_mime(paths, staged, stage_dir, None)
}

fn commit_staged_resource_with_mime(
    paths: &MyOpenPanelsPaths,
    staged: &StagedResource,
    stage_dir: &Path,
    mime_override: Option<(&str, &str)>,
) -> Result<ActivePointer, CliError> {
    let (active_path, pointer) = prepare_staged_resource(
        paths,
        staged,
        stage_dir,
        mime_override,
    )?;
    write_json_atomic(&active_path, &pointer)?;
    Ok(pointer)
}

fn prepare_staged_resource(
    paths: &MyOpenPanelsPaths,
    staged: &StagedResource,
    stage_dir: &Path,
    mime_override: Option<(&str, &str)>,
) -> Result<(PathBuf, ActivePointer), CliError> {
    let kind = ResourceKind::parse(&staged.resource_kind)?;
    let resource = resource_dir(paths, &staged.project_id, kind, &staged.resource_key);
    fs::create_dir_all(&resource).map_err(to_cli_error)?;
    write_json_atomic(&resource.join("resource.json"), &json!({ "resourceKey": staged.resource_key }))?;
    let revision_id = crate::ids::random_id("revision");
    let temporary = resource.join(format!(".{revision_id}.tmp"));
    let files_dir = temporary.join("files");
    let replace_all = staged
        .metadata
        .get("replaceAll")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || read_json::<bool>(&stage_dir.join("replace-all.json")).unwrap_or(false);
    if !replace_all {
        if let Some(parent) = staged.base_revision_id.as_deref() {
            copy_tree(&resource.join(parent).join("files"), &files_dir)?;
        }
    }
    copy_tree(&stage_dir.join("files"), &files_dir)?;
    let mut files = Vec::new();
    for (logical_path, path) in revision_files(&files_dir)? {
        if logical_path.ends_with(".mopmeta") {
            continue;
        }
        let bytes = fs::read(&path).map_err(to_cli_error)?;
        let mime_type = mime_override
            .filter(|value| value.0 == logical_path)
            .map(|value| value.1.to_owned())
            .unwrap_or_else(|| mime_for_path(&path));
        files.push(RevisionFile {
            logical_path,
            content_hash: hash_bytes(&bytes),
            size_bytes: bytes.len() as i64,
            mime_type,
        });
    }
    files.sort_by(|left, right| left.logical_path.cmp(&right.logical_path));
    if kind == ResourceKind::WikiSpace && files.len() > MAX_WIKI_FILES {
        return Err(CliError::with_code("content_too_large", "Wiki contains too many files."));
    }
    let manifest = RevisionManifest {
        revision_id: revision_id.clone(),
        content_version: staged.base_content_version + 1,
        parent_revision_id: staged.base_revision_id.clone(),
        created_at: now_iso(),
        files,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(to_cli_error)?;
    fs::create_dir_all(&temporary).map_err(to_cli_error)?;
    fs::write(temporary.join("manifest.json"), &manifest_bytes).map_err(to_cli_error)?;
    fs::rename(&temporary, resource.join(&revision_id)).map_err(to_cli_error)?;
    let pointer = ActivePointer {
        revision_id,
        content_version: manifest.content_version,
        manifest_hash: hash_bytes(&manifest_bytes),
        archived: false,
    };
    Ok((resource.join("active.json"), pointer))
}

fn read_active_pointer(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<Option<ActivePointer>, CliError> {
    let path = resource_dir(paths, project_id, kind, resource_key).join("active.json");
    if !path.is_file() {
        return Ok(None);
    }
    let pointer: ActivePointer = read_json(&path)?;
    Ok((!pointer.archived).then_some(pointer))
}

fn resource_dir(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> PathBuf {
    paths
        .storage_dir
        .join("projects")
        .join(sanitize_path_part(project_id))
        .join("content")
        .join(kind.as_str())
        .join(sanitize_path_part(resource_key))
}

fn staging_task_dir(paths: &MyOpenPanelsPaths, context: &ExecutionContext) -> PathBuf {
    staging_root(paths, &context.project_id, &context.task_id, context.generation)
}

fn staging_root(paths: &MyOpenPanelsPaths, project_id: &str, task_id: &str, generation: i64) -> PathBuf {
    paths
        .storage_dir
        .join("projects")
        .join(sanitize_path_part(project_id))
        .join("content/.staging")
        .join(sanitize_path_part(task_id))
        .join(generation.to_string())
}

fn staging_resource_dir(
    paths: &MyOpenPanelsPaths,
    context: &ExecutionContext,
    kind: ResourceKind,
    resource_key: &str,
) -> PathBuf {
    staging_task_dir(paths, context)
        .join(kind.as_str())
        .join(sanitize_path_part(resource_key))
}

fn staging_root_for_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Option<PathBuf>, CliError> {
    let storage = Storage::open(paths)?;
    storage
        .connection()
        .query_row(
            "SELECT project_id, execution_generation FROM tasks WHERE id = ?",
            [task_id],
            |row| Ok(staging_root(paths, &row.get::<_, String>(0)?, task_id, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(to_cli_error)
}

fn task_input_from_storage(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
) -> Result<(String, Value), CliError> {
    if let Some((project_id, input_json)) = Storage::open(paths)?
        .connection()
        .query_row(
            "SELECT project_id, input_json FROM tasks WHERE id = ?",
            [task_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(to_cli_error)?
    {
        return Ok((
            project_id,
            serde_json::from_str(&input_json).unwrap_or_else(|_| json!({})),
        ));
    }
    Ok((
        crate::control::read_project_bootstrap(paths, crate::control::BootstrapRequest::new())?
            .project
            .id,
        json!({}),
    ))
}

fn validate_logical_path(value: &str) -> Result<(), CliError> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path.components().any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(CliError::with_code("invalid_content_path", "Content path must be relative and cannot traverse directories."));
    }
    Ok(())
}

fn logical_path_buf(value: &str) -> Result<PathBuf, CliError> {
    validate_logical_path(value)?;
    Ok(value.split('/').fold(PathBuf::new(), |path, part| path.join(sanitize_path_part(part))))
}

fn revision_files(root: &Path) -> Result<Vec<(String, PathBuf)>, CliError> {
    fn visit(root: &Path, current: &Path, output: &mut Vec<(String, PathBuf)>) -> Result<(), CliError> {
        if !current.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(current).map_err(to_cli_error)? {
            let path = entry.map_err(to_cli_error)?.path();
            if path.is_dir() {
                visit(root, &path, output)?;
            } else {
                let relative = path.strip_prefix(root).map_err(to_cli_error)?
                    .components().filter_map(|part| part.as_os_str().to_str()).collect::<Vec<_>>().join("/");
                output.push((relative, path));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(root, root, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), CliError> {
    if !source.is_dir() {
        return Ok(());
    }
    for (relative, path) in revision_files(source)? {
        if relative.ends_with(".mopmeta") {
            continue;
        }
        let target = destination.join(logical_path_buf(&relative)?);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(to_cli_error)?;
        }
        if target.exists() {
            fs::remove_file(&target).map_err(to_cli_error)?;
        }
        if fs::hard_link(&path, &target).is_err() {
            fs::copy(&path, &target).map_err(to_cli_error)?;
        }
    }
    Ok(())
}

fn directory_size(path: &Path) -> Result<u64, CliError> {
    revision_files(path).map(|files| {
        files.into_iter().filter_map(|(_, path)| fs::metadata(path).ok().map(|metadata| metadata.len())).sum()
    })
}

fn read_dirs(path: &Path) -> Result<Vec<PathBuf>, CliError> {
    if !path.is_dir() {
        return Ok(Vec::new());
    }
    let mut values = fs::read_dir(path)
        .map_err(to_cli_error)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    values.sort();
    Ok(values)
}

fn read_resource_key(resource_dir: &Path) -> Option<String> {
    fs::read(resource_dir.join("resource.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| value.get("resourceKey").and_then(Value::as_str).map(str::to_owned))
}

fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<(), CliError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(to_cli_error)?;
    write_materialized_file(path, &bytes)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, CliError> {
    serde_json::from_slice(&fs::read(path).map_err(to_cli_error)?).map_err(to_cli_error)
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn mime_for_path(path: &Path) -> String {
    mime_guess::from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_owned()
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod filesystem_recovery_tests {
    use super::*;
    use crate::control::{ensure_project_bootstrap, BootstrapRequest};
    use crate::types::PanelKind;

    #[test]
    fn startup_finishes_committed_publication_and_removes_orphans() {
        let temp = tempfile::tempdir().expect("temp");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("content-recovery"),
        )
        .expect("paths");
        let bootstrap = ensure_project_bootstrap(&paths, BootstrapRequest::new())
            .expect("bootstrap");
        let wiki_panel = bootstrap
            .panels
            .iter()
            .find(|panel| panel.panel.kind == PanelKind::Wiki)
            .expect("wiki panel");

        commit_immediate_text(
            &paths,
            &bootstrap.project.id,
            Some(&wiki_panel.panel.id),
            ResourceKind::WikiSpace,
            "wiki:default",
            "index.md",
            b"one",
            "text/markdown",
            true,
        )
        .expect("initial revision");
        let active = read_active_pointer(
            &paths,
            &bootstrap.project.id,
            ResourceKind::WikiSpace,
            "wiki:default",
        )
        .expect("active pointer")
        .expect("active revision");
        let staged = StagedResource {
            project_id: bootstrap.project.id.clone(),
            panel_id: wiki_panel.panel.id.clone(),
            resource_kind: ResourceKind::WikiSpace.as_str().to_owned(),
            resource_key: "wiki:default".to_owned(),
            base_revision_id: Some(active.revision_id),
            base_content_version: active.content_version,
            metadata: json!({ "replaceAll": true }),
        };
        let stage = tempfile::tempdir_in(&storage_dir).expect("stage");
        write_json_atomic(&stage.path().join("resource.json"), &staged)
            .expect("staged metadata");
        write_materialized_file(&stage.path().join("files/index.md"), b"two")
            .expect("staged file");
        let (_, pointer) = prepare_staged_resource(&paths, &staged, stage.path(), None)
            .expect("prepared revision");

        let storage = Storage::open(&paths).expect("storage");
        let task = storage
            .insert_task(
                &bootstrap.project.id,
                &wiki_panel.panel.id,
                "wiki",
                "maintain_wiki",
                "wiki.maintain",
                "wiki:default",
                &json!({ "changeEvents": [] }),
                &json!({ "agentSkillId": "wiki-default" }),
            )
            .expect("task");
        let result = json!({
            "contentCommits": [{
                "resourceKind": ResourceKind::WikiSpace.as_str(),
                "resourceKey": "wiki:default",
                "revisionId": pointer.revision_id,
                "contentVersion": pointer.content_version,
                "manifestHash": pointer.manifest_hash,
            }]
        });
        storage
            .connection()
            .execute(
                "UPDATE tasks SET status = 'succeeded', result_json = ?, completed_at = ? WHERE id = ?",
                params![result.to_string(), now_iso(), task["id"].as_str().unwrap()],
            )
            .expect("committed task result");
        drop(storage);

        let resource = resource_dir(
            &paths,
            &bootstrap.project.id,
            ResourceKind::WikiSpace,
            "wiki:default",
        );
        let orphan = resource.join("revision:orphan");
        fs::create_dir_all(&orphan).expect("orphan");
        let abandoned_stage = storage_dir
            .join("projects")
            .join(sanitize_path_part(&bootstrap.project.id))
            .join("content/.staging/task:old/1");
        fs::create_dir_all(&abandoned_stage).expect("abandoned staging");

        recover_filesystem(&paths).expect("recovery");

        assert_eq!(
            read_active_text(
                &paths,
                &bootstrap.project.id,
                ResourceKind::WikiSpace,
                "wiki:default",
                "index.md",
            )
            .expect("active text")
            .as_deref(),
            Some("two")
        );
        assert!(!orphan.exists());
        assert!(!abandoned_stage.exists());
    }

    #[test]
    fn startup_recovery_preserves_asset_revisions() {
        let temp = tempfile::tempdir().expect("temp");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("asset-recovery"),
        )
        .expect("paths");
        let bootstrap = ensure_project_bootstrap(&paths, BootstrapRequest::new())
            .expect("bootstrap");
        let panel = bootstrap.panels.first().expect("panel");
        let storage = Storage::open(&paths).expect("storage");
        let written = storage
            .write_asset_from_buffer(
                &bootstrap.project.id,
                &panel.panel.id,
                "cover.png",
                b"cover bytes",
                false,
            )
            .expect("asset");

        recover_filesystem(&paths).expect("recovery");

        assert_eq!(
            storage.read_asset(&written.asset_ref).expect("asset bytes"),
            b"cover bytes"
        );
    }
}
