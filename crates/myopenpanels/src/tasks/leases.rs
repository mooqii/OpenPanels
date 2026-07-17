pub fn claim_next(
    paths: &MyOpenPanelsPaths,
    target_id: &str,
    capability: Option<&str>,
    wait_ms: Option<u64>,
) -> Result<Value, CliError> {
    claim_next_filtered(paths, target_id, capability, None, wait_ms)
}

pub fn claim_next_filtered(
    paths: &MyOpenPanelsPaths,
    target_id: &str,
    capability: Option<&str>,
    queue: Option<&str>,
    wait_ms: Option<u64>,
) -> Result<Value, CliError> {
    let wait_ms = wait_ms
        .unwrap_or(DEFAULT_LONG_POLL_MS)
        .min(DEFAULT_LONG_POLL_MS);
    let started = Instant::now();
    let project_id = read_project_bootstrap(paths, BootstrapRequest::new())?
        .project
        .id;
    loop {
        match claim_once(paths, &project_id, target_id, None, capability, queue) {
            Ok(Some(payload)) => return Ok(payload),
            Ok(None) => {}
            Err(error) if is_database_locked(&error) => {}
            Err(error) => return Err(error),
        }
        if started.elapsed() >= Duration::from_millis(wait_ms) {
            return Ok(json!({ "task": Value::Null, "leaseToken": Value::Null }));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn is_database_locked(error: &CliError) -> bool {
    error
        .message()
        .to_ascii_lowercase()
        .contains("database is locked")
}

pub fn claim_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    target_id: &str,
) -> Result<Value, CliError> {
    let project_id = task_project_id(paths, task_id)?;
    claim_once(paths, &project_id, target_id, Some(task_id), None, None)?.ok_or_else(|| {
        CliError::with_code(
            "task_not_claimable",
            format!("Project task is not claimable: {task_id}"),
        )
    })
}

fn claim_once(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    target_id: &str,
    task_id: Option<&str>,
    requested_capability: Option<&str>,
    requested_queue: Option<&str>,
) -> Result<Option<Value>, CliError> {
    recover_expired_tasks_in_session(paths, project_id)?;
    heartbeat_target_in_session(paths, project_id, target_id)?;
    let mut storage = Storage::open(paths)?;
    let target =
        read_target_value(storage.connection(), project_id, target_id)?.ok_or_else(|| {
            CliError::with_code(
                "target_not_found",
                format!("Agent target not found: {target_id}"),
            )
        })?;
    let reserved = reserve_task(
        &mut storage,
        project_id,
        &target,
        task_id,
        requested_capability,
        requested_queue,
    )?;
    let Some(reserved) = reserved else {
        return Ok(None);
    };

    let claim_result = match reserved.queue.as_str() {
        "wiki" => crate::wiki::claim_task(paths, &reserved.id),
        "writing" => crate::writing::claim_task(paths, &reserved.id),
        queue => Err(CliError::with_code(
            "queue_adapter_missing",
            format!("No task lifecycle adapter is available for queue: {queue}"),
        )),
    };
    let claimed = match claim_result {
        Ok(claimed) => claimed,
        Err(error) => {
            release_reservation(paths, project_id, &reserved)?;
            return Err(error);
        }
    };
    let claimed_status = claimed["task"]["status"].as_str().unwrap_or("running");
    let claimed_attempt = claimed["task"]["attempt"].as_i64().unwrap_or(1);

    let lease_token = random_secret("lease");
    let lease_expires_at = lease_expires_at();
    let attempt_id = random_id("task-attempt");
    let broker_url = crate::content::task_broker_url_for_claim();
    if broker_url.as_deref().is_none_or(str::is_empty) && !cfg!(test) {
        release_reservation(paths, project_id, &reserved)?;
        return Err(CliError::with_code(
            "broker_unavailable",
            "Execution protocol v3 requires a running Studio Task Broker.",
        ));
    }
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let execution_generation = tx.query_row(
            r#"
            UPDATE tasks
            SET status = ?, attempts = ?, assigned_agent_id = ?, lease_owner = ?, lease_token_hash = ?,
                lease_expires_at = ?, last_heartbeat_at = ?, retry_after = NULL,
                error_json = NULL, updated_at = ?, execution_generation = execution_generation + 1
            WHERE id = ? AND project_id = ?
            RETURNING execution_generation
            "#,
            params![
                claimed_status,
                claimed_attempt,
                target_id,
                target_id,
                hash_secret(&lease_token),
                lease_expires_at,
                now,
                now,
                reserved.id,
                project_id,
            ],
            |row| row.get::<_, i64>(0),
        )
        .map_err(to_cli_error)?;
    let model_gateway_connection_id = target
        .get("modelGatewayConnectionId")
        .and_then(Value::as_str);
    let executor_snapshot = json!({
        "targetId": target_id,
        "targetName": target.get("name"),
        "host": target.get("host"),
        "transport": target.get("transport"),
        "modelGatewayConnectionId": model_gateway_connection_id,
    });
    let workflow_id = tx
        .query_row(
            "SELECT workflow_id FROM tasks WHERE id = ?",
            [&reserved.id],
            |row| row.get::<_, String>(0),
        )
        .map_err(to_cli_error)?;
    tx.execute(
        r#"
        INSERT INTO task_attempts (
          id, task_id, attempt_number, execution_generation, agent_target_id,
          status, started_at, heartbeat_at, model_gateway_connection_id,
          executor_snapshot_json
        ) VALUES (?, ?, ?, ?, ?, 'leased', ?, ?, ?, ?)
        "#,
        params![
            attempt_id,
            reserved.id,
            claimed_attempt,
            execution_generation,
            target_id,
            now,
            now,
            model_gateway_connection_id,
            executor_snapshot.to_string(),
        ],
    )
    .map_err(to_cli_error)?;
    let execution = if broker_url.as_deref().is_some_and(|value| !value.is_empty()) {
        Some(crate::content::create_execution_context_in_transaction(
            &tx,
            &reserved.id,
            &attempt_id,
            execution_generation,
            &lease_expires_at,
        )?)
    } else {
        None
    };
    tx.execute(
        "INSERT INTO task_events (task_id, workflow_id, event_type, from_status, to_status, attempt_id, agent_target_id, created_at) VALUES (?, ?, 'claimed', ?, ?, ?, ?, ?)",
        params![reserved.id, workflow_id, reserved.previous_status, claimed_status, attempt_id, target_id, now],
    )
    .map_err(to_cli_error)?;
    crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    let mut payload = inspect_task(paths, &reserved.id)?;
    payload["leaseToken"] = json!(lease_token);
    payload["target"] = target;
    payload["attemptId"] = json!(attempt_id);
    payload["executionGeneration"] = json!(execution_generation);
    payload["executionProtocolVersion"] = json!(reserved.required_protocol_version);
    payload["taskBrokerUrl"] = broker_url.map(Value::from).unwrap_or(Value::Null);
    payload["executionToken"] = execution
        .as_ref()
        .map(|value| Value::from(value.0.clone()))
        .unwrap_or(Value::Null);
    payload["executionTokenExpiresAt"] = execution
        .as_ref()
        .map(|_| Value::from(lease_expires_at))
        .unwrap_or(Value::Null);
    payload["inputManifest"] = json!(read_task_inputs(paths, &reserved.id)?);
    Ok(Some(payload))
}

pub fn heartbeat_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
) -> Result<Value, CliError> {
    let lease = verify_lease(paths, task_id, lease_token)?;
    let expires_at = lease_expires_at();
    match lease["queue"].as_str().unwrap_or("") {
        "wiki" => crate::wiki::heartbeat_task(paths, task_id, &expires_at)?,
        "writing" => crate::writing::heartbeat_task(paths, task_id)?,
        queue => {
            return Err(CliError::with_code(
                "queue_adapter_missing",
                format!("No task lifecycle adapter is available for queue: {queue}"),
            ))
        }
    };
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    tx.execute(
            "UPDATE tasks SET lease_expires_at = ?, last_heartbeat_at = ?, updated_at = ? WHERE id = ? AND lease_token_hash = ?",
            params![expires_at, now, now, task_id, hash_secret(lease_token)],
        )
        .map_err(to_cli_error)?;
    tx.execute(
        "UPDATE task_attempts SET heartbeat_at = ? WHERE task_id = ? AND status = 'leased'",
        params![now, task_id],
    )
    .map_err(to_cli_error)?;
    tx.execute(
        "UPDATE task_attempts SET execution_token_expires_at = ? WHERE task_id = ? AND status = 'leased' AND execution_token_hash IS NOT NULL",
        params![expires_at, task_id],
    )
    .map_err(to_cli_error)?;
    tx.execute(
        "UPDATE task_staging_sessions SET expires_at = ?, updated_at = ? WHERE task_id = ? AND status IN ('open', 'prepared')",
        params![expires_at, now, task_id],
    )
    .map_err(to_cli_error)?;
    let project_id = lease["projectId"].as_str().unwrap_or_default();
    crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    inspect_task_in_session(paths, project_id, task_id)
}

pub fn complete_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
    result: Option<Value>,
) -> Result<Value, CliError> {
    let lease = verify_lease(paths, task_id, lease_token)?;
    let prepared_panel_state: Option<(String, Value)> = match lease["queue"].as_str().unwrap_or("")
    {
        "wiki" => Some((
            lease["panelId"].as_str().unwrap_or_default().to_owned(),
            crate::wiki::prepare_task_completion(paths, task_id, result.clone())?["state"].clone(),
        )),
        "writing" => crate::writing::prepare_task_completion(paths, task_id)?,
        queue => {
            return Err(CliError::with_code(
                "queue_adapter_missing",
                format!("No task lifecycle adapter is available for queue: {queue}"),
            ))
        }
    };
    let project_id = lease["projectId"].as_str().unwrap_or_default();
    if let Err(error) = finalize_task_runtime(
        paths,
        project_id,
        task_id,
        "succeeded",
        result,
        None,
        None,
        None,
        prepared_panel_state
            .as_ref()
            .map(|(panel_id, state)| (panel_id.as_str(), state)),
        lease["executionGeneration"].as_i64(),
    ) {
        if error.code() == Some("content_conflict") {
            let _ = supersede_task_for_content_conflict(paths, task_id, "content-resource");
        }
        return Err(error);
    }
    inspect_task_in_session(paths, project_id, task_id)
}

