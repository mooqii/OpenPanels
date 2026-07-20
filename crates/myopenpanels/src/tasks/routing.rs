fn annotate_dispatch_state(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    tasks: Vec<Value>,
) -> Result<Vec<Value>, CliError> {
    let targets = read_targets(paths, project_id)?;
    let storage = Storage::open(paths)?;
    let mut output = Vec::with_capacity(tasks.len());
    for mut task in tasks {
        let capability = task.get("capability").and_then(Value::as_str).unwrap_or("");
        let matching = matching_targets(&targets, capability);
        let assigned_target = task
            .get("assignedTargetId")
            .and_then(Value::as_str)
            .and_then(|id| targets.iter().find(|target| target["id"] == id))
            .cloned();
        let dependencies =
            read_task_dependency_values(storage.connection(), task["id"].as_str().unwrap_or(""))?;
        let mutation_blocked =
            mutation_task_blocked(storage.connection(), task["id"].as_str().unwrap_or(""))?;
        let required_protocol = task
            .get("requiredProtocolVersion")
            .and_then(Value::as_i64)
            .unwrap_or(crate::content::EXECUTION_PROTOCOL_VERSION);
        let compatible_target_count = matching
            .iter()
            .filter(|target| {
                target
                    .get("protocolVersion")
                    .and_then(Value::as_i64)
                    .unwrap_or(crate::content::EXECUTION_PROTOCOL_VERSION)
                    == required_protocol
            })
            .count();
        let dispatch_state = if is_active_task(&task) {
            "running"
        } else if task.get("status").and_then(Value::as_str) == Some("waiting") || mutation_blocked
        {
            "waiting"
        } else if !is_pending_task(&task) {
            "done"
        } else if matching.is_empty() {
            "noTarget"
        } else if compatible_target_count == 0 {
            "incompatible"
        } else {
            "eligible"
        };
        if let Some(object) = task.as_object_mut() {
            object.insert("dispatchState".to_owned(), json!(dispatch_state));
            object.insert("matchedTargetCount".to_owned(), json!(matching.len()));
            object.insert(
                "compatibleTargetCount".to_owned(),
                json!(compatible_target_count),
            );
            object.insert("dependencies".to_owned(), json!(dependencies));
            object.insert("mutationBlocked".to_owned(), json!(mutation_blocked));
            if mutation_blocked {
                object.insert("ready".to_owned(), json!(false));
                object.insert("blockedReason".to_owned(), json!("mutationPredecessor"));
            }
            object.insert(
                "assignedTarget".to_owned(),
                assigned_target.unwrap_or(Value::Null),
            );
        }
        output.push(task);
    }
    Ok(output)
}

