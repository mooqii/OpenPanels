impl Storage {
    pub fn write_direct_operation(&self, operation: &Value) -> Result<(), CliError> {
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        Self::write_direct_operation_in_transaction(&tx, operation)?;
        tx.commit().map_err(to_cli_error)
    }

    pub(crate) fn write_direct_operation_in_transaction(
        tx: &Transaction<'_>,
        operation: &Value,
    ) -> Result<(), CliError> {
        let required = |name: &str| -> Result<&str, CliError> {
            operation
                .get(name)
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    CliError::with_code(
                        "invalid_operation",
                        format!("Direct Operation is missing {name}."),
                    )
                })
        };
        let id = required("id")?;
        let owner_context_id = required("ownerContextId")?;
        let intent = required("intent")?;
        let status = required("status")?;
        if !matches!(status, "active" | "completed" | "failed" | "cancelled") {
            return Err(CliError::with_code(
                "invalid_operation_status",
                format!("Unsupported Direct Operation status: {status}"),
            ));
        }
        let project_id = required("projectId")?;
        let panel_id = required("panelId")?;
        let target_id = required("targetId")?;
        let base_revision = operation
            .get("baseRevision")
            .and_then(Value::as_i64)
            .filter(|value| *value >= 0)
            .ok_or_else(|| {
                CliError::with_code(
                    "invalid_operation",
                    "Direct Operation is missing baseRevision.",
                )
            })?;
        let created_at = required("createdAt")?;
        let updated_at = required("updatedAt")?;
        let completed_at = operation.get("completedAt").and_then(Value::as_str);
        let payload_json = serde_json::to_string(&direct_operation_payload(operation))
            .map_err(to_cli_error)?;
        tx.execute(
            r#"
            INSERT INTO direct_operations (
              id, owner_context_id, intent, status, project_id, panel_id,
              target_id, base_revision, payload_json, created_at, updated_at, completed_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
              owner_context_id = excluded.owner_context_id,
              intent = excluded.intent,
              status = excluded.status,
              project_id = excluded.project_id,
              panel_id = excluded.panel_id,
              target_id = excluded.target_id,
              base_revision = excluded.base_revision,
              payload_json = excluded.payload_json,
              updated_at = excluded.updated_at,
              completed_at = excluded.completed_at
            "#,
            params![
                id,
                owner_context_id,
                intent,
                status,
                project_id,
                panel_id,
                target_id,
                base_revision,
                payload_json,
                created_at,
                updated_at,
                completed_at,
            ],
        )
        .map_err(to_cli_error)?;
        Ok(())
    }

    pub fn read_direct_operation(
        &self,
        operation_id: &str,
    ) -> Result<Option<Value>, CliError> {
        Ok(self
            .connection
            .query_row(
                r#"
                SELECT id, owner_context_id, intent, status, project_id, panel_id,
                       target_id, base_revision, payload_json,
                       created_at, updated_at, completed_at
                FROM direct_operations WHERE id = ?
                "#,
                [operation_id],
                direct_operation_from_row,
            )
            .optional()
            .map_err(to_cli_error)?)
    }

    pub fn list_direct_operations(
        &self,
        owner_context_id: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<Value>, CliError> {
        let mut statement = self
            .connection
            .prepare(
                r#"
                SELECT id, owner_context_id, intent, status, project_id, panel_id,
                       target_id, base_revision, payload_json,
                       created_at, updated_at, completed_at
                FROM direct_operations
                WHERE (? IS NULL OR owner_context_id = ?)
                  AND (? IS NULL OR status = ?)
                ORDER BY updated_at DESC, id ASC
                "#,
            )
            .map_err(to_cli_error)?;
        let operations = statement
            .query_map(
                params![owner_context_id, owner_context_id, status, status],
                direct_operation_from_row,
            )
            .map_err(to_cli_error)?
            .map(|row| row.map_err(to_cli_error))
            .collect();
        operations
    }

    pub(crate) fn list_terminal_direct_operation_ids_before(
        &self,
        completed_before: &str,
    ) -> Result<Vec<String>, CliError> {
        let mut statement = self
            .connection
            .prepare(
                r#"
                SELECT id FROM direct_operations
                WHERE status IN ('completed', 'failed', 'cancelled')
                  AND completed_at IS NOT NULL AND completed_at <= ?
                ORDER BY completed_at ASC, id ASC
                "#,
            )
            .map_err(to_cli_error)?;
        let operation_ids = statement
            .query_map([completed_before], |row| row.get::<_, String>(0))
            .map_err(to_cli_error)?
            .map(|row| row.map_err(to_cli_error))
            .collect();
        operation_ids
    }

    pub(crate) fn direct_operation_ids(&self) -> Result<Vec<String>, CliError> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM direct_operations ORDER BY id")
            .map_err(to_cli_error)?;
        let operation_ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(to_cli_error)?
            .map(|row| row.map_err(to_cli_error))
            .collect();
        operation_ids
    }
}

fn direct_operation_payload(operation: &Value) -> Value {
    let mut payload = operation.as_object().cloned().unwrap_or_default();
    for key in [
        "id",
        "ownerContextId",
        "intent",
        "status",
        "projectId",
        "panelId",
        "targetId",
        "baseRevision",
        "createdAt",
        "updatedAt",
        "completedAt",
    ] {
        payload.remove(key);
    }
    if let Some(target) = payload.get_mut("target").and_then(Value::as_object_mut) {
        for key in ["placeholderShapeId", "documentId", "baseContentVersion"] {
            target.remove(key);
        }
    }
    Value::Object(payload)
}

fn direct_operation_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let id = row.get::<_, String>(0)?;
    let owner_context_id = row.get::<_, String>(1)?;
    let intent = row.get::<_, String>(2)?;
    let status = row.get::<_, String>(3)?;
    let project_id = row.get::<_, String>(4)?;
    let panel_id = row.get::<_, String>(5)?;
    let target_id = row.get::<_, String>(6)?;
    let base_revision = row.get::<_, i64>(7)?;
    let payload_json = row.get::<_, String>(8)?;
    let created_at = row.get::<_, String>(9)?;
    let updated_at = row.get::<_, String>(10)?;
    let completed_at = row.get::<_, Option<String>>(11)?;
    let mut operation = serde_json::from_str::<Value>(&payload_json).unwrap_or_else(|_| json!({}));
    if !operation.is_object() {
        operation = json!({});
    }
    operation["id"] = json!(id);
    operation["ownerContextId"] = json!(owner_context_id);
    operation["intent"] = json!(intent);
    operation["status"] = json!(status);
    operation["projectId"] = json!(project_id);
    operation["panelId"] = json!(panel_id);
    operation["targetId"] = json!(target_id);
    operation["baseRevision"] = json!(base_revision);
    operation["createdAt"] = json!(created_at);
    operation["updatedAt"] = json!(updated_at);
    operation["completedAt"] = completed_at.map_or(Value::Null, Value::String);
    if operation.get("target").is_none_or(|value| !value.is_object()) {
        operation["target"] = json!({});
    }
    match intent.as_str() {
        "canvas.image.generate" => {
            operation["target"]["placeholderShapeId"] = operation["targetId"].clone();
        }
        "my-document.create" | "my-document.revise" => {
            operation["target"]["documentId"] = operation["targetId"].clone();
            operation["target"]["baseContentVersion"] = operation["baseRevision"].clone();
        }
        _ => {}
    }
    Ok(operation)
}
