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

#[derive(Debug, Clone)]
struct ExecutionContext {
    task_id: String,
    attempt_id: String,
    staging_session_id: String,
    project_id: String,
    panel_id: String,
    task_type: String,
    capability: String,
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
    let staging_id = format!("staging:{:032x}", rand::random::<u128>());
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
    seed_existing_output_resource(tx, task_id, &staging_id)?;
    tx.execute(
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
        "#,
        params![now, task_id],
    )
    .map_err(to_cli_error)?;
    Ok((token, staging_id))
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
    validate_resource_scope(&context, kind, &request.resource_key, true)?;
    ensure_legacy_resource(paths, &context, kind, &request.resource_key)?;
    let object = write_object(paths, &bytes)?;
    let mut storage = Storage::open(paths)?;
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
    validate_resource_scope(&context, kind, &request.resource_key, false)?;
    ensure_legacy_resource(paths, &context, kind, &request.resource_key)?;
    let storage = Storage::open(paths)?;
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
        None => active_file_entry(
            storage.connection(),
            &context.project_id,
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
    let expected_title = context
        .input
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let parsed = crate::agent::parse_skill(&request.source, "SKILL.md")?;
    if request.skill_id != expected_id
        || parsed.metadata.id != expected_id
        || parsed.metadata.title != expected_title
        || parsed.metadata.source != "custom"
        || parsed.metadata.applies_to != ["writing"]
        || parsed.metadata.task_types != ["generate_document"]
        || parsed.body.trim().is_empty()
    {
        return Err(CliError::with_code(
            "writing_skill_file_invalid",
            "Generated Writing Skill does not match the claimed refinement Task.",
        ));
    }
    let manifest = json!({
        "schemaVersion": 1,
        "source": "custom",
        "originProjectId": context.project_id,
        "taskId": context.task_id,
        "skillId": expected_id,
        "title": expected_title,
        "createdAt": now_iso(),
    });
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

pub fn commit_task_staging_in_transaction(
    paths: &MyOpenPanelsPaths,
    tx: &Transaction<'_>,
    task_id: &str,
    now: &str,
) -> Result<Vec<Value>, CliError> {
    let attempt = tx
        .query_row(
            r#"
        SELECT a.id, a.execution_generation, a.staging_session_id, t.capability
        FROM tasks t JOIN task_attempts a
          ON a.task_id = t.id AND a.execution_generation = t.execution_generation
        WHERE t.id = ? AND a.status = 'leased'
        "#,
            [task_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()
        .map_err(to_cli_error)?
        .ok_or_else(|| {
            CliError::with_code("execution_fenced", "The active Attempt no longer exists.")
        })?;
    let staging_id = attempt
        .2
        .ok_or_else(|| CliError::with_code("invalid_output", "The Task has no staging session."))?;
    let mut statement = tx.prepare(
        "SELECT resource_kind, resource_key, content_resource_id, base_revision_id, base_content_version, metadata_json FROM task_staging_resources WHERE staging_session_id = ? ORDER BY resource_kind, resource_key"
    ).map_err(to_cli_error)?;
    let resources = statement
        .query_map([&staging_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    let staged_file_count = tx
        .query_row(
            "SELECT COUNT(*) FROM task_staged_files WHERE staging_session_id = ?",
            [&staging_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(to_cli_error)?;
    if (resources.is_empty() || staged_file_count == 0) && is_content_capability(&attempt.3) {
        return Err(CliError::with_code(
            "invalid_output",
            "The Task completed without staged content.",
        ));
    }
    let mut committed = Vec::new();
    for (
        kind_text,
        resource_key,
        existing_resource_id,
        base_revision_id,
        base_version,
        metadata_json,
    ) in resources
    {
        let kind = ResourceKind::parse(&kind_text)?;
        let current = tx.query_row(
            "SELECT id, active_revision_id, content_version FROM content_resources WHERE project_id = (SELECT project_id FROM tasks WHERE id = ?) AND resource_kind = ? AND resource_key = ?",
            params![task_id, kind.as_str(), resource_key],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, i64>(2)?)),
        ).optional().map_err(to_cli_error)?;
        if current.as_ref().and_then(|value| value.1.as_deref()) != base_revision_id.as_deref()
            || current.as_ref().map(|value| value.2).unwrap_or(0) != base_version
        {
            return Err(CliError::with_code(
                "content_conflict",
                format!("Content changed while the Task was running: {resource_key}"),
            ));
        }
        let mut manifest = if matches!(
            kind,
            ResourceKind::WikiMarkdown | ResourceKind::GeneratedDocument
        ) {
            BTreeMap::new()
        } else {
            base_manifest(tx, base_revision_id.as_deref())?
        };
        let mut staged = tx.prepare(
            "SELECT logical_path, object_hash, size_bytes, mime_type, operation FROM task_staged_files WHERE staging_session_id = ? AND resource_kind = ? AND resource_key = ? ORDER BY logical_path"
        ).map_err(to_cli_error)?;
        let staged_files = staged
            .query_map(params![staging_id, kind.as_str(), resource_key], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(to_cli_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_cli_error)?;
        for (path, hash, size, mime, operation) in staged_files {
            if operation == "delete" {
                manifest.remove(&path);
            } else {
                manifest.insert(
                    path,
                    FileEntry {
                        object_hash: hash.ok_or_else(|| {
                            CliError::with_code("invalid_output", "Staged file has no object.")
                        })?,
                        size_bytes: size,
                        mime_type: mime.unwrap_or_else(|| "application/octet-stream".to_owned()),
                    },
                );
            }
        }
        validate_manifest(paths, &tx, kind, &resource_key, &manifest)?;
        let task_scope = tx
            .query_row(
                "SELECT project_id, panel_id FROM tasks WHERE id = ?",
                [task_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .map_err(to_cli_error)?;
        let metadata: Value = serde_json::from_str(&metadata_json).unwrap_or_else(|_| json!({}));
        let panel_id = metadata
            .get("targetPanelId")
            .and_then(Value::as_str)
            .unwrap_or(&task_scope.1);
        let resource_id = existing_resource_id
            .or_else(|| current.as_ref().map(|value| value.0.clone()))
            .unwrap_or_else(|| format!("content-resource:{:032x}", rand::random::<u128>()));
        tx.execute(
            "INSERT OR IGNORE INTO content_resources (id, project_id, panel_id, resource_kind, resource_key, content_version, created_at, updated_at) VALUES (?, ?, ?, ?, ?, 0, ?, ?)",
            params![resource_id, task_scope.0, panel_id, kind.as_str(), resource_key, now, now],
        ).map_err(to_cli_error)?;
        let revision_number = base_version + 1;
        let revision_id = format!("content-revision:{:032x}", rand::random::<u128>());
        let manifest_json = manifest_value(&manifest);
        let manifest_text = serde_json::to_string(&manifest_json).map_err(to_cli_error)?;
        let manifest_hash = format!("{:x}", Sha256::digest(manifest_text.as_bytes()));
        if let Some(previous) = base_revision_id.as_deref() {
            tx.execute("UPDATE content_revisions SET status = 'prunable', prunable_at = ? WHERE id = ? AND status = 'active'", params![now, previous]).map_err(to_cli_error)?;
        }
        tx.execute(
            "INSERT INTO content_revisions (id, content_resource_id, parent_revision_id, revision_number, manifest_json, manifest_hash, status, source_task_id, source_attempt_id, execution_generation, created_at, activated_at) VALUES (?, ?, ?, ?, ?, ?, 'active', ?, ?, ?, ?, ?)",
            params![revision_id, resource_id, base_revision_id, revision_number, manifest_text, manifest_hash, task_id, attempt.0, attempt.1, now, now],
        ).map_err(to_cli_error)?;
        for (path, entry) in &manifest {
            tx.execute("INSERT INTO content_revision_files (revision_id, logical_path, object_hash, size_bytes, mime_type) VALUES (?, ?, ?, ?, ?)", params![revision_id, path, entry.object_hash, entry.size_bytes, entry.mime_type]).map_err(to_cli_error)?;
        }
        tx.execute("UPDATE content_resources SET active_revision_id = ?, content_version = ?, updated_at = ? WHERE id = ?", params![revision_id, revision_number, now, resource_id]).map_err(to_cli_error)?;
        committed.push(json!({ "resourceKind": kind.as_str(), "resourceKey": resource_key, "revisionId": revision_id, "contentVersion": revision_number, "manifestHash": manifest_hash }));
    }
    tx.execute("UPDATE task_staging_sessions SET status = 'committed', committed_at = ?, updated_at = ? WHERE id = ? AND status IN ('open', 'prepared')", params![now, now, staging_id]).map_err(to_cli_error)?;
    tx.execute("UPDATE task_attempts SET execution_token_hash = NULL, execution_token_expires_at = NULL WHERE id = ?", [&attempt.0]).map_err(to_cli_error)?;
    let commits_json = serde_json::to_string(&committed).map_err(to_cli_error)?;
    tx.execute(
        "UPDATE agent_operations SET status = 'completed', result_json = json_set(COALESCE(result_json, '{}'), '$.committed', json('true'), '$.contentCommits', json(?)), completed_at = ?, updated_at = ? WHERE status = 'prepared' AND json_extract(input_json, '$.taskId') = ?",
        params![commits_json, now, now, task_id],
    ).map_err(to_cli_error)?;
    Ok(committed)
}

pub fn read_active_text(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
) -> Result<Option<String>, CliError> {
    let storage = Storage::open(paths)?;
    let Some((hash, _)) = active_file_entry(
        storage.connection(),
        project_id,
        kind,
        resource_key,
        logical_path,
    )?
    else {
        return Ok(None);
    };
    String::from_utf8(read_object(paths, &hash)?)
        .map(Some)
        .map_err(|_| CliError::with_code("invalid_content", "Stored content is not UTF-8."))
}

pub fn commit_immediate_text(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: Option<&str>,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
    content: &[u8],
    mime_type: &str,
    replace_all: bool,
) -> Result<Value, CliError> {
    validate_logical_path(logical_path)?;
    if content.len() > MAX_TEXT_FILE_BYTES || std::str::from_utf8(content).is_err() {
        return Err(CliError::with_code(
            "invalid_output",
            "Content must be bounded UTF-8 text.",
        ));
    }
    let object = write_object(paths, content)?;
    let storage = Storage::open_without_content_migration(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let current = tx.query_row(
        "SELECT id, active_revision_id, content_version FROM content_resources WHERE project_id = ? AND resource_kind = ? AND resource_key = ?",
        params![project_id, kind.as_str(), resource_key],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, i64>(2)?)),
    ).optional().map_err(to_cli_error)?;
    let resource_id = current
        .as_ref()
        .map(|value| value.0.clone())
        .unwrap_or_else(|| format!("content-resource:{:032x}", rand::random::<u128>()));
    let parent = current.as_ref().and_then(|value| value.1.clone());
    let version = current.as_ref().map(|value| value.2 + 1).unwrap_or(1);
    let mut manifest = if replace_all {
        BTreeMap::new()
    } else {
        base_manifest(&tx, parent.as_deref())?
    };
    manifest.insert(
        logical_path.to_owned(),
        FileEntry {
            object_hash: object.object_hash.clone(),
            size_bytes: object.size_bytes,
            mime_type: mime_type.to_owned(),
        },
    );
    if kind == ResourceKind::WikiSpace && manifest.len() > MAX_WIKI_FILES {
        return Err(CliError::with_code(
            "content_too_large",
            "Wiki revision contains too many files.",
        ));
    }
    let now = now_iso();
    tx.execute("INSERT OR IGNORE INTO content_resources (id, project_id, panel_id, resource_kind, resource_key, content_version, created_at, updated_at) VALUES (?, ?, ?, ?, ?, 0, ?, ?)", params![resource_id, project_id, panel_id, kind.as_str(), resource_key, now, now]).map_err(to_cli_error)?;
    if let Some(parent) = parent.as_deref() {
        tx.execute("UPDATE content_revisions SET status = 'prunable', prunable_at = ? WHERE id = ? AND status = 'active'", params![now, parent]).map_err(to_cli_error)?;
    }
    let revision_id = format!("content-revision:{:032x}", rand::random::<u128>());
    let manifest_json = manifest_value(&manifest);
    let manifest_text = serde_json::to_string(&manifest_json).map_err(to_cli_error)?;
    let manifest_hash = format!("{:x}", Sha256::digest(manifest_text.as_bytes()));
    tx.execute("INSERT INTO content_revisions (id, content_resource_id, parent_revision_id, revision_number, manifest_json, manifest_hash, status, created_at, activated_at) VALUES (?, ?, ?, ?, ?, ?, 'active', ?, ?)", params![revision_id, resource_id, parent, version, manifest_text, manifest_hash, now, now]).map_err(to_cli_error)?;
    for (path, entry) in &manifest {
        tx.execute("INSERT INTO content_revision_files (revision_id, logical_path, object_hash, size_bytes, mime_type) VALUES (?, ?, ?, ?, ?)", params![revision_id, path, entry.object_hash, entry.size_bytes, entry.mime_type]).map_err(to_cli_error)?;
    }
    tx.execute("UPDATE content_resources SET active_revision_id = ?, content_version = ?, panel_id = COALESCE(panel_id, ?), updated_at = ? WHERE id = ?", params![revision_id, version, panel_id, now, resource_id]).map_err(to_cli_error)?;
    tx.commit().map_err(to_cli_error)?;
    Ok(
        json!({ "revisionId": revision_id, "contentVersion": version, "manifestHash": manifest_hash }),
    )
}

pub fn active_writing_skill_sources(
    paths: &MyOpenPanelsPaths,
) -> Result<Vec<(String, String, Value, String)>, CliError> {
    let storage = Storage::open_without_content_migration(paths)?;
    let mut statement = storage
        .connection()
        .prepare(
            r#"
        SELECT r.resource_key, r.active_revision_id,
               skill.object_hash, manifest.object_hash
        FROM content_resources r
        JOIN content_revision_files skill
          ON skill.revision_id = r.active_revision_id AND skill.logical_path = 'SKILL.md'
        JOIN content_revision_files manifest
          ON manifest.revision_id = r.active_revision_id AND manifest.logical_path = 'manifest.json'
        WHERE r.resource_kind = 'writing_skill' AND r.archived_at IS NULL
        ORDER BY r.resource_key
        "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(to_cli_error)?;
    rows.map(|row| {
        let (id, revision_id, skill_hash, manifest_hash) = row.map_err(to_cli_error)?;
        let source = String::from_utf8(read_object(paths, &skill_hash)?).map_err(|_| {
            CliError::with_code("invalid_custom_skill", "Writing Skill is not UTF-8.")
        })?;
        let manifest: Value =
            serde_json::from_slice(&read_object(paths, &manifest_hash)?).map_err(to_cli_error)?;
        let materialized = paths
            .storage_dir
            .join("content/materialized/writing-skills")
            .join(sanitize_path_part(&id))
            .join(sanitize_path_part(&revision_id));
        fs::create_dir_all(&materialized).map_err(to_cli_error)?;
        let skill_path = materialized.join("SKILL.md");
        let manifest_path = materialized.join("manifest.json");
        if !skill_path.is_file() {
            fs::write(&skill_path, source.as_bytes()).map_err(to_cli_error)?;
        }
        if !manifest_path.is_file() {
            fs::write(
                &manifest_path,
                serde_json::to_vec_pretty(&manifest).map_err(to_cli_error)?,
            )
            .map_err(to_cli_error)?;
        }
        Ok((id, source, manifest, materialized.display().to_string()))
    })
    .collect()
}

pub fn active_writing_skill_dir(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
) -> Option<std::path::PathBuf> {
    active_writing_skill_sources(paths)
        .ok()?
        .into_iter()
        .find(|value| value.0 == skill_id)
        .map(|value| std::path::PathBuf::from(value.3))
}

pub fn writing_skill_project_id(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
) -> Result<Option<String>, CliError> {
    let storage = Storage::open_without_content_migration(paths)?;
    storage.connection().query_row(
        "SELECT project_id FROM content_resources WHERE resource_kind = 'writing_skill' AND resource_key = ? AND archived_at IS NULL",
        [skill_id],
        |row| row.get::<_, String>(0),
    ).optional().map_err(to_cli_error)
}

pub fn archive_resource(
    paths: &MyOpenPanelsPaths,
    project_id: Option<&str>,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<(), CliError> {
    let storage = Storage::open_without_content_migration(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = now_iso();
    tx.execute(
        "UPDATE content_revisions SET status = 'prunable', prunable_at = ? WHERE status = 'active' AND id IN (SELECT active_revision_id FROM content_resources WHERE resource_kind = ? AND resource_key = ? AND (? IS NULL OR project_id = ?))",
        params![now, kind.as_str(), resource_key, project_id, project_id],
    ).map_err(to_cli_error)?;
    tx.execute(
        "UPDATE content_resources SET active_revision_id = NULL, archived_at = ?, updated_at = ? WHERE resource_kind = ? AND resource_key = ? AND (? IS NULL OR project_id = ?)",
        params![now, now, kind.as_str(), resource_key, project_id, project_id],
    ).map_err(to_cli_error)?;
    tx.commit().map_err(to_cli_error)
}

pub fn import_legacy_file(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: Option<&str>,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
    file_path: &Path,
    mime_type: &str,
) -> Result<(), CliError> {
    if !file_path.is_file() {
        return Ok(());
    }
    let storage = Storage::open_without_content_migration(paths)?;
    let exists = storage.connection().query_row(
        "SELECT EXISTS(SELECT 1 FROM content_resources WHERE project_id = ? AND resource_kind = ? AND resource_key = ?)",
        params![project_id, kind.as_str(), resource_key], |row| row.get::<_, bool>(0)
    ).map_err(to_cli_error)?;
    if exists {
        return Ok(());
    }
    let bytes = fs::read(file_path).map_err(to_cli_error)?;
    let object = write_object(paths, &bytes)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let resource_id = format!("content-resource:{:032x}", rand::random::<u128>());
    let revision_id = format!("content-revision:{:032x}", rand::random::<u128>());
    let now = now_iso();
    let manifest = json!({ logical_path: { "objectHash": object.object_hash, "sizeBytes": object.size_bytes, "mimeType": mime_type } });
    let manifest_text = serde_json::to_string(&manifest).map_err(to_cli_error)?;
    let manifest_hash = format!("{:x}", Sha256::digest(manifest_text.as_bytes()));
    tx.execute("INSERT INTO content_resources (id, project_id, panel_id, resource_kind, resource_key, active_revision_id, content_version, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?)", params![resource_id, project_id, panel_id, kind.as_str(), resource_key, revision_id, now, now]).map_err(to_cli_error)?;
    tx.execute("INSERT INTO content_revisions (id, content_resource_id, revision_number, manifest_json, manifest_hash, status, created_at, activated_at) VALUES (?, ?, 1, ?, ?, 'active', ?, ?)", params![revision_id, resource_id, manifest_text, manifest_hash, now, now]).map_err(to_cli_error)?;
    tx.execute("INSERT INTO content_revision_files (revision_id, logical_path, object_hash, size_bytes, mime_type) VALUES (?, ?, ?, ?, ?)", params![revision_id, logical_path, object.object_hash, object.size_bytes, mime_type]).map_err(to_cli_error)?;
    tx.commit().map_err(to_cli_error)
}

pub fn run_legacy_content_migration(
    paths: &MyOpenPanelsPaths,
    storage: &Storage,
) -> Result<(), CliError> {
    let status = storage
        .connection()
        .query_row(
            "SELECT status FROM content_migration_state WHERE id = 'legacy_content_v1'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(to_cli_error)?;
    if status.as_deref() == Some("completed") || status.is_none() {
        return Ok(());
    }
    storage.connection().execute(
        "UPDATE content_migration_state SET status = 'running', updated_at = ? WHERE id = 'legacy_content_v1'",
        [now_iso()],
    ).map_err(to_cli_error)?;
    let panels = {
        let mut statement = storage.connection().prepare(
            "SELECT ps.project_id, ps.panel_id, ps.state_json FROM panel_states ps JOIN panels p ON p.project_id = ps.project_id AND p.id = ps.panel_id WHERE p.kind = 'wiki'"
        ).map_err(to_cli_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(to_cli_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_cli_error)?;
        rows
    };
    let result = (|| -> Result<(), CliError> {
        for (project_id, panel_id, state_json) in panels {
            let state: Value = serde_json::from_str(&state_json).map_err(to_cli_error)?;
            let panel_dir = storage.panel_dir(&project_id, &panel_id);
            for document in state
                .get("rawDocuments")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(id) = document.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let Some(reference) = document.get("markdownRef").and_then(Value::as_str) else {
                    continue;
                };
                let file = panel_dir.join(reference);
                if file.is_file() {
                    import_legacy_file(
                        paths,
                        &project_id,
                        Some(&panel_id),
                        ResourceKind::WikiMarkdown,
                        id,
                        "source.md",
                        &file,
                        "text/markdown",
                    )?;
                }
            }
            for document in state
                .get("generatedDocuments")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(id) = document.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let root = panel_dir.join("generated").join(sanitize_path_part(id));
                let mut files = Vec::new();
                if root.is_dir() {
                    collect_legacy_files(&root, &root, &mut files)?;
                }
                files.retain(|(_, bytes, _)| !bytes.is_empty());
                if !files.is_empty() {
                    import_legacy_files(
                        paths,
                        &project_id,
                        Some(&panel_id),
                        ResourceKind::GeneratedDocument,
                        id,
                        &files,
                    )?;
                }
            }
            for space in state
                .get("wikiSpaces")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(id) = space.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let root = panel_dir
                    .join("wikis")
                    .join(sanitize_path_part(id))
                    .join("pages");
                let mut files = Vec::new();
                if root.is_dir() {
                    collect_legacy_files(&root, &root, &mut files)?;
                }
                if !files.is_empty() {
                    import_legacy_files(
                        paths,
                        &project_id,
                        Some(&panel_id),
                        ResourceKind::WikiSpace,
                        id,
                        &files,
                    )?;
                }
            }
        }
        let skills_root = paths.storage_dir.join("writing-skills");
        if skills_root.is_dir() {
            for entry in fs::read_dir(&skills_root).map_err(to_cli_error)? {
                let entry = entry.map_err(to_cli_error)?;
                if !entry.file_type().map_err(to_cli_error)?.is_dir()
                    || entry.file_name().to_string_lossy().starts_with('.')
                {
                    continue;
                }
                let manifest_path = entry.path().join("manifest.json");
                if !manifest_path.is_file() {
                    continue;
                }
                let manifest: Value =
                    serde_json::from_slice(&fs::read(&manifest_path).map_err(to_cli_error)?)
                        .map_err(to_cli_error)?;
                let Some(project_id) = manifest.get("originProjectId").and_then(Value::as_str)
                else {
                    continue;
                };
                let skill_id = manifest
                    .get("skillId")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| entry.file_name().to_string_lossy().to_string());
                let panel_id = storage
                    .connection()
                    .query_row(
                        "SELECT id FROM panels WHERE project_id = ? AND kind = 'writing' LIMIT 1",
                        [project_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(to_cli_error)?;
                let mut files = Vec::new();
                collect_legacy_files(&entry.path(), &entry.path(), &mut files)?;
                if !files.is_empty() {
                    import_legacy_files(
                        paths,
                        project_id,
                        panel_id.as_deref(),
                        ResourceKind::WritingSkill,
                        &skill_id,
                        &files,
                    )?;
                }
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            let now = now_iso();
            storage.connection().execute("UPDATE content_migration_state SET status = 'completed', checkpoint_json = json_object('completed', 1), updated_at = ?, completed_at = ? WHERE id = 'legacy_content_v1'", params![now, now]).map_err(to_cli_error)?;
            Ok(())
        }
        Err(error) => {
            let _ = storage.connection().execute("UPDATE content_migration_state SET status = 'failed', checkpoint_json = json_object('error', ?), updated_at = ? WHERE id = 'legacy_content_v1'", params![error.message(), now_iso()]);
            Err(error)
        }
    }
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
    let cutoff = (chrono::Utc::now() - chrono::Duration::hours(24))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let storage = Storage::open_without_content_migration(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let revisions = {
        let mut statement = tx.prepare(
            "SELECT id FROM content_revisions r WHERE status = 'prunable' AND prunable_at <= ? AND NOT EXISTS (SELECT 1 FROM content_pins p WHERE p.revision_id = r.id)"
        ).map_err(to_cli_error)?;
        let rows = statement
            .query_map([&cutoff], |row| row.get::<_, String>(0))
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    for revision_id in &revisions {
        tx.execute(
            "DELETE FROM content_revision_files WHERE revision_id = ?",
            [revision_id],
        )
        .map_err(to_cli_error)?;
        tx.execute(
            "UPDATE content_revisions SET status = 'pruned', pruned_at = ? WHERE id = ?",
            params![now_iso(), revision_id],
        )
        .map_err(to_cli_error)?;
    }
    let sessions = tx
        .execute(
            "DELETE FROM task_staging_sessions WHERE status = 'abandoned' AND updated_at <= ?",
            [&cutoff],
        )
        .map_err(to_cli_error)?;
    let objects = {
        let mut statement = tx.prepare(
            "SELECT hash, storage_ref FROM content_objects o WHERE NOT EXISTS (SELECT 1 FROM content_revision_files f WHERE f.object_hash = o.hash) AND NOT EXISTS (SELECT 1 FROM task_staged_files sf WHERE sf.object_hash = o.hash)"
        ).map_err(to_cli_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    for (hash, _) in &objects {
        tx.execute("DELETE FROM content_objects WHERE hash = ?", [hash])
            .map_err(to_cli_error)?;
    }
    tx.commit().map_err(to_cli_error)?;
    for (_, storage_ref) in &objects {
        let _ = fs::remove_file(paths.storage_dir.join(storage_ref));
    }
    Ok(
        json!({ "prunedRevisions": revisions.len(), "removedStagingSessions": sessions, "removedObjects": objects.len() }),
    )
}

pub fn broker_stage_file(request: &StageFileRequest) -> Result<Value, CliError> {
    broker_json("/api/task-broker/v3/stage", request)
}

pub fn broker_read_file(request: &ReadFileRequest) -> Result<Value, CliError> {
    broker_json("/api/task-broker/v3/read", request)
}

pub fn broker_prepare_operation(request: &PrepareOperationRequest) -> Result<Value, CliError> {
    broker_json("/api/task-broker/v3/operations/prepare", request)
}

pub fn broker_begin_operation(request: &BeginOperationRequest) -> Result<Value, CliError> {
    broker_json("/api/task-broker/v3/operations/begin", request)
}

pub fn broker_prepare_skill(request: &PrepareSkillRequest) -> Result<Value, CliError> {
    broker_json("/api/task-broker/v3/skills/prepare", request)
}

pub fn broker_task_context(request: &TaskContextRequest) -> Result<Value, CliError> {
    broker_json("/api/task-broker/v3/task-context", request)
}

pub fn broker_read_skill(request: &SkillReadRequest) -> Result<Value, CliError> {
    broker_json("/api/task-broker/v3/skills/read", request)
}

pub fn broker_execution_available() -> bool {
    std::env::var("MYOPENPANELS_TASK_BROKER_URL").is_ok_and(|value| !value.trim().is_empty())
        && std::env::var("MYOPENPANELS_TASK_TOKEN").is_ok_and(|value| !value.trim().is_empty())
}

pub fn task_execution_detected() -> bool {
    std::env::var("MYOPENPANELS_TASK_ID").is_ok_and(|value| !value.trim().is_empty())
}

pub fn require_broker_for_task_execution() -> Result<(), CliError> {
    if task_execution_detected() && !broker_execution_available() {
        return Err(CliError::with_code(
            "broker_unavailable",
            "Task-scoped content access requires the Studio Task Broker.",
        ));
    }
    Ok(())
}

fn broker_json<T: Serialize>(path: &str, body: &T) -> Result<Value, CliError> {
    let url = std::env::var("MYOPENPANELS_TASK_BROKER_URL").map_err(|_| {
        CliError::with_code(
            "broker_unavailable",
            "This v3 Task requires a running Studio Task Broker.",
        )
    })?;
    let token = std::env::var("MYOPENPANELS_TASK_TOKEN").map_err(|_| {
        CliError::with_code(
            "broker_unavailable",
            "The Task Broker execution token is missing.",
        )
    })?;
    let response = ureq::post(&format!("{}{}", url.trim_end_matches('/'), path))
        .set("authorization", &format!("Bearer {token}"))
        .set("content-type", "application/json")
        .send_json(serde_json::to_value(body).map_err(to_cli_error)?);
    match response {
        Ok(response) => response.into_json::<Value>().map_err(to_cli_error),
        Err(ureq::Error::Status(_, response)) => {
            let payload = response.into_json::<Value>().unwrap_or_else(|_| json!({}));
            Err(CliError::with_code(
                payload
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or("broker_rejected"),
                payload
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("Task Broker rejected the request."),
            ))
        }
        Err(error) => Err(CliError::with_code("broker_unavailable", error.to_string())),
    }
}

fn authorize(paths: &MyOpenPanelsPaths, token: &str) -> Result<ExecutionContext, CliError> {
    let storage = Storage::open(paths)?;
    execution_context(storage.connection(), token)
}

fn authorize_in_transaction(
    tx: &Transaction<'_>,
    token: &str,
    expected: &ExecutionContext,
) -> Result<(), CliError> {
    let actual = execution_context(tx, token)?;
    if actual.attempt_id != expected.attempt_id || actual.generation != expected.generation {
        return Err(CliError::with_code(
            "execution_fenced",
            "Execution generation changed.",
        ));
    }
    Ok(())
}

fn execution_context(
    connection: &rusqlite::Connection,
    token: &str,
) -> Result<ExecutionContext, CliError> {
    let now = now_iso();
    connection
        .query_row(
            r#"
        SELECT t.id, a.id, a.staging_session_id, t.project_id, t.panel_id,
               t.type, t.capability, a.execution_generation, t.input_json, t.source_json
        FROM task_attempts a JOIN tasks t ON t.id = a.task_id
        JOIN task_staging_sessions ss ON ss.id = a.staging_session_id
        WHERE a.execution_token_hash = ? AND a.status = 'leased'
          AND a.execution_generation = t.execution_generation
          AND a.execution_token_expires_at > ? AND t.lease_expires_at > ?
          AND t.status IN ('running', 'claimed', 'converting', 'indexing')
          AND ss.status IN ('open', 'prepared')
        "#,
            params![hash_secret(token), now, now],
            |row| {
                let input: String = row.get(8)?;
                let source: String = row.get(9)?;
                Ok(ExecutionContext {
                    task_id: row.get(0)?,
                    attempt_id: row.get(1)?,
                    staging_session_id: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    project_id: row.get(3)?,
                    panel_id: row.get(4)?,
                    task_type: row.get(5)?,
                    capability: row.get(6)?,
                    generation: row.get(7)?,
                    input: serde_json::from_str(&input).unwrap_or_else(|_| json!({})),
                    source: serde_json::from_str(&source).unwrap_or_else(|_| json!({})),
                })
            },
        )
        .optional()
        .map_err(to_cli_error)?
        .ok_or_else(|| {
            CliError::with_code(
                "execution_fenced",
                "The execution token is invalid, expired, or fenced.",
            )
        })
}

fn validate_resource_scope(
    context: &ExecutionContext,
    kind: ResourceKind,
    key: &str,
    write: bool,
) -> Result<(), CliError> {
    let capability_allows_kind = match kind {
        ResourceKind::WikiMarkdown => {
            context.capability == "wiki.convertDocument"
                || (!write && context.capability == "wiki.ingestMarkdown")
        }
        ResourceKind::WikiSpace => matches!(
            context.capability.as_str(),
            "wiki.ingestMarkdown" | "wiki.maintain" | "wiki.rebuildIndex"
        ),
        ResourceKind::GeneratedDocument => context.capability == "writing.generateDocument",
        ResourceKind::WritingSkill => context.capability == "writing.refineSkill",
    };
    let allowed = match kind {
        ResourceKind::WikiMarkdown => {
            context.input.get("documentId").and_then(Value::as_str) == Some(key)
        }
        ResourceKind::WikiSpace => {
            context
                .source
                .get("wikiSpaceId")
                .or_else(|| context.input.get("wikiSpaceId"))
                .and_then(Value::as_str)
                == Some(key)
        }
        ResourceKind::GeneratedDocument => {
            context
                .input
                .get("targetGeneratedDocumentId")
                .and_then(Value::as_str)
                == Some(key)
                || context.task_type == "generate_document"
        }
        ResourceKind::WritingSkill => {
            context.input.get("skillId").and_then(Value::as_str) == Some(key)
        }
    };
    if !allowed || !capability_allows_kind {
        return Err(CliError::with_code(
            "execution_fenced",
            "The execution token is not scoped to this content resource.",
        ));
    }
    Ok(())
}

fn ensure_staging_resource(
    tx: &Transaction<'_>,
    context: &ExecutionContext,
    kind: ResourceKind,
    key: &str,
    metadata: &Value,
) -> Result<(), CliError> {
    let current = tx.query_row(
        "SELECT id, active_revision_id, content_version FROM content_resources WHERE project_id = ? AND resource_kind = ? AND resource_key = ?",
        params![context.project_id, kind.as_str(), key],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, i64>(2)?)),
    ).optional().map_err(to_cli_error)?;
    let (resource_id, revision_id, version) = current
        .map(|value| (Some(value.0), value.1, value.2))
        .unwrap_or((None, None, 0));
    let mut combined = metadata.clone();
    if kind == ResourceKind::GeneratedDocument {
        combined["targetPanelId"] = context
            .source
            .get("wikiPanelId")
            .cloned()
            .unwrap_or_else(|| json!(context.panel_id));
    }
    tx.execute(
        "INSERT OR IGNORE INTO task_staging_resources (staging_session_id, resource_kind, resource_key, content_resource_id, base_revision_id, base_content_version, metadata_json) VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![context.staging_session_id, kind.as_str(), key, resource_id, revision_id, version, combined.to_string()],
    ).map_err(to_cli_error)?;
    Ok(())
}

fn seed_existing_output_resource(
    tx: &Transaction<'_>,
    task_id: &str,
    staging_id: &str,
) -> Result<(), CliError> {
    let (project_id, panel_id, capability, input_json, source_json) = tx.query_row(
        "SELECT project_id, panel_id, capability, input_json, source_json FROM tasks WHERE id = ?",
        [task_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?)),
    ).map_err(to_cli_error)?;
    let input: Value = serde_json::from_str(&input_json).unwrap_or_else(|_| json!({}));
    let source: Value = serde_json::from_str(&source_json).unwrap_or_else(|_| json!({}));
    let output = match capability.as_str() {
        "wiki.convertDocument" => input
            .get("documentId")
            .and_then(Value::as_str)
            .map(|key| (ResourceKind::WikiMarkdown, key, panel_id.as_str())),
        "wiki.ingestMarkdown" | "wiki.maintain" | "wiki.rebuildIndex" => source
            .get("wikiSpaceId")
            .or_else(|| input.get("wikiSpaceId"))
            .and_then(Value::as_str)
            .map(|key| (ResourceKind::WikiSpace, key, panel_id.as_str())),
        "writing.generateDocument" => input
            .get("targetGeneratedDocumentId")
            .and_then(Value::as_str)
            .map(|key| {
                (
                    ResourceKind::GeneratedDocument,
                    key,
                    source
                        .get("wikiPanelId")
                        .and_then(Value::as_str)
                        .unwrap_or(panel_id.as_str()),
                )
            }),
        "writing.refineSkill" => input
            .get("skillId")
            .and_then(Value::as_str)
            .map(|key| (ResourceKind::WritingSkill, key, panel_id.as_str())),
        _ => None,
    };
    let Some((kind, key, target_panel_id)) = output else {
        return Ok(());
    };
    let current = tx.query_row(
        "SELECT id, active_revision_id, content_version FROM content_resources WHERE project_id = ? AND resource_kind = ? AND resource_key = ? AND archived_at IS NULL",
        params![project_id, kind.as_str(), key],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, i64>(2)?)),
    ).optional().map_err(to_cli_error)?;
    let Some((resource_id, revision_id, version)) = current else {
        return Ok(());
    };
    tx.execute(
        "INSERT OR IGNORE INTO task_staging_resources (staging_session_id, resource_kind, resource_key, content_resource_id, base_revision_id, base_content_version, metadata_json) VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![staging_id, kind.as_str(), key, resource_id, revision_id, version, json!({ "targetPanelId": target_panel_id }).to_string()],
    ).map_err(to_cli_error)?;
    Ok(())
}

fn ensure_legacy_resource(
    paths: &MyOpenPanelsPaths,
    context: &ExecutionContext,
    kind: ResourceKind,
    key: &str,
) -> Result<(), CliError> {
    let storage = Storage::open_without_content_migration(paths)?;
    let exists = storage.connection().query_row(
        "SELECT EXISTS(SELECT 1 FROM content_resources WHERE project_id = ? AND resource_kind = ? AND resource_key = ?)",
        params![context.project_id, kind.as_str(), key],
        |row| row.get::<_, bool>(0),
    ).map_err(to_cli_error)?;
    if exists {
        return Ok(());
    }
    let panel_id = if kind == ResourceKind::GeneratedDocument {
        context
            .source
            .get("wikiPanelId")
            .and_then(Value::as_str)
            .unwrap_or(&context.panel_id)
    } else {
        &context.panel_id
    };
    let panel_dir = storage.panel_dir(&context.project_id, panel_id);
    let root = match kind {
        ResourceKind::WikiMarkdown => panel_dir.join("raw").join(sanitize_path_part(key)),
        ResourceKind::WikiSpace => panel_dir
            .join("wikis")
            .join(sanitize_path_part(key))
            .join("pages"),
        ResourceKind::GeneratedDocument => {
            panel_dir.join("generated").join(sanitize_path_part(key))
        }
        ResourceKind::WritingSkill => paths
            .storage_dir
            .join("writing-skills")
            .join(sanitize_path_part(key)),
    };
    if kind == ResourceKind::WikiMarkdown {
        let source = root.join("source.md");
        if source.is_file() {
            return import_legacy_file(
                paths,
                &context.project_id,
                Some(panel_id),
                kind,
                key,
                "source.md",
                &source,
                "text/markdown",
            );
        }
        return Ok(());
    }
    if !root.is_dir() {
        return Ok(());
    }
    let mut files = Vec::new();
    collect_legacy_files(&root, &root, &mut files)?;
    if kind == ResourceKind::GeneratedDocument {
        files.retain(|(_, bytes, _)| !bytes.is_empty());
    }
    if files.is_empty() {
        return Ok(());
    }
    import_legacy_files(
        paths,
        &context.project_id,
        Some(panel_id),
        kind,
        key,
        &files,
    )
}

fn collect_legacy_files(
    root: &Path,
    directory: &Path,
    output: &mut Vec<(String, Vec<u8>, String)>,
) -> Result<(), CliError> {
    for entry in fs::read_dir(directory).map_err(to_cli_error)? {
        let entry = entry.map_err(to_cli_error)?;
        let metadata = entry.file_type().map_err(to_cli_error)?;
        if metadata.is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_legacy_files(root, &entry.path(), output)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(to_cli_error)?
            .components()
            .filter_map(|part| match part {
                Component::Normal(value) => value.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");
        validate_logical_path(&relative)?;
        let bytes = fs::read(entry.path()).map_err(to_cli_error)?;
        if bytes.len() > MAX_TEXT_FILE_BYTES || std::str::from_utf8(&bytes).is_err() {
            continue;
        }
        let mime = mime_guess::from_path(&relative)
            .first_or_text_plain()
            .essence_str()
            .to_owned();
        output.push((relative, bytes, mime));
    }
    Ok(())
}

fn import_legacy_files(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: Option<&str>,
    kind: ResourceKind,
    resource_key: &str,
    files: &[(String, Vec<u8>, String)],
) -> Result<(), CliError> {
    let objects = files
        .iter()
        .map(|(path, bytes, mime)| Ok((path.clone(), write_object(paths, bytes)?, mime.clone())))
        .collect::<Result<Vec<_>, CliError>>()?;
    let storage = Storage::open_without_content_migration(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let exists = tx.query_row("SELECT EXISTS(SELECT 1 FROM content_resources WHERE project_id = ? AND resource_kind = ? AND resource_key = ?)", params![project_id, kind.as_str(), resource_key], |row| row.get::<_, bool>(0)).map_err(to_cli_error)?;
    if exists {
        return tx.commit().map_err(to_cli_error);
    }
    let resource_id = format!("content-resource:{:032x}", rand::random::<u128>());
    let revision_id = format!("content-revision:{:032x}", rand::random::<u128>());
    let manifest = Value::Object(objects.iter().map(|(path, object, mime)| (path.clone(), json!({ "objectHash": object.object_hash, "sizeBytes": object.size_bytes, "mimeType": mime }))).collect());
    let manifest_text = serde_json::to_string(&manifest).map_err(to_cli_error)?;
    let manifest_hash = format!("{:x}", Sha256::digest(manifest_text.as_bytes()));
    let now = now_iso();
    tx.execute("INSERT INTO content_resources (id, project_id, panel_id, resource_kind, resource_key, active_revision_id, content_version, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?)", params![resource_id, project_id, panel_id, kind.as_str(), resource_key, revision_id, now, now]).map_err(to_cli_error)?;
    tx.execute("INSERT INTO content_revisions (id, content_resource_id, revision_number, manifest_json, manifest_hash, status, created_at, activated_at) VALUES (?, ?, 1, ?, ?, 'active', ?, ?)", params![revision_id, resource_id, manifest_text, manifest_hash, now, now]).map_err(to_cli_error)?;
    for (path, object, mime) in objects {
        tx.execute("INSERT INTO content_revision_files (revision_id, logical_path, object_hash, size_bytes, mime_type) VALUES (?, ?, ?, ?, ?)", params![revision_id, path, object.object_hash, object.size_bytes, mime]).map_err(to_cli_error)?;
    }
    tx.commit().map_err(to_cli_error)
}

fn validate_manifest(
    paths: &MyOpenPanelsPaths,
    tx: &Transaction<'_>,
    kind: ResourceKind,
    resource_key: &str,
    manifest: &BTreeMap<String, FileEntry>,
) -> Result<(), CliError> {
    if manifest.is_empty() {
        return Err(CliError::with_code(
            "invalid_output",
            "Content manifest is empty.",
        ));
    }
    if kind == ResourceKind::WikiSpace {
        if manifest.len() > MAX_WIKI_FILES {
            return Err(CliError::with_code(
                "content_too_large",
                "Wiki revision contains too many files.",
            ));
        }
    }
    if kind == ResourceKind::WikiMarkdown
        && (manifest.len() != 1 || !manifest.contains_key("source.md"))
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Converted Markdown must contain exactly source.md.",
        ));
    }
    if kind == ResourceKind::GeneratedDocument && manifest.len() != 1 {
        return Err(CliError::with_code(
            "invalid_output",
            "Generated document must contain exactly one content file.",
        ));
    }
    if kind == ResourceKind::WritingSkill
        && (!manifest.contains_key("SKILL.md") || !manifest.contains_key("manifest.json"))
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Writing Skill must contain SKILL.md and manifest.json.",
        ));
    }
    if kind == ResourceKind::WritingSkill {
        validate_writing_skill_manifest(paths, tx, resource_key, manifest)?;
    }
    for (logical_path, entry) in manifest {
        let bytes = read_object(paths, &entry.object_hash)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| {
            CliError::with_code(
                "invalid_output",
                format!("Content is not UTF-8: {logical_path}"),
            )
        })?;
        if matches!(
            kind,
            ResourceKind::WikiMarkdown | ResourceKind::GeneratedDocument
        ) && text.trim().is_empty()
        {
            return Err(CliError::with_code(
                "invalid_output",
                format!("Content is empty: {logical_path}"),
            ));
        }
        if kind == ResourceKind::WikiSpace && logical_path.ends_with(".md") {
            validate_wiki_links(logical_path, text, manifest)?;
        }
    }
    Ok(())
}

fn validate_writing_skill_manifest(
    paths: &MyOpenPanelsPaths,
    tx: &Transaction<'_>,
    resource_key: &str,
    manifest: &BTreeMap<String, FileEntry>,
) -> Result<(), CliError> {
    let skill_entry = manifest
        .get("SKILL.md")
        .ok_or_else(|| CliError::with_code("invalid_output", "Writing Skill has no SKILL.md."))?;
    let manifest_entry = manifest.get("manifest.json").ok_or_else(|| {
        CliError::with_code("invalid_output", "Writing Skill has no manifest.json.")
    })?;
    let source = String::from_utf8(read_object(paths, &skill_entry.object_hash)?)
        .map_err(|_| CliError::with_code("invalid_output", "Writing Skill is not valid UTF-8."))?;
    let parsed = crate::agent::parse_skill(&source, "SKILL.md")?;
    let server_manifest: Value =
        serde_json::from_slice(&read_object(paths, &manifest_entry.object_hash)?).map_err(
            |_| CliError::with_code("invalid_output", "Writing Skill manifest is invalid."),
        )?;
    if parsed.metadata.id != resource_key
        || parsed.metadata.source != "custom"
        || parsed.metadata.applies_to != ["writing"]
        || parsed.metadata.task_types != ["generate_document"]
        || parsed.body.trim().is_empty()
        || server_manifest.get("skillId").and_then(Value::as_str) != Some(resource_key)
        || server_manifest.get("title").and_then(Value::as_str)
            != Some(parsed.metadata.title.as_str())
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Writing Skill metadata does not match its resource identity.",
        ));
    }
    let mut statement = tx
        .prepare(
            r#"
            SELECT f.object_hash
            FROM content_resources r
            JOIN content_revision_files f ON f.revision_id = r.active_revision_id
            WHERE r.resource_kind = 'writing_skill' AND r.resource_key <> ?
              AND r.archived_at IS NULL AND f.logical_path = 'manifest.json'
            "#,
        )
        .map_err(to_cli_error)?;
    let hashes = statement
        .query_map([resource_key], |row| row.get::<_, String>(0))
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    for hash in hashes {
        let existing: Value = match serde_json::from_slice(&read_object(paths, &hash)?) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if existing.get("title").and_then(Value::as_str) == Some(parsed.metadata.title.as_str()) {
            return Err(CliError::with_code(
                "invalid_output",
                "Another active Writing Skill already uses this title.",
            ));
        }
    }
    Ok(())
}

fn validate_wiki_links(
    page_path: &str,
    markdown: &str,
    manifest: &BTreeMap<String, FileEntry>,
) -> Result<(), CliError> {
    let mut remaining = markdown;
    while let Some(start) = remaining.find("](") {
        remaining = &remaining[start + 2..];
        let Some(end) = remaining.find(')') else {
            break;
        };
        let target = remaining[..end]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .split('#')
            .next()
            .unwrap_or("");
        remaining = &remaining[end + 1..];
        if target.is_empty()
            || target.starts_with('#')
            || target.starts_with('/')
            || target.contains("://")
            || target.starts_with("mailto:")
            || !target.to_ascii_lowercase().ends_with(".md")
        {
            continue;
        }
        let parent = Path::new(page_path)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        let joined = parent.join(target);
        let mut normalized = Vec::new();
        for component in joined.components() {
            match component {
                Component::Normal(value) => normalized.push(value.to_string_lossy().to_string()),
                Component::ParentDir if normalized.pop().is_some() => {}
                Component::CurDir => {}
                _ => {
                    return Err(CliError::with_code(
                        "invalid_output",
                        format!("Unsafe Wiki link in {page_path}: {target}"),
                    ))
                }
            }
        }
        let resolved = normalized.join("/");
        if !manifest.contains_key(&resolved) {
            return Err(CliError::with_code(
                "invalid_output",
                format!("Broken Wiki link in {page_path}: {target}"),
            ));
        }
    }
    Ok(())
}

fn base_manifest(
    tx: &Transaction<'_>,
    revision_id: Option<&str>,
) -> Result<BTreeMap<String, FileEntry>, CliError> {
    let Some(revision_id) = revision_id else {
        return Ok(BTreeMap::new());
    };
    let mut statement = tx.prepare("SELECT logical_path, object_hash, size_bytes, mime_type FROM content_revision_files WHERE revision_id = ? ORDER BY logical_path").map_err(to_cli_error)?;
    let rows = statement
        .query_map([revision_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                FileEntry {
                    object_hash: row.get(1)?,
                    size_bytes: row.get(2)?,
                    mime_type: row.get(3)?,
                },
            ))
        })
        .map_err(to_cli_error)?;
    rows.collect::<Result<BTreeMap<_, _>, _>>()
        .map_err(to_cli_error)
}

fn manifest_value(manifest: &BTreeMap<String, FileEntry>) -> Value {
    Value::Object(manifest.iter().map(|(path, entry)| (path.clone(), json!({ "objectHash": entry.object_hash, "sizeBytes": entry.size_bytes, "mimeType": entry.mime_type }))).collect())
}

fn active_file_entry(
    connection: &rusqlite::Connection,
    project_id: &str,
    kind: ResourceKind,
    key: &str,
    logical_path: &str,
) -> Result<Option<(String, String)>, CliError> {
    connection
        .query_row(
            r#"
        SELECT f.object_hash, f.mime_type FROM content_resources r
        JOIN content_revision_files f ON f.revision_id = r.active_revision_id
        WHERE r.project_id = ? AND r.resource_kind = ? AND r.resource_key = ?
          AND f.logical_path = ? AND r.archived_at IS NULL
        "#,
            params![project_id, kind.as_str(), key, logical_path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(to_cli_error)
}

struct StoredObject {
    object_hash: String,
    size_bytes: i64,
}

fn write_object(paths: &MyOpenPanelsPaths, bytes: &[u8]) -> Result<StoredObject, CliError> {
    let object_hash = format!("{:x}", Sha256::digest(bytes));
    let relative = format!(
        "content/objects/sha256/{}/{}",
        &object_hash[..2],
        object_hash
    );
    let path = paths.storage_dir.join(&relative);
    if !path.is_file() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(to_cli_error)?;
        }
        let mut temp =
            tempfile::NamedTempFile::new_in(path.parent().unwrap()).map_err(to_cli_error)?;
        temp.write_all(bytes).map_err(to_cli_error)?;
        temp.as_file().sync_all().map_err(to_cli_error)?;
        match temp.persist_noclobber(&path) {
            Ok(_) => {}
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(to_cli_error(error.error)),
        }
    }
    let storage = Storage::open_without_content_migration(paths)?;
    storage.connection().execute(
        "INSERT OR IGNORE INTO content_objects (hash, size_bytes, storage_ref, created_at) VALUES (?, ?, ?, ?)",
        params![object_hash, bytes.len() as i64, relative, now_iso()],
    ).map_err(to_cli_error)?;
    Ok(StoredObject {
        object_hash,
        size_bytes: bytes.len() as i64,
    })
}

fn read_object(paths: &MyOpenPanelsPaths, object_hash: &str) -> Result<Vec<u8>, CliError> {
    if object_hash.len() != 64 || !object_hash.chars().all(|value| value.is_ascii_hexdigit()) {
        return Err(CliError::with_code(
            "invalid_content",
            "Stored object hash is invalid.",
        ));
    }
    fs::read(
        paths
            .storage_dir
            .join("content/objects/sha256")
            .join(&object_hash[..2])
            .join(object_hash),
    )
    .map_err(to_cli_error)
}

fn validate_logical_path(path: &str) -> Result<(), CliError> {
    if path.is_empty() || Path::new(path).is_absolute() || path.contains('\\') {
        return Err(CliError::with_code(
            "invalid_content_path",
            "Content path must be a relative POSIX path.",
        ));
    }
    for component in Path::new(path).components() {
        match component {
            Component::Normal(value) if !value.is_empty() => {}
            _ => {
                return Err(CliError::with_code(
                    "invalid_content_path",
                    "Content path contains an unsafe component.",
                ))
            }
        }
    }
    Ok(())
}

fn is_content_capability(capability: &str) -> bool {
    matches!(
        capability,
        "wiki.convertDocument"
            | "wiki.ingestMarkdown"
            | "wiki.rebuildIndex"
            | "writing.generateDocument"
            | "writing.refineSkill"
    )
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{ensure_project_bootstrap, BootstrapRequest};
    use crate::paths::resolve_myopenpanels_paths;
    use crate::types::PanelKind;

    #[test]
    fn converted_markdown_is_invisible_until_atomic_task_commit() {
        let temp = tempfile::tempdir().expect("temp");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("content-broker-test"),
        )
        .expect("paths");
        let bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let uploaded = crate::wiki::add_raw_document(
            &paths,
            "source.pdf",
            Some("Source"),
            Some("application/pdf"),
            "user",
            Some("wiki:default"),
            b"pdf fixture",
        )
        .expect("upload");
        let document_id = uploaded["document"]["id"].as_str().unwrap();
        let storage = Storage::open(&paths).expect("storage");
        let tasks = storage.list_tasks(&bootstrap.project.id).expect("tasks");
        let task_id = tasks
            .iter()
            .find(|task| task["type"] == "convert_document_to_markdown")
            .and_then(|task| task["id"].as_str())
            .expect("conversion task");
        storage
            .connection()
            .execute(
                "UPDATE tasks SET required_protocol_version = 3 WHERE id = ?",
                [task_id],
            )
            .expect("protocol");
        std::env::set_var("MYOPENPANELS_TASK_BROKER_URL", "http://127.0.0.1:9");
        let target = crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "v3-converter",
                host: Some("test"),
                transport: "poll",
                capabilities: vec!["wiki.convertDocument".to_owned()],
                priority: 0,
                protocol_version: 3,
                max_concurrency: 1,
                model_gateway_connection_id: None,
            },
        )
        .expect("target");
        let claim =
            crate::tasks::claim_task(&paths, task_id, target["target"]["id"].as_str().unwrap())
                .expect("claim");
        assert_eq!(claim["executionProtocolVersion"], 3);
        let execution_token = claim["executionToken"].as_str().expect("execution token");
        stage_file(
            &paths,
            execution_token,
            &StageFileRequest {
                resource_kind: ResourceKind::WikiMarkdown.as_str().to_owned(),
                resource_key: document_id.to_owned(),
                logical_path: "source.md".to_owned(),
                content_base64: base64::engine::general_purpose::STANDARD.encode(b"# Converted\n"),
                mime_type: "text/markdown".to_owned(),
                metadata: json!({}),
            },
        )
        .expect("stage");
        assert_eq!(
            crate::wiki::read_markdown(&paths, document_id).expect("before")["markdown"],
            ""
        );
        crate::tasks::complete_task(&paths, task_id, claim["leaseToken"].as_str().unwrap(), None)
            .expect("complete");
        assert_eq!(
            crate::wiki::read_markdown(&paths, document_id).expect("after")["markdown"],
            "# Converted\n"
        );
        let fenced = stage_file(
            &paths,
            execution_token,
            &StageFileRequest {
                resource_kind: ResourceKind::WikiMarkdown.as_str().to_owned(),
                resource_key: document_id.to_owned(),
                logical_path: "source.md".to_owned(),
                content_base64: base64::engine::general_purpose::STANDARD.encode(b"late"),
                mime_type: "text/markdown".to_owned(),
                metadata: json!({}),
            },
        )
        .expect_err("completed Attempt must be fenced");
        assert_eq!(fenced.code(), Some("execution_fenced"));
        let session_status: String = storage
            .connection()
            .query_row(
                "SELECT status FROM task_staging_sessions WHERE task_id = ?",
                [task_id],
                |row| row.get(0),
            )
            .expect("staging status");
        assert_eq!(session_status, "committed");
        std::env::remove_var("MYOPENPANELS_TASK_BROKER_URL");
    }

