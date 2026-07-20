fn read_task_inputs(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Vec<Value>, CliError> {
    let storage = Storage::open(paths)?;
    let mut statement = storage
        .connection()
        .prepare(
            r#"
        SELECT ti.id, ti.resource_kind, ti.resource_id, ti.resource_version,
               ti.content_hash, ti.snapshot_ref, ti.missing_policy, ti.changed_policy,
               ti.created_at, cr.active_revision_id, cr.content_version, rev.manifest_hash
        FROM task_inputs ti JOIN tasks t ON t.id = ti.task_id
        LEFT JOIN content_resources cr ON cr.project_id = t.project_id
          AND cr.resource_key = ti.resource_id AND cr.archived_at IS NULL
          AND cr.resource_kind = CASE ti.resource_kind
            WHEN 'wiki.rawDocument' THEN 'wiki_markdown'
            WHEN 'wiki.space' THEN 'wiki_space'
            WHEN 'wiki.generatedDocument' THEN 'generated_document'
            WHEN 'writing.targetDocument' THEN 'generated_document'
            WHEN 'writing.skill' THEN 'writing_skill'
            ELSE '' END
        LEFT JOIN content_revisions rev ON rev.id = cr.active_revision_id
        WHERE ti.task_id = ? ORDER BY ti.id
        "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([task_id], |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "resourceKind": row.get::<_, String>(1)?,
                "resourceId": row.get::<_, String>(2)?,
                "resourceVersion": row.get::<_, Option<String>>(3)?,
                "contentHash": row.get::<_, Option<String>>(4)?,
                "snapshotRef": row.get::<_, Option<String>>(5)?,
                "missingPolicy": row.get::<_, String>(6)?,
                "changedPolicy": row.get::<_, String>(7)?,
                "createdAt": row.get::<_, String>(8)?,
                "activeRevisionId": row.get::<_, Option<String>>(9)?,
                "activeContentVersion": row.get::<_, Option<i64>>(10)?,
                "activeManifestHash": row.get::<_, Option<String>>(11)?,
            }))
        })
        .map_err(to_cli_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)
}

