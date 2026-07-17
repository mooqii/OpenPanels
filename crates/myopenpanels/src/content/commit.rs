pub fn commit_task_staging_in_transaction(
    paths: &MyOpenPanelsPaths,
    tx: &Transaction<'_>,
    task_id: &str,
    now: &str,
    allow_empty: bool,
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
    if staged_file_count == 0 && allow_empty {
        tx.execute(
            "UPDATE task_staging_sessions SET status = 'committed', committed_at = ?, updated_at = ? WHERE id = ? AND status IN ('open', 'prepared')",
            params![now, now, staging_id],
        )
        .map_err(to_cli_error)?;
        tx.execute(
            "UPDATE task_attempts SET execution_token_hash = NULL, execution_token_expires_at = NULL WHERE id = ?",
            [&attempt.0],
        )
        .map_err(to_cli_error)?;
        return Ok(Vec::new());
    }
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
            .unwrap_or_else(|| crate::ids::random_id("content-resource"));
        tx.execute(
            "INSERT OR IGNORE INTO content_resources (id, project_id, panel_id, resource_kind, resource_key, content_version, created_at, updated_at) VALUES (?, ?, ?, ?, ?, 0, ?, ?)",
            params![resource_id, task_scope.0, panel_id, kind.as_str(), resource_key, now, now],
        ).map_err(to_cli_error)?;
        let revision_number = base_version + 1;
        let revision_id = crate::ids::random_id("content-revision");
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

pub(crate) fn pinned_task_input_text(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
) -> Result<Option<String>, CliError> {
    let storage = Storage::open(paths)?;
    let object_hash = storage
        .connection()
        .query_row(
            r#"
            SELECT file.object_hash
            FROM content_pins pin
            JOIN content_revisions revision ON revision.id = pin.revision_id
            JOIN content_resources resource ON resource.id = revision.content_resource_id
            JOIN content_revision_files file ON file.revision_id = revision.id
            WHERE pin.task_id = ? AND resource.resource_kind = ?
              AND resource.resource_key = ? AND file.logical_path = ?
            ORDER BY revision.revision_number DESC
            LIMIT 1
            "#,
            params![task_id, kind.as_str(), resource_key, logical_path],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(to_cli_error)?;
    let Some(object_hash) = object_hash else {
        return Ok(None);
    };
    String::from_utf8(read_object(paths, &object_hash)?)
        .map(Some)
        .map_err(|_| CliError::with_code("invalid_content", "Stored content is not UTF-8."))
}

pub(crate) fn pinned_task_input_paths(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<Vec<String>, CliError> {
    let storage = Storage::open(paths)?;
    let mut statement = storage
        .connection()
        .prepare(
            r#"
            SELECT file.logical_path
            FROM content_pins pin
            JOIN content_revisions revision ON revision.id = pin.revision_id
            JOIN content_resources resource ON resource.id = revision.content_resource_id
            JOIN content_revision_files file ON file.revision_id = revision.id
            WHERE pin.task_id = ? AND resource.resource_kind = ?
              AND resource.resource_key = ?
            ORDER BY file.logical_path
            "#,
        )
        .map_err(to_cli_error)?;
    let paths = statement
        .query_map(params![task_id, kind.as_str(), resource_key], |row| {
            row.get::<_, String>(0)
        })
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    Ok(paths)
}

fn pinned_task_file_entry(
    connection: &rusqlite::Connection,
    task_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
) -> Result<Option<(String, String)>, CliError> {
    connection
        .query_row(
            r#"
            SELECT file.object_hash, file.mime_type
            FROM content_pins pin
            JOIN content_revisions revision ON revision.id = pin.revision_id
            JOIN content_resources resource ON resource.id = revision.content_resource_id
            JOIN content_revision_files file ON file.revision_id = revision.id
            WHERE pin.task_id = ? AND resource.resource_kind = ?
              AND resource.resource_key = ? AND file.logical_path = ?
            ORDER BY revision.revision_number DESC
            LIMIT 1
            "#,
            params![task_id, kind.as_str(), resource_key, logical_path],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(to_cli_error)
}

pub(crate) fn task_wiki_base_paths(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    wiki_space_id: &str,
) -> Result<Vec<String>, CliError> {
    let storage = Storage::open(paths)?;
    let mut statement = storage
        .connection()
        .prepare(
            r#"
            SELECT file.logical_path
            FROM task_staging_resources resource
            JOIN task_staging_sessions session ON session.id = resource.staging_session_id
            JOIN task_attempts attempt ON attempt.id = session.attempt_id
            JOIN tasks task ON task.id = session.task_id
              AND task.execution_generation = attempt.execution_generation
            JOIN content_revision_files file ON file.revision_id = resource.base_revision_id
            WHERE session.task_id = ? AND resource.resource_kind = 'wiki_space'
              AND resource.resource_key = ? AND session.status IN ('open', 'prepared')
            ORDER BY file.logical_path
            "#,
        )
        .map_err(to_cli_error)?;
    let paths = statement
        .query_map(params![task_id, wiki_space_id], |row| {
            row.get::<_, String>(0)
        })
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    Ok(paths)
}
