fn persist_publications(
    connection: &Connection,
    project_id: &str,
    panel_id: &str,
    state: &Value,
) -> Result<i64, CliError> {
    let publications = state
        .get("publications")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut active = BTreeSet::new();
    let mut revision = 0;
    for (position, publication) in publications.iter().enumerate() {
        let Some(id) = publication.get("id").and_then(Value::as_str) else {
            continue;
        };
        active.insert(id.to_owned());
        let title = publication
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("");
        let selected_title = selected_publication_title_value(publication);
        let source_document_id = publication
            .get("sourceDocumentId")
            .and_then(Value::as_str)
            .filter(|id| resource_exists(connection, project_id, id).unwrap_or(false));
        let cover_document_id = publication
            .get("coverDocumentId")
            .and_then(Value::as_str)
            .filter(|id| resource_exists(connection, project_id, id).unwrap_or(false));
        let config_version = publication
            .get("contentVersion")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let config_json = serde_json::to_string(&strip_fields(
            publication,
            &[
                "id",
                "title",
                "sourceDocumentId",
                "coverDocumentId",
                "contentVersion",
                "createdAt",
                "updatedAt",
            ],
        ))
        .map_err(to_cli_error)?;
        let current = connection
            .query_row(
                "SELECT p.config_json, p.position, r.title, r.deleted_at, r.revision FROM publications p JOIN resources r ON r.id = p.resource_id WHERE p.resource_id = ?",
                [id],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                )),
            )
            .optional()
            .map_err(to_cli_error)?;
        if current.as_ref().is_some_and(|value| {
            value.0 == config_json
                && value.1 == position as i64
                && value.2 == title
                && value.3.is_none()
        }) {
            revision = revision.max(current.expect("checked").4);
            continue;
        }
        let resource_revision = record_resource_scope(connection, project_id, panel_id, id)?;
        upsert_resource(
            connection,
            id,
            project_id,
            panel_id,
            "publication",
            title,
            resource_revision,
            publication,
        )?;
        connection
            .execute(
                r#"
                INSERT INTO publications (
                  project_id, resource_id, source_document_id, cover_document_id,
                  config_version, selected_title, position, config_json
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(resource_id) DO UPDATE SET
                  project_id = excluded.project_id,
                  source_document_id = excluded.source_document_id,
                  cover_document_id = excluded.cover_document_id,
                  config_version = excluded.config_version,
                  selected_title = excluded.selected_title,
                  position = excluded.position,
                  config_json = excluded.config_json
                "#,
                params![
                    project_id,
                    id,
                    source_document_id,
                    cover_document_id,
                    config_version,
                    selected_title,
                    position as i64,
                    config_json,
                ],
            )
            .map_err(to_cli_error)?;
        revision = revision.max(resource_revision);
    }
    revision = revision.max(soft_delete_missing_resources(
        connection,
        project_id,
        panel_id,
        "publication",
        &active,
    )?);
    Ok(revision)
}

fn hydrate_publications(
    connection: &Connection,
    project_id: &str,
    _panel_id: &str,
    state: &mut Value,
) -> Result<bool, CliError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT p.config_json, r.id, r.title, p.source_document_id,
                   p.cover_document_id, p.config_version, r.created_at, r.updated_at
            FROM publications p
            JOIN resources r ON r.id = p.resource_id
            WHERE r.project_id = ? AND r.deleted_at IS NULL
            ORDER BY p.position ASC, r.id ASC
            "#,
        )
        .map_err(to_cli_error)?;
    let publications = statement
        .query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })
        .map_err(to_cli_error)?
        .map(|row| {
            let (raw, id, title, source_id, cover_id, config_version, created_at, updated_at) =
                row.map_err(to_cli_error)?;
            let mut value = serde_json::from_str::<Value>(&raw).map_err(to_cli_error)?;
            value["id"] = json!(id);
            value["title"] = json!(title);
            value["sourceDocumentId"] = source_id.map_or(Value::Null, Value::String);
            value["coverDocumentId"] = cover_id.map_or(Value::Null, Value::String);
            value["contentVersion"] = json!(config_version);
            value["createdAt"] = json!(created_at);
            value["updatedAt"] = json!(updated_at);
            Ok(value)
        })
        .collect::<Result<Vec<Value>, CliError>>()?;
    let has_state = !publications.is_empty();
    state["publications"] = Value::Array(publications);
    Ok(has_state)
}

