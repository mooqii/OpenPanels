impl Storage {
    pub fn read_setting(&self, namespace: &str, key: &str) -> Result<Option<String>, CliError> {
        self.connection
            .query_row(
                "SELECT value_json FROM settings WHERE namespace = ? AND key = ?",
                params![namespace, key],
                |row| row.get(0),
            )
            .optional()
            .map_err(to_cli_error)
    }

    pub fn write_setting(
        &self,
        namespace: &str,
        key: &str,
        value_json: &str,
    ) -> Result<(), CliError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(to_cli_error)?;
        tx.execute(
            r#"
            INSERT INTO settings (namespace, key, value_json, updated_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(namespace, key) DO UPDATE SET
              value_json = excluded.value_json,
              updated_at = excluded.updated_at
            "#,
            params![namespace, key, value_json, crate::control::now_iso()],
        )
        .map_err(to_cli_error)?;
        record_scope(&tx, "catalog", None, None)?;
        tx.commit().map_err(to_cli_error)
    }

    pub fn write_artifact(&self, project_id: &str, artifact: &Value) -> Result<(), CliError> {
        let id = artifact
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| CliError::new("Artifact id is required."))?;
        let now = artifact
            .get("updatedAt")
            .and_then(Value::as_str)
            .or_else(|| artifact.get("createdAt").and_then(Value::as_str))
            .map(str::to_owned)
            .unwrap_or_else(crate::control::now_iso);
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        tx.execute(
            r#"
                INSERT INTO artifacts (
                  id, project_id, panel_id, kind, title, payload_json, created_at, updated_at
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                  panel_id = excluded.panel_id,
                  kind = excluded.kind,
                  title = excluded.title,
                  payload_json = excluded.payload_json,
                  updated_at = excluded.updated_at
                "#,
            params![
                id,
                project_id,
                artifact.get("panelId").and_then(Value::as_str),
                artifact
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("file"),
                artifact.get("title").and_then(Value::as_str),
                serde_json::to_string(artifact).map_err(to_cli_error)?,
                artifact
                    .get("createdAt")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(crate::control::now_iso),
                now,
            ],
        )
        .map_err(to_cli_error)?;
        record_scope(&tx, "artifacts", Some(project_id), None)?;
        tx.commit().map_err(to_cli_error)
    }

    pub fn write_panel_selection(
        &self,
        project_id: &str,
        panel_id: &str,
        selection: &Value,
    ) -> Result<(), CliError> {
        let selection_json = serde_json::to_string(selection).map_err(to_cli_error)?;
        let content_hash = hash_text(&selection_json);
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        let current = tx.query_row(
            "SELECT revision, content_hash FROM panel_selections WHERE project_id = ? AND panel_id = ?",
            params![project_id, panel_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        ).optional().map_err(to_cli_error)?;
        if current
            .as_ref()
            .is_some_and(|value| value.1 == content_hash)
        {
            tx.commit().map_err(to_cli_error)?;
            return Ok(());
        }
        let revision = record_scope(&tx, "panel_selection", Some(project_id), Some(panel_id))?;
        tx.execute(
            r#"
                INSERT INTO panel_selections (
                  project_id, panel_id, revision, content_hash, selection_json, updated_at
                )
                VALUES (?, ?, ?, ?, ?, ?)
                ON CONFLICT(project_id, panel_id) DO UPDATE SET
                  revision = excluded.revision,
                  content_hash = excluded.content_hash,
                  selection_json = excluded.selection_json,
                  updated_at = excluded.updated_at
                "#,
            params![
                project_id,
                panel_id,
                revision,
                content_hash,
                selection_json,
                selection
                    .get("updatedAt")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(crate::control::now_iso),
            ],
        )
        .map_err(to_cli_error)?;
        tx.commit().map_err(to_cli_error)
    }

    pub fn read_panel_selection(
        &self,
        project_id: &str,
        panel_id: &str,
    ) -> Result<Option<Value>, CliError> {
        let selection_json = self
            .connection
            .query_row(
                "SELECT selection_json FROM panel_selections WHERE project_id = ? AND panel_id = ?",
                params![project_id, panel_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(to_cli_error)?;
        selection_json
            .map(|raw| serde_json::from_str::<Value>(&raw).map_err(to_cli_error))
            .transpose()
    }

    pub fn write_asset_from_buffer(
        &self,
        project_id: &str,
        panel_id: &str,
        requested_name: &str,
        bytes: &[u8],
        overwrite: bool,
    ) -> Result<WrittenAsset, CliError> {
        let assets_dir = self.panel_dir(project_id, panel_id).join("assets");
        fs::create_dir_all(&assets_dir).map_err(to_cli_error)?;
        let file_name = if overwrite {
            sanitize_asset_path(requested_name)
        } else {
            unique_file_name(&assets_dir, requested_name)
        };
        let file_path = assets_dir.join(&file_name);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(to_cli_error)?;
        }
        if !file_path.starts_with(&self.root_dir) {
            return Err(CliError::new(
                "Resolved asset path escapes storage directory.",
            ));
        }
        fs::write(&file_path, bytes).map_err(to_cli_error)?;
        let asset_ref = format!(
            "projects/{}/panels/{}/assets/{}",
            project_id,
            panel_id,
            file_name
                .split('/')
                .map(sanitize_path_part)
                .collect::<Vec<_>>()
                .join("/")
        );
        Ok(WrittenAsset {
            asset_ref,
            file_name,
            file_path,
        })
    }

    pub fn read_asset(&self, asset_ref: &str) -> Result<Vec<u8>, CliError> {
        let path = self.asset_path(asset_ref)?;
        fs::read(path).map_err(to_cli_error)
    }

    pub fn asset_path(&self, asset_ref: &str) -> Result<PathBuf, CliError> {
        let mut path = self.root_dir.clone();
        for part in asset_ref.split('/') {
            path.push(sanitize_path_part(part));
        }
        if !path.starts_with(&self.root_dir) {
            return Err(CliError::new(
                "Resolved asset path escapes storage directory.",
            ));
        }
        Ok(path)
    }

    pub fn panel_dir(&self, project_id: &str, panel_id: &str) -> PathBuf {
        self.project_dir(project_id)
            .join("panels")
            .join(sanitize_path_part(panel_id))
    }

    pub fn project_dir(&self, project_id: &str) -> PathBuf {
        self.root_dir
            .join("projects")
            .join(sanitize_path_part(project_id))
    }

    pub fn read_change_seq(&self) -> Result<i64, CliError> {
        self.connection
            .query_row(
                "SELECT global_revision FROM storage_meta WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .map_err(to_cli_error)
    }

    pub fn read_changes_after(
        &self,
        revision: i64,
        project_id: Option<&str>,
    ) -> Result<(i64, Vec<ChangeScope>), CliError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(to_cli_error)?;
        let global_revision = tx
            .query_row(
                "SELECT global_revision FROM storage_meta WHERE id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(to_cli_error)?;
        let mut statement = tx
            .prepare(
                r#"
            SELECT kind, project_id, panel_id, revision
            FROM change_scopes
            WHERE revision > ? AND revision <= ?
              AND (project_id IS NULL OR project_id = ?)
            ORDER BY revision ASC, scope_key ASC
            "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![revision, global_revision, project_id], |row| {
                Ok(ChangeScope {
                    kind: row.get(0)?,
                    project_id: row.get(1)?,
                    panel_id: row.get(2)?,
                    revision: row.get(3)?,
                })
            })
            .map_err(to_cli_error)?;
        let changes = rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?;
        drop(statement);
        tx.commit().map_err(to_cli_error)?;
        Ok((global_revision, changes))
    }

    pub fn read_panel_state_revision(
        &self,
        project_id: &str,
        panel_id: &str,
    ) -> Result<i64, CliError> {
        self.connection
            .query_row(
                "SELECT COALESCE((SELECT revision FROM panel_states WHERE project_id = ? AND panel_id = ?), 0)",
                params![project_id, panel_id],
                |row| row.get(0),
            )
            .map_err(to_cli_error)
    }

    pub fn read_panel_selection_revision(
        &self,
        project_id: &str,
        panel_id: &str,
    ) -> Result<i64, CliError> {
        self.connection
            .query_row(
                "SELECT COALESCE((SELECT revision FROM panel_selections WHERE project_id = ? AND panel_id = ?), 0)",
                params![project_id, panel_id],
                |row| row.get(0),
            )
            .map_err(to_cli_error)
    }

    fn panel_ids(&self, project_id: &str) -> Result<Vec<String>, CliError> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM panels WHERE project_id = ? ORDER BY CASE kind WHEN 'wiki' THEN 0 WHEN 'writing' THEN 1 WHEN 'canvas' THEN 2 WHEN 'typesetting' THEN 3 WHEN 'publishing' THEN 4 ELSE 5 END, id ASC")
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![project_id], |row| row.get::<_, String>(0))
            .map_err(to_cli_error)?;
        rows.map(|row| row.map_err(to_cli_error)).collect()
    }
}

