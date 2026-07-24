impl Storage {
    pub fn read_setting(&self, namespace: &str, key: &str) -> Result<Option<String>, CliError> {
        let setting_key = format!("{namespace}.{key}");
        self.connection
            .query_row(
                "SELECT value_json FROM settings WHERE key = ?",
                params![setting_key],
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
        let setting_key = format!("{namespace}.{key}");
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(to_cli_error)?;
        tx.execute(
            r#"
            INSERT INTO settings (key, value_json, updated_at)
            VALUES (?, ?, ?)
            ON CONFLICT(key) DO UPDATE SET
              value_json = excluded.value_json,
              updated_at = excluded.updated_at
            "#,
            params![setting_key, value_json, crate::control::now_iso()],
        )
        .map_err(to_cli_error)?;
        record_scope(&tx, "settings", None, None)?;
        tx.commit().map_err(to_cli_error)
    }

    pub fn write_artifact(&self, project_id: &str, artifact: &Value) -> Result<(), CliError> {
        let id = artifact
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| CliError::new("Artifact id is required."))?;
        let artifacts_dir = self.project_dir(project_id).join("artifacts");
        fs::create_dir_all(&artifacts_dir).map_err(to_cli_error)?;
        let destination = artifacts_dir.join(format!("{}.json", sanitize_path_part(id)));
        let temporary = artifacts_dir.join(format!(".{}.tmp", crate::ids::random_id("artifact")));
        fs::write(
            &temporary,
            serde_json::to_vec_pretty(artifact).map_err(to_cli_error)?,
        )
        .map_err(to_cli_error)?;
        fs::rename(&temporary, &destination).map_err(to_cli_error)?;
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        record_scope(&tx, "project", Some(project_id), None)?;
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
        let prepared =
            self.prepare_asset_from_buffer(project_id, panel_id, requested_name, bytes, overwrite)?;
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(to_cli_error)?;
        Self::write_prepared_asset_in_transaction(&tx, project_id, panel_id, &prepared)?;
        tx.commit().map_err(to_cli_error)?;
        Ok(prepared.written_asset())
    }

    pub(crate) fn prepare_asset_from_buffer(
        &self,
        project_id: &str,
        panel_id: &str,
        requested_name: &str,
        bytes: &[u8],
        overwrite: bool,
    ) -> Result<PreparedAssetWrite, CliError> {
        let file_name = sanitize_asset_path(requested_name);
        let asset_id = if overwrite {
            let stable_key =
                format!("projects/{project_id}/content/asset/{panel_id}/{file_name}");
            format!("asset:{}", &hash_text(&stable_key)[..32])
        } else {
            crate::ids::random_id("asset")
        };
        let current_version = self
            .connection
            .query_row(
                "SELECT content_version FROM assets WHERE resource_id = ?",
                [&asset_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(to_cli_error)?
            .unwrap_or(0);
        let content_version = current_version + 1;
        let canonical_ref = format!(
            "projects/{}/content/asset/{}/{}/{}",
            sanitize_path_part(project_id),
            sanitize_path_part(&asset_id),
            content_version,
            file_name
                .split('/')
                .map(sanitize_path_part)
                .collect::<Vec<_>>()
                .join("/")
        );
        let canonical_path = self.asset_path(&canonical_ref)?;
        if let Some(parent) = canonical_path.parent() {
            fs::create_dir_all(parent).map_err(to_cli_error)?;
        }
        let temporary = canonical_path.with_extension("asset.tmp");
        fs::write(&temporary, bytes).map_err(to_cli_error)?;
        fs::rename(&temporary, &canonical_path).map_err(to_cli_error)?;
        Ok(PreparedAssetWrite {
            resource_id: asset_id,
            asset_ref: canonical_ref,
            file_name,
            file_path: canonical_path,
            content_version,
            content_hash: format!("{:x}", Sha256::digest(bytes)),
            size_bytes: bytes.len() as i64,
        })
    }

    pub(crate) fn write_prepared_asset_in_transaction(
        tx: &Transaction<'_>,
        project_id: &str,
        panel_id: &str,
        prepared: &PreparedAssetWrite,
    ) -> Result<(), CliError> {
        let asset_id = &prepared.resource_id;
        let file_name = &prepared.file_name;
        let revision = record_resource_scope(&tx, project_id, panel_id, &asset_id)?;
        let metadata = json!({ "originPanelId": panel_id });
        upsert_resource(
            &tx,
            &asset_id,
            project_id,
            panel_id,
            "asset",
            &file_name,
            revision,
            &metadata,
        )?;
        tx.execute(
            r#"
            INSERT INTO assets (
              resource_id, media_type, file_name, active_revision_id, content_version,
              content_hash, byte_size, metadata_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(resource_id) DO UPDATE SET
              media_type = excluded.media_type,
              file_name = excluded.file_name,
              active_revision_id = excluded.active_revision_id,
              content_version = excluded.content_version,
              content_hash = excluded.content_hash,
              byte_size = excluded.byte_size,
              metadata_json = excluded.metadata_json
            "#,
            params![
                asset_id,
                asset_media_type(&file_name),
                file_name,
                prepared.asset_ref,
                prepared.content_version,
                prepared.content_hash,
                prepared.size_bytes,
                serde_json::to_string(&metadata).map_err(to_cli_error)?,
            ],
        )
        .map_err(to_cli_error)?;
        Ok(())
    }

    pub fn read_asset(&self, asset_ref: &str) -> Result<Vec<u8>, CliError> {
        let path = self.asset_path(asset_ref)?;
        fs::read(path).map_err(to_cli_error)
    }

    pub fn list_assets(&self, project_id: &str) -> Result<Vec<Value>, CliError> {
        let mut statement = self
            .connection
            .prepare(
                r#"
                SELECT r.id, r.title, r.revision, r.created_at, r.updated_at,
                       a.media_type, a.file_name, a.active_revision_id, a.content_version,
                       a.content_hash, a.byte_size, a.width, a.height, a.metadata_json
                FROM assets a JOIN resources r ON r.id = a.resource_id
                WHERE r.project_id = ? AND r.deleted_at IS NULL
                ORDER BY r.updated_at DESC, r.id ASC
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map([project_id], |row| {
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "title": row.get::<_, String>(1)?,
                    "revision": row.get::<_, i64>(2)?,
                    "createdAt": row.get::<_, String>(3)?,
                    "updatedAt": row.get::<_, String>(4)?,
                    "mimeType": row.get::<_, String>(5)?,
                    "fileName": row.get::<_, String>(6)?,
                    "contentRef": row.get::<_, Option<String>>(7)?,
                    "contentVersion": row.get::<_, i64>(8)?,
                    "contentHash": row.get::<_, String>(9)?,
                    "byteSize": row.get::<_, Option<i64>>(10)?,
                    "width": row.get::<_, Option<i64>>(11)?,
                    "height": row.get::<_, Option<i64>>(12)?,
                    "metadata": serde_json::from_str::<Value>(
                        &row.get::<_, String>(13)?
                    ).unwrap_or_else(|_| json!({})),
                }))
            })
            .map_err(to_cli_error)?;
        rows.map(|row| row.map_err(to_cli_error)).collect()
    }

    pub fn read_asset_by_id(&self, project_id: &str, asset_id: &str) -> Result<Vec<u8>, CliError> {
        let content_ref = self
            .connection
            .query_row(
                r#"
                SELECT a.active_revision_id FROM assets a
                JOIN resources r ON r.id = a.resource_id
                WHERE r.project_id = ? AND r.id = ? AND r.deleted_at IS NULL
                "#,
                params![project_id, asset_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(to_cli_error)?
            .flatten()
            .ok_or_else(|| CliError::with_code("not_found", "Asset not found."))?;
        self.read_asset(&content_ref)
    }

    pub fn asset_path(&self, asset_ref: &str) -> Result<PathBuf, CliError> {
        let parts = asset_ref.split('/').collect::<Vec<_>>();
        if parts.len() < 7
            || parts[0] != "projects"
            || parts[1].is_empty()
            || parts[2] != "content"
            || parts[3] != "asset"
            || parts[4].is_empty()
            || parts[5].parse::<u64>().ok().filter(|version| *version > 0).is_none()
            || parts[6..].iter().any(|part| part.is_empty())
        {
            return Err(CliError::with_code(
                "invalid_asset_ref",
                "Asset reference must use projects/<project>/content/asset/<asset>/<version>/<file>.",
            ));
        }
        let mut path = self.root_dir.clone();
        for part in parts {
            path.push(sanitize_path_part(part));
        }
        if !path.starts_with(&self.root_dir) {
            return Err(CliError::new(
                "Resolved asset path escapes storage directory.",
            ));
        }
        Ok(path)
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
            SELECT kind, project_id, panel_id, resource_id, revision
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
                    resource_id: row.get(3)?,
                    revision: row.get(4)?,
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
        panel_state_revision(&self.connection, project_id, panel_id)
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
            .prepare("SELECT id FROM panels WHERE project_id = ? ORDER BY position ASC, id ASC")
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![project_id], |row| row.get::<_, String>(0))
            .map_err(to_cli_error)?;
        rows.map(|row| row.map_err(to_cli_error)).collect()
    }
}

fn asset_media_type(file_name: &str) -> &'static str {
    match std::path::Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
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
    let raw_kind = row.get::<_, String>(2)?;
    let project_id = row.get::<_, String>(0)?;
    let panel_id = row.get::<_, String>(1)?;
    let kind = PanelKind::parse(&raw_kind).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            format!("unknown panel kind: {raw_kind}").into(),
        )
    })?;
    Ok(Panel {
        id: panel_id.clone(),
        project_id: project_id.clone(),
        kind,
        title: kind.default_title().to_owned(),
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
        state_ref: Some(format!("sqlite:panel-states/{project_id}/{panel_id}")),
    })
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
    upsert_scope_with_resource(
        connection,
        revision,
        kind,
        project_id,
        panel_id,
        None,
    )
}