pub fn fail_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
    message: &str,
    retry_after: Option<&str>,
) -> Result<Value, CliError> {
    fail_task_with_class(
        paths,
        task_id,
        lease_token,
        message,
        retry_after,
        TaskFailureClass::RetryableChannel,
    )
}

pub fn fail_task_with_class(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
    message: &str,
    retry_after: Option<&str>,
    failure_class: TaskFailureClass,
) -> Result<Value, CliError> {
    let lease = verify_lease(paths, task_id, lease_token)?;
    if retry_after.is_some_and(|value| chrono::DateTime::parse_from_rfc3339(value).is_err()) {
        return Err(CliError::with_code(
            "invalid_retry_after",
            "Expected --retry-after to be an RFC 3339 timestamp.",
        ));
    }
    let has_fallback = !matches!(
        failure_class,
        TaskFailureClass::TerminalTask | TaskFailureClass::RetryableInterruption
    ) && has_untried_eligible_target(
        paths,
        lease["projectId"].as_str().unwrap_or_default(),
        task_id,
        lease["targetId"].as_str().unwrap_or_default(),
    )?;
    let retry_after = if failure_class == TaskFailureClass::TerminalTask {
        None
    } else if failure_class == TaskFailureClass::RetryableInterruption {
        Some(crate::control::now_iso())
    } else if has_fallback {
        Some(crate::control::now_iso())
    } else if let Some(retry_after) = retry_after {
        Some(retry_after.to_owned())
    } else {
        Some(execution_retry_after(
            lease["attempt"].as_i64().unwrap_or(1),
        ))
    };
    fail_queue_projection(
        paths,
        lease["queue"].as_str().unwrap_or(""),
        task_id,
        message,
        retry_after.as_deref(),
    )?;
    finalize_task_runtime(
        paths,
        lease["projectId"].as_str().unwrap_or_default(),
        task_id,
        "failed",
        None,
        Some(
            if failure_class == TaskFailureClass::RetryableInterruption {
                json!({ "code": "studio_restart", "message": message })
            } else {
                json!(message)
            },
        ),
        retry_after.as_deref(),
        Some(failure_class),
        None,
        lease["executionGeneration"].as_i64(),
    )?;
    inspect_task_in_session(
        paths,
        lease["projectId"].as_str().unwrap_or_default(),
        task_id,
    )
}