fn persist_releases(
    connection: &Connection,
    project_id: &str,
    panel_id: &str,
    state: &Value,
) -> Result<i64, CliError> {
    let releases = state
        .get("releases")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut active = BTreeSet::new();
    let mut revision = 0;
    for (position, release) in releases.iter().enumerate() {
        let Some(id) = release.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(publication_id) = release
            .get("sourcePublicationId")
            .and_then(Value::as_str)
        else {
            continue;
        };
        if !resource_exists(connection, project_id, publication_id)? {
            continue;
        }
        active.insert(id.to_owned());
        let platform = release
            .get("platform")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let source_updated_at = release.get("sourceUpdatedAt").and_then(Value::as_str);
        let attempts = release
            .get("attempts")
            .cloned()
            .unwrap_or_else(|| json!([]));
        let mut snapshot = release
            .get("snapshot")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if let Some(snapshot) = snapshot.as_object_mut() {
            snapshot.remove("title");
        }
        let snapshot_json = serde_json::to_string(&snapshot).map_err(to_cli_error)?;
        let result_json =
            serde_json::to_string(&json!({ "attempts": attempts })).map_err(to_cli_error)?;
        let title = release
            .pointer("/snapshot/title")
            .and_then(Value::as_str)
            .unwrap_or("");
        let current = connection
            .query_row(
                r#"
                SELECT l.snapshot_json, l.result_json, l.position, r.deleted_at,
                       r.revision, l.publication_id, l.platform_key,
                       l.source_updated_at, r.title
                FROM releases l
                JOIN resources r ON r.id = l.resource_id
                WHERE l.resource_id = ?
                "#,
                [id],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, String>(8)?,
                )),
            )
            .optional()
            .map_err(to_cli_error)?;
        if current.as_ref().is_some_and(|value| {
            value.0 == snapshot_json
                && value.1.as_deref() == Some(result_json.as_str())
                && value.2 == position as i64
                && value.3.is_none()
                && value.5 == publication_id
                && value.6 == platform
                && value.7.as_deref() == source_updated_at
                && value.8 == title
        }) {
            revision = revision.max(current.expect("checked").4);
            continue;
        }
        let resource_revision = record_resource_scope(connection, project_id, panel_id, id)?;
        upsert_resource(
            connection,
            id,
            project_id,
            panel_id,
            "release",
            title,
            resource_revision,
            release,
        )?;
        connection
            .execute(
                r#"
                INSERT INTO releases (
                  project_id, resource_id, publication_id, platform_key,
                  source_updated_at, position, snapshot_json, result_json
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(resource_id) DO UPDATE SET
                  project_id = excluded.project_id,
                  publication_id = excluded.publication_id,
                  platform_key = excluded.platform_key,
                  source_updated_at = excluded.source_updated_at,
                  position = excluded.position,
                  snapshot_json = excluded.snapshot_json,
                  result_json = excluded.result_json
                "#,
                params![
                    project_id,
                    id,
                    publication_id,
                    platform,
                    source_updated_at,
                    position as i64,
                    snapshot_json,
                    result_json,
                ],
            )
            .map_err(to_cli_error)?;
        revision = revision.max(resource_revision);
    }
    revision = revision.max(soft_delete_missing_resources(
        connection,
        project_id,
        panel_id,
        "release",
        &active,
    )?);
    Ok(revision)
}

fn hydrate_releases(
    connection: &Connection,
    project_id: &str,
    _panel_id: &str,
    state: &mut Value,
) -> Result<bool, CliError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT l.snapshot_json, l.result_json, r.id, l.platform_key,
                   l.publication_id, l.source_updated_at, r.title,
                   r.created_at, r.updated_at
            FROM releases l
            JOIN resources r ON r.id = l.resource_id
            WHERE r.project_id = ? AND r.deleted_at IS NULL
            ORDER BY l.position ASC, r.id ASC
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        })
        .map_err(to_cli_error)?;
    let releases = rows
        .map(|row| {
            let (
                snapshot,
                result,
                id,
                platform,
                publication_id,
                source_updated_at,
                title,
                created_at,
                updated_at,
            ) = row.map_err(to_cli_error)?;
            let mut snapshot =
                serde_json::from_str::<Value>(&snapshot).map_err(to_cli_error)?;
            snapshot["title"] = json!(title);
            let attempts = result
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                .and_then(|result| result.get("attempts").cloned())
                .unwrap_or_else(|| json!([]));
            Ok(json!({
                "id": id,
                "platform": platform,
                "sourcePublicationId": publication_id,
                "sourceUpdatedAt": source_updated_at,
                "snapshot": snapshot,
                "attempts": attempts,
                "createdAt": created_at,
                "updatedAt": updated_at,
            }))
        })
        .collect::<Result<Vec<Value>, CliError>>()?;
    let has_state = !releases.is_empty();
    state["releases"] = Value::Array(releases);
    Ok(has_state)
}

