use crate::control::now_iso;
use crate::error::CliError;
use crate::paths::{sanitize_path_part, MyOpenPanelsPaths};
use crate::storage::Storage;
use base64::Engine;
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path};

pub const EXECUTION_PROTOCOL_VERSION: i64 = 3;
pub const MAX_TEXT_FILE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_STAGING_BYTES: i64 = 128 * 1024 * 1024;
pub const MAX_WIKI_FILES: usize = 10_000;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ResourceKind {
    WikiMarkdown,
    WikiSpace,
    GeneratedDocument,
    WritingSkill,
}

impl ResourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WikiMarkdown => "wiki_markdown",
            Self::WikiSpace => "wiki_space",
            Self::GeneratedDocument => "generated_document",
            Self::WritingSkill => "writing_skill",
        }
    }

    fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            "wiki_markdown" => Ok(Self::WikiMarkdown),
            "wiki_space" => Ok(Self::WikiSpace),
            "generated_document" => Ok(Self::GeneratedDocument),
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
pub struct PrepareOperationRequest {
    pub operation_id: String,
    pub file_name: String,
    pub content_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeginOperationRequest {
    pub task_id: String,
    pub title: String,
    pub document_format: String,
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
    attempt_id: String,
    staging_session_id: String,
    project_id: String,
    panel_id: String,
    queue: String,
    task_type: String,
    task_capability: String,
    generation: i64,
    input: Value,
    source: Value,
}

#[derive(Debug, Clone)]
struct FileEntry {
    object_hash: String,
    size_bytes: i64,
    mime_type: String,
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
            format!(
                "Task Handler does not allow Agent-side Broker capability {capability}."
            ),
        ))
    }
}

pub fn create_execution_context_in_transaction(
    tx: &Transaction<'_>,
    task_id: &str,
    attempt_id: &str,
    generation: i64,
    expires_at: &str,
) -> Result<(String, String), CliError> {
    let token = format!(
        "execution:{:032x}{:032x}",
        rand::random::<u128>(),
        rand::random::<u128>()
    );
    let staging_id = crate::ids::random_id("staging");
    let now = now_iso();
    tx.execute(
        "UPDATE task_attempts SET execution_token_hash = ?, execution_token_expires_at = ?, staging_session_id = ? WHERE id = ? AND task_id = ? AND execution_generation = ?",
        params![hash_secret(&token), expires_at, staging_id, attempt_id, task_id, generation],
    )
    .map_err(to_cli_error)?;
    tx.execute(
        "INSERT INTO task_staging_sessions (id, task_id, attempt_id, execution_generation, status, created_at, updated_at, expires_at) VALUES (?, ?, ?, ?, 'open', ?, ?, ?)",
        params![staging_id, task_id, attempt_id, generation, now, now, expires_at],
    )
    .map_err(to_cli_error)?;
    seed_declared_output_resource(tx, task_id, &staging_id)?;
    pin_task_inputs_in_transaction(tx, task_id, &now)?;
    Ok((token, staging_id))
}

pub(crate) fn pin_task_inputs_in_transaction(
    connection: &rusqlite::Connection,
    task_id: &str,
    now: &str,
) -> Result<(), CliError> {
    connection.execute(
        r#"
        INSERT OR IGNORE INTO content_pins (task_id, revision_id, created_at)
        SELECT ti.task_id, r.active_revision_id, ?
        FROM task_inputs ti JOIN tasks t ON t.id = ti.task_id
        JOIN content_resources r ON r.project_id = t.project_id
          AND r.resource_key = ti.resource_id AND r.active_revision_id IS NOT NULL
          AND r.resource_kind = CASE ti.resource_kind
            WHEN 'wiki.rawDocument' THEN 'wiki_markdown'
            WHEN 'wiki.generatedDocument' THEN 'generated_document'
            WHEN 'writing.targetDocument' THEN 'generated_document'
            WHEN 'writing.skill' THEN 'writing_skill'
            ELSE '' END
        WHERE ti.task_id = ?
          AND (
            ti.content_hash IS NULL OR EXISTS (
              SELECT 1 FROM content_revision_files input_file
              WHERE input_file.revision_id = r.active_revision_id
                AND input_file.object_hash = ti.content_hash
            )
          )
          AND NOT EXISTS (
            SELECT 1
            FROM content_pins existing_pin
            JOIN content_revisions existing_revision
              ON existing_revision.id = existing_pin.revision_id
            JOIN content_resources existing_resource
              ON existing_resource.id = existing_revision.content_resource_id
            WHERE existing_pin.task_id = ti.task_id
              AND existing_resource.resource_kind = r.resource_kind
              AND existing_resource.resource_key = r.resource_key
          )
        "#,
        params![now, task_id],
    )
    .map_err(to_cli_error)?;
    Ok(())
}