fn mutation_task_blocked(
    connection: &rusqlite::Connection,
    task_id: &str,
) -> Result<bool, CliError> {
    connection
        .query_row(
            r#"
            SELECT EXISTS(
              SELECT 1
              FROM tasks AS candidate
              WHERE candidate.id = ?
                AND candidate.mutation_key IS NOT NULL
                AND (
                  EXISTS (
                    SELECT 1 FROM tasks AS active
                    WHERE active.project_id = candidate.project_id
                      AND active.mutation_key = candidate.mutation_key
                      AND active.id <> candidate.id
                      AND active.status IN ('reserved', 'running', 'claimed', 'converting', 'indexing')
                  )
                  OR EXISTS (
                    SELECT 1 FROM tasks AS predecessor
                    WHERE predecessor.project_id = candidate.project_id
                      AND predecessor.mutation_key = candidate.mutation_key
                      AND predecessor.mutation_sequence < candidate.mutation_sequence
                      AND (
                        predecessor.status IN ('waiting', 'queued', 'reserved', 'running', 'claimed', 'converting', 'indexing', 'failed')
                      )
                  )
                )
            )
            "#,
            [task_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(to_cli_error)
}

fn read_targets(paths: &MyOpenPanelsPaths, project_id: &str) -> Result<Vec<Value>, CliError> {
    let storage = Storage::open(paths)?;
    read_targets_from_connection(storage.connection(), project_id)
}

fn read_targets_from_connection(
    connection: &rusqlite::Connection,
    project_id: &str,
) -> Result<Vec<Value>, CliError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT id, name, host, capabilities_json,
                   priority, status, last_error, last_heartbeat_at, created_at, updated_at,
                   protocol_version, max_concurrency,
                   (SELECT COUNT(*) FROM tasks active
                    WHERE active.assigned_agent_id = agent_targets.id
                      AND active.status IN ('reserved', 'running', 'claimed', 'converting', 'indexing')),
                   model_gateway_connection_id
            FROM agent_targets
            WHERE project_id = ? AND transport = 'command'
            ORDER BY priority DESC, last_heartbeat_at DESC, id ASC
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map(params![project_id], target_from_row)
        .map_err(to_cli_error)?;
    rows.map(|row| row.map(compute_target_status).map_err(to_cli_error))
        .collect()
}

fn read_target_value(
    connection: &rusqlite::Connection,
    project_id: &str,
    target_id: &str,
) -> Result<Option<Value>, CliError> {
    connection
        .query_row(
            r#"
            SELECT id, name, host, capabilities_json,
                   priority, status, last_error, last_heartbeat_at, created_at, updated_at,
                   protocol_version, max_concurrency,
                   (SELECT COUNT(*) FROM tasks active
                    WHERE active.assigned_agent_id = agent_targets.id
                      AND active.status IN ('reserved', 'running', 'claimed', 'converting', 'indexing')),
                   model_gateway_connection_id
            FROM agent_targets
            WHERE project_id = ? AND id = ? AND transport = 'command'
            "#,
            params![project_id, target_id],
            target_from_row,
        )
        .optional()
        .map(|target| target.map(compute_target_status))
        .map_err(to_cli_error)
}

fn target_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let capabilities_json = row.get::<_, String>(3)?;
    let capabilities =
        serde_json::from_str::<Value>(&capabilities_json).unwrap_or_else(|_| json!([]));
    Ok(json!({
        "id": row.get::<_, String>(0)?,
        "name": row.get::<_, String>(1)?,
        "host": row.get::<_, String>(2)?,
        "capabilities": capabilities,
        "priority": row.get::<_, i64>(4)?,
        "status": row.get::<_, String>(5)?,
        "lastError": row.get::<_, Option<String>>(6)?,
        "lastHeartbeatAt": row.get::<_, String>(7)?,
        "createdAt": row.get::<_, String>(8)?,
        "updatedAt": row.get::<_, String>(9)?,
        "protocolVersion": row.get::<_, i64>(10)?,
        "maxConcurrency": row.get::<_, i64>(11)?,
        "activeAttempts": row.get::<_, i64>(12)?,
        "modelGatewayConnectionId": row.get::<_, Option<String>>(13)?,
    }))
}

fn compute_target_status(mut target: Value) -> Value {
    let stale = target
        .get("lastHeartbeatAt")
        .and_then(Value::as_str)
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .is_none_or(|value| {
            value.with_timezone(&chrono::Utc)
                < chrono::Utc::now() - chrono::Duration::seconds(TARGET_ONLINE_WINDOW_SECONDS)
        });
    if stale && target.get("status").and_then(Value::as_str) == Some("online") {
        target["status"] = json!("offline");
    }
    target
}

fn matching_targets<'a>(targets: &'a [Value], capability: &str) -> Vec<&'a Value> {
    targets
        .iter()
        .filter(|target| target.get("status").and_then(Value::as_str) == Some("online"))
        .filter(|target| capability_matches_any(&target_capabilities(target), capability))
        .collect()
}

