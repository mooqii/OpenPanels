pub fn set_task_dispatch(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    mode: &str,
    requested_connection_id: Option<&str>,
) -> Result<Value, CliError> {
    if !matches!(mode, "auto" | "prefer") {
        return Err(CliError::with_code(
            "invalid_dispatch_mode",
            "Dispatch mode must be auto or prefer.",
        ));
    }
    let requested_connection_id = requested_connection_id.filter(|value| !value.trim().is_empty());
    if mode == "auto" && requested_connection_id.is_some() {
        return Err(CliError::with_code(
            "invalid_dispatch_mode",
            "Automatic dispatch cannot pin a model gateway connection.",
        ));
    }
    if mode == "prefer" && requested_connection_id.is_none() {
        return Err(CliError::with_code(
            "invalid_dispatch_mode",
            "Preferred dispatch requires a model gateway connection.",
        ));
    }
    let project_id = task_project_id(paths, task_id)?;
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    if let Some(connection_id) = requested_connection_id {
        let exists = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM model_gateway_connections WHERE id = ? AND enabled = 1)",
                [connection_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(to_cli_error)?;
        if !exists {
            return Err(CliError::with_code(
                "model_gateway_connection_not_found",
                format!("Model gateway connection not found: {connection_id}"),
            ));
        }
    }
    let (status, workflow_id) = tx
        .query_row(
            "SELECT status, workflow_id FROM tasks WHERE id = ? AND project_id = ?",
            params![task_id, project_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(to_cli_error)?;
    if !matches!(status.as_str(), "waiting" | "queued" | "failed") {
        return Err(CliError::with_code(
            "invalid_task_transition",
            "Task dispatch can only change while the Task is waiting or ready.",
        ));
    }
    let now = crate::control::now_iso();
    tx.execute(
        "UPDATE tasks SET dispatch_mode = ?, requested_gateway_connection_id = ?, updated_at = ? WHERE id = ? AND project_id = ?",
        params![mode, requested_connection_id, now, task_id, project_id],
    )
    .map_err(to_cli_error)?;
    let reason = json!({
        "dispatchMode": mode,
        "requestedModelGatewayConnectionId": requested_connection_id,
    });
    tx.execute(
        "INSERT INTO task_events (task_id, workflow_id, event_type, from_status, to_status, reason_json, created_at) VALUES (?, ?, 'dispatch_updated', ?, ?, ?, ?)",
        params![task_id, workflow_id, status, status, reason.to_string(), now],
    )
    .map_err(to_cli_error)?;
    crate::storage::record_scope(&tx, "tasks", Some(&project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    inspect_task_in_session(paths, &project_id, task_id)
}

pub fn set_wiki_update_group_dispatch(
    paths: &MyOpenPanelsPaths,
    mutation_key: &str,
    mode: &str,
    requested_connection_id: Option<&str>,
) -> Result<Value, CliError> {
    if mutation_key.trim().is_empty() {
        return Err(CliError::with_code(
            "invalid_mutation_key",
            "Wiki update group mutation key is required.",
        ));
    }
    if !matches!(mode, "auto" | "prefer")
        || (mode == "auto" && requested_connection_id.is_some())
        || (mode == "prefer" && requested_connection_id.is_none())
    {
        return Err(CliError::with_code(
            "invalid_dispatch_mode",
            "Wiki update group dispatch must be automatic or prefer one connection.",
        ));
    }
    let project_id = read_project_bootstrap(paths, BootstrapRequest::new())?
        .project
        .id;
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    if let Some(connection_id) = requested_connection_id {
        let exists = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM model_gateway_connections WHERE id = ? AND enabled = 1)",
                [connection_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(to_cli_error)?;
        if !exists {
            return Err(CliError::with_code(
                "model_gateway_connection_not_found",
                format!("Model gateway connection not found: {connection_id}"),
            ));
        }
    }
    let now = crate::control::now_iso();
    let member_tasks = {
        let mut statement = tx
            .prepare(
                r#"
                WITH RECURSIVE scoped(id) AS (
                  SELECT id FROM tasks
                  WHERE project_id = ? AND queue = 'wiki' AND mutation_key = ?
                    AND type IN ('ingest_markdown_into_wiki', 'maintain_wiki')
                    AND status IN ('waiting', 'queued', 'failed')
                  UNION
                  SELECT dependencies.prerequisite_task_id
                  FROM task_dependencies dependencies
                  JOIN scoped ON scoped.id = dependencies.task_id
                )
                SELECT tasks.id, tasks.workflow_id, tasks.status FROM tasks
                JOIN scoped ON scoped.id = tasks.id
                WHERE tasks.project_id = ?
                  AND tasks.status IN ('waiting', 'queued', 'failed')
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![project_id, mutation_key, project_id], |row| {
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
    if member_tasks.is_empty() {
        return Err(CliError::with_code(
            "invalid_task_transition",
            "The Wiki update group has no pending Tasks whose dispatch can change.",
        ));
    }
    let changed = tx
        .execute(
            r#"
            WITH RECURSIVE scoped(id) AS (
              SELECT id FROM tasks
              WHERE project_id = ? AND queue = 'wiki' AND mutation_key = ?
                AND type IN ('ingest_markdown_into_wiki', 'maintain_wiki')
                AND status IN ('waiting', 'queued', 'failed')
              UNION
              SELECT dependencies.prerequisite_task_id
              FROM task_dependencies dependencies
              JOIN scoped ON scoped.id = dependencies.task_id
            )
            UPDATE tasks
            SET dispatch_mode = ?, requested_gateway_connection_id = ?, updated_at = ?
            WHERE project_id = ? AND id IN (SELECT id FROM scoped)
              AND status IN ('waiting', 'queued', 'failed')
            "#,
            params![
                project_id,
                mutation_key,
                mode,
                requested_connection_id,
                now,
                project_id
            ],
        )
        .map_err(to_cli_error)?;
    let reason = json!({
        "dispatchMode": mode,
        "requestedModelGatewayConnectionId": requested_connection_id,
        "mutationKey": mutation_key,
    });
    for (task_id, workflow_id, status) in member_tasks {
        tx.execute(
            "INSERT INTO task_events (task_id, workflow_id, event_type, from_status, to_status, reason_json, created_at) VALUES (?, ?, 'dispatch_updated', ?, ?, ?, ?)",
            params![task_id, workflow_id, status, status, reason.to_string(), now],
        )
        .map_err(to_cli_error)?;
    }
    crate::storage::record_scope(&tx, "tasks", Some(&project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    Ok(json!({
        "mutationKey": mutation_key,
        "dispatchMode": mode,
        "requestedGatewayConnectionId": requested_connection_id,
        "updatedTaskCount": changed,
    }))
}

pub fn cancel_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let task = inspect_task(paths, task_id)?;
    if matches!(
        task["task"]["status"].as_str(),
        Some("succeeded" | "cancelled" | "stale" | "superseded")
    ) {
        return Err(CliError::with_code(
            "invalid_task_transition",
            "Terminal tasks cannot be cancelled.",
        ));
    }
    let project_id = task["task"]["projectId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    match task["task"]["queue"].as_str().unwrap_or("") {
        "wiki" => crate::wiki::cancel_task(paths, task_id)?,
        "writing" => crate::writing::cancel_task(paths, task_id)?,
        queue => {
            return Err(CliError::with_code(
                "queue_adapter_missing",
                format!("No task lifecycle adapter is available for queue: {queue}"),
            ))
        }
    };
    finalize_task_runtime(
        paths,
        &project_id,
        task_id,
        "cancelled",
        None,
        Some(json!({ "code": "user_cancelled" })),
        None,
        None,
        None,
        None,
    )?;
    inspect_task_in_session(paths, &project_id, task_id)
}

pub fn list_task_events(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let project_id = task_project_id(paths, task_id)?;
    let storage = Storage::open(paths)?;
    let mut statement = storage
        .connection()
        .prepare(
            r#"
            SELECT id, task_id, workflow_id, event_type, from_status, to_status,
                   reason_json, attempt_id, agent_target_id, created_at
            FROM task_events WHERE task_id = ? ORDER BY id ASC
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([task_id], task_event_from_row)
        .map_err(to_cli_error)?;
    let events = rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?;
    Ok(json!({ "projectId": project_id, "events": events }))
}

pub fn list_task_attempts(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let project_id = task_project_id(paths, task_id)?;
    let storage = Storage::open(paths)?;
    let mut statement = storage
        .connection()
        .prepare(
            r#"
            SELECT a.id, a.task_id, a.attempt_number, a.execution_generation, a.agent_target_id,
                   a.status, a.started_at, a.heartbeat_at, a.finished_at, a.result_json, a.error_json,
                   ss.status, ss.total_bytes, ss.updated_at,
                   a.model_gateway_connection_id, a.executor_snapshot_json, a.failure_class
            FROM task_attempts a
            LEFT JOIN task_staging_sessions ss ON ss.id = a.staging_session_id
            WHERE a.task_id = ? ORDER BY a.attempt_number ASC, a.started_at ASC
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([task_id], task_attempt_from_row)
        .map_err(to_cli_error)?;
    let attempts = rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?;
    Ok(json!({ "projectId": project_id, "attempts": attempts }))
}

pub fn list_workflows(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let project_id = read_project_bootstrap(paths, BootstrapRequest::new())?
        .project
        .id;
    let storage = Storage::open(paths)?;
    let mut statement = storage
        .connection()
        .prepare(
            r#"
            SELECT w.id, w.type, w.status, w.source_workflow_id, w.source_json,
                   w.created_at, w.updated_at, w.archived_at,
                   COUNT(t.id),
                   SUM(CASE WHEN t.status = 'succeeded' THEN 1 ELSE 0 END)
            FROM workflows w
            LEFT JOIN tasks t ON t.workflow_id = w.id
            WHERE w.project_id = ? AND w.archived_at IS NULL
            GROUP BY w.id
            ORDER BY w.updated_at DESC, w.id
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([&project_id], workflow_summary_from_row)
        .map_err(to_cli_error)?;
    let workflows = rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?;
    Ok(json!({ "projectId": project_id, "workflows": workflows }))
}

pub fn read_workflow(paths: &MyOpenPanelsPaths, workflow_id: &str) -> Result<Value, CliError> {
    let project_id = read_project_bootstrap(paths, BootstrapRequest::new())?
        .project
        .id;
    let storage = Storage::open(paths)?;
    let workflow = storage
        .connection()
        .query_row(
            r#"
            SELECT w.id, w.type, w.status, w.source_workflow_id, w.source_json,
                   w.created_at, w.updated_at, w.archived_at,
                   COUNT(t.id), SUM(CASE WHEN t.status = 'succeeded' THEN 1 ELSE 0 END)
            FROM workflows w LEFT JOIN tasks t ON t.workflow_id = w.id
            WHERE w.project_id = ? AND w.id = ? GROUP BY w.id
            "#,
            params![project_id, workflow_id],
            workflow_summary_from_row,
        )
        .optional()
        .map_err(to_cli_error)?
        .ok_or_else(|| {
            CliError::with_code(
                "workflow_not_found",
                format!("Workflow not found: {workflow_id}"),
            )
        })?;
    let tasks = storage
        .list_tasks(&project_id)?
        .into_iter()
        .filter(|task| task.get("workflowId").and_then(Value::as_str) == Some(workflow_id))
        .collect::<Vec<_>>();
    let mut dependencies = storage.connection().prepare(
        "SELECT task_id, prerequisite_task_id, success_condition, failure_policy, created_at FROM task_dependencies WHERE task_id IN (SELECT id FROM tasks WHERE workflow_id = ?) ORDER BY task_id, prerequisite_task_id"
    ).map_err(to_cli_error)?;
    let dependencies = dependencies
        .query_map([workflow_id], |row| {
            Ok(json!({
                "taskId": row.get::<_, String>(0)?,
                "prerequisiteTaskId": row.get::<_, String>(1)?,
                "successCondition": row.get::<_, String>(2)?,
                "failurePolicy": row.get::<_, String>(3)?,
                "createdAt": row.get::<_, String>(4)?,
            }))
        })
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    Ok(json!({ "workflow": workflow, "tasks": tasks, "dependencies": dependencies }))
}

pub fn archive_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let task = inspect_task(paths, task_id)?;
    let status = task["task"]["status"].as_str().unwrap_or_default();
    if !matches!(status, "succeeded" | "cancelled" | "stale" | "superseded")
        && !(status == "failed"
            && task["task"]["attempt"].as_i64().unwrap_or(0)
                >= task["task"]["maxAttempts"].as_i64().unwrap_or(8))
    {
        return Err(CliError::with_code(
            "invalid_task_transition",
            "Only terminal tasks can be archived.",
        ));
    }
    let project_id = task["task"]["projectId"].as_str().unwrap_or_default();
    let workflow_id = task["task"]["workflowId"].as_str().unwrap_or_default();
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    tx.execute(
        "UPDATE tasks SET archived_at = ?, updated_at = ? WHERE id = ? AND archived_at IS NULL",
        params![now, now, task_id],
    )
    .map_err(to_cli_error)?;
    tx.execute("INSERT INTO task_events (task_id, workflow_id, event_type, from_status, to_status, created_at) VALUES (?, ?, 'archived', ?, 'archived', ?)", params![task_id, workflow_id, status, now]).map_err(to_cli_error)?;
    tx.execute(
        r#"UPDATE workflows SET status = 'archived', archived_at = ?, updated_at = ?
           WHERE id = ? AND NOT EXISTS (
             SELECT 1 FROM tasks WHERE workflow_id = ? AND archived_at IS NULL
           )"#,
        params![now, now, workflow_id, workflow_id],
    )
    .map_err(to_cli_error)?;
    tx.execute(
        r#"DELETE FROM content_pins
           WHERE task_id IN (SELECT id FROM tasks WHERE workflow_id = ?)
             AND NOT EXISTS (SELECT 1 FROM tasks WHERE workflow_id = ? AND archived_at IS NULL)"#,
        params![workflow_id, workflow_id],
    )
    .map_err(to_cli_error)?;
    crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    inspect_task_in_session(paths, project_id, task_id)
}

pub fn list_agent_routes(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let project_id = read_project_bootstrap(paths, BootstrapRequest::new())?
        .project
        .id;
    let storage = Storage::open(paths)?;
    let mut statement = storage.connection().prepare(
        r#"SELECT r.capability, r.agent_target_id, r.position, t.name, t.status, t.protocol_version, t.max_concurrency
           FROM agent_routes r JOIN agent_targets t ON t.id = r.agent_target_id
           WHERE r.project_id = ? AND t.transport = 'command'
           ORDER BY r.capability, r.position"#,
    ).map_err(to_cli_error)?;
    let routes = statement
        .query_map([&project_id], |row| {
            Ok(json!({
                "capability": row.get::<_, String>(0)?,
                "targetId": row.get::<_, String>(1)?,
                "position": row.get::<_, i64>(2)?,
                "targetName": row.get::<_, String>(3)?,
                "targetStatus": row.get::<_, String>(4)?,
                "protocolVersion": row.get::<_, i64>(5)?,
                "maxConcurrency": row.get::<_, i64>(6)?,
            }))
        })
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    Ok(json!({ "projectId": project_id, "routes": routes }))
}

pub fn set_agent_route(
    paths: &MyOpenPanelsPaths,
    capability: &str,
    target_ids: &[String],
) -> Result<Value, CliError> {
    if capability.trim().is_empty() || target_ids.is_empty() {
        return Err(CliError::with_code(
            "invalid_route",
            "A capability and at least one target are required.",
        ));
    }
    let project_id = read_project_bootstrap(paths, BootstrapRequest::new())?
        .project
        .id;
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    tx.execute(
        "DELETE FROM agent_routes WHERE project_id = ? AND capability = ?",
        params![project_id, capability],
    )
    .map_err(to_cli_error)?;
    for (position, target_id) in target_ids.iter().enumerate() {
        let exists = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM agent_targets WHERE project_id = ? AND id = ? AND transport = 'command')",
                params![project_id, target_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(to_cli_error)?;
        if !exists {
            return Err(CliError::with_code(
                "target_not_found",
                format!("Agent target not found: {target_id}"),
            ));
        }
        tx.execute("INSERT INTO agent_routes (project_id, capability, agent_target_id, position, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)", params![project_id, capability, target_id, position as i64, now, now]).map_err(to_cli_error)?;
    }
    crate::storage::record_scope(&tx, "agent_targets", Some(&project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    list_agent_routes(paths)
}

pub fn remove_agent_route(paths: &MyOpenPanelsPaths, capability: &str) -> Result<Value, CliError> {
    let project_id = read_project_bootstrap(paths, BootstrapRequest::new())?
        .project
        .id;
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    tx.execute(
        "DELETE FROM agent_routes WHERE project_id = ? AND capability = ?",
        params![project_id, capability],
    )
    .map_err(to_cli_error)?;
    crate::storage::record_scope(&tx, "agent_targets", Some(&project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    list_agent_routes(paths)
}
