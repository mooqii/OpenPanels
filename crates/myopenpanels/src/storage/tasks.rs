impl Storage {
    pub fn upsert_tasks(
        &self,
        project_id: &str,
        panel_id: &str,
        queue: &str,
        tasks: &[Value],
    ) -> Result<(), CliError> {
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        Self::upsert_tasks_in_transaction(&tx, project_id, panel_id, queue, tasks)?;
        tx.commit().map_err(to_cli_error)
    }

    pub(crate) fn upsert_tasks_in_transaction(
        tx: &Transaction<'_>,
        project_id: &str,
        panel_id: &str,
        queue: &str,
        tasks: &[Value],
    ) -> Result<(), CliError> {
        let mut changed = false;
        let mut dependencies = Vec::new();
        for task in tasks {
            let id = required_task_string(task, "id")?;
            let task_type = task.get("type").and_then(Value::as_str).unwrap_or("unknown");
            let route = crate::capabilities::task_route_for_queue_and_type(queue, task_type)?;
            let input = json!({
                "documentId": task.get("documentId"),
                "documentKind": task.get("documentKind"),
                "markdownVersion": task.get("markdownVersion"),
                "changeEvents": task.get("changeEvents"),
            });
            let source = json!({
                "wikiSpaceId": task.get("wikiSpaceId"),
                "agentSkillId": task.get("agentSkillId"),
            });
            crate::capabilities::validate_task_local_skill(
                queue,
                task_type,
                &route.capability,
                &input,
                &source,
            )?;
            let created_at = task
                .get("createdAt")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(crate::control::now_iso);
            let updated_at = task
                .get("updatedAt")
                .and_then(Value::as_str)
                .unwrap_or(&created_at);
            let status = canonical_task_status(
                task.get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("queued"),
            )?;
            let depends_on = task
                .get("dependsOnTaskId")
                .and_then(Value::as_str)
                .or_else(|| {
                    task.get("dependsOnTaskIds")
                        .and_then(Value::as_array)
                        .and_then(|values| values.first())
                        .and_then(Value::as_str)
                });
            dependencies.push((id.to_owned(), depends_on.map(str::to_owned)));
            changed |= tx
                .execute(
                    r#"
                    INSERT INTO tasks (
                      id, project_id, origin_panel_id, handler_key, status, target_ref,
                      input_json, source_json, error_json, depends_on_task_id, mutation_key,
                      attempt_count, available_at, idempotency_key, created_at, updated_at
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    ON CONFLICT(id) DO UPDATE SET
                      status = CASE
                        WHEN tasks.status = 'succeeded' THEN tasks.status
                        WHEN excluded.status IN ('failed', 'cancelled', 'superseded', 'succeeded')
                          THEN excluded.status
                        WHEN tasks.status = 'failed' AND excluded.status = 'queued'
                          THEN excluded.status
                        ELSE tasks.status
                      END,
                      input_json = excluded.input_json,
                      source_json = excluded.source_json,
                      error_json = CASE
                        WHEN excluded.status IN ('failed', 'cancelled', 'superseded')
                          THEN excluded.error_json
                        WHEN tasks.status = 'failed' AND excluded.status = 'queued'
                          THEN NULL
                        ELSE tasks.error_json
                      END,
                      depends_on_task_id = COALESCE(tasks.depends_on_task_id, excluded.depends_on_task_id),
                      mutation_key = COALESCE(tasks.mutation_key, excluded.mutation_key),
                      updated_at = MAX(tasks.updated_at, excluded.updated_at)
                    "#,
                    params![
                        id,
                        project_id,
                        panel_id,
                        route.handler_key,
                        status,
                        task.get("targetId").and_then(Value::as_str).unwrap_or(""),
                        serde_json::to_string(&input).map_err(to_cli_error)?,
                        serde_json::to_string(&source).map_err(to_cli_error)?,
                        task.get("error")
                            .filter(|value| !value.is_null())
                            .map(serde_json::to_string)
                            .transpose()
                            .map_err(to_cli_error)?,
                        Option::<&str>::None,
                        task.get("mutationKey").and_then(Value::as_str),
                        task.get("attempt").and_then(Value::as_i64).unwrap_or(0).clamp(0, crate::tasks::TASK_EXECUTION_LIMIT),
                        task.get("availableAt").and_then(Value::as_str).unwrap_or(&created_at),
                        task.get("idempotencyKey").and_then(Value::as_str),
                        created_at,
                        updated_at,
                    ],
                )
                .map_err(to_cli_error)?
                > 0;
        }
        for (task_id, depends_on) in dependencies {
            changed |= tx
                .execute(
                    "UPDATE tasks SET depends_on_task_id = ? WHERE id = ? AND project_id = ? AND depends_on_task_id IS NOT ?",
                    params![depends_on, task_id, project_id, depends_on],
                )
                .map_err(to_cli_error)?
                > 0;
        }
        if changed {
            record_scope(tx, "tasks", Some(project_id), None)?;
        }
        sync_task_resources_for_project(tx, project_id)
    }

    pub fn insert_capability_task(
        &self,
        project_id: &str,
        panel_id: &str,
        capability_key: &str,
        task_type: &str,
        target_ref: &str,
        input: &Value,
        source: &Value,
    ) -> Result<Value, CliError> {
        let route = crate::capabilities::task_route_for_capability(capability_key, task_type)?;
        self.insert_task(
            project_id,
            panel_id,
            &route.queue,
            &route.task_type,
            &route.capability,
            target_ref,
            input,
            source,
        )
    }

    pub fn insert_task(
        &self,
        project_id: &str,
        panel_id: &str,
        queue: &str,
        task_type: &str,
        capability: &str,
        target_ref: &str,
        input: &Value,
        source: &Value,
    ) -> Result<Value, CliError> {
        crate::capabilities::validate_task_local_skill(
            queue,
            task_type,
            capability,
            input,
            source,
        )?;
        let route = crate::capabilities::task_route(queue, task_type, capability)?
            .ok_or_else(|| CliError::with_code("task_route_not_found", "Task handler not found."))?;
        let id = crate::ids::random_id("task");
        let now = crate::control::now_iso();
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        insert_task_row(
            &tx,
            &id,
            project_id,
            panel_id,
            route.handler_key.as_str(),
            target_ref,
            input,
            source,
            None,
            None,
            None,
            &now,
        )?;
        sync_task_resources_for_project(&tx, project_id)?;
        record_scope(&tx, "tasks", Some(project_id), None)?;
        tx.commit().map_err(to_cli_error)?;
        self.read_task_value(project_id, &id)?
            .ok_or_else(|| CliError::new(format!("Created task was not found: {id}")))
    }

    pub fn insert_tasks_with_panel_states(
        &self,
        project_id: &str,
        panel_id: &str,
        tasks: &[TaskInsert],
        panel_states: &[(&str, &Value)],
    ) -> Result<(Vec<Value>, Vec<i64>), CliError> {
        if tasks.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let now = crate::control::now_iso();
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        for task in tasks {
            crate::capabilities::validate_task_local_skill(
                &task.queue,
                &task.task_type,
                &task.capability,
                &task.input,
                &task.source,
            )?;
            let route = crate::capabilities::task_route(
                &task.queue,
                &task.task_type,
                &task.capability,
            )?
            .ok_or_else(|| CliError::with_code("task_route_not_found", "Task handler not found."))?;
            if task.exclusive_non_terminal {
                let busy = tx
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM tasks WHERE project_id = ? AND origin_panel_id = ? AND handler_key = ? AND target_ref = ? AND status IN ('queued', 'running'))",
                        params![project_id, panel_id, route.handler_key, task.target_ref],
                        |row| row.get::<_, bool>(0),
                    )
                    .map_err(to_cli_error)?;
                if busy {
                    return Err(CliError::with_code(
                        "task_target_busy",
                        "The Task target already has active work.",
                    ));
                }
            }
            insert_task_row(
                &tx,
                &task.id,
                project_id,
                panel_id,
                &route.handler_key,
                &task.target_ref,
                &task.input,
                &task.source,
                None,
                None,
                task.idempotency_key.as_deref(),
                &now,
            )?;
        }
        let revisions = panel_states
            .iter()
            .map(|(state_panel_id, state)| {
                Self::write_panel_state_in_transaction(&tx, project_id, state_panel_id, state)
            })
            .collect::<Result<Vec<_>, _>>()?;
        sync_task_resources_for_project(&tx, project_id)?;
        record_scope(&tx, "tasks", Some(project_id), None)?;
        tx.commit().map_err(to_cli_error)?;

        let created = tasks
            .iter()
            .map(|task| {
                self.read_task_value(project_id, &task.id)?
                    .ok_or_else(|| CliError::new(format!("Created task was not found: {}", task.id)))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok((created, revisions))
    }

    pub fn list_tasks(&self, project_id: &str) -> Result<Vec<Value>, CliError> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT t.id, t.project_id, t.origin_panel_id, COALESCE(p.kind, ''), t.handler_key,
                   t.status, t.target_ref, t.input_json, t.source_json, t.result_json,
                   t.error_json, t.depends_on_task_id, t.retry_of_task_id, t.mutation_key,
                   t.attempt_count, t.attempt_history_json, t.current_runner_key, t.available_at,
                   t.execution_generation, t.lease_owner, t.lease_expires_at,
                   t.heartbeat_at, t.created_at, t.updated_at, t.completed_at, t.archived_at
            FROM tasks t
            LEFT JOIN panels p ON p.project_id = t.project_id AND p.id = t.origin_panel_id
            WHERE t.project_id = ?
            ORDER BY t.updated_at DESC, t.id ASC
            "#,
        ).map_err(to_cli_error)?;
        let rows = statement
            .query_map([project_id], task_value_from_row)
            .map_err(to_cli_error)?;
        let mut tasks = rows
            .map(|row| row.map_err(to_cli_error))
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        for task in &mut tasks {
            let task_id = task.get("id").and_then(Value::as_str).unwrap_or("");
            task["resources"] = json!(self.task_resource_links(task_id)?);
        }
        Ok(tasks)
    }

    fn read_task_value(&self, project_id: &str, task_id: &str) -> Result<Option<Value>, CliError> {
        self.list_tasks(project_id).map(|tasks| {
            tasks
                .into_iter()
                .find(|task| task.get("id").and_then(Value::as_str) == Some(task_id))
        })
    }

    pub fn task_panel_target(&self, task_id: &str) -> Result<Option<(String, String)>, CliError> {
        self.connection
            .query_row(
                "SELECT project_id, origin_panel_id FROM tasks WHERE id = ? AND origin_panel_id IS NOT NULL LIMIT 1",
                [task_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(to_cli_error)
    }

    fn task_resource_links(&self, task_id: &str) -> Result<Vec<Value>, CliError> {
        let mut statement = self
            .connection
            .prepare(
                r#"
                SELECT tr.resource_id, r.kind, tr.role, tr.captured_version
                FROM task_resources tr
                JOIN resources r
                  ON r.project_id = tr.project_id AND r.id = tr.resource_id
                WHERE tr.task_id = ?
                ORDER BY
                  CASE tr.role
                    WHEN 'primary' THEN 0
                    WHEN 'input' THEN 1
                    WHEN 'output' THEN 2
                    ELSE 3
                  END,
                  tr.resource_id
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map([task_id], |row| {
                Ok(json!({
                    "resourceId": row.get::<_, String>(0)?,
                    "resourceKind": row.get::<_, String>(1)?,
                    "role": row.get::<_, String>(2)?,
                    "capturedVersion": row.get::<_, Option<i64>>(3)?,
                }))
            })
            .map_err(to_cli_error)?;
        rows.map(|row| row.map_err(to_cli_error)).collect()
    }
}

#[allow(clippy::too_many_arguments)]
fn insert_task_row(
    connection: &Connection,
    id: &str,
    project_id: &str,
    panel_id: &str,
    handler_key: &str,
    target_ref: &str,
    input: &Value,
    source: &Value,
    depends_on_task_id: Option<&str>,
    retry_of_task_id: Option<&str>,
    idempotency_key: Option<&str>,
    now: &str,
) -> Result<(), CliError> {
    connection.execute(
        r#"
        INSERT INTO tasks (
          id, project_id, origin_panel_id, handler_key, status, target_ref,
          input_json, source_json, depends_on_task_id, retry_of_task_id,
          available_at, idempotency_key, created_at, updated_at
        ) VALUES (?, ?, ?, ?, 'queued', ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            id,
            project_id,
            panel_id,
            handler_key,
            target_ref,
            serde_json::to_string(input).map_err(to_cli_error)?,
            serde_json::to_string(source).map_err(to_cli_error)?,
            depends_on_task_id,
            retry_of_task_id,
            now,
            idempotency_key,
            now,
            now,
        ],
    ).map_err(to_cli_error)?;
    Ok(())
}

fn task_value_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let parse = |index: usize| -> Value {
        row.get::<_, String>(index)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or(Value::Null)
    };
    let parse_optional = |index: usize| -> Value {
        row.get::<_, Option<String>>(index)
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or(Value::Null)
    };
    let handler_key = row.get::<_, String>(4)?;
    let route = crate::capabilities::task_route_for_handler(&handler_key)
        .ok()
        .flatten();
    let current_runner = row.get::<_, Option<String>>(16)?;
    let execution_method = current_runner
        .as_deref()
        .map(|provider_id| json!({
            "kind": "localCli",
            "connectionId": format!("local-cli:{provider_id}"),
            "providerId": provider_id,
        }))
        .unwrap_or(Value::Null);
    let attempt_history = parse(15);
    let error = parse_optional(10);
    let available_at = row.get::<_, String>(17)?;
    let retry_after = (available_at > crate::control::now_iso())
        .then(|| available_at.clone());
    Ok(json!({
        "id": row.get::<_, String>(0)?,
        "projectId": row.get::<_, String>(1)?,
        "panelId": row.get::<_, Option<String>>(2)?,
        "panelKind": row.get::<_, String>(3)?,
        "handlerKey": handler_key,
        "queue": route.map(|value| value.queue.as_str()).unwrap_or(""),
        "type": route.map(|value| value.task_type.as_str()).unwrap_or(""),
        "capability": route.map(|value| value.capability.as_str()).unwrap_or(""),
        "status": row.get::<_, String>(5)?,
        "targetId": row.get::<_, String>(6)?,
        "input": parse(7),
        "source": parse(8),
        "result": parse_optional(9),
        "error": error,
        "terminalReason": error,
        "dependsOnTaskId": row.get::<_, Option<String>>(11)?,
        "retryOfTaskId": row.get::<_, Option<String>>(12)?,
        "mutationKey": row.get::<_, Option<String>>(13)?,
        "attempt": row.get::<_, i64>(14)?,
        "attemptLimit": crate::tasks::TASK_EXECUTION_LIMIT,
        "attempts": attempt_history,
        "currentRunnerKey": current_runner,
        "availableAt": available_at,
        "retryAfter": retry_after,
        "executionGeneration": row.get::<_, i64>(18)?,
        "executionMethod": execution_method,
        "lease": {
            "owner": row.get::<_, Option<String>>(19)?,
            "expiresAt": row.get::<_, Option<String>>(20)?,
            "heartbeatAt": row.get::<_, Option<String>>(21)?,
        },
        "createdAt": row.get::<_, String>(22)?,
        "updatedAt": row.get::<_, String>(23)?,
        "completedAt": row.get::<_, Option<String>>(24)?,
        "archivedAt": row.get::<_, Option<String>>(25)?,
    }))
}

fn canonical_task_status(status: &str) -> Result<&str, CliError> {
    if matches!(
        status,
        "queued" | "running" | "succeeded" | "failed" | "cancelled" | "superseded"
    ) {
        Ok(status)
    } else {
        Err(CliError::with_code(
            "invalid_task_status",
            format!("Unsupported Task status: {status}"),
        ))
    }
}

fn required_task_string<'a>(task: &'a Value, key: &str) -> Result<&'a str, CliError> {
    task.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::new(format!("Project task {key} is required.")))
}