fn upsert_resource(
    connection: &Connection,
    id: &str,
    project_id: &str,
    _panel_id: &str,
    kind: &str,
    title: &str,
    revision: i64,
    source: &Value,
) -> Result<(), CliError> {
    let existing_resource = connection
        .query_row(
            "SELECT project_id, kind FROM resources WHERE id = ?",
            [id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(to_cli_error)?;
    if existing_resource
        .as_ref()
        .is_some_and(|(existing_project, existing_kind)| {
            existing_project != project_id || existing_kind != kind
        })
    {
        return Err(CliError::with_code(
            "resource_identity_conflict",
            format!("Resource {id} already belongs to another Project or module."),
        ));
    }
    let now = crate::control::now_iso();
    let created_at = source
        .get("createdAt")
        .and_then(Value::as_str)
        .unwrap_or(&now);
    let updated_at = source
        .get("updatedAt")
        .and_then(Value::as_str)
        .unwrap_or(&now);
    connection
        .execute(
            r#"
            INSERT INTO resources (
              id, project_id, kind, title, revision,
              created_at, updated_at, deleted_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, NULL)
            ON CONFLICT(id) DO UPDATE SET
              title = excluded.title,
              revision = excluded.revision,
              updated_at = excluded.updated_at,
              deleted_at = NULL
            "#,
            params![
                id, project_id, kind, title, revision, created_at, updated_at,
            ],
        )
        .map_err(to_cli_error)?;
    Ok(())
}

fn soft_delete_missing_resources(
    connection: &Connection,
    project_id: &str,
    panel_id: &str,
    kind: &str,
    active: &BTreeSet<String>,
) -> Result<i64, CliError> {
    let mut statement = connection
        .prepare(
            "SELECT id FROM resources WHERE project_id = ? AND kind = ? AND deleted_at IS NULL",
        )
        .map_err(to_cli_error)?;
    let existing = statement
        .query_map(params![project_id, kind], |row| {
            row.get::<_, String>(0)
        })
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    drop(statement);
    let now = crate::control::now_iso();
    let mut last_revision = 0;
    let mut cancelled_tasks = false;
    for id in existing
        .into_iter()
        .filter(|id| !active.contains(id.as_str()))
    {
        let revision = record_resource_scope(connection, project_id, panel_id, &id)?;
        connection
            .execute(
                "UPDATE resources SET deleted_at = ?, updated_at = ?, revision = ? WHERE id = ?",
                params![now, now, revision, id],
            )
            .map_err(to_cli_error)?;
        cancelled_tasks |= cancel_tasks_for_resource_in_transaction(
            connection,
            project_id,
            &id,
            "prerequisite_deleted",
            &now,
        )?;
        last_revision = last_revision.max(revision);
    }
    if cancelled_tasks {
        last_revision =
            last_revision.max(record_scope(connection, "tasks", Some(project_id), None)?);
    }
    Ok(last_revision)
}

fn cancel_tasks_for_resource_in_transaction(
    connection: &Connection,
    project_id: &str,
    resource_id: &str,
    reason_code: &str,
    now: &str,
) -> Result<bool, CliError> {
    let reason = json!({
        "code": reason_code,
        "resourceId": resource_id,
    })
    .to_string();
    let task_ids = {
        let mut statement = connection
            .prepare(
                r#"
                SELECT DISTINCT task.id
                FROM tasks task
                JOIN task_resources link
                  ON link.project_id = task.project_id
                 AND link.task_id = task.id
                WHERE task.project_id = ? AND link.resource_id = ?
                  AND task.status IN ('queued', 'running')
                ORDER BY task.created_at, task.id
                "#,
            )
            .map_err(to_cli_error)?;
        let collected = statement
            .query_map(params![project_id, resource_id], |row| {
                row.get::<_, String>(0)
            })
            .map_err(to_cli_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_cli_error)?;
        collected
    };
    for task_id in &task_ids {
        connection
            .execute(
                r#"
                UPDATE tasks SET status = 'cancelled', error_json = ?,
                  execution_generation = execution_generation + 1,
                  execution_token_hash = NULL, lease_owner = NULL,
                  lease_expires_at = NULL, heartbeat_at = NULL,
                  current_runner_key = NULL, completed_at = ?, updated_at = ?
                WHERE project_id = ? AND id = ?
                  AND status IN ('queued', 'running')
                "#,
                params![reason, now, now, project_id, task_id],
            )
            .map_err(to_cli_error)?;
        crate::content::abandon_task_staging_in_transaction(connection, task_id, now)?;
    }
    for task_id in &task_ids {
        crate::tasks::terminate_task_descendants_in_transaction(
            connection,
            project_id,
            task_id,
            "cancelled",
            now,
        )?;
    }
    Ok(!task_ids.is_empty())
}

fn sync_task_resources_for_project(
    connection: &Connection,
    project_id: &str,
) -> Result<(), CliError> {
    let resources = {
        let mut statement = connection
            .prepare("SELECT id FROM resources WHERE project_id = ?")
            .map_err(to_cli_error)?;
        let collected = statement
            .query_map([project_id], |row| row.get::<_, String>(0))
            .map_err(to_cli_error)?
            .collect::<Result<BTreeSet<_>, _>>()
            .map_err(to_cli_error)?;
        collected
    };
    let mut statement = connection
        .prepare(
            "SELECT id, target_ref, input_json, source_json, created_at FROM tasks WHERE project_id = ?",
        )
        .map_err(to_cli_error)?;
    let tasks = statement
        .query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    drop(statement);
    for (task_id, target_ref, input, source, created_at) in tasks {
        let mut links = BTreeMap::<String, String>::new();
        if resources.contains(&target_ref) {
            links.insert(target_ref, "primary".to_owned());
        }
        let input = serde_json::from_str::<Value>(&input).unwrap_or_else(|_| json!({}));
        let source = serde_json::from_str::<Value>(&source).unwrap_or_else(|_| json!({}));
        collect_named_resource_links(&input, &source, &resources, &mut links);
        for (resource_id, role) in links {
            let captured_version = resource_content_version(connection, &resource_id)?;
            connection
                .execute(
                    "INSERT OR IGNORE INTO task_resources (project_id, task_id, resource_id, role, captured_version, created_at) VALUES (?, ?, ?, ?, ?, ?)",
                    params![
                        project_id,
                        task_id,
                        resource_id,
                        role,
                        captured_version,
                        created_at
                    ],
                )
                .map_err(to_cli_error)?;
        }
    }
    Ok(())
}

fn collect_named_resource_links(
    input: &Value,
    source: &Value,
    resources: &BTreeSet<String>,
    output: &mut BTreeMap<String, String>,
) {
    for key in [
        "documentId",
        "targetMyDocumentId",
        "publicationId",
        "releaseId",
    ] {
        collect_resource_values(input.get(key), "primary", resources, output);
    }
    for key in ["sourceDocumentId", "sourcePublicationId"] {
        collect_resource_values(input.get(key), "input", resources, output);
        collect_resource_values(source.get(key), "input", resources, output);
    }
    collect_resource_values(
        source.get("wikiSpaceId"),
        "context",
        resources,
        output,
    );
    if let Some(context) = input.get("contextSnapshot") {
        for key in [
            "selectedRawDocumentIds",
            "selectedMyDocumentIds",
            "documentIds",
        ] {
            collect_resource_values(context.get(key), "context", resources, output);
        }
    }
}

fn collect_resource_values(
    value: Option<&Value>,
    role: &str,
    resources: &BTreeSet<String>,
    output: &mut BTreeMap<String, String>,
) {
    let Some(value) = value else {
        return;
    };
    match value {
        Value::String(id) if resources.contains(id) => {
            output
                .entry(id.clone())
                .and_modify(|current| {
                    if role == "primary" {
                        *current = role.to_owned();
                    }
                })
                .or_insert_with(|| role.to_owned());
        }
        Value::Array(items) => {
            for item in items {
                collect_resource_values(Some(item), role, resources, output);
            }
        }
        _ => {}
    }
}

fn resource_content_version(
    connection: &Connection,
    resource_id: &str,
) -> Result<Option<i64>, CliError> {
    connection
        .query_row(
            r#"
            SELECT COALESCE(
              (SELECT content_version FROM resources WHERE id = ?
                AND active_content_revision_id IS NOT NULL),
              (SELECT state_revision FROM canvas_documents WHERE resource_id = ?),
              0
            )
            "#,
            params![resource_id, resource_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .map_err(to_cli_error)
}

fn resource_exists(
    connection: &Connection,
    project_id: &str,
    resource_id: &str,
) -> Result<bool, CliError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM resources WHERE project_id = ? AND id = ? AND deleted_at IS NULL)",
            params![project_id, resource_id],
            |row| row.get(0),
        )
        .map_err(to_cli_error)
}

fn strip_fields(value: &Value, fields: &[&str]) -> Value {
    let mut object = value.as_object().cloned().unwrap_or_default();
    for field in fields {
        object.remove(*field);
    }
    Value::Object(object)
}

fn selected_publication_title_value(publication: &Value) -> String {
    let fallback = publication
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("");
    let selected_id = publication.get("selectedTitleId").and_then(Value::as_str);
    publication
        .get("titles")
        .and_then(Value::as_array)
        .and_then(|titles| {
            titles
                .iter()
                .find(|title| {
                    selected_id.is_some_and(|id| {
                        title.get("id").and_then(Value::as_str) == Some(id)
                    })
                })
                .or_else(|| titles.first())
        })
        .and_then(|title| title.get("value"))
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_owned()
}

impl Storage {
    pub fn publication_revision(&self, project_id: &str) -> Result<i64, CliError> {
        self.connection
            .query_row(
                "SELECT COALESCE(MAX(revision), 0) FROM resources WHERE project_id = ? AND kind = 'publication'",
                [project_id],
                |row| row.get(0),
            )
            .map_err(to_cli_error)
    }

    pub fn list_publications(&self, project_id: &str) -> Result<Vec<Value>, CliError> {
        let mut state = json!({});
        hydrate_publications(&self.connection, project_id, "", &mut state)?;
        Ok(state["publications"]
            .as_array()
            .cloned()
            .unwrap_or_default())
    }

    pub fn list_releases(&self, project_id: &str) -> Result<Vec<Value>, CliError> {
        let mut state = json!({});
        hydrate_releases(&self.connection, project_id, "", &mut state)?;
        Ok(state["releases"].as_array().cloned().unwrap_or_default())
    }

    pub fn write_publications_if_current(
        &self,
        project_id: &str,
        publications: &[Value],
        expected_revision: i64,
    ) -> Result<Result<i64, PanelStateWriteConflict>, CliError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(to_cli_error)?;
        let current_revision = tx
            .query_row(
                "SELECT COALESCE(MAX(revision), 0) FROM resources WHERE project_id = ? AND kind = 'publication'",
                [project_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(to_cli_error)?;
        if current_revision != expected_revision {
            return Ok(Err(PanelStateWriteConflict {
                base_revision: expected_revision,
                current_revision,
            }));
        }
        let state = json!({ "publications": publications });
        let revision = persist_publications(&tx, project_id, "", &state)?;
        sync_task_resources_for_project(&tx, project_id)?;
        tx.commit().map_err(to_cli_error)?;
        Ok(Ok(revision.max(current_revision)))
    }
}

fn record_resource_scope(
    connection: &Connection,
    project_id: &str,
    _panel_id: &str,
    resource_id: &str,
) -> Result<i64, CliError> {
    let revision = connection
        .query_row(
            "UPDATE storage_meta SET global_revision = global_revision + 1 WHERE id = 1 RETURNING global_revision",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(to_cli_error)?;
    upsert_scope_with_resource(
        connection,
        revision,
        "resource",
        Some(project_id),
        None,
        Some(resource_id),
    )?;
    Ok(revision)
}