pub(crate) fn interrupt_task_for_studio_restart(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
) -> Result<Value, CliError> {
    let lease = verify_lease(paths, task_id, lease_token)?;
    let message = "Studio restarted while the local executor was running.";
    let retry_after = crate::control::now_iso();
    let _ = fail_queue_projection(
        paths,
        lease["queue"].as_str().unwrap_or(""),
        task_id,
        message,
        Some(&retry_after),
    );
    finalize_task_runtime(
        paths,
        lease["projectId"].as_str().unwrap_or_default(),
        task_id,
        "failed",
        None,
        Some(json!({ "code": "studio_restart", "message": message })),
        Some(&retry_after),
        Some(TaskFailureClass::RetryableInterruption),
        None,
        lease["executionGeneration"].as_i64(),
    )?;
    inspect_task_in_session(
        paths,
        lease["projectId"].as_str().unwrap_or_default(),
        task_id,
    )
}

fn fail_queue_projection(
    paths: &MyOpenPanelsPaths,
    queue: &str,
    task_id: &str,
    message: &str,
    retry_after: Option<&str>,
) -> Result<(), CliError> {
    match queue {
        "wiki" => {
            crate::wiki::fail_task_with_retry(paths, task_id, message, retry_after)?;
        }
        "writing" => {
            crate::writing::fail_task(paths, task_id, message)?;
        }
        queue => {
            return Err(CliError::with_code(
                "queue_adapter_missing",
                format!("No task lifecycle adapter is available for queue: {queue}"),
            ));
        }
    }
    Ok(())
}

