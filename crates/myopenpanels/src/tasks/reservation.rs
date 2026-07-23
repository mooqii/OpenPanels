fn read_task_inputs(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Vec<Value>, CliError> {
    let storage = Storage::open(paths)?;
    let (input_json, created_at) = storage
        .connection()
        .query_row(
            "SELECT input_json, created_at FROM tasks WHERE id = ?",
            [task_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(to_cli_error)?;
    let input: Value = serde_json::from_str(&input_json).unwrap_or_else(|_| json!({}));
    let mut manifest = Vec::new();
    if let Some(document_id) = input.get("documentId").and_then(Value::as_str) {
        manifest.push(json!({
            "resourceKind": if input.get("documentKind").and_then(Value::as_str) == Some("my_document") { "myDocument" } else { "wiki.rawDocument" },
            "resourceId": document_id,
            "resourceVersion": input.get("markdownVersion"),
            "contentHash": input.get("contentHash"),
            "createdAt": created_at,
        }));
    }
    if let Some(snapshot) = input.get("contextSnapshot") {
        manifest.push(json!({
            "resourceKind": "task.contextSnapshot",
            "resourceId": task_id,
            "snapshot": snapshot,
            "createdAt": created_at,
        }));
    }
    if let Some(skill) = input.get("writingSkillSnapshot") {
        manifest.push(json!({
            "resourceKind": "writing.skill",
            "resourceId": skill.get("id"),
            "contentHash": skill.get("contentHash"),
            "snapshot": skill,
            "createdAt": created_at,
        }));
    }
    Ok(manifest)
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
            SELECT t.handler_key, t.attempt_count, t.current_runner_key, t.lease_owner,
                   t.lease_expires_at, t.project_id, COALESCE(t.origin_panel_id, ''), t.status,
                   t.execution_generation
            FROM tasks t WHERE t.id = ?
            "#,
            [task_id],
            |row| {
                let handler_key = row.get::<_, String>(0)?;
                let route = crate::capabilities::task_route_for_handler(&handler_key)
                    .ok()
                    .flatten();
                Ok(json!({
                    "handlerKey": handler_key,
                    "queue": route.map(|route| route.queue.as_str()).unwrap_or(""),
                    "attempt": row.get::<_, i64>(1)?,
                    "targetId": row.get::<_, Option<String>>(2)?,
                    "tokenHash": row.get::<_, Option<String>>(3)?,
                    "expiresAt": row.get::<_, Option<String>>(4)?,
                    "projectId": row.get::<_, String>(5)?,
                    "panelId": row.get::<_, String>(6)?,
                    "status": row.get::<_, String>(7)?,
                    "executionGeneration": row.get::<_, i64>(8)?,
                }))
            },
        )
        .optional()
        .map_err(to_cli_error)?
        .ok_or_else(|| CliError::with_code("task_not_found", format!("Project task not found: {task_id}")))?;
    if lease["status"].as_str() != Some("running") {
        return Err(CliError::with_code(
            "execution_fenced",
            "Task execution has been cancelled, replaced, or completed.",
        ));
    }
    if lease["tokenHash"].as_str() != Some(hash_secret(lease_token).as_str()) {
        return Err(CliError::with_code("invalid_lease", "Task lease token is invalid."));
    }
    if lease["expiresAt"]
        .as_str()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .is_none_or(|value| value.with_timezone(&chrono::Utc) <= chrono::Utc::now())
    {
        return Err(CliError::with_code("lease_expired", "Task lease has expired."));
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
        _ => return Err(CliError::with_code(
            "execution_fenced",
            "Task-scoped writes require the active execution lease token.",
        )),
    };
    verify_lease(paths, task_id, &lease_token)?;
    if !crate::content::broker_execution_available() && !cfg!(test) {
        return Err(CliError::with_code(
            "broker_unavailable",
            "Task-scoped writes must use the Studio Task Broker.",
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
    Ok(())
}

pub(crate) fn supersede_tasks_for_changed_resource(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    resource_id: &str,
) -> Result<Vec<String>, CliError> {
    update_tasks_for_resource(
        paths,
        project_id,
        "content",
        resource_id,
        "superseded",
        "content_conflict",
    )
}

fn update_tasks_for_resource(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    resource_kind: &str,
    resource_id: &str,
    status: &str,
    reason_code: &str,
) -> Result<Vec<String>, CliError> {
    let storage = Storage::open(paths)?;
    let tx = storage.connection().unchecked_transaction().map_err(to_cli_error)?;
    let mut statement = tx
        .prepare(
            r#"
            SELECT DISTINCT t.id FROM tasks t
            JOIN task_resources tr ON tr.task_id = t.id
            WHERE t.project_id = ? AND t.status IN ('queued', 'running')
              AND tr.resource_id = ?
            "#,
        )
        .map_err(to_cli_error)?;
    let ids = statement
        .query_map(params![project_id, resource_id], |row| row.get::<_, String>(0))
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    drop(statement);
    let now = crate::control::now_iso();
    let reason = json!({
        "code": reason_code,
        "resourceKind": resource_kind,
        "resourceId": resource_id,
    });
    for task_id in &ids {
        tx.execute(
            r#"
            UPDATE tasks SET status = ?, error_json = ?, execution_generation = execution_generation + 1,
              execution_token_hash = NULL, lease_owner = NULL, lease_expires_at = NULL,
              heartbeat_at = NULL, completed_at = ?, updated_at = ? WHERE id = ?
            "#,
            params![status, reason.to_string(), now, now, task_id],
        )
        .map_err(to_cli_error)?;
        tx.execute(
            "UPDATE tasks SET status = 'cancelled', error_json = ?, completed_at = ?, updated_at = ? WHERE depends_on_task_id = ? AND status = 'queued'",
            params![reason.to_string(), now, now, task_id],
        )
        .map_err(to_cli_error)?;
        crate::content::abandon_task_staging_in_transaction(&tx, task_id, &now)?;
    }
    if !ids.is_empty() {
        crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    }
    tx.commit().map_err(to_cli_error)?;
    Ok(ids)
}

pub(crate) fn supersede_active_wiki_mutations(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    mutation_key: &str,
) -> Result<Vec<String>, CliError> {
    let storage = Storage::open(paths)?;
    let tx = storage.connection().unchecked_transaction().map_err(to_cli_error)?;
    let mut statement = tx
        .prepare("SELECT id FROM tasks WHERE project_id = ? AND mutation_key = ? AND status = 'running'")
        .map_err(to_cli_error)?;
    let ids = statement
        .query_map(params![project_id, mutation_key], |row| row.get::<_, String>(0))
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    drop(statement);
    let now = crate::control::now_iso();
    let reason = json!({ "code": "content_conflict", "mutationKey": mutation_key });
    for task_id in &ids {
        tx.execute(
            r#"
            UPDATE tasks SET status = 'superseded', error_json = ?, execution_generation = execution_generation + 1,
              execution_token_hash = NULL, lease_owner = NULL, lease_expires_at = NULL,
              heartbeat_at = NULL, completed_at = ?, updated_at = ? WHERE id = ?
            "#,
            params![reason.to_string(), now, now, task_id],
        )
        .map_err(to_cli_error)?;
        crate::content::abandon_task_staging_in_transaction(&tx, task_id, &now)?;
    }
    if !ids.is_empty() {
        crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    }
    tx.commit().map_err(to_cli_error)?;
    Ok(ids)
}

pub(crate) fn supersede_task_for_content_conflict(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    resource_id: &str,
) -> Result<Value, CliError> {
    let task = inspect_task(paths, task_id)?;
    let project_id = task["task"]["projectId"].as_str().unwrap_or_default();
    if matches!(
        task["task"]["status"].as_str(),
        Some("succeeded" | "cancelled" | "superseded")
    ) {
        return Ok(task);
    }
    let storage = Storage::open(paths)?;
    let tx = storage.connection().unchecked_transaction().map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let reason = json!({ "code": "content_conflict", "resourceId": resource_id });
    tx.execute(
        r#"
        UPDATE tasks SET status = 'superseded', error_json = ?, execution_generation = execution_generation + 1,
          execution_token_hash = NULL, lease_owner = NULL, lease_expires_at = NULL,
          heartbeat_at = NULL, completed_at = ?, updated_at = ? WHERE id = ?
        "#,
        params![reason.to_string(), now, now, task_id],
    )
    .map_err(to_cli_error)?;
    crate::content::abandon_task_staging_in_transaction(&tx, task_id, &now)?;
    crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    inspect_task_in_session(paths, project_id, task_id)
}
