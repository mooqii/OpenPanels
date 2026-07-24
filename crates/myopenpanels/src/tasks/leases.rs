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
    let now = crate::control::now_iso();
    let lease_token = random_secret("lease");
    let lease_expires_at = lease_expires_at();
    let attempt_id = random_id("attempt");
    let broker_url = explicit_broker_url
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .or_else(crate::content::task_broker_url_for_claim);
    if broker_url.as_deref().is_none_or(str::is_empty) && !cfg!(test) {
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
    let candidates = {
        let mut statement = tx
            .prepare(
                r#"
                SELECT id, handler_key, input_json, attempt_history_json
                FROM tasks
                WHERE project_id = ? AND status = 'queued'
                  AND (? IS NULL OR id = ?)
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
                      AND predecessor.status = 'running'
                  )
                ORDER BY created_at, id
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map(
                params![
                    project_id,
                    requested_task_id,
                    requested_task_id,
                    TASK_EXECUTION_LIMIT,
                    now,
                    settings.max_concurrency,
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .map_err(to_cli_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_cli_error)?;
        rows
    };
    let mut candidate_id = None;
    for (task_id, handler_key, input_json, attempt_history_json) in candidates {
        let Some(route) = crate::capabilities::task_route_for_handler(&handler_key)? else {
            return Err(CliError::with_code(
                "task_route_not_found",
                format!("Task Handler has no Capability route: {handler_key}"),
            ));
        };
        if requested_capability.is_some_and(|capability| route.capability != capability)
            || requested_queue.is_some_and(|queue| route.queue != queue)
        {
            continue;
        }
        let task = json!({
            "input": serde_json::from_str::<Value>(&input_json).map_err(to_cli_error)?,
            "attempts": serde_json::from_str::<Value>(&attempt_history_json).map_err(to_cli_error)?,
        });
        if requested_task_id.is_some() || runner_matches_task(&settings, &task, runner_key) {
            candidate_id = Some(task_id);
            break;
        }
    }
    let Some(task_id) = candidate_id else {
        return Ok(None);
    };
    let generation = tx
        .query_row(
            r#"
            UPDATE tasks SET status = 'running', attempt_count = attempt_count + 1,
              current_runner_key = ?, lease_owner = ?, execution_token_hash = NULL,
              lease_expires_at = ?, heartbeat_at = ?, error_json = NULL,
              execution_generation = execution_generation + 1, updated_at = ?
            WHERE id = ? AND project_id = ? AND status = 'queued'
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
            ],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(to_cli_error)?;
    let Some(generation) = generation else {
        return Ok(None);
    };
    let execution = if broker_url.as_deref().is_some_and(|value| !value.is_empty()) {
        Some(crate::content::create_execution_context_in_transaction(
            &tx,
            &task_id,
            &attempt_id,
            generation,
            &lease_expires_at,
        )?)
    } else {
        None
    };
    crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    tx.commit().map_err(to_cli_error)?;

    let mut payload = inspect_task(paths, &task_id)?;
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
    payload["inputManifest"] = json!(read_task_inputs(paths, &task_id)?);
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

pub(crate) fn heartbeat_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
) -> Result<Value, CliError> {
    let lease = verify_lease(paths, task_id, lease_token)?;
    let expires_at = lease_expires_at();
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
    let prepared_panel_state =
        match task_domain(lease["queue"].as_str().unwrap_or(""))? {
            TaskDomain::Wiki => {
                crate::wiki::prepare_task_completion(paths, task_id, result.clone())?
            }
            TaskDomain::Writing => crate::writing::prepare_task_completion(paths, task_id)?,
            TaskDomain::Publication => {
                crate::publication::prepare_task_completion(paths, task_id, result.clone())?
            }
            TaskDomain::Release => {
                crate::release::prepare_task_completion(paths, task_id, result.clone())?
            }
        };
    let project_id = lease["projectId"].as_str().unwrap_or_default();
    finalize_task_runtime(
        paths,
        project_id,
        task_id,
        "succeeded",
        TaskOutputPlan::completed(result, prepared_panel_state),
        None,
        None,
        None,
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
    let domain = task_domain(lease["queue"].as_str().unwrap_or(""))?;
    let project_id = lease["projectId"].as_str().unwrap_or_default();
    finalize_task_runtime(
        paths,
        project_id,
        task_id,
        "failed",
        TaskOutputPlan::empty(),
        Some(json!({ "message": message })),
        retry_after.as_deref(),
        Some(failure_class),
        lease["executionGeneration"].as_i64(),
    )?;
    if domain == TaskDomain::Writing {
        crate::writing::cleanup_uncommitted_writing_skill(paths, task_id)?;
    }
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
    let domain = task_domain(lease["queue"].as_str().unwrap_or(""))?;
    finalize_task_runtime(
        paths,
        lease["projectId"].as_str().unwrap_or_default(),
        task_id,
        "failed",
        TaskOutputPlan::empty(),
        Some(json!({ "code": "execution_released" })),
        Some(&crate::control::now_iso()),
        Some(TaskFailureClass::RetryableInterruption),
        lease["executionGeneration"].as_i64(),
    )?;
    if domain == TaskDomain::Writing {
        crate::writing::cleanup_uncommitted_writing_skill(paths, task_id)?;
    }
    inspect_task(paths, task_id)
}

pub(crate) fn supersede_task_for_content_conflict(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_token: &str,
    message: &str,
) -> Result<Value, CliError> {
    let lease = verify_lease(paths, task_id, lease_token)?;
    let domain = task_domain(lease["queue"].as_str().unwrap_or(""))?;
    finalize_task_runtime(
        paths,
        lease["projectId"].as_str().unwrap_or_default(),
        task_id,
        "superseded",
        TaskOutputPlan::empty(),
        Some(json!({ "code": "content_conflict", "message": message })),
        None,
        None,
        lease["executionGeneration"].as_i64(),
    )?;
    if domain == TaskDomain::Writing {
        crate::writing::cleanup_uncommitted_writing_skill(paths, task_id)?;
    }
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
    let new_id = crate::ids::random_id("task");
    let input = recapture_retry_skill_snapshot(paths, &task, &new_id)?;
    let input_json = serde_json::to_string(&input).map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let storage = Storage::open(paths)?;
    let tx = storage.connection().unchecked_transaction().map_err(to_cli_error)?;
    tx.execute(
        r#"
        INSERT INTO tasks (
          id, project_id, origin_panel_id, handler_key, status, target_ref, input_json, source_json,
          retry_of_task_id, mutation_key, available_at, idempotency_key, created_at, updated_at
        ) SELECT ?, project_id, origin_panel_id, handler_key, 'queued', target_ref, ?, source_json,
          id, mutation_key, ?, NULL, ?, ?
        FROM tasks WHERE id = ?
        "#,
        params![new_id, input_json, now, now, now, task_id],
    ).map_err(to_cli_error)?;
    tx.execute(
        r#"
        INSERT INTO task_resources (task_id, resource_id, role, captured_version, created_at)
        SELECT ?, resource_id, role, captured_version, ? FROM task_resources WHERE task_id = ?
        "#,
        params![new_id, now, task_id],
    )
    .map_err(to_cli_error)?;
    crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    inspect_task(paths, &new_id)
}

fn recapture_retry_skill_snapshot(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    retry_task_id: &str,
) -> Result<Value, CliError> {
    let captured = match task.get("queue").and_then(Value::as_str) {
        Some("writing") => crate::writing::recapture_retry_skill_snapshot(paths, task),
        Some("publication") => {
            crate::publication::recapture_retry_skill_snapshot(paths, task, retry_task_id)
        }
        Some("release") => {
            crate::release::recapture_retry_skill_snapshot(paths, task, retry_task_id)
        }
        _ => Ok(None),
    };
    captured
        .map_err(|error| {
            CliError::with_code(
                "task_retry_skill_snapshot_failed",
                format!(
                    "Task retry failed because the required Skill Snapshot could not be captured: {}",
                    error.message()
                ),
            )
        })?
        .or_else(|| task.get("input").cloned())
        .ok_or_else(|| CliError::new("The original Task input is missing."))
}
