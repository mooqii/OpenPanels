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
        let operation_json = serde_json::to_string(operation).map_err(to_cli_error)?;
        tx.execute(
            r#"
            INSERT INTO direct_operations (
              id, owner_context_id, intent, status, project_id, panel_id,
              target_id, base_revision, operation_json, created_at, updated_at, completed_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
              owner_context_id = excluded.owner_context_id,
              intent = excluded.intent,
              status = excluded.status,
              project_id = excluded.project_id,
              panel_id = excluded.panel_id,
              target_id = excluded.target_id,
              base_revision = excluded.base_revision,
              operation_json = excluded.operation_json,
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
                operation_json,
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
        self.connection
            .query_row(
                "SELECT operation_json FROM direct_operations WHERE id = ?",
                [operation_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(to_cli_error)?
            .map(|raw| serde_json::from_str(&raw).map_err(to_cli_error))
            .transpose()
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
                SELECT operation_json FROM direct_operations
                WHERE (? IS NULL OR owner_context_id = ?)
                  AND (? IS NULL OR status = ?)
                ORDER BY updated_at DESC, id ASC
                "#,
            )
            .map_err(to_cli_error)?;
        let operations = statement
            .query_map(
                params![owner_context_id, owner_context_id, status, status],
                |row| row.get::<_, String>(0),
            )
            .map_err(to_cli_error)?
            .map(|row| {
                let raw = row.map_err(to_cli_error)?;
                serde_json::from_str(&raw).map_err(to_cli_error)
            })
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
