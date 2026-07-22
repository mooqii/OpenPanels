fn finalize_task_runtime(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    task_id: &str,
    status: &str,
    result: Option<Value>,
    error: Option<Value>,
    retry_after: Option<&str>,
    failure_class: Option<TaskFailureClass>,
    panel_state: Option<(&str, &Value)>,
    expected_generation: Option<i64>,
) -> Result<(), CliError> {
    let mut storage = Storage::open(paths)?;
    let tx = storage
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let mut result = result;
    let error_json = error
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(to_cli_error)?;
    let (
        previous_status,
        workflow_run_id,
        attempt_id,
        attempts,
        max_attempts,
        execution_generation,
        task_type,
        task_input,
    ): (String, String, Option<String>, i64, i64, i64, String, String) = tx
        .query_row(
            r#"
            SELECT t.status, t.workflow_run_id,
                   (SELECT id FROM task_attempts a
                    WHERE a.task_id = t.id AND a.execution_generation = t.execution_generation),
                   t.attempts, t.max_attempts, t.execution_generation, t.type, t.input_json
            FROM tasks t WHERE t.id = ? AND t.project_id = ?
            "#,
            params![task_id, project_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .map_err(to_cli_error)?;
    if expected_generation.is_some_and(|expected| expected != execution_generation) {
        return Err(CliError::with_code(
            "execution_fenced",
            "The Task is now owned by a newer execution generation.",
        ));
    }
    let mut merged_layout_panel_state = None;
    if status == "succeeded" && task_type == crate::typesetting::LAYOUT_TASK_TYPE {
        let (panel_id, formatted_state) = panel_state.ok_or_else(|| {
            CliError::with_code(
                "invalid_output",
                "Typesetting Layout completion requires a panel state.",
            )
        })?;
        let input: Value = serde_json::from_str(&task_input).map_err(to_cli_error)?;
        let publication_id = input
            .get("publicationId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let expected_hash = input
            .pointer("/snapshot/contentHash")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let current_state_json: String = tx
            .query_row(
                "SELECT state_json FROM panel_states WHERE project_id = ? AND panel_id = ?",
                params![project_id, panel_id],
                |row| row.get(0),
            )
            .map_err(to_cli_error)?;
        let mut current_state: Value =
            serde_json::from_str(&current_state_json).map_err(to_cli_error)?;
        let current_publication = current_state
            .get_mut("publications")
            .and_then(Value::as_array_mut)
            .and_then(|publications| {
                publications.iter_mut().find(|publication| {
                    publication.get("id").and_then(Value::as_str) == Some(publication_id)
                })
            })
            .ok_or_else(|| {
                CliError::with_code(
                    "typesetting_publication_not_found",
                    format!("Typesetting publication not found: {publication_id}"),
                )
            })?;
        let current_content = current_publication
            .get("content")
            .cloned()
            .unwrap_or(Value::Null);
        if crate::typesetting::hash_json(&current_content)? != expected_hash {
            return Err(CliError::with_code(
                "content_conflict",
                format!("Publication content changed before Layout Task {task_id} completed."),
            ));
        }
        let formatted_publication = formatted_state
            .get("publications")
            .and_then(Value::as_array)
            .and_then(|publications| {
                publications.iter().find(|publication| {
                    publication.get("id").and_then(Value::as_str) == Some(publication_id)
                })
            })
            .ok_or_else(|| {
                CliError::with_code(
                    "invalid_output",
                    "Typesetting Layout completion is missing its publication.",
                )
            })?;
        current_publication["content"] = formatted_publication
            .get("content")
            .cloned()
            .unwrap_or(Value::Null);
        current_publication["updatedAt"] = formatted_publication
            .get("updatedAt")
            .cloned()
            .unwrap_or_else(|| json!(now));
        merged_layout_panel_state = Some((panel_id.to_owned(), current_state));
    }
    let has_staging = attempt_id.as_deref().is_some_and(|attempt_id| {
        tx.query_row(
            "SELECT staging_session_id IS NOT NULL FROM task_attempts WHERE id = ?",
            [attempt_id],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(false)
    });
    if status == "succeeded" && has_staging {
        let allow_empty = result
            .as_ref()
            .and_then(|value| value.get("outcome"))
            .and_then(Value::as_str)
            == Some("no_change")
            && result
                .as_ref()
                .and_then(|value| value.get("bridgeValidated"))
                .and_then(Value::as_bool)
                == Some(true);
        let commits = crate::content::commit_task_staging_in_transaction(
            paths,
            &tx,
            task_id,
            &now,
            allow_empty,
        )?;
        if !commits.is_empty() {
            let payload = result.get_or_insert_with(|| json!({}));
            if !payload.is_object() {
                *payload = json!({ "agentResult": payload.clone() });
            }
            payload["contentCommits"] = json!(commits);
        }
    } else if status != "succeeded" && has_staging {
        crate::content::abandon_task_staging_in_transaction(&tx, task_id, &now)?;
    }
    if let Some(object) = result.as_mut().and_then(Value::as_object_mut) {
        object.remove("bridgeValidated");
    }
    let result_json = result
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(to_cli_error)?;
    let terminal_reason_json = if matches!(status, "cancelled" | "stale" | "superseded") {
        error_json.clone()
    } else {
        None
    };
    tx.execute(
        r#"
            UPDATE tasks
            SET status = ?, assigned_agent_id = NULL, lease_owner = NULL,
                lease_expires_at = NULL, last_heartbeat_at = NULL,
                lease_token_hash = NULL, retry_after = ?, result_json = ?,
                error_json = ?, terminal_reason_json = COALESCE(?, terminal_reason_json),
                max_attempts = CASE
                  WHEN ? = 'terminal_task' THEN attempts
                  WHEN ? THEN max_attempts + 1
                  ELSE max_attempts END,
                execution_generation = execution_generation + CASE
                  WHEN ? IN ('failed', 'queued', 'cancelled') THEN 1 ELSE 0 END,
                completed_at = ?, updated_at = ?
            WHERE id = ? AND project_id = ?
            "#,
        params![
            status,
            retry_after,
            result_json,
            error_json,
            terminal_reason_json,
            failure_class.map(TaskFailureClass::as_str),
            failure_class == Some(TaskFailureClass::RetryableInterruption),
            status,
            if matches!(status, "succeeded" | "cancelled")
                || (status == "failed"
                    && failure_class != Some(TaskFailureClass::RetryableInterruption)
                    && (attempts >= max_attempts
                        || failure_class == Some(TaskFailureClass::TerminalTask)))
            {
                Some(now.clone())
            } else {
                None
            },
            now,
            task_id,
            project_id,
        ],
    )
    .map_err(to_cli_error)?;
    if let Some(attempt_id) = attempt_id.as_deref() {
        let attempt_status = match status {
            "succeeded" => "succeeded",
            "cancelled" => "cancelled",
            "failed" if failure_class == Some(TaskFailureClass::TerminalTask) => "failed_terminal",
            "failed" if failure_class == Some(TaskFailureClass::RetryableInterruption) => {
                "interrupted"
            }
            "failed" if attempts >= max_attempts => "failed_terminal",
            "failed" => "failed_retryable",
            _ => "interrupted",
        };
        tx.execute(
            "UPDATE task_attempts SET status = ?, heartbeat_at = ?, finished_at = ?, result_json = ?, error_json = ?, failure_class = ? WHERE id = ? AND status = 'leased'",
            params![attempt_status, now, now, result_json, error_json, failure_class.map(TaskFailureClass::as_str), attempt_id],
        )
        .map_err(to_cli_error)?;
    }
    tx.execute(
        "INSERT INTO task_events (task_id, workflow_run_id, event_type, from_status, to_status, reason_json, attempt_id, created_at) VALUES (?, ?, 'status_changed', ?, ?, ?, ?, ?)",
        params![task_id, workflow_run_id, previous_status, status, terminal_reason_json.or(error_json), attempt_id, now],
    )
    .map_err(to_cli_error)?;
    if status == "succeeded" {
        activate_ready_dependents(&tx, task_id, &now)?;
    } else if status == "failed"
        && failure_class != Some(TaskFailureClass::RetryableInterruption)
        && (attempts >= max_attempts || failure_class == Some(TaskFailureClass::TerminalTask))
    {
        propagate_prerequisite_failure(&tx, task_id, "failed_terminal", &now)?;
    } else if matches!(status, "cancelled" | "stale" | "superseded") {
        propagate_prerequisite_failure(&tx, task_id, status, &now)?;
    }
    refresh_workflow_run_status(&tx, &workflow_run_id, &now)?;
    let panel_state = merged_layout_panel_state
        .as_ref()
        .map(|(panel_id, state)| (panel_id.as_str(), state))
        .or(panel_state);
    if let Some((panel_id, state)) = panel_state {
        Storage::write_panel_state_in_transaction(&tx, project_id, panel_id, state)?;
    }
    crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    tx.commit().map_err(to_cli_error)
}

fn activate_ready_dependents(
    connection: &rusqlite::Connection,
    prerequisite_task_id: &str,
    now: &str,
) -> Result<(), CliError> {
    let dependents = {
        let mut statement = connection
            .prepare(
                r#"
                SELECT t.id, t.workflow_run_id, t.capability
                FROM tasks t
                JOIN task_dependencies d ON d.task_id = t.id
                WHERE d.prerequisite_task_id = ? AND t.status = 'waiting'
                  AND NOT EXISTS (
                    SELECT 1 FROM task_dependencies remaining
                    JOIN tasks prerequisite ON prerequisite.id = remaining.prerequisite_task_id
                    WHERE remaining.task_id = t.id
                      AND prerequisite.status <> 'succeeded'
                  )
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map([prerequisite_task_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    for (task_id, workflow_run_id, _) in dependents {
        connection
            .execute(
                "UPDATE tasks SET status = 'queued', available_at = ?, updated_at = ? WHERE id = ? AND status = 'waiting'",
                params![now, now, task_id],
            )
            .map_err(to_cli_error)?;
        connection
            .execute(
                "INSERT INTO task_events (task_id, workflow_run_id, event_type, from_status, to_status, reason_json, created_at) VALUES (?, ?, 'dependency_satisfied', 'waiting', 'queued', ?, ?)",
                params![task_id, workflow_run_id, json!({ "prerequisiteTaskId": prerequisite_task_id }).to_string(), now],
            )
            .map_err(to_cli_error)?;
    }
    Ok(())
}

fn propagate_prerequisite_failure(
    connection: &rusqlite::Connection,
    prerequisite_task_id: &str,
    prerequisite_status: &str,
    now: &str,
) -> Result<(), CliError> {
    let dependents = {
        let mut statement = connection
            .prepare(
                r#"
                SELECT t.id, t.workflow_run_id, t.status, d.failure_policy
                FROM tasks t
                JOIN task_dependencies d ON d.task_id = t.id
                WHERE d.prerequisite_task_id = ?
                  AND t.status IN ('waiting', 'queued', 'failed', 'reserved', 'running', 'claimed', 'converting', 'indexing')
                  AND d.failure_policy <> 'continue_snapshot'
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map([prerequisite_task_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    for (task_id, workflow_run_id, previous_status, policy) in dependents {
        let next_status = if policy == "supersede" {
            "superseded"
        } else {
            "cancelled"
        };
        let reason = json!({
            "code": "prerequisite_failed",
            "prerequisiteTaskId": prerequisite_task_id,
            "prerequisiteStatus": prerequisite_status,
        });
        connection
            .execute(
                r#"
                UPDATE tasks SET status = ?, assigned_agent_id = NULL, lease_owner = NULL,
                  lease_token_hash = NULL, lease_expires_at = NULL, last_heartbeat_at = NULL,
                  execution_generation = execution_generation + 1,
                  terminal_reason_json = ?, completed_at = ?, updated_at = ?
                WHERE id = ?
                "#,
                params![next_status, reason.to_string(), now, now, task_id],
            )
            .map_err(to_cli_error)?;
        connection
            .execute(
                "UPDATE task_attempts SET status = 'cancelled', finished_at = ?, error_json = ? WHERE task_id = ? AND status = 'leased'",
                params![now, reason.to_string(), task_id],
            )
            .map_err(to_cli_error)?;
        connection
            .execute(
                "INSERT INTO task_events (task_id, workflow_run_id, event_type, from_status, to_status, reason_json, created_at) VALUES (?, ?, 'prerequisite_propagated', ?, ?, ?, ?)",
                params![task_id, workflow_run_id, previous_status, next_status, reason.to_string(), now],
            )
            .map_err(to_cli_error)?;
    }
    Ok(())
}

fn refresh_workflow_run_status(
    connection: &rusqlite::Connection,
    workflow_run_id: &str,
    now: &str,
) -> Result<(), CliError> {
    connection
        .execute(
            r#"
            UPDATE workflow_runs
            SET status = CASE
                  WHEN EXISTS (SELECT 1 FROM tasks WHERE workflow_run_id = ?
                    AND status NOT IN ('succeeded', 'cancelled', 'stale', 'superseded')
                    AND NOT (status = 'failed' AND attempts >= max_attempts)) THEN 'active'
                  WHEN EXISTS (SELECT 1 FROM tasks WHERE workflow_run_id = ? AND status = 'succeeded')
                       AND NOT EXISTS (SELECT 1 FROM tasks WHERE workflow_run_id = ? AND status IN ('cancelled', 'stale', 'superseded')) THEN 'succeeded'
                  WHEN EXISTS (SELECT 1 FROM tasks WHERE workflow_run_id = ? AND status IN ('cancelled', 'stale', 'superseded')) THEN 'cancelled'
                  ELSE 'failed'
                END,
                updated_at = ?
            WHERE id = ?
            "#,
            params![workflow_run_id, workflow_run_id, workflow_run_id, workflow_run_id, now, workflow_run_id],
        )
        .map_err(to_cli_error)?;
    Ok(())
}

fn recover_expired_tasks(paths: &MyOpenPanelsPaths) -> Result<(), CliError> {
    let bootstrap = match read_project_bootstrap(paths, BootstrapRequest::new()) {
        Ok(bootstrap) => bootstrap,
        Err(error) if error.code() == Some("no_current_project") => return Ok(()),
        Err(error) => return Err(error),
    };
    recover_expired_tasks_in_session(paths, &bootstrap.project.id)
}

pub(crate) fn recover_builtin_worker_tasks_after_restart(
    paths: &MyOpenPanelsPaths,
) -> Result<usize, CliError> {
    let storage = Storage::open(paths)?;
    let candidates = {
        let mut statement = storage
            .connection()
            .prepare(
                r#"
                SELECT t.id, t.project_id, t.queue, t.execution_generation, target.name
                FROM tasks t
                JOIN task_attempts attempt
                  ON attempt.task_id = t.id
                 AND attempt.execution_generation = t.execution_generation
                 AND attempt.status = 'leased'
                JOIN agent_targets target ON target.id = attempt.agent_target_id
                WHERE t.status IN ('running', 'claimed', 'converting', 'indexing')
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    drop(storage);

    let target_prefix = format!("model-gateway:{}:", paths.context_id);
    let message = "Studio restarted while the local executor was running.";
    let mut recovered = 0;
    for (task_id, project_id, queue, execution_generation, target_name) in candidates {
        if !target_name.starts_with(&target_prefix) {
            continue;
        }
        let retry_after = crate::control::now_iso();
        let _ = fail_queue_projection(paths, &queue, &task_id, message, Some(&retry_after));
        finalize_task_runtime(
            paths,
            &project_id,
            &task_id,
            "failed",
            None,
            Some(json!({ "code": "studio_restart", "message": message })),
            Some(&retry_after),
            Some(TaskFailureClass::RetryableInterruption),
            None,
            Some(execution_generation),
        )?;
        recovered += 1;
    }
    Ok(recovered)
}

fn recover_expired_tasks_in_session(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    let now = crate::control::now_iso();
    storage
        .connection()
        .execute(
            "UPDATE tasks SET status = 'queued', assigned_agent_id = NULL, updated_at = ? WHERE project_id = ? AND status = 'reserved' AND updated_at < ?",
            params![
                now,
                project_id,
                (chrono::Utc::now() - chrono::Duration::seconds(30))
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
            ],
        )
        .map_err(to_cli_error)?;
    let expired = {
        let mut statement = storage
            .connection()
            .prepare(
                r#"
                SELECT id, queue, attempts, assigned_agent_id
                FROM tasks
                WHERE project_id = ?
                  AND status IN ('running', 'claimed', 'converting', 'indexing')
                  AND lease_expires_at IS NOT NULL
                  AND lease_expires_at <= ?
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![project_id, now], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    drop(storage);
    for (task_id, queue, attempt, target_id) in expired {
        let retry_after = if has_untried_eligible_target(
            paths,
            project_id,
            &task_id,
            target_id.as_deref().unwrap_or_default(),
        )? {
            crate::control::now_iso()
        } else {
            execution_retry_after(attempt.max(1))
        };
        if queue == "wiki" {
            crate::wiki::fail_task_with_retry(
                paths,
                &task_id,
                "Task lease expired.",
                Some(&retry_after),
            )?;
        }
        finalize_task_runtime(
            paths,
            project_id,
            &task_id,
            "failed",
            None,
            Some(json!("Task lease expired.")),
            Some(&retry_after),
            Some(TaskFailureClass::RetryableChannel),
            None,
            None,
        )?;
    }
    Ok(())
}
