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
        let mut changed = 0usize;
        for task in tasks {
            let id = task
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| CliError::new("Project task id is required."))?;
            let created_at = task
                .get("createdAt")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(crate::control::now_iso);
            let updated_at = task
                .get("updatedAt")
                .and_then(Value::as_str)
                .unwrap_or(&created_at);
            let task_type = task
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let input = json!({
                "documentId": task.get("documentId"),
                "markdownVersion": task.get("markdownVersion"),
                "changeEvents": task.get("changeEvents"),
            });
            let source = json!({
                "wikiSpaceId": task.get("wikiSpaceId"),
                "agentSkillId": task.get("agentSkillId"),
            });
            let workflow_run_id = task
                .get("workflowRunId")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| workflow_run_id_for_task(id));
            insert_workflow_run_if_missing(
                &tx,
                &workflow_run_id,
                project_id,
                panel_id,
                &format!("{queue}.{task_type}"),
                task.get("sourceWorkflowRunId").and_then(Value::as_str),
                &source,
                &created_at,
            )?;
            let already_exists = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM tasks WHERE id = ?)",
                    [id],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(to_cli_error)?;
            changed += tx
                .execute(
                    r#"
                    INSERT INTO tasks (
                      id, project_id, panel_id, queue, type, capability, status, target_ref,
                      input_json, source_json, attempts, max_attempts, created_at, updated_at,
                      workflow_run_id, idempotency_key, available_at, required_protocol_version,
                      mutation_key, mutation_sequence
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    ON CONFLICT(id) DO UPDATE SET
                      status = CASE
                        WHEN excluded.status IN ('waiting', 'cancelled', 'stale', 'superseded')
                             AND tasks.status NOT IN ('succeeded', 'cancelled', 'stale', 'superseded')
                          THEN excluded.status
                        ELSE tasks.status
                      END,
                      updated_at = MAX(tasks.updated_at, excluded.updated_at),
                      input_json = excluded.input_json,
                      source_json = excluded.source_json,
                      mutation_key = COALESCE(tasks.mutation_key, excluded.mutation_key),
                      mutation_sequence = COALESCE(tasks.mutation_sequence, excluded.mutation_sequence),
                      terminal_reason_json = CASE
                        WHEN excluded.status IN ('cancelled', 'stale', 'superseded')
                          THEN COALESCE(tasks.terminal_reason_json, json_object('code', excluded.status))
                        ELSE tasks.terminal_reason_json
                      END
                    "#,
                    params![
                        id,
                        project_id,
                        panel_id,
                        queue,
                        task_type,
                        project_task_capability(queue, task_type),
                        task.get("status")
                            .and_then(Value::as_str)
                            .unwrap_or("queued"),
                        task.get("targetId").and_then(Value::as_str).unwrap_or(""),
                        serde_json::to_string(&input).map_err(to_cli_error)?,
                        serde_json::to_string(&source).map_err(to_cli_error)?,
                        task.get("attempt").and_then(Value::as_i64).unwrap_or(0),
                        task.get("maxAttempts").and_then(Value::as_i64).unwrap_or(8),
                        created_at,
                        updated_at,
                        workflow_run_id,
                        task.get("idempotencyKey").and_then(Value::as_str),
                        task.get("availableAt")
                            .and_then(Value::as_str)
                            .unwrap_or(&created_at),
                        crate::content::EXECUTION_PROTOCOL_VERSION,
                        task.get("mutationKey").and_then(Value::as_str),
                        task.get("mutationSequence").and_then(Value::as_i64),
                    ],
                )
                .map_err(to_cli_error)?;
            if !already_exists {
                insert_task_created_records(
                    &tx,
                    id,
                    &workflow_run_id,
                    project_id,
                    task.get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("queued"),
                    &project_task_capability(queue, task_type),
                    &input,
                    &created_at,
                )?;
            }
        }
        for task in tasks {
            let Some(task_id) = task.get("id").and_then(Value::as_str) else {
                continue;
            };
            if let Some(dependencies) = task.get("dependsOnTaskIds").and_then(Value::as_array) {
                for prerequisite_id in dependencies.iter().filter_map(Value::as_str) {
                    tx.execute(
                        "INSERT OR IGNORE INTO task_dependencies (task_id, prerequisite_task_id, success_condition, failure_policy, created_at) VALUES (?, ?, 'succeeded', 'cancel', ?)",
                        params![task_id, prerequisite_id, crate::control::now_iso()],
                    )
                    .map_err(to_cli_error)?;
                }
            }
        }
        if changed > 0 {
            record_scope(&tx, "tasks", Some(project_id), None)?;
        }
        tx.commit().map_err(to_cli_error)
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
        let id = crate::ids::random_id("task");
        let workflow_run_id = workflow_run_id_for_task(&id);
        let now = crate::control::now_iso();
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        insert_workflow_run_if_missing(
            &tx,
            &workflow_run_id,
            project_id,
            panel_id,
            &format!("{queue}.{task_type}"),
            None,
            source,
            &now,
        )?;
        tx.execute(
            r#"
                INSERT INTO tasks (
                  id, project_id, panel_id, queue, type, capability, status, target_ref,
                  input_json, source_json, attempts, max_attempts, created_at, updated_at,
                  workflow_run_id, idempotency_key, available_at, required_protocol_version,
                  dispatch_mode
                ) VALUES (?, ?, ?, ?, ?, ?, 'queued', ?, ?, ?, 0, 8, ?, ?, ?, NULL, ?, ?, 'auto')
            "#,
            params![
                id,
                project_id,
                panel_id,
                queue,
                task_type,
                capability,
                target_ref,
                serde_json::to_string(input).map_err(to_cli_error)?,
                serde_json::to_string(source).map_err(to_cli_error)?,
                now,
                now,
                workflow_run_id,
                now,
                crate::content::EXECUTION_PROTOCOL_VERSION,
            ],
        )
        .map_err(to_cli_error)?;
        insert_task_created_records(
            &tx,
            &id,
            &workflow_run_id,
            project_id,
            "queued",
            capability,
            input,
            &now,
        )?;
        record_scope(&tx, "tasks", Some(project_id), None)?;
        tx.commit().map_err(to_cli_error)?;
        self.list_tasks(project_id)?
            .into_iter()
            .find(|task| task.get("id").and_then(Value::as_str) == Some(id.as_str()))
            .ok_or_else(|| CliError::new(format!("Created task was not found: {id}")))
    }

    pub(crate) fn ensure_workflow_run(
        &self,
        project_id: &str,
        panel_id: &str,
        workflow_run_id: &str,
        definition_key: &str,
        status: &str,
        source: &Value,
    ) -> Result<(), CliError> {
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        let now = crate::control::now_iso();
        insert_workflow_run_if_missing(
            &tx,
            workflow_run_id,
            project_id,
            panel_id,
            definition_key,
            None,
            source,
            &now,
        )?;
        tx.execute(
            "UPDATE workflow_runs SET definition_key = ?, status = ?, source_json = ?, updated_at = ? WHERE id = ?",
            params![
                definition_key,
                status,
                serde_json::to_string(source).map_err(to_cli_error)?,
                now,
                workflow_run_id
            ],
        )
        .map_err(to_cli_error)?;
        tx.commit().map_err(to_cli_error)
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
        let task_ids = tasks.iter().map(|task| task.id.clone()).collect::<Vec<_>>();
        let workflow_run_id = crate::ids::random_id("workflow-run");
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        insert_workflow_run_if_missing(
            &tx,
            &workflow_run_id,
            project_id,
            panel_id,
            tasks
                .first()
                .map(|task| task.queue.as_str())
                .unwrap_or("task"),
            None,
            &json!({ "createdBy": "task.insertMany" }),
            &now,
        )?;
        for (task, id) in tasks.iter().zip(&task_ids) {
            tx.execute(
                r#"
                INSERT INTO tasks (
                  id, project_id, panel_id, queue, type, capability, status, target_ref,
                  input_json, source_json, attempts, max_attempts, created_at, updated_at,
                  workflow_run_id, idempotency_key, available_at, required_protocol_version,
                  dispatch_mode
                ) VALUES (?, ?, ?, ?, ?, ?, 'queued', ?, ?, ?, 0, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
                params![
                    id,
                    project_id,
                    panel_id,
                    &task.queue,
                    &task.task_type,
                    &task.capability,
                    &task.target_ref,
                    serde_json::to_string(&task.input).map_err(to_cli_error)?,
                    serde_json::to_string(&task.source).map_err(to_cli_error)?,
                    task.max_attempts,
                    now,
                    now,
                    workflow_run_id,
                    task.idempotency_key,
                    now,
                    crate::content::EXECUTION_PROTOCOL_VERSION,
                    &task.dispatch_mode,
                ],
            )
            .map_err(to_cli_error)?;
            insert_task_created_records(
                &tx,
                id,
                &workflow_run_id,
                project_id,
                "queued",
                &task.capability,
                &task.input,
                &now,
            )?;
        }
        let revisions = panel_states
            .iter()
            .map(|(state_panel_id, state)| {
                Self::write_panel_state_in_transaction(&tx, project_id, state_panel_id, state)
            })
            .collect::<Result<Vec<_>, _>>()?;
        record_scope(&tx, "tasks", Some(project_id), None)?;
        tx.commit().map_err(to_cli_error)?;

        let mut created = self
            .list_tasks(project_id)?
            .into_iter()
            .filter_map(|task| {
                let id = task.get("id").and_then(Value::as_str)?.to_owned();
                Some((id, task))
            })
            .collect::<HashMap<_, _>>();
        let tasks = task_ids
            .into_iter()
            .map(|id| {
                created
                    .remove(&id)
                    .ok_or_else(|| CliError::new(format!("Created task was not found: {id}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok((tasks, revisions))
    }

    pub fn list_tasks(&self, project_id: &str) -> Result<Vec<Value>, CliError> {
        let mut statement = self
            .connection
            .prepare(
                r#"
                SELECT
                  t.id, t.queue, t.project_id, t.panel_id, p.kind, t.type, t.status,
                  t.target_ref, t.created_at, t.updated_at, t.attempts, t.max_attempts,
                  lease_owner, lease_expires_at, last_heartbeat_at, retry_after,
                  t.capability, t.assigned_agent_id, t.result_json, t.error_json, t.completed_at,
                  t.input_json, t.source_json, t.workflow_run_id, t.execution_generation,
                  t.available_at, t.archived_at, t.terminal_reason_json,
                  t.required_protocol_version, t.dispatch_mode,
                  t.requested_gateway_connection_id,
                  t.mutation_key, t.mutation_sequence
                FROM tasks t
                JOIN panels p ON p.project_id = t.project_id AND p.id = t.panel_id
                WHERE t.project_id = ?
                ORDER BY t.updated_at DESC, t.id ASC
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![project_id], |row| {
                let input_json: String = row.get(21)?;
                let source_json: String = row.get(22)?;
                let input =
                    serde_json::from_str::<Value>(&input_json).unwrap_or_else(|_| json!({}));
                let source =
                    serde_json::from_str::<Value>(&source_json).unwrap_or_else(|_| json!({}));
                let queue = row.get::<_, String>(1)?;
                let panel_kind = row.get::<_, String>(4)?;
                let task_type = row.get::<_, String>(5)?;
                let status = row.get::<_, String>(6)?;
                let target_id = row.get::<_, String>(7)?;
                let attempts = row.get::<_, i64>(10)?;
                let max_attempts = row.get::<_, i64>(11)?;
                let lease_owner = row.get::<_, Option<String>>(12)?;
                let lease_expires_at = row.get::<_, Option<String>>(13)?;
                let last_heartbeat_at = row.get::<_, Option<String>>(14)?;
                let retry_after = row.get::<_, Option<String>>(15)?;
                let capability = row.get::<_, Option<String>>(16)?;
                let assigned_agent_id = row.get::<_, Option<String>>(17)?;
                let result_json = row.get::<_, Option<String>>(18)?;
                let error_json = row.get::<_, Option<String>>(19)?;
                let completed_at = row.get::<_, Option<String>>(20)?;
                let terminal_reason_json = row.get::<_, Option<String>>(27)?;
                let result = result_json
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                    .unwrap_or(Value::Null);
                let error = error_json
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                    .unwrap_or(Value::Null);
                let terminal_reason = terminal_reason_json
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                    .unwrap_or(Value::Null);
                Ok(TaskRecord {
                    id: row.get(0)?,
                    workflow_run_id: row.get(23)?,
                    queue: queue.clone(),
                    project_id: row.get(2)?,
                    panel_id: row.get(3)?,
                    panel_kind,
                    task_type: task_type.clone(),
                    status: TaskStatus(status),
                    target_id,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                    attempt: attempts,
                    max_attempts,
                    lease: TaskLease {
                        owner: assigned_agent_id.clone().or(lease_owner),
                        expires_at: lease_expires_at,
                        heartbeat_at: last_heartbeat_at,
                    },
                    retry_after,
                    capability: capability.unwrap_or_default(),
                    assigned_target_id: assigned_agent_id,
                    completed_at,
                    execution_generation: row.get(24)?,
                    available_at: row.get(25)?,
                    archived_at: row.get(26)?,
                    terminal_reason,
                    required_protocol_version: row.get(28)?,
                    dispatch_mode: row.get(29)?,
                    requested_gateway_connection_id: row.get(30)?,
                    mutation_key: row.get(31)?,
                    mutation_sequence: row.get(32)?,
                    input,
                    source,
                    result,
                    error,
                })
            })
            .map_err(to_cli_error)?;
        rows.map(|row| {
            row.map_err(to_cli_error)
                .and_then(|task| serde_json::to_value(task).map_err(to_cli_error))
        })
        .collect()
    }

    pub fn task_panel_target(&self, task_id: &str) -> Result<Option<(String, String)>, CliError> {
        self.connection
            .query_row(
                "SELECT project_id, panel_id FROM tasks WHERE id = ? LIMIT 1",
                [task_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(to_cli_error)
    }
}