    #[test]
    fn prepared_writing_document_is_invisible_until_task_commit() {
        let temp = tempfile::tempdir().expect("temp");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("writing-content-broker-test"),
        )
        .expect("paths");
        let initial = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id),
            },
        )
        .expect("writing panel");
        let created = crate::writing::create_requests(
            &paths,
            "Write an atomic report",
            "create",
            None,
            &["writing-default".to_owned()],
        )
        .expect("request");
        let task_id = created["tasks"][0]["id"].as_str().expect("task id");
        let document_id = created["documents"][0]["id"].as_str().expect("document id");
        Storage::open(&paths)
            .expect("storage")
            .connection()
            .execute(
                "UPDATE tasks SET required_protocol_version = 3 WHERE id = ?",
                [task_id],
            )
            .expect("protocol");
        std::env::set_var("MYOPENPANELS_TASK_BROKER_URL", "http://127.0.0.1:9");
        let target = crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "v3-writer",
                host: Some("test"),
                transport: "poll",
                capabilities: vec!["writing.generateDocument".to_owned()],
                priority: 0,
                protocol_version: 3,
                max_concurrency: 1,
                model_gateway_connection_id: None,
            },
        )
        .expect("target");
        let claim = crate::tasks::claim_task(
            &paths,
            task_id,
            target["target"]["id"].as_str().expect("target id"),
        )
        .expect("claim");
        let execution_token = claim["executionToken"].as_str().expect("execution token");
        let started = begin_operation(
            &paths,
            execution_token,
            &BeginOperationRequest {
                task_id: task_id.to_owned(),
                title: "Atomic report".to_owned(),
                document_format: "markdown".to_owned(),
            },
        )
        .expect("begin");
        let operation_id = started["operation"]["id"].as_str().expect("operation id");
        prepare_operation(
            &paths,
            execution_token,
            &PrepareOperationRequest {
                operation_id: operation_id.to_owned(),
                file_name: "report.md".to_owned(),
                content_base64: base64::engine::general_purpose::STANDARD
                    .encode(b"# Atomic report\n\nCommitted once.\n"),
            },
        )
        .expect("prepare");
        assert_eq!(
            crate::wiki::read_generated_document(&paths, document_id).expect("before")["content"],
            ""
        );
        crate::tasks::complete_task(
            &paths,
            task_id,
            claim["leaseToken"].as_str().expect("lease"),
            None,
        )
        .expect("complete");
        assert_eq!(
            crate::wiki::read_generated_document(&paths, document_id).expect("after")["content"],
            "# Atomic report\n\nCommitted once.\n"
        );
        std::env::remove_var("MYOPENPANELS_TASK_BROKER_URL");
    }
}
