#[allow(clippy::too_many_arguments)]
fn finalize_task_runtime(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    task_id: &str,
    requested_status: &str,
    output_plan: TaskOutputPlan,
    error: Option<Value>,
    retry_after: Option<&str>,
    failure_class: Option<TaskFailureClass>,
    expected_generation: Option<i64>,
) -> Result<(), CliError> {
    let mut storage = Storage::open(paths)?;
    let tx = storage
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let (current_status, attempt_count, generation, handler_key, history_json) = tx
        .query_row(
            "SELECT status, attempt_count, execution_generation, handler_key, attempt_history_json FROM tasks WHERE id = ? AND project_id = ?",
            params![task_id, project_id],
            |row| Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            )),
        )
        .map_err(to_cli_error)?;
    if expected_generation.is_some_and(|expected| expected != generation) {
        return Err(CliError::with_code(
            "execution_fenced",
            "The Task is now owned by a newer execution generation.",
        ));
    }

    let mut result = output_plan.result;
    let mut prepared_content = None;
    if requested_status == "succeeded" {
        let prepared = crate::content::prepare_task_staging_in_transaction(
            paths,
            &tx,
            task_id,
            &now,
            result
                .as_ref()
                .and_then(|value| value.get("outcome"))
                .and_then(Value::as_str)
                == Some("no_change")
                || handler_key.starts_with("handler.release.")
                || handler_key.starts_with("handler.publication."),
        )?;
        if !prepared.commits.is_empty() {
            let payload = result.get_or_insert_with(|| json!({}));
            if !payload.is_object() {
                *payload = json!({ "agentResult": payload.clone() });
            }
            payload["contentCommits"] = json!(prepared.commits);
        }
        prepared_content = Some(prepared);
    } else {
        crate::content::abandon_task_staging_in_transaction(&tx, task_id, &now)?;
    }
    if let Some(object) = result.as_mut().and_then(Value::as_object_mut) {
        object.remove("bridgeValidated");
    }

    let retryable = requested_status == "failed"
        && failure_class != Some(TaskFailureClass::TerminalTask)
        && attempt_count < TASK_EXECUTION_LIMIT;
    let status = if retryable {
        "queued"
    } else {
        match requested_status {
            "succeeded" => "succeeded",
            "cancelled" => "cancelled",
            "superseded" => "superseded",
            _ => "failed",
        }
    };
    let completed_at = matches!(status, "succeeded" | "failed" | "cancelled" | "superseded")
        .then_some(now.as_str());
    let available_at = if retryable {
        retry_after.unwrap_or(&now)
    } else {
        &now
    };
    let result_json = result
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(to_cli_error)?;
    let error_json = error
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(to_cli_error)?;
    let mut history = serde_json::from_str::<Vec<Value>>(&history_json).unwrap_or_default();
    if current_status == "running" || attempt_count > history.len() as i64 {
        history.push(json!({
            "attempt": attempt_count,
            "generation": generation,
            "runnerKey": tx.query_row(
                "SELECT current_runner_key FROM tasks WHERE id = ?",
                [task_id],
                |row| row.get::<_, Option<String>>(0),
            ).map_err(to_cli_error)?,
            "status": if status == "queued" { "failed_retryable" } else { status },
            "finishedAt": now,
            "result": result,
            "error": error,
            "failureClass": failure_class.map(TaskFailureClass::as_str),
        }));
    }
    if history.len() > TASK_EXECUTION_LIMIT as usize {
        history.drain(..history.len() - TASK_EXECUTION_LIMIT as usize);
    }
    tx.execute(
        r#"
        UPDATE tasks SET status = ?, result_json = ?, error_json = ?,
          attempt_history_json = ?, available_at = ?, execution_generation = execution_generation + 1,
          execution_token_hash = NULL, lease_owner = NULL, lease_expires_at = NULL,
          heartbeat_at = NULL, current_runner_key = NULL, completed_at = ?, updated_at = ?
        WHERE id = ? AND project_id = ?
        "#,
        params![
            status,
            result_json,
            error_json,
            serde_json::to_string(&history).map_err(to_cli_error)?,
            available_at,
            completed_at,
            now,
            task_id,
            project_id,
        ],
    )
    .map_err(to_cli_error)?;

    if matches!(status, "failed" | "cancelled" | "superseded") {
        let reason = json!({
            "code": "prerequisite_failed",
            "prerequisiteTaskId": task_id,
            "prerequisiteStatus": status,
        });
        let dependent_status = if status == "failed" {
            "failed"
        } else {
            "cancelled"
        };
        tx.execute(
            r#"
            UPDATE tasks SET status = ?, error_json = ?, completed_at = ?,
              execution_generation = execution_generation + 1,
              execution_token_hash = NULL, lease_owner = NULL, lease_expires_at = NULL,
              heartbeat_at = NULL, current_runner_key = NULL, updated_at = ?
            WHERE depends_on_task_id = ? AND status IN ('queued', 'running')
            "#,
            params![
                dependent_status,
                reason.to_string(),
                now,
                now,
                task_id
            ],
        )
        .map_err(to_cli_error)?;
    }

    if let Some(panel_state) = output_plan.panel_state {
        Storage::write_panel_state_if_revision_in_transaction(
            &tx,
            project_id,
            &panel_state.panel_id,
            panel_state.base_revision,
            &panel_state.state,
        )?;
    }
    crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    if let Some(prepared) = prepared_content {
        crate::content::publish_prepared_task_content(paths, prepared)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn complete_task_with_prepared_panel_state_for_test(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    task_id: &str,
    execution_generation: i64,
    panel_state: PreparedPanelState,
) -> Result<(), CliError> {
    finalize_task_runtime(
        paths,
        project_id,
        task_id,
        "succeeded",
        TaskOutputPlan::completed(Some(json!({ "outcome": "no_change" })), Some(panel_state)),
        None,
        None,
        None,
        Some(execution_generation),
    )
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
            .prepare("SELECT id, project_id, execution_generation FROM tasks WHERE status = 'running'")
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(to_cli_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_cli_error)?;
        rows
    };
    drop(storage);
    for (task_id, project_id, generation) in &candidates {
        finalize_task_runtime(
            paths,
            project_id,
            task_id,
            "failed",
            TaskOutputPlan::empty(),
            Some(json!({
                "code": "studio_restart",
                "message": "Studio restarted while the local executor was running."
            })),
            Some(&crate::control::now_iso()),
            Some(TaskFailureClass::RetryableInterruption),
            Some(*generation),
        )?;
    }
    Ok(candidates.len())
}

fn recover_expired_tasks_in_session(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    let now = crate::control::now_iso();
    let expired = {
        let mut statement = storage.connection().prepare(
            "SELECT id, execution_generation, attempt_count FROM tasks WHERE project_id = ? AND status = 'running' AND lease_expires_at IS NOT NULL AND lease_expires_at <= ?",
        ).map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![project_id, now], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(to_cli_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_cli_error)?;
        rows
    };
    drop(storage);
    for (task_id, generation, attempt) in expired {
        let retry_after = execution_retry_after(attempt);
        finalize_task_runtime(
            paths,
            project_id,
            &task_id,
            "failed",
            TaskOutputPlan::empty(),
            Some(json!({ "code": "lease_expired", "message": "Task lease expired." })),
            Some(&retry_after),
            Some(TaskFailureClass::RetryableChannel),
            Some(generation),
        )?;
    }
    Ok(())
}