pub(crate) fn mark_latest_attempt_invalid_output(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    message: &str,
) -> Result<(), CliError> {
    let project_id = task_project_id(paths, task_id)?;
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let reason = json!({ "code": "invalid_output", "message": message });
    tx.execute(
        "UPDATE task_attempts SET status = 'invalid_output', error_json = ?, failure_class = 'retryable_output' WHERE id = (SELECT id FROM task_attempts WHERE task_id = ? ORDER BY execution_generation DESC LIMIT 1) AND status IN ('failed_retryable', 'failed_terminal')",
        params![reason.to_string(), task_id],
    ).map_err(to_cli_error)?;
    let workflow_id = tx
        .query_row(
            "SELECT workflow_id FROM tasks WHERE id = ?",
            [task_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(to_cli_error)?;
    tx.execute(
        "INSERT INTO task_events (task_id, workflow_id, event_type, from_status, to_status, reason_json, created_at) VALUES (?, ?, 'invalid_output', 'leased', 'failed', ?, ?)",
        params![task_id, workflow_id, reason.to_string(), now],
    ).map_err(to_cli_error)?;
    crate::storage::record_scope(&tx, "tasks", Some(&project_id), None)?;
    tx.commit().map_err(to_cli_error)
}

pub fn release_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
) -> Result<Value, CliError> {
    let lease = verify_lease(paths, task_id, lease_token)?;
    match lease["queue"].as_str().unwrap_or("") {
        "wiki" => crate::wiki::release_task(paths, task_id)?,
        "writing" => crate::writing::release_task(paths, task_id)?,
        queue => {
            return Err(CliError::with_code(
                "queue_adapter_missing",
                format!("No task lifecycle adapter is available for queue: {queue}"),
            ))
        }
    };
    finalize_task_runtime(
        paths,
        lease["projectId"].as_str().unwrap_or_default(),
        task_id,
        "queued",
        None,
        None,
        None,
        None,
        None,
        lease["executionGeneration"].as_i64(),
    )?;
    inspect_task_in_session(
        paths,
        lease["projectId"].as_str().unwrap_or_default(),
        task_id,
    )
}

pub fn retry_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let task = inspect_task(paths, task_id)?;
    if !matches!(task["task"]["status"].as_str(), Some("failed" | "queued")) {
        return Err(CliError::with_code(
            "invalid_task_transition",
            "Only queued or failed tasks can be retried. Succeeded and cancelled tasks require a new Workflow.",
        ));
    }
    if task["task"]["attempt"].as_i64().unwrap_or(0)
        >= task["task"]["maxAttempts"].as_i64().unwrap_or(8)
    {
        return Err(CliError::with_code(
            "invalid_task_transition",
            "The Task exhausted its Attempts. Start a new Workflow instead of retrying it in place.",
        ));
    }
    let project_id = task["task"]["projectId"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    match task["task"]["queue"].as_str().unwrap_or("") {
        "wiki" => crate::wiki::retry_task(paths, task_id)?,
        "writing" => crate::writing::retry_task(paths, task_id)?,
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
        "queued",
        None,
        None,
        None,
        None,
        None,
        None,
    )?;
    inspect_task_in_session(paths, &project_id, task_id)
}