fn upsert_scope_with_resource(
    connection: &Connection,
    revision: i64,
    kind: &str,
    project_id: Option<&str>,
    panel_id: Option<&str>,
    resource_id: Option<&str>,
) -> Result<(), CliError> {
    let scope_key = match (kind, project_id, panel_id, resource_id) {
        ("catalog", _, _, _) => "catalog".to_owned(),
        (_, Some(project_id), Some(panel_id), Some(resource_id)) => {
            format!("{kind}:{project_id}:{panel_id}:{resource_id}")
        }
        (_, Some(project_id), Some(panel_id), None) => {
            format!("{kind}:{project_id}:{panel_id}")
        }
        (_, Some(project_id), None, Some(resource_id)) => {
            format!("{kind}:{project_id}:{resource_id}")
        }
        (_, Some(project_id), None, None) => format!("{kind}:{project_id}"),
        _ => kind.to_owned(),
    };
    connection
        .execute(
            r#"
            INSERT INTO change_scopes (
              scope_key, kind, project_id, panel_id, resource_id, revision, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(scope_key) DO UPDATE SET
              kind = excluded.kind,
              project_id = excluded.project_id,
              panel_id = excluded.panel_id,
              resource_id = excluded.resource_id,
              revision = excluded.revision,
              updated_at = excluded.updated_at
            "#,
            params![
                scope_key,
                kind,
                project_id,
                panel_id,
                resource_id,
                revision,
                crate::control::now_iso()
            ],
        )
        .map_err(to_cli_error)?;
    Ok(())
}