fn project_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        title: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        panel_ids: Vec::new(),
    })
}

fn panel_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Panel> {
    let kind = row.get::<_, String>(2)?;
    let project_id = row.get::<_, String>(0)?;
    let panel_id = row.get::<_, String>(1)?;
    Ok(Panel {
        id: panel_id.clone(),
        project_id: project_id.clone(),
        kind: PanelKind::parse(&kind).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                format!("unknown panel kind: {kind}").into(),
            )
        })?,
        title: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        state_ref: Some(format!("sqlite:panel-states/{project_id}/{panel_id}")),
    })
}

fn operation_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT o.id, o.owner_context_id, o.intent, o.status, o.project_id,
               o.panel_id, p.kind, o.guide_id, o.protocol_version,
               o.target_json, o.input_json, o.result_json, o.error_json,
               o.created_at, o.updated_at, o.completed_at
        FROM agent_operations o
        JOIN panels p ON p.project_id = o.project_id AND p.id = o.panel_id
        {where_clause}
        "#
    )
}

fn operation_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let parse = |index| -> rusqlite::Result<Value> {
        let raw = row.get::<_, String>(index)?;
        serde_json::from_str(&raw).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
    };
    let parse_optional = |index| -> rusqlite::Result<Value> {
        row.get::<_, Option<String>>(index)?
            .map(|raw| {
                serde_json::from_str(&raw).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        index,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
            })
            .transpose()
            .map(|value| value.unwrap_or(Value::Null))
    };
    Ok(json!({
        "id": row.get::<_, String>(0)?,
        "ownerContextId": row.get::<_, String>(1)?,
        "intent": row.get::<_, String>(2)?,
        "status": row.get::<_, String>(3)?,
        "projectId": row.get::<_, String>(4)?,
        "panelId": row.get::<_, String>(5)?,
        "panelKind": row.get::<_, String>(6)?,
        "guideId": row.get::<_, Option<String>>(7)?,
        "protocolVersion": row.get::<_, i64>(8)?,
        "target": parse(9)?,
        "input": parse(10)?,
        "result": parse_optional(11)?,
        "error": parse_optional(12)?,
        "createdAt": row.get::<_, String>(13)?,
        "updatedAt": row.get::<_, String>(14)?,
        "completedAt": row.get::<_, Option<String>>(15)?,
    }))
}