pub fn abandon_task_staging_in_transaction(
    tx: &Transaction<'_>,
    task_id: &str,
    now: &str,
) -> Result<(), CliError> {
    tx.execute(
        "UPDATE task_staging_sessions SET status = 'abandoned', abandoned_at = ?, updated_at = ? WHERE task_id = ? AND status IN ('open', 'prepared')",
        params![now, now, task_id],
    )
    .map_err(to_cli_error)?;
    tx.execute(
        "UPDATE task_attempts SET execution_token_hash = NULL, execution_token_expires_at = NULL WHERE task_id = ? AND status = 'leased'",
        [task_id],
    )
    .map_err(to_cli_error)?;
    tx.execute(
        "UPDATE agent_operations SET status = 'cancelled', error_json = json_object('message', 'Task staging was abandoned.'), completed_at = ?, updated_at = ? WHERE status = 'prepared' AND json_extract(input_json, '$.taskId') = ?",
        params![now, now, task_id],
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

fn stage_file_internal(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &StageFileRequest,
    domain_validated: bool,
) -> Result<Value, CliError> {
    let kind = ResourceKind::parse(&request.resource_kind)?;
    if !domain_validated
        && matches!(
            kind,
            ResourceKind::GeneratedDocument | ResourceKind::WritingSkill
        )
    {
        return Err(CliError::with_code(
            "invalid_broker_route",
            "Generated documents and Writing Skills must use their domain preparation endpoint.",
        ));
    }
    validate_logical_path(&request.logical_path)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&request.content_base64)
        .map_err(|_| {
            CliError::with_code("invalid_output", "Staged content is not valid base64.")
        })?;
    if bytes.len() > MAX_TEXT_FILE_BYTES {
        return Err(CliError::with_code(
            "content_too_large",
            format!("A staged text file cannot exceed {MAX_TEXT_FILE_BYTES} bytes."),
        ));
    }
    std::str::from_utf8(&bytes).map_err(|_| {
        CliError::with_code("invalid_output", "Staged content must be valid UTF-8 text.")
    })?;
    let context = authorize(paths, execution_token)?;
    let mut storage = Storage::open(paths)?;
    validate_resource_access(
        storage.connection(),
        &context,
        kind,
        &request.resource_key,
        true,
    )?;
    let object = write_object(paths, &bytes)?;
    let tx = storage
        .connection_mut()
        .transaction()
        .map_err(to_cli_error)?;
    authorize_in_transaction(&tx, execution_token, &context)?;
    ensure_staging_resource(
        &tx,
        &context,
        kind,
        &request.resource_key,
        &request.metadata,
    )?;
    let previous_size = tx
        .query_row(
            "SELECT size_bytes FROM task_staged_files WHERE staging_session_id = ? AND resource_kind = ? AND resource_key = ? AND logical_path = ?",
            params![context.staging_session_id, kind.as_str(), request.resource_key, request.logical_path],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(to_cli_error)?
        .unwrap_or(0);
    let current_total = tx
        .query_row(
            "SELECT total_bytes FROM task_staging_sessions WHERE id = ?",
            [&context.staging_session_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(to_cli_error)?;
    let next_total = current_total - previous_size + object.size_bytes;
    if next_total > MAX_STAGING_BYTES {
        return Err(CliError::with_code(
            "content_too_large",
            format!("An Attempt cannot stage more than {MAX_STAGING_BYTES} bytes."),
        ));
    }
    let now = now_iso();
    tx.execute(
        r#"
        INSERT INTO task_staged_files (
          staging_session_id, resource_kind, resource_key, logical_path,
          object_hash, size_bytes, mime_type, operation, metadata_json, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, 'upsert', ?, ?, ?)
        ON CONFLICT(staging_session_id, resource_kind, resource_key, logical_path) DO UPDATE SET
          object_hash = excluded.object_hash, size_bytes = excluded.size_bytes,
          mime_type = excluded.mime_type, operation = 'upsert',
          metadata_json = excluded.metadata_json, updated_at = excluded.updated_at
        "#,
        params![
            context.staging_session_id,
            kind.as_str(),
            request.resource_key,
            request.logical_path,
            object.object_hash,
            object.size_bytes,
            request.mime_type,
            request.metadata.to_string(),
            now,
            now,
        ],
    )
    .map_err(to_cli_error)?;
    tx.execute(
        "UPDATE task_staging_sessions SET total_bytes = ?, updated_at = ? WHERE id = ? AND status IN ('open', 'prepared')",
        params![next_total, now, context.staging_session_id],
    )
    .map_err(to_cli_error)?;
    tx.commit().map_err(to_cli_error)?;
    Ok(json!({
        "staged": true,
        "taskId": context.task_id,
        "attemptId": context.attempt_id,
        "executionGeneration": context.generation,
        "resourceKind": kind.as_str(),
        "resourceKey": request.resource_key,
        "logicalPath": request.logical_path,
        "contentHash": object.object_hash,
        "sizeBytes": object.size_bytes,
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
    let storage = Storage::open(paths)?;
    let access = validate_resource_access(
        storage.connection(),
        &context,
        kind,
        &request.resource_key,
        false,
    )?;
    let staged = storage
        .connection()
        .query_row(
            r#"
            SELECT sf.object_hash, sf.mime_type
            FROM task_staged_files sf
            WHERE sf.staging_session_id = ? AND sf.resource_kind = ?
              AND sf.resource_key = ? AND sf.logical_path = ? AND sf.operation = 'upsert'
            "#,
            params![
                context.staging_session_id,
                kind.as_str(),
                request.resource_key,
                request.logical_path
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(to_cli_error)?;
    let entry = match staged {
        Some(value) => Some(value),
        None if access == ResourceAccess::Input => pinned_task_file_entry(
            storage.connection(),
            &context.task_id,
            kind,
            &request.resource_key,
            &request.logical_path,
        )?,
        None => staged_output_base_file_entry(
            storage.connection(),
            &context.staging_session_id,
            kind,
            &request.resource_key,
            &request.logical_path,
        )?,
    };
    let Some((object_hash, mime_type)) = entry else {
        return Err(CliError::with_code(
            "content_not_found",
            format!("Content file not found: {}", request.logical_path),
        ));
    };
    let bytes = read_object(paths, &object_hash)?;
    Ok(json!({
        "resourceKind": kind.as_str(),
        "resourceKey": request.resource_key,
        "logicalPath": request.logical_path,
        "mimeType": mime_type,
        "contentBase64": base64::engine::general_purpose::STANDARD.encode(bytes),
        "contentHash": object_hash,
    }))
}

pub fn prepare_operation(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &PrepareOperationRequest,
) -> Result<Value, CliError> {
    let context = authorize(paths, execution_token)?;
    let mut storage = Storage::open(paths)?;
    let operation = storage
        .read_agent_operation(&request.operation_id)?
        .ok_or_else(|| {
            CliError::with_code("operation_not_found", "Writing Operation not found.")
        })?;
    if operation.pointer("/input/taskId").and_then(Value::as_str) != Some(&context.task_id)
        || operation.get("status").and_then(Value::as_str) != Some("active")
    {
        return Err(CliError::with_code(
            "execution_fenced",
            "The Operation does not belong to this active Attempt.",
        ));
    }
    let document_id = operation
        .pointer("/target/documentId")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::with_code("invalid_output", "Operation target is missing."))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&request.content_base64)
        .map_err(|_| {
            CliError::with_code("invalid_output", "Operation content is not valid base64.")
        })?;
    let logical_path = if request.file_name.to_ascii_lowercase().ends_with(".txt") {
        "content.txt"
    } else {
        "content.md"
    };
    let staged = stage_file_internal(
        paths,
        execution_token,
        &StageFileRequest {
            resource_kind: ResourceKind::GeneratedDocument.as_str().to_owned(),
            resource_key: document_id.to_owned(),
            logical_path: logical_path.to_owned(),
            content_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
            mime_type: if logical_path.ends_with(".txt") {
                "text/plain"
            } else {
                "text/markdown"
            }
            .to_owned(),
            metadata: json!({
                "operationId": request.operation_id,
                "fileName": sanitize_path_part(&request.file_name),
                "baseContentVersion": operation.pointer("/target/baseContentVersion").cloned().unwrap_or(Value::Null),
                "runtimeOutputPlanHash": operation.pointer("/input/runtimeOutputPlanHash").cloned().unwrap_or(Value::Null),
            }),
        },
        true,
    )?;
    let result = json!({
        "stagingSessionId": context.staging_session_id,
        "contentHash": staged["contentHash"],
        "fileName": request.file_name,
    });
    let result_json = serde_json::to_string(&result).map_err(to_cli_error)?;
    let now = now_iso();
    let tx = storage
        .connection_mut()
        .transaction()
        .map_err(to_cli_error)?;
    authorize_in_transaction(&tx, execution_token, &context)?;
    let changed = tx
        .execute(
            r#"
            UPDATE agent_operations
            SET status = 'prepared', result_json = ?, error_json = NULL,
                updated_at = ?, completed_at = NULL
            WHERE id = ? AND status = 'active'
              AND json_extract(input_json, '$.taskId') = ?
            "#,
            params![result_json, now, request.operation_id, context.task_id],
        )
        .map_err(to_cli_error)?;
    if changed != 1 {
        return Err(CliError::with_code(
            "execution_fenced",
            "The Writing Operation is no longer active for this Attempt.",
        ));
    }
    crate::storage::record_scope(
        &tx,
        "agent_operations",
        Some(&context.project_id),
        Some(&context.panel_id),
    )?;
    tx.commit().map_err(to_cli_error)?;
    let mut operation = operation;
    operation["status"] = json!("prepared");
    operation["updatedAt"] = json!(now);
    operation["result"] = result;
    Ok(json!({ "operation": operation, "staged": staged }))
}

pub fn begin_operation(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &BeginOperationRequest,
) -> Result<Value, CliError> {
    let context = authorize(paths, execution_token)?;
    if context.task_id != request.task_id || context.task_type != "generate_document" {
        return Err(CliError::with_code(
            "execution_fenced",
            "The execution token cannot begin this Writing Operation.",
        ));
    }
    crate::operations::begin_writing_for_broker(
        paths,
        &request.task_id,
        &request.title,
        &request.document_format,
    )
}

pub fn publishing_checkpoint(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &PublishingCheckpointRequest,
) -> Result<Value, CliError> {
    let context = authorize(paths, execution_token)?;
    if context.task_id != request.task_id
        || !crate::publishing::is_publishing_task_type(&context.task_type)
    {
        return Err(CliError::with_code(
            "execution_fenced",
            "The execution token cannot checkpoint this Publishing Task.",
        ));
    }
    crate::publishing::checkpoint_attempt_for_broker(paths, &request.task_id, &request.phase)
}

pub fn read_task_context(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &TaskContextRequest,
) -> Result<Value, CliError> {
    let context = authorize(paths, execution_token)?;
    if context.task_id != request.task_id {
        return Err(CliError::with_code(
            "execution_fenced",
            "The execution token belongs to another Task.",
        ));
    }
    match request.context_kind.as_str() {
        "writing_request" => crate::writing::read_request(paths, &request.task_id),
        "writing_refinement" => crate::writing::read_refinement(paths, &request.task_id),
        _ => Err(CliError::with_code(
            "invalid_task_context",
            "Unsupported Task context request.",
        )),
    }
}

pub fn read_skill(
    paths: &MyOpenPanelsPaths,
    execution_token: &str,
    request: &SkillReadRequest,
) -> Result<Value, CliError> {
    let context = authorize(paths, execution_token)?;
    if context.task_id != request.task_id {
        return Err(CliError::with_code(
            "execution_fenced",
            "The execution token belongs to another Task.",
        ));
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
    let expected_id = context
        .input
        .get("skillId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let expected_name = context
        .input
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if request.skill_id != expected_id {
        return Err(CliError::with_code(
            "writing_skill_file_invalid",
            "Generated Writing Skill does not match the claimed refinement Task.",
        ));
    }
    crate::agent::validate_portable_writing_skill(
        &request.source,
        "SKILL.md",
        expected_id,
    )?;
    let mut manifest = json!({
        "schemaVersion": 2,
        "source": "custom",
        "originProjectId": context.project_id,
        "taskId": context.task_id,
        "skillId": expected_id,
        "name": expected_name,
        "binding": {
            "appliesTo": ["writing"],
            "taskTypes": ["generate_document"],
        },
        "createdAt": now_iso(),
    });
    if let Some(output_plan_hash) = request
        .manifest
        .get("runtimeOutputPlanHash")
        .and_then(Value::as_str)
    {
        manifest["runtimeOutputPlanHash"] = json!(output_plan_hash);
    }
    let first = stage_file_internal(
        paths,
        execution_token,
        &StageFileRequest {
            resource_kind: ResourceKind::WritingSkill.as_str().to_owned(),
            resource_key: request.skill_id.clone(),
            logical_path: "SKILL.md".to_owned(),
            content_base64: base64::engine::general_purpose::STANDARD
                .encode(request.source.as_bytes()),
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
    let storage = Storage::open(paths)?;
    storage.connection().execute(
        "UPDATE task_staging_sessions SET status = 'prepared', updated_at = ? WHERE id = (SELECT staging_session_id FROM task_attempts WHERE task_id = ? AND status = 'leased')",
        params![now_iso(), first["taskId"].as_str().unwrap_or_default()],
    ).map_err(to_cli_error)?;
    Ok(json!({ "skillId": request.skill_id, "staged": true, "contentHash": first["contentHash"] }))
}

pub fn task_has_staged_resource(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    kind: ResourceKind,
) -> Result<bool, CliError> {
    let storage = Storage::open(paths)?;
    storage
        .connection()
        .query_row(
            r#"
            SELECT EXISTS(
              SELECT 1 FROM task_staging_resources sr
              JOIN task_staging_sessions ss ON ss.id = sr.staging_session_id
              JOIN task_attempts a ON a.id = ss.attempt_id
              JOIN tasks t ON t.id = ss.task_id AND t.execution_generation = a.execution_generation
              WHERE ss.task_id = ? AND sr.resource_kind = ? AND ss.status IN ('open', 'prepared')
            )
            "#,
            params![task_id, kind.as_str()],
            |row| row.get::<_, bool>(0),
        )
        .map_err(to_cli_error)
}

pub fn staged_files_for_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    kind: ResourceKind,
) -> Result<Vec<(String, String, Vec<u8>, Value)>, CliError> {
    let storage = Storage::open(paths)?;
    let mut statement = storage
        .connection()
        .prepare(
            r#"
        SELECT sf.resource_key, sf.logical_path, sf.object_hash, sf.metadata_json
        FROM task_staged_files sf
        JOIN task_staging_sessions ss ON ss.id = sf.staging_session_id
        JOIN task_attempts a ON a.id = ss.attempt_id
        JOIN tasks t ON t.id = ss.task_id AND t.execution_generation = a.execution_generation
        WHERE ss.task_id = ? AND sf.resource_kind = ? AND sf.operation = 'upsert'
          AND ss.status IN ('open', 'prepared')
        ORDER BY sf.resource_key, sf.logical_path
        "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map(params![task_id, kind.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(to_cli_error)?;
    rows.map(|row| {
        let (key, path, hash, metadata) = row.map_err(to_cli_error)?;
        Ok((
            key,
            path,
            read_object(paths, &hash)?,
            serde_json::from_str(&metadata).unwrap_or_else(|_| json!({})),
        ))
    })
    .collect()
}
