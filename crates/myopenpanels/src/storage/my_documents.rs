fn require_my_document_resource(
    connection: &Connection,
    project_id: &str,
    document_id: &str,
) -> Result<(), CliError> {
    let identity = connection
        .query_row(
            r#"
            SELECT r.project_id, r.kind, d.document_kind
            FROM documents d
            JOIN resources r ON r.id = d.resource_id
            WHERE d.resource_id = ? AND r.deleted_at IS NULL
            "#,
            [document_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(to_cli_error)?
        .ok_or_else(|| {
            CliError::with_code(
                "not_found",
                format!("My Document not found: {document_id}"),
            )
        })?;
    if identity.0 != project_id || identity.1 != "document" || identity.2 != "my_document" {
        return Err(CliError::with_code(
            "resource_identity_conflict",
            "The My Document target belongs to another Project or module.",
        ));
    }
    Ok(())
}

impl Storage {
    pub(crate) fn delete_my_document_in_transaction(
        connection: &Connection,
        project_id: &str,
        panel_id: &str,
        document_id: &str,
    ) -> Result<i64, CliError> {
        require_my_document_resource(connection, project_id, document_id)?;
        let now = crate::control::now_iso();
        let revision = record_resource_scope(connection, project_id, panel_id, document_id)?;
        connection
            .execute(
                "UPDATE resources SET deleted_at = ?, updated_at = ?, revision = ? WHERE project_id = ? AND id = ?",
                params![now, now, revision, project_id, document_id],
            )
            .map_err(to_cli_error)?;
        Ok(revision)
    }

    pub(crate) fn create_my_document_in_transaction(
        connection: &Connection,
        project_id: &str,
        panel_id: &str,
        document: &Value,
    ) -> Result<i64, CliError> {
        let document_id = document
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| CliError::new("My Document id is required."))?;
        let exists = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM resources WHERE id = ?)",
                [document_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(to_cli_error)?;
        if exists {
            return Err(CliError::with_code(
                "resource_identity_conflict",
                format!("My Document already exists: {document_id}"),
            ));
        }
        connection
            .execute(
                r#"
                UPDATE documents
                SET position = position + 1
                WHERE document_kind = 'my_document'
                  AND resource_id IN (
                    SELECT id FROM resources
                    WHERE project_id = ? AND deleted_at IS NULL
                  )
                "#,
                [project_id],
            )
            .map_err(to_cli_error)?;
        let revision =
            persist_document(connection, project_id, panel_id, "my_document", 0, document)?;
        sync_task_resources_for_project(connection, project_id)?;
        Ok(revision)
    }

    pub(crate) fn rename_my_document_resource(
        &self,
        project_id: &str,
        panel_id: &str,
        document_id: &str,
        title: &str,
    ) -> Result<i64, CliError> {
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        require_my_document_resource(&tx, project_id, document_id)?;
        let revision = record_resource_scope(&tx, project_id, panel_id, document_id)?;
        tx.execute(
            r#"
            UPDATE resources
            SET title = ?, revision = ?, updated_at = ?
            WHERE project_id = ? AND id = ?
            "#,
            params![
                title,
                revision,
                crate::control::now_iso(),
                project_id,
                document_id
            ],
        )
        .map_err(to_cli_error)?;
        tx.commit().map_err(to_cli_error)?;
        Ok(revision)
    }
}