fn reserve_task(
    storage: &mut Storage,
    project_id: &str,
    target: &Value,
    requested_task_id: Option<&str>,
    requested_capability: Option<&str>,
    requested_queue: Option<&str>,
) -> Result<Option<ReservedTask>, CliError> {
    let capabilities = target_capabilities(target);
    let now = crate::control::now_iso();
    let tx = storage
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(to_cli_error)?;
    let targets = read_targets_from_connection(&tx, project_id)?;
    tx.execute(
        r#"
        UPDATE tasks
        SET status = CASE WHEN status = 'reserved' THEN 'queued' ELSE status END,
            assigned_agent_id = CASE WHEN status = 'reserved' THEN NULL ELSE assigned_agent_id END
        WHERE project_id = ? AND status = 'reserved' AND updated_at < ?
        "#,
        params![
            project_id,
            (chrono::Utc::now() - chrono::Duration::seconds(30))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        ],
    )
    .map_err(to_cli_error)?;
    let candidates = {
        let mut statement = tx
            .prepare(
                r#"
                SELECT candidate.id, candidate.queue, candidate.status, candidate.capability,
                       candidate.required_protocol_version
                FROM tasks AS candidate
                WHERE candidate.project_id = ?
                  AND candidate.status IN ('queued', 'failed')
                  AND candidate.attempts < candidate.max_attempts
                  AND (candidate.retry_after IS NULL OR candidate.retry_after <= ?)
                  AND (candidate.lease_expires_at IS NULL OR candidate.lease_expires_at <= ?)
                  AND (? IS NULL OR candidate.id = ?)
                  AND (? IS NULL OR candidate.capability = ?)
                  AND (? IS NULL OR candidate.queue = ?)
                  AND (
                    candidate.mutation_key IS NULL
                    OR NOT EXISTS (
                      SELECT 1 FROM tasks AS active
                      WHERE active.project_id = candidate.project_id
                        AND active.mutation_key = candidate.mutation_key
                        AND active.id <> candidate.id
                        AND active.status IN ('reserved', 'running', 'claimed', 'converting', 'indexing')
                    )
                  )
                  AND (
                    candidate.mutation_key IS NULL
                    OR NOT EXISTS (
                      SELECT 1 FROM tasks AS predecessor
                      WHERE predecessor.project_id = candidate.project_id
                        AND predecessor.mutation_key = candidate.mutation_key
                        AND predecessor.mutation_sequence < candidate.mutation_sequence
                        AND (
                          predecessor.status IN ('waiting', 'queued', 'reserved', 'running', 'claimed', 'converting', 'indexing', 'failed')
                        )
                    )
                  )
                ORDER BY CASE candidate.status WHEN 'queued' THEN 0 ELSE 1 END,
                         candidate.updated_at ASC, candidate.id ASC
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map(
                params![
                    project_id,
                    now,
                    now,
                    requested_task_id,
                    requested_task_id,
                    requested_capability,
                    requested_capability,
                    requested_queue,
                    requested_queue,
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    let target_id = target.get("id").and_then(Value::as_str);
    let candidate = candidates
        .into_iter()
        .find(|(task_id, _, _, capability, protocol)| {
            capability_matches_any(&capabilities, capability)
                && preferred_target_id(&tx, project_id, &targets, task_id, capability, *protocol)
                    .ok()
                    .flatten()
                    .as_deref()
                    == target_id
        });
    let Some((id, queue, previous_status, _, required_protocol_version)) = candidate else {
        tx.commit().map_err(to_cli_error)?;
        return Ok(None);
    };
    let changed = tx
        .execute(
            "UPDATE tasks SET status = 'reserved', assigned_agent_id = ?, updated_at = ? WHERE id = ? AND status = ?",
            params![target["id"].as_str(), now, id, previous_status],
        )
        .map_err(to_cli_error)?;
    if changed != 1 {
        tx.commit().map_err(to_cli_error)?;
        return Ok(None);
    }
    tx.commit().map_err(to_cli_error)?;
    Ok(Some(ReservedTask {
        id,
        previous_status,
        queue,
        required_protocol_version,
    }))
}

fn release_reservation(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    reserved: &ReservedTask,
) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    tx.execute(
            "UPDATE tasks SET status = ?, assigned_agent_id = NULL, updated_at = ? WHERE id = ? AND project_id = ? AND status = 'reserved'",
            params![reserved.previous_status, crate::control::now_iso(), reserved.id, project_id],
        )
        .map_err(to_cli_error)?;
    tx.commit().map_err(to_cli_error)
}

fn verify_lease(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
) -> Result<Value, CliError> {
    recover_expired_tasks(paths)?;
    let storage = Storage::open(paths)?;
    let lease = storage
        .connection()
        .query_row(
            r#"
            SELECT queue, attempts, assigned_agent_id, lease_token_hash, lease_expires_at,
                   project_id, panel_id, status, execution_generation, required_protocol_version
            FROM tasks
            WHERE id = ?
            "#,
            params![task_id],
            |row| {
                Ok(json!({
                    "queue": row.get::<_, String>(0)?,
                    "attempt": row.get::<_, i64>(1)?,
                    "targetId": row.get::<_, Option<String>>(2)?,
                    "tokenHash": row.get::<_, Option<String>>(3)?,
                    "expiresAt": row.get::<_, Option<String>>(4)?,
                    "projectId": row.get::<_, String>(5)?,
                    "panelId": row.get::<_, String>(6)?,
                    "status": row.get::<_, String>(7)?,
                    "executionGeneration": row.get::<_, i64>(8)?,
                    "requiredProtocolVersion": row.get::<_, i64>(9)?,
                }))
            },
        )
        .optional()
        .map_err(to_cli_error)?
        .ok_or_else(|| {
            CliError::with_code(
                "task_not_found",
                format!("Project task not found: {task_id}"),
            )
        })?;
    if !matches!(
        lease["status"].as_str(),
        Some("running" | "claimed" | "converting" | "indexing")
    ) {
        return Err(CliError::with_code(
            "execution_fenced",
            "Task execution has been cancelled, replaced, or completed.",
        ));
    }
    if lease["tokenHash"].as_str() != Some(hash_secret(lease_token).as_str()) {
        return Err(CliError::with_code(
            "invalid_lease",
            "Task lease token is invalid.",
        ));
    }
    if lease["expiresAt"]
        .as_str()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .is_none_or(|value| value.with_timezone(&chrono::Utc) <= chrono::Utc::now())
    {
        return Err(CliError::with_code(
            "lease_expired",
            "Task lease has expired.",
        ));
    }
    Ok(lease)
}

pub(crate) fn verify_task_lease(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
) -> Result<Value, CliError> {
    verify_lease(paths, task_id, lease_token)
}

pub(crate) fn verify_task_write_access(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
) -> Result<(), CliError> {
    let lease_token = match std::env::var("MYOPENPANELS_TASK_LEASE_TOKEN") {
        Ok(token) if !token.trim().is_empty() => token,
        _ if cfg!(test) => return Ok(()),
        _ => {
            return Err(CliError::with_code(
                "execution_fenced",
                "Task-scoped writes require the active execution lease token.",
            ))
        }
    };
    let lease = verify_lease(paths, task_id, &lease_token)?;
    if !crate::content::broker_execution_available() && !cfg!(test) {
        return Err(CliError::with_code(
            "broker_unavailable",
            "Execution protocol v3 does not permit direct content writes.",
        ));
    }
    if std::env::var("MYOPENPANELS_TASK_ID")
        .ok()
        .is_some_and(|expected| expected != task_id)
    {
        return Err(CliError::with_code(
            "execution_fenced",
            "The task-scoped token belongs to a different Task.",
        ));
    }
    if lease["status"].as_str().is_none() {
        return Err(CliError::with_code(
            "execution_fenced",
            "Task execution is no longer writable.",
        ));
    }
    Ok(())
}

pub(crate) fn cancel_tasks_for_resource(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    resource_kind: &str,
    resource_id: &str,
    reason_code: &str,
) -> Result<Vec<String>, CliError> {
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let tasks = {
        let mut statement = tx.prepare(
            r#"
            SELECT DISTINCT t.id, t.workflow_run_id, t.status
            FROM tasks t
            LEFT JOIN task_inputs i ON i.task_id = t.id
            WHERE t.project_id = ?
              AND t.status IN ('waiting', 'queued', 'failed', 'reserved', 'running', 'claimed', 'converting', 'indexing')
              AND ((i.resource_kind = ? AND i.resource_id = ? AND i.missing_policy = 'cancel')
                   OR json_extract(t.input_json, '$.documentId') = ?)
            "#,
        ).map_err(to_cli_error)?;
        let rows = statement
            .query_map(
                params![project_id, resource_kind, resource_id, resource_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    let reason =
        json!({ "code": reason_code, "resourceKind": resource_kind, "resourceId": resource_id });
    let mut cancelled = Vec::with_capacity(tasks.len());
    for (task_id, workflow_run_id, previous_status) in tasks {
        tx.execute(
            r#"UPDATE tasks SET status = 'cancelled', assigned_agent_id = NULL,
               lease_owner = NULL, lease_token_hash = NULL, lease_expires_at = NULL,
               last_heartbeat_at = NULL, terminal_reason_json = ?,
               execution_generation = execution_generation + 1,
               completed_at = ?, updated_at = ? WHERE id = ?"#,
            params![reason.to_string(), now, now, task_id],
        )
        .map_err(to_cli_error)?;
        tx.execute(
            "UPDATE task_attempts SET status = 'cancelled', finished_at = ?, error_json = ? WHERE task_id = ? AND status = 'leased'",
            params![now, reason.to_string(), task_id],
        ).map_err(to_cli_error)?;
        tx.execute(
            "INSERT INTO task_events (task_id, workflow_run_id, event_type, from_status, to_status, reason_json, created_at) VALUES (?, ?, 'input_missing', ?, 'cancelled', ?, ?)",
            params![task_id, workflow_run_id, previous_status, reason.to_string(), now],
        ).map_err(to_cli_error)?;
        propagate_prerequisite_failure(&tx, &task_id, "cancelled", &now)?;
        refresh_workflow_run_status(&tx, &workflow_run_id, &now)?;
        cancelled.push(task_id);
    }
    if !cancelled.is_empty() {
        crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    }
    tx.commit().map_err(to_cli_error)?;
    Ok(cancelled)
}

pub(crate) fn supersede_tasks_for_changed_resource(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    resource_id: &str,
) -> Result<Vec<String>, CliError> {
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let tasks = {
        let mut statement = tx.prepare(
            r#"SELECT id, workflow_run_id, status FROM tasks
               WHERE project_id = ? AND capability = 'wiki.ingestMarkdown'
                 AND json_extract(input_json, '$.documentId') = ?
                 AND status IN ('waiting', 'queued', 'failed', 'reserved', 'running', 'claimed', 'indexing')"#,
        ).map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![project_id, resource_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    let reason = json!({ "code": "content_conflict", "resourceId": resource_id });
    let mut superseded = Vec::new();
    for (task_id, workflow_run_id, previous_status) in tasks {
        tx.execute(
            r#"UPDATE tasks SET status = 'superseded', assigned_agent_id = NULL, lease_owner = NULL,
               lease_token_hash = NULL, lease_expires_at = NULL, last_heartbeat_at = NULL,
               execution_generation = execution_generation + 1, terminal_reason_json = ?,
               completed_at = ?, updated_at = ? WHERE id = ?"#,
            params![reason.to_string(), now, now, task_id],
        )
        .map_err(to_cli_error)?;
        tx.execute("UPDATE task_attempts SET status = 'cancelled', finished_at = ?, error_json = ? WHERE task_id = ? AND status = 'leased'", params![now, reason.to_string(), task_id]).map_err(to_cli_error)?;
        tx.execute("INSERT INTO task_events (task_id, workflow_run_id, event_type, from_status, to_status, reason_json, created_at) VALUES (?, ?, 'input_changed', ?, 'superseded', ?, ?)", params![task_id, workflow_run_id, previous_status, reason.to_string(), now]).map_err(to_cli_error)?;
        refresh_workflow_run_status(&tx, &workflow_run_id, &now)?;
        superseded.push(task_id);
    }
    if !superseded.is_empty() {
        crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    }
    tx.commit().map_err(to_cli_error)?;
    Ok(superseded)
}

pub(crate) fn supersede_active_wiki_mutations(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    mutation_key: &str,
) -> Result<Vec<String>, CliError> {
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let tasks = {
        let mut statement = tx
            .prepare(
                r#"SELECT id, workflow_run_id, status FROM tasks
                   WHERE project_id = ? AND mutation_key = ?
                     AND status IN ('reserved', 'running', 'claimed', 'indexing')"#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![project_id, mutation_key], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    let reason = json!({ "code": "content_conflict", "mutationKey": mutation_key });
    let mut superseded = Vec::new();
    for (task_id, workflow_run_id, previous_status) in tasks {
        tx.execute(
            r#"UPDATE tasks SET status = 'superseded', assigned_agent_id = NULL,
               lease_owner = NULL, lease_token_hash = NULL, lease_expires_at = NULL,
               last_heartbeat_at = NULL, execution_generation = execution_generation + 1,
               terminal_reason_json = ?, completed_at = ?, updated_at = ? WHERE id = ?"#,
            params![reason.to_string(), now, now, task_id],
        )
        .map_err(to_cli_error)?;
        tx.execute(
            "UPDATE task_attempts SET status = 'cancelled', finished_at = ?, error_json = ? WHERE task_id = ? AND status = 'leased'",
            params![now, reason.to_string(), task_id],
        )
        .map_err(to_cli_error)?;
        crate::content::abandon_task_staging_in_transaction(&tx, &task_id, &now)?;
        tx.execute(
            "INSERT INTO task_events (task_id, workflow_run_id, event_type, from_status, to_status, reason_json, created_at) VALUES (?, ?, 'content_conflict', ?, 'superseded', ?, ?)",
            params![task_id, workflow_run_id, previous_status, reason.to_string(), now],
        )
        .map_err(to_cli_error)?;
        refresh_workflow_run_status(&tx, &workflow_run_id, &now)?;
        superseded.push(task_id);
    }
    if !superseded.is_empty() {
        crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    }
    tx.commit().map_err(to_cli_error)?;
    Ok(superseded)
}

pub(crate) fn supersede_task_for_content_conflict(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    resource_id: &str,
) -> Result<Value, CliError> {
    let task = inspect_task(paths, task_id)?;
    let project_id = task["task"]["projectId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let workflow_run_id = task["task"]["workflowRunId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let previous_status = task["task"]["status"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    if matches!(
        previous_status.as_str(),
        "succeeded" | "cancelled" | "stale" | "superseded"
    ) {
        return Ok(task);
    }
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let reason = json!({ "code": "content_conflict", "resourceId": resource_id });
    tx.execute(
        r#"UPDATE tasks SET status = 'superseded', assigned_agent_id = NULL,
           lease_owner = NULL, lease_token_hash = NULL, lease_expires_at = NULL,
           last_heartbeat_at = NULL, terminal_reason_json = ?,
           execution_generation = execution_generation + 1,
           completed_at = ?, updated_at = ? WHERE id = ?"#,
        params![reason.to_string(), now, now, task_id],
    )
    .map_err(to_cli_error)?;
    tx.execute(
        "UPDATE task_attempts SET status = 'cancelled', finished_at = ?, error_json = ? WHERE task_id = ? AND status = 'leased'",
        params![now, reason.to_string(), task_id],
    )
    .map_err(to_cli_error)?;
    tx.execute(
        "INSERT INTO task_events (task_id, workflow_run_id, event_type, from_status, to_status, reason_json, created_at) VALUES (?, ?, 'input_changed', ?, 'superseded', ?, ?)",
        params![task_id, workflow_run_id, previous_status, reason.to_string(), now],
    )
    .map_err(to_cli_error)?;
    propagate_prerequisite_failure(&tx, task_id, "superseded", &now)?;
    refresh_workflow_run_status(&tx, &workflow_run_id, &now)?;
    crate::storage::record_scope(&tx, "tasks", Some(&project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    inspect_task_in_session(paths, &project_id, task_id)
}