fn preferred_target_id(
    connection: &rusqlite::Connection,
    project_id: &str,
    targets: &[Value],
    task_id: &str,
    capability: &str,
    required_protocol_version: i64,
) -> Result<Option<String>, CliError> {
    let eligible = matching_targets(targets, capability)
        .into_iter()
        .filter(|target| {
            target
                .get("protocolVersion")
                .and_then(Value::as_i64)
                .unwrap_or(crate::content::EXECUTION_PROTOCOL_VERSION)
                == required_protocol_version
        })
        .filter(|target| {
            target
                .get("activeAttempts")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                < target
                    .get("maxConcurrency")
                    .and_then(Value::as_i64)
                    .unwrap_or(1)
        })
        .collect::<Vec<_>>();
    let (dispatch_mode, requested_connection_id) = connection
        .query_row(
            "SELECT dispatch_mode, requested_gateway_connection_id FROM tasks WHERE id = ?",
            [task_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .map_err(to_cli_error)?;
    let mut ordered = Vec::new();
    let routed_ids = route_target_ids(connection, project_id, capability)?;
    for routed_id in routed_ids {
        if let Some(target) = eligible
            .iter()
            .copied()
            .find(|target| target.get("id").and_then(Value::as_str) == Some(&routed_id))
        {
            ordered.push(target);
        }
    }
    for target in eligible {
        if !ordered
            .iter()
            .any(|candidate| candidate["id"] == target["id"])
        {
            ordered.push(target);
        }
    }
    if let Some(requested) = requested_connection_id.as_deref() {
        if dispatch_mode == "prefer" {
            ordered.sort_by_key(|target| usize::from(target_channel_key(target) != requested));
        }
    }
    let attempt_counts = channel_attempt_counts(connection, task_id)?;
    let minimum_attempts = ordered
        .iter()
        .map(|target| {
            attempt_counts
                .get(target_channel_key(target))
                .copied()
                .unwrap_or(0)
        })
        .min();
    Ok(ordered
        .iter()
        .find(|target| {
            Some(
                attempt_counts
                    .get(target_channel_key(target))
                    .copied()
                    .unwrap_or(0),
            ) == minimum_attempts
        })
        .and_then(|target| target.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned))
}

fn target_channel_key(target: &Value) -> &str {
    target
        .get("modelGatewayConnectionId")
        .and_then(Value::as_str)
        .or_else(|| target.get("id").and_then(Value::as_str))
        .unwrap_or("")
}

fn channel_attempt_counts(
    connection: &rusqlite::Connection,
    task_id: &str,
) -> Result<HashMap<String, i64>, CliError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT COALESCE(model_gateway_connection_id, agent_target_id), COUNT(*)
            FROM task_attempts
            WHERE task_id = ?
              AND status IN ('failed_retryable', 'invalid_output', 'interrupted')
              AND NOT (
                status = 'interrupted'
                AND COALESCE(json_extract(error_json, '$.code'), '') = 'studio_restart'
              )
            GROUP BY COALESCE(model_gateway_connection_id, agent_target_id)
            "#,
        )
        .map_err(to_cli_error)?;
    let counts = statement
        .query_map([task_id], |row| {
            Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(to_cli_error)?
        .filter_map(|row| match row {
            Ok((Some(key), count)) => Some(Ok((key, count))),
            Ok((None, _)) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(to_cli_error)?;
    Ok(counts)
}

fn has_untried_eligible_target(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    task_id: &str,
    current_target_id: &str,
) -> Result<bool, CliError> {
    let storage = Storage::open(paths)?;
    let connection = storage.connection();
    let (capability, required_protocol) = connection
        .query_row(
            r#"
            SELECT capability, required_protocol_version
            FROM tasks WHERE id = ? AND project_id = ?
            "#,
            params![task_id, project_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(to_cli_error)?;
    let targets = read_targets_from_connection(connection, project_id)?;
    let mut attempt_counts = channel_attempt_counts(connection, task_id)?;
    let current_key = targets
        .iter()
        .find(|target| target.get("id").and_then(Value::as_str) == Some(current_target_id))
        .map(target_channel_key)
        .unwrap_or(current_target_id);
    *attempt_counts.entry(current_key.to_owned()).or_insert(0) += 1;
    let current_attempts = attempt_counts.get(current_key).copied().unwrap_or(1);
    Ok(matching_targets(&targets, &capability)
        .into_iter()
        .filter(|target| {
            target
                .get("protocolVersion")
                .and_then(Value::as_i64)
                .unwrap_or(crate::content::EXECUTION_PROTOCOL_VERSION)
                == required_protocol
        })
        .any(|target| {
            target_channel_key(target) != current_key
                && attempt_counts
                    .get(target_channel_key(target))
                    .copied()
                    .unwrap_or(0)
                    < current_attempts
        }))
}

fn route_target_ids(
    connection: &rusqlite::Connection,
    project_id: &str,
    capability: &str,
) -> Result<Vec<String>, CliError> {
    let mut routes = connection
        .prepare(
            "SELECT agent_target_id FROM agent_routes WHERE project_id = ? AND capability = ? ORDER BY position ASC",
        )
        .map_err(to_cli_error)?;
    let target_ids = routes
        .query_map(params![project_id, capability], |row| {
            row.get::<_, String>(0)
        })
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    Ok(target_ids)
}

fn target_capabilities(target: &Value) -> Vec<String> {
    target
        .get("capabilities")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn capability_matches_any(patterns: &[String], capability: &str) -> bool {
    patterns.iter().any(|pattern| {
        pattern == "*"
            || pattern == capability
            || pattern
                .strip_suffix(".*")
                .is_some_and(|prefix| capability.starts_with(&format!("{prefix}.")))
    })
}

fn normalize_capabilities(capabilities: Vec<String>) -> Vec<String> {
    let mut capabilities = capabilities
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    capabilities.sort();
    capabilities.dedup();
    capabilities
}

fn read_task_dependency_values(
    connection: &rusqlite::Connection,
    task_id: &str,
) -> Result<Vec<Value>, CliError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT d.prerequisite_task_id, p.status, d.success_condition, d.failure_policy
            FROM task_dependencies d JOIN tasks p ON p.id = d.prerequisite_task_id
            WHERE d.task_id = ? ORDER BY d.prerequisite_task_id
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([task_id], |row| {
            Ok(json!({
                "prerequisiteTaskId": row.get::<_, String>(0)?,
                "status": row.get::<_, String>(1)?,
                "successCondition": row.get::<_, String>(2)?,
                "failurePolicy": row.get::<_, String>(3)?,
            }))
        })
        .map_err(to_cli_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)
}

fn task_event_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let reason = row
        .get::<_, Option<String>>(6)?
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .unwrap_or(Value::Null);
    Ok(json!({
        "id": row.get::<_, i64>(0)?,
        "taskId": row.get::<_, String>(1)?,
        "workflowId": row.get::<_, String>(2)?,
        "eventType": row.get::<_, String>(3)?,
        "fromStatus": row.get::<_, Option<String>>(4)?,
        "toStatus": row.get::<_, Option<String>>(5)?,
        "reason": reason,
        "attemptId": row.get::<_, Option<String>>(7)?,
        "targetId": row.get::<_, Option<String>>(8)?,
        "createdAt": row.get::<_, String>(9)?,
    }))
}

fn task_attempt_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let parse = |index| -> rusqlite::Result<Value> {
        Ok(row
            .get::<_, Option<String>>(index)?
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .unwrap_or(Value::Null))
    };
    Ok(json!({
        "id": row.get::<_, String>(0)?,
        "taskId": row.get::<_, String>(1)?,
        "attemptNumber": row.get::<_, i64>(2)?,
        "executionGeneration": row.get::<_, i64>(3)?,
        "targetId": row.get::<_, Option<String>>(4)?,
        "status": row.get::<_, String>(5)?,
        "startedAt": row.get::<_, String>(6)?,
        "heartbeatAt": row.get::<_, Option<String>>(7)?,
        "finishedAt": row.get::<_, Option<String>>(8)?,
        "result": parse(9)?,
        "error": parse(10)?,
        "stagingStatus": row.get::<_, Option<String>>(11)?,
        "stagedBytes": row.get::<_, Option<i64>>(12)?.unwrap_or(0),
        "stagingUpdatedAt": row.get::<_, Option<String>>(13)?,
        "modelGatewayConnectionId": row.get::<_, Option<String>>(14)?,
        "executorSnapshot": parse(15)?,
        "failureClass": row.get::<_, Option<String>>(16)?,
    }))
}

fn workflow_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let source = row
        .get::<_, String>(4)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .unwrap_or_else(|| json!({}));
    let total = row.get::<_, i64>(8)?;
    let succeeded = row.get::<_, i64>(9)?;
    Ok(json!({
        "id": row.get::<_, String>(0)?,
        "type": row.get::<_, String>(1)?,
        "status": row.get::<_, String>(2)?,
        "sourceWorkflowId": row.get::<_, Option<String>>(3)?,
        "source": source,
        "createdAt": row.get::<_, String>(5)?,
        "updatedAt": row.get::<_, String>(6)?,
        "archivedAt": row.get::<_, Option<String>>(7)?,
        "taskCount": total,
        "succeededTaskCount": succeeded,
        "progress": if total == 0 { 0.0 } else { succeeded as f64 / total as f64 },
    }))
}