fn hash_text(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

pub(crate) fn record_scope(
    connection: &Connection,
    kind: &str,
    project_id: Option<&str>,
    panel_id: Option<&str>,
) -> Result<i64, CliError> {
    let revision = connection
        .query_row(
            "UPDATE storage_meta SET global_revision = global_revision + 1 WHERE id = 1 RETURNING global_revision",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(to_cli_error)?;
    upsert_scope(connection, revision, kind, project_id, panel_id)?;
    Ok(revision)
}

fn upsert_scope(
    connection: &Connection,
    revision: i64,
    kind: &str,
    project_id: Option<&str>,
    panel_id: Option<&str>,
) -> Result<(), CliError> {
    let scope_key = match (kind, project_id, panel_id) {
        ("catalog", _, _) => "catalog".to_owned(),
        (_, Some(project_id), Some(panel_id)) => format!("{kind}:{project_id}:{panel_id}"),
        (_, Some(project_id), None) => format!("{kind}:{project_id}"),
        _ => kind.to_owned(),
    };
    connection
        .execute(
            r#"
            INSERT INTO change_scopes (scope_key, kind, project_id, panel_id, revision, updated_at)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(scope_key) DO UPDATE SET
              kind = excluded.kind,
              project_id = excluded.project_id,
              panel_id = excluded.panel_id,
              revision = excluded.revision,
              updated_at = excluded.updated_at
            "#,
            params![
                scope_key,
                kind,
                project_id,
                panel_id,
                revision,
                crate::control::now_iso()
            ],
        )
        .map_err(to_cli_error)?;
    Ok(())
}
