pub(crate) fn claim_for_worker(
    paths: &MyOpenPanelsPaths,
    target_id: &str,
    capability: Option<&str>,
    queue: Option<&str>,
) -> Result<Value, CliError> {
    let project_id = read_project_bootstrap(paths, BootstrapRequest::new())?.project.id;
    for attempt in 0..3 {
        match claim_once(
            paths,
            &project_id,
            target_id,
            None,
            capability,
            queue,
            None,
        ) {
            Ok(payload) => {
                return Ok(payload.unwrap_or_else(|| {
                    json!({ "task": Value::Null, "leaseToken": Value::Null })
                }))
            }
            Err(error) if attempt < 2 && task_database_locked(&error) => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("claim retry loop always returns")
}

fn task_database_locked(error: &CliError) -> bool {
    error.message().to_ascii_lowercase().contains("database is locked")
}

#[cfg(test)]
pub(crate) fn claim_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    target_id: &str,
) -> Result<Value, CliError> {
    let project_id = task_project_id(paths, task_id)?;
    claim_once(
        paths,
        &project_id,
        target_id,
        Some(task_id),
        None,
        None,
        None,
    )?
    .ok_or_else(|| {
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
    requested_task_id: Option<&str>,
    requested_capability: Option<&str>,
    requested_queue: Option<&str>,
    explicit_broker_url: Option<&str>,
) -> Result<Option<Value>, CliError> {
    recover_expired_tasks_in_session(paths, project_id)?;
    let runner_key = target_id
        .strip_prefix("agent-cli:")
        .or_else(|| target_id.strip_prefix("agent-target:"))
        .unwrap_or(target_id);
    let settings = crate::model_gateway::read_settings(paths)?;
    let storage = Storage::open(paths)?;
    let running = storage
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE status = 'running'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(to_cli_error)?;
    if running >= settings.max_concurrency {
        return Ok(None);
    }
    let now = crate::control::now_iso();
    let mut candidates = storage.list_tasks(project_id)?;
    candidates.sort_by(|left, right| {
        left.get("createdAt")
            .and_then(Value::as_str)
            .unwrap_or("")
            .cmp(right.get("createdAt").and_then(Value::as_str).unwrap_or(""))
    });
    let candidate = candidates.into_iter().find(|task| {
        if requested_task_id.is_some_and(|id| task.get("id").and_then(Value::as_str) != Some(id)) {
            return false;
        }
        if requested_capability.is_some_and(|capability| {
            task.get("capability").and_then(Value::as_str) != Some(capability)
        }) {
            return false;
        }
        if requested_queue.is_some_and(|queue| task.get("queue").and_then(Value::as_str) != Some(queue)) {
            return false;
        }
        if task.get("status").and_then(Value::as_str) != Some("queued")
            || task.get("attempt").and_then(Value::as_i64).unwrap_or(0)
                >= TASK_EXECUTION_LIMIT
            || future_time(task.get("availableAt").and_then(Value::as_str)).is_some()
        {
            return false;
        }
        if !task_dependency_ready(storage.connection(), task) {
            return false;
        }
        if mutation_task_blocked(
            storage.connection(),
            task.get("id").and_then(Value::as_str).unwrap_or(""),
        )
        .unwrap_or(true)
        {
            return false;
        }
        requested_task_id.is_some() || runner_matches_task(&settings, task, runner_key)
    });
    let Some(task) = candidate else {
        return Ok(None);
    };
    let task_id = task.get("id").and_then(Value::as_str).unwrap_or_default();
    let queue = task.get("queue").and_then(Value::as_str).unwrap_or_default();
    let claim_projection = match task_queue_adapter(queue)? {
        TaskQueueAdapter::Wiki => crate::wiki::claim_task(paths, task_id),
        TaskQueueAdapter::Writing => crate::writing::claim_task(paths, task_id),
        TaskQueueAdapter::Publication => crate::publication::claim_task(paths, task_id),
        TaskQueueAdapter::Release => crate::release::claim_task(paths, task_id),
    };
    if let Err(error) = claim_projection {
        return Err(error);
    }

    let lease_token = random_secret("lease");
    let lease_expires_at = lease_expires_at();
    let attempt_id = random_id("attempt");
    let broker_url = explicit_broker_url
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .or_else(crate::content::task_broker_url_for_claim);
    if broker_url.as_deref().is_none_or(str::is_empty) && !cfg!(test) {
        release_queue_projection(paths, queue, task_id)?;
        return Err(CliError::with_code(
            "broker_unavailable",
            "Task execution requires a running Studio Task Broker.",
        ));
    }
    let mut storage = Storage::open(paths)?;
    let tx = storage
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(to_cli_error)?;
    let generation = tx
        .query_row(
            r#"
            UPDATE tasks SET status = 'running', attempt_count = attempt_count + 1,
              current_runner_key = ?, lease_owner = ?, execution_token_hash = NULL,
              lease_expires_at = ?, heartbeat_at = ?, error_json = NULL,
              execution_generation = execution_generation + 1, updated_at = ?
            WHERE id = ? AND project_id = ? AND status = 'queued'
              AND attempt_count < ? AND available_at <= ?
              AND (SELECT COUNT(*) FROM tasks AS running WHERE running.status = 'running') < ?
              AND (
                depends_on_task_id IS NULL OR EXISTS (
                  SELECT 1 FROM tasks AS dependency
                  WHERE dependency.id = tasks.depends_on_task_id
                    AND dependency.status = 'succeeded'
                )
              )
              AND NOT EXISTS (
                SELECT 1 FROM tasks AS predecessor
                WHERE tasks.mutation_key IS NOT NULL
                  AND predecessor.project_id = tasks.project_id
                  AND predecessor.mutation_key = tasks.mutation_key
                  AND predecessor.id <> tasks.id
                  AND predecessor.status IN ('queued', 'running')
                  AND (
                    predecessor.status = 'running'
                    OR predecessor.created_at < tasks.created_at
                    OR (
                      predecessor.created_at = tasks.created_at
                      AND predecessor.id < tasks.id
                    )
                  )
              )
            RETURNING execution_generation
            "#,
            params![
                runner_key,
                hash_secret(&lease_token),
                lease_expires_at,
                now,
                now,
                task_id,
                project_id,
                TASK_EXECUTION_LIMIT,
                now,
                settings.max_concurrency,
            ],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(to_cli_error)?;
    let Some(generation) = generation else {
        drop(tx);
        release_queue_projection(paths, queue, task_id)?;
        return Ok(None);
    };
    let execution = if broker_url.as_deref().is_some_and(|value| !value.is_empty()) {
        Some(crate::content::create_execution_context_in_transaction(
            &tx,
            task_id,
            &attempt_id,
            generation,
            &lease_expires_at,
        )?)
    } else {
        None
    };
    crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    tx.commit().map_err(to_cli_error)?;

    let mut payload = inspect_task(paths, task_id)?;
    payload["leaseToken"] = json!(lease_token);
    payload["target"] = json!({
        "id": target_id,
        "providerId": runner_key,
        "status": "online",
    });
    payload["attemptId"] = json!(attempt_id);
    payload["executionGeneration"] = json!(generation);
    payload["taskBrokerUrl"] = broker_url.map(Value::from).unwrap_or(Value::Null);
    payload["executionToken"] = execution
        .as_ref()
        .map(|value| Value::from(value.0.clone()))
        .unwrap_or(Value::Null);
    payload["executionTokenExpiresAt"] = execution
        .as_ref()
        .map(|_| Value::from(lease_expires_at))
        .unwrap_or(Value::Null);
    payload["inputManifest"] = json!(read_task_inputs(paths, task_id)?);
    Ok(Some(payload))
}

fn runner_matches_task(
    settings: &crate::model_gateway::ModelGatewaySettings,
    task: &Value,
    runner_key: &str,
) -> bool {
    if !settings
        .local_cli
        .provider_order
        .iter()
        .any(|provider| provider == runner_key)
    {
        return true;
    }
    if task
        .pointer("/input/executionMode")
        .and_then(Value::as_str)
        == Some("manual")
    {
        return false;
    }
    let order = settings.local_cli.provider_order.clone();
    if order.is_empty() {
        return true;
    }
    let history = task.get("attempts").and_then(Value::as_array);
    let used = history
        .into_iter()
        .flatten()
        .filter_map(|attempt| attempt.get("runnerKey").and_then(Value::as_str))
        .collect::<Vec<_>>();
    let expected = order
        .iter()
        .find(|candidate| !used.iter().any(|used| *used == candidate.as_str()))
        .or_else(|| order.first());
    expected.is_some_and(|expected| expected == runner_key)
}

fn task_dependency_ready(connection: &rusqlite::Connection, task: &Value) -> bool {
    let Some(dependency) = task.get("dependsOnTaskId").and_then(Value::as_str) else {
        return true;
    };
    connection
        .query_row("SELECT status = 'succeeded' FROM tasks WHERE id = ?", [dependency], |row| row.get::<_, bool>(0))
        .unwrap_or(false)
}

pub(crate) fn heartbeat_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
) -> Result<Value, CliError> {
    let lease = verify_lease(paths, task_id, lease_token)?;
    let expires_at = lease_expires_at();
    match task_queue_adapter(lease["queue"].as_str().unwrap_or(""))? {
        TaskQueueAdapter::Wiki => crate::wiki::heartbeat_task(paths, task_id, &expires_at)?,
        TaskQueueAdapter::Writing => crate::writing::heartbeat_task(paths, task_id)?,
        TaskQueueAdapter::Publication => crate::publication::heartbeat_task(paths, task_id)?,
        TaskQueueAdapter::Release => crate::release::heartbeat_task(paths, task_id)?,
    };
    let storage = Storage::open(paths)?;
    let tx = storage.connection().unchecked_transaction().map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let changed = tx.execute(
        "UPDATE tasks SET lease_expires_at = ?, heartbeat_at = ?, updated_at = ? WHERE id = ? AND status = 'running' AND lease_owner = ?",
        params![expires_at, now, now, task_id, hash_secret(lease_token)],
    ).map_err(to_cli_error)?;
    if changed != 1 {
        return Err(CliError::with_code("execution_fenced", "Task lease is no longer active."));
    }
    crate::storage::record_scope(&tx, "tasks", lease["projectId"].as_str(), None)?;
    tx.commit().map_err(to_cli_error)?;
    inspect_task_in_session(paths, lease["projectId"].as_str().unwrap_or_default(), task_id)
}

pub(crate) fn complete_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
    result: Option<Value>,
) -> Result<Value, CliError> {
    let lease = verify_lease(paths, task_id, lease_token)?;
    let prepared_panel_state: Option<(String, Value)> =
        match task_queue_adapter(lease["queue"].as_str().unwrap_or(""))? {
            TaskQueueAdapter::Wiki => Some((
                lease["panelId"].as_str().unwrap_or_default().to_owned(),
                crate::wiki::prepare_task_completion(paths, task_id, result.clone())?["state"].clone(),
            )),
            TaskQueueAdapter::Writing => crate::writing::prepare_task_completion(paths, task_id)?,
            TaskQueueAdapter::Publication => crate::publication::prepare_task_completion(paths, task_id, result.clone())?,
            TaskQueueAdapter::Release => crate::release::prepare_task_completion(paths, task_id, result.clone())?,
        };
    let project_id = lease["projectId"].as_str().unwrap_or_default();
    finalize_task_runtime(
        paths,
        project_id,
        task_id,
        "succeeded",
        result,
        None,
        None,
        None,
        prepared_panel_state.as_ref().map(|(panel_id, state)| (panel_id.as_str(), state)),
        lease["executionGeneration"].as_i64(),
    )?;
    inspect_task_in_session(paths, project_id, task_id)
}

pub(crate) fn fail_task(
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

pub(crate) fn fail_task_with_class(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
    message: &str,
    retry_after: Option<&str>,
    failure_class: TaskFailureClass,
) -> Result<Value, CliError> {
    let lease = verify_lease(paths, task_id, lease_token)?;
    if retry_after.is_some_and(|value| chrono::DateTime::parse_from_rfc3339(value).is_err()) {
        return Err(CliError::with_code("invalid_retry_after", "Expected an RFC 3339 timestamp."));
    }
    let retry_after = if failure_class == TaskFailureClass::TerminalTask {
        None
    } else {
        retry_after.map(str::to_owned).or_else(|| {
            Some(execution_retry_after(lease["attempt"].as_i64().unwrap_or(1)))
        })
    };
    fail_queue_projection(
        paths,
        lease["queue"].as_str().unwrap_or(""),
        task_id,
        message,
        retry_after.as_deref(),
    )?;
    let project_id = lease["projectId"].as_str().unwrap_or_default();
    finalize_task_runtime(
        paths,
        project_id,
        task_id,
        "failed",
        None,
        Some(json!({ "message": message })),
        retry_after.as_deref(),
        Some(failure_class),
        None,
        lease["executionGeneration"].as_i64(),
    )?;
    inspect_task_in_session(paths, project_id, task_id)
}

pub(crate) fn interrupt_task_for_studio_restart(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
) -> Result<Value, CliError> {
    fail_task_with_class(
        paths,
        task_id,
        lease_token,
        "Studio restarted while the local executor was running.",
        Some(&crate::control::now_iso()),
        TaskFailureClass::RetryableInterruption,
    )
}

fn fail_queue_projection(
    paths: &MyOpenPanelsPaths,
    queue: &str,
    task_id: &str,
    message: &str,
    retry_after: Option<&str>,
) -> Result<(), CliError> {
    match task_queue_adapter(queue)? {
        TaskQueueAdapter::Wiki => crate::wiki::fail_task_with_retry(paths, task_id, message, retry_after)?,
        TaskQueueAdapter::Writing => crate::writing::fail_task(paths, task_id, message)?,
        TaskQueueAdapter::Publication => crate::publication::fail_task(paths, task_id, message)?,
        TaskQueueAdapter::Release => crate::release::fail_task(paths, task_id, message)?,
    };
    Ok(())
}

fn release_queue_projection(paths: &MyOpenPanelsPaths, queue: &str, task_id: &str) -> Result<(), CliError> {
    match task_queue_adapter(queue)? {
        TaskQueueAdapter::Wiki => crate::wiki::release_task(paths, task_id)?,
        TaskQueueAdapter::Writing => crate::writing::release_task(paths, task_id)?,
        TaskQueueAdapter::Publication => crate::publication::release_task(paths, task_id)?,
        TaskQueueAdapter::Release => crate::release::release_task(paths, task_id)?,
    };
    Ok(())
}

pub(crate) fn mark_latest_attempt_invalid_output(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    message: &str,
) -> Result<(), CliError> {
    let project_id = task_project_id(paths, task_id)?;
    let storage = Storage::open(paths)?;
    let tx = storage.connection().unchecked_transaction().map_err(to_cli_error)?;
    let history_json = tx.query_row(
        "SELECT attempt_history_json FROM tasks WHERE id = ?",
        [task_id],
        |row| row.get::<_, String>(0),
    ).map_err(to_cli_error)?;
    let mut history = serde_json::from_str::<Vec<Value>>(&history_json).unwrap_or_default();
    if let Some(last) = history.last_mut() {
        last["status"] = json!("invalid_output");
        last["error"] = json!({ "code": "invalid_output", "message": message });
    }
    tx.execute(
        "UPDATE tasks SET attempt_history_json = ?, updated_at = ? WHERE id = ?",
        params![serde_json::to_string(&history).map_err(to_cli_error)?, crate::control::now_iso(), task_id],
    ).map_err(to_cli_error)?;
    crate::storage::record_scope(&tx, "tasks", Some(&project_id), None)?;
    tx.commit().map_err(to_cli_error)
}

pub(crate) fn release_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
) -> Result<Value, CliError> {
    let lease = verify_lease(paths, task_id, lease_token)?;
    let queue = lease["queue"].as_str().unwrap_or("");
    release_queue_projection(paths, queue, task_id)?;
    finalize_task_runtime(
        paths,
        lease["projectId"].as_str().unwrap_or_default(),
        task_id,
        "failed",
        None,
        Some(json!({ "code": "execution_released" })),
        Some(&crate::control::now_iso()),
        Some(TaskFailureClass::RetryableInterruption),
        None,
        lease["executionGeneration"].as_i64(),
    )?;
    inspect_task(paths, task_id)
}

pub fn retry_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let task = inspect_task(paths, task_id)?["task"].clone();
    if !matches!(task.get("status").and_then(Value::as_str), Some("failed" | "cancelled" | "superseded")) {
        return Err(CliError::with_code(
            "invalid_task_transition",
            "Only terminal unsuccessful Tasks can be retried.",
        ));
    }
    let project_id = task.get("projectId").and_then(Value::as_str).unwrap_or_default();
    let panel_id = task.get("panelId").and_then(Value::as_str).unwrap_or_default();
    let new_id = crate::ids::random_id("task");
    let now = crate::control::now_iso();
    let storage = Storage::open(paths)?;
    let tx = storage.connection().unchecked_transaction().map_err(to_cli_error)?;
    tx.execute(
        r#"
        INSERT INTO tasks (
          id, project_id, origin_panel_id, handler_key, status, target_ref, input_json, source_json,
          retry_of_task_id, mutation_key, available_at, idempotency_key, created_at, updated_at
        ) SELECT ?, project_id, origin_panel_id, handler_key, 'queued', target_ref, input_json, source_json,
          id, mutation_key, ?, NULL, ?, ?
        FROM tasks WHERE id = ?
        "#,
        params![new_id, now, now, now, task_id],
    ).map_err(to_cli_error)?;
    tx.execute(
        r#"
        INSERT INTO task_resources (task_id, resource_id, role, captured_version, created_at)
        SELECT ?, resource_id, role, captured_version, ? FROM task_resources WHERE task_id = ?
        "#,
        params![new_id, now, task_id],
    )
    .map_err(to_cli_error)?;
    if let Some(mut state) = storage.read_panel_state(project_id, panel_id)? {
        clone_task_projection(&mut state, task_id, &new_id, &now);
        Storage::write_panel_state_in_transaction(&tx, project_id, panel_id, &state)?;
    }
    crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    inspect_task(paths, &new_id)
}

fn clone_task_projection(value: &mut Value, task_id: &str, new_id: &str, now: &str) -> bool {
    match value {
        Value::Array(items) => {
            if let Some(position) = items.iter().position(|item| item.get("id").and_then(Value::as_str) == Some(task_id)) {
                let mut clone = items[position].clone();
                clone["id"] = json!(new_id);
                clone["status"] = json!("queued");
                clone["attempt"] = json!(0);
                clone["createdAt"] = json!(now);
                clone["updatedAt"] = json!(now);
                items.push(clone);
                true
            } else {
                items.iter_mut().any(|item| clone_task_projection(item, task_id, new_id, now))
            }
        }
        Value::Object(object) => object.values_mut().any(|item| clone_task_projection(item, task_id, new_id, now)),
        _ => false,
    }
}
