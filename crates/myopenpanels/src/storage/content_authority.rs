impl Storage {
    pub(crate) fn write_content_commit_in_transaction(
        connection: &Connection,
        project_id: &str,
        commit: &crate::content::ContentCommit,
    ) -> Result<i64, CliError> {
        let row = connection
            .query_row(
                r#"
                SELECT r.kind, d.document_kind, r.active_content_revision_id,
                       r.content_version, r.content_manifest_hash, r.content_hash, r.revision
                FROM resources r
                LEFT JOIN documents d ON d.resource_id = r.id
                WHERE r.project_id = ? AND r.id = ? AND r.deleted_at IS NULL
                "#,
                params![project_id, commit.resource_key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .optional()
            .map_err(to_cli_error)?
            .ok_or_else(|| {
                CliError::with_code(
                    "content_resource_missing",
                    format!(
                        "Content commit targets missing resource {}.",
                        commit.resource_key
                    ),
                )
            })?;
        let expected_kind = match commit.resource_kind.as_str() {
            "wiki_markdown" => row.0 == "document" && row.1.as_deref() == Some("wiki_source"),
            "my_document" => row.0 == "document" && row.1.as_deref() == Some("my_document"),
            "wiki_space" => row.0 == "wiki_space",
            other => {
                return Err(CliError::with_code(
                    "invalid_content_resource",
                    format!("Unsupported database-owned content kind: {other}"),
                ));
            }
        };
        if !expected_kind {
            return Err(CliError::with_code(
                "content_resource_mismatch",
                format!(
                    "Content commit kind {} does not match resource {}.",
                    commit.resource_kind, commit.resource_key
                ),
            ));
        }
        if row.2.as_deref() == Some(commit.revision_id.as_str())
            && row.3 == commit.content_version
            && row.4 == commit.manifest_hash
            && row.5 == commit.content_hash
        {
            return Ok(row.6);
        }
        if commit.content_version != row.3 + 1
            && !(row.2.is_none() && row.3 == 0 && commit.content_version > 0)
        {
            return Err(CliError::with_code(
                "content_conflict",
                format!(
                    "Content version for {} changed from {} to {}; commit expected {}.",
                    commit.resource_key,
                    row.3,
                    commit.content_version,
                    row.3 + 1
                ),
            ));
        }
        let revision =
            record_resource_scope(connection, project_id, "", &commit.resource_key)?;
        connection
            .execute(
                r#"
                UPDATE resources
                SET active_content_revision_id = ?, content_version = ?,
                    content_manifest_hash = ?, content_hash = ?,
                    revision = ?, updated_at = ?
                WHERE project_id = ? AND id = ? AND deleted_at IS NULL
                "#,
                params![
                    commit.revision_id,
                    commit.content_version,
                    commit.manifest_hash,
                    commit.content_hash,
                    revision,
                    crate::control::now_iso(),
                    project_id,
                    commit.resource_key,
                ],
            )
            .map_err(to_cli_error)?;
        Ok(revision)
    }
}
