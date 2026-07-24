use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
struct StoredTaskProjection {
    id: String,
    task_type: String,
    status: String,
    depends_on_task_id: Option<String>,
    input: Value,
    source: Value,
    error: Value,
    updated_at: String,
}

pub(crate) fn read_composed_panel_state(
    connection: &Connection,
    project_id: &str,
    panel_id: &str,
) -> Result<Option<Value>, CliError> {
    let panel = connection
        .query_row(
            "SELECT kind, ui_state_json, ui_state_revision FROM panels WHERE project_id = ? AND id = ?",
            params![project_id, panel_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(to_cli_error)?;
    let Some((kind, ui_json, ui_revision)) = panel else {
        return Ok(None);
    };
    let mut state = serde_json::from_str::<Value>(&ui_json).map_err(to_cli_error)?;
    if !state.is_object() {
        state = json!({});
    }
    let has_domain_state = match kind.as_str() {
        "canvas" => hydrate_canvas_state(connection, project_id, panel_id, &mut state)?,
        "wiki" => hydrate_wiki_state(connection, project_id, panel_id, &mut state)?,
        "typesetting" => {
            hydrate_publications(connection, project_id, panel_id, &mut state)?
        }
        "publishing" => hydrate_releases(connection, project_id, panel_id, &mut state)?,
        _ => false,
    };
    if ui_revision == 0 && !has_domain_state {
        Ok(None)
    } else {
        Ok(Some(state))
    }
}

fn write_decomposed_panel_state(
    connection: &Connection,
    project_id: &str,
    panel_id: &str,
    state: &Value,
) -> Result<i64, CliError> {
    let kind = connection
        .query_row(
            "SELECT kind FROM panels WHERE project_id = ? AND id = ?",
            params![project_id, panel_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(to_cli_error)?;
    let mut ui_state = state.clone();
    if !ui_state.is_object() {
        return Err(CliError::new("Panel state must be a JSON object."));
    }

    let mut revision = panel_state_revision(connection, project_id, panel_id)?;
    match kind.as_str() {
        "canvas" => {
            revision = revision.max(persist_canvas_state(
                connection,
                project_id,
                panel_id,
                state,
            )?);
            ui_state = json!({
                "activeCanvasId": canvas_resource_id(project_id),
            });
        }
        "wiki" => {
            revision = revision.max(persist_wiki_state(
                connection,
                project_id,
                panel_id,
                state,
            )?);
            for key in ["rawDocuments", "myDocuments", "wikiSpaces"] {
                ui_state
                    .as_object_mut()
                    .expect("validated object")
                    .remove(key);
            }
        }
        "typesetting" => {
            revision = revision.max(persist_publications(
                connection,
                project_id,
                panel_id,
                state,
            )?);
            ui_state
                .as_object_mut()
                .expect("validated object")
                .remove("publications");
        }
        "publishing" => {
            revision = revision.max(persist_releases(
                connection,
                project_id,
                panel_id,
                state,
            )?);
            ui_state
                .as_object_mut()
                .expect("validated object")
                .remove("releases");
        }
        _ => {}
    }

    let ui_json = serde_json::to_string(&ui_state).map_err(to_cli_error)?;
    let ui_hash = hash_text(&ui_json);
    let current_hash = connection
        .query_row(
            "SELECT ui_state_hash FROM panels WHERE project_id = ? AND id = ?",
            params![project_id, panel_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(to_cli_error)?;
    if current_hash != ui_hash {
        let ui_revision = record_scope(connection, "panel_ui", Some(project_id), Some(panel_id))?;
        connection
            .execute(
                r#"
                UPDATE panels SET ui_state_revision = ?, ui_state_hash = ?, ui_state_json = ?,
                  updated_at = ? WHERE project_id = ? AND id = ?
                "#,
                params![
                    ui_revision,
                    ui_hash,
                    ui_json,
                    crate::control::now_iso(),
                    project_id,
                    panel_id,
                ],
            )
            .map_err(to_cli_error)?;
        revision = revision.max(ui_revision);
    }
    sync_task_resources_for_project(connection, project_id)?;
    Ok(revision)
}

fn panel_state_revision(
    connection: &Connection,
    project_id: &str,
    panel_id: &str,
) -> Result<i64, CliError> {
    let panel_kind = connection
        .query_row(
            "SELECT kind FROM panels WHERE project_id = ? AND id = ?",
            params![project_id, panel_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(to_cli_error)?;
    let resource_kinds = match panel_kind.as_str() {
        "canvas" => Some("('canvas')"),
        "wiki" => Some("('document', 'wiki_space')"),
        "typesetting" => Some("('publication', 'asset')"),
        "publishing" => Some("('publication', 'release', 'asset')"),
        _ => None,
    };
    let ui_revision = connection
        .query_row(
            "SELECT ui_state_revision FROM panels WHERE project_id = ? AND id = ?",
            params![project_id, panel_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(to_cli_error)?;
    let Some(resource_kinds) = resource_kinds else {
        return Ok(ui_revision);
    };
    let sql = format!(
        "SELECT COALESCE(MAX(revision), 0) FROM resources \
         WHERE project_id = ? AND kind IN {resource_kinds}"
    );
    let resource_revision = connection
        .query_row(&sql, [project_id], |row| row.get::<_, i64>(0))
        .map_err(to_cli_error)?;
    Ok(ui_revision.max(resource_revision))
}

fn canvas_resource_id(project_id: &str) -> String {
    format!("canvas:{project_id}")
}

fn persist_canvas_state(
    connection: &Connection,
    project_id: &str,
    panel_id: &str,
    state: &Value,
) -> Result<i64, CliError> {
    let resource_id = canvas_resource_id(project_id);
    let state_json = serde_json::to_string(state).map_err(to_cli_error)?;
    let state_hash = hash_text(&state_json);
    let current = connection
        .query_row(
            "SELECT state_revision, state_hash FROM canvas_documents WHERE resource_id = ?",
            [&resource_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(to_cli_error)?;
    if current.as_ref().is_some_and(|value| value.1 == state_hash) {
        return Ok(current.map(|value| value.0).unwrap_or(0));
    }
    let revision = record_resource_scope(connection, project_id, panel_id, &resource_id)?;
    upsert_resource(
        connection,
        &resource_id,
        project_id,
        panel_id,
        "canvas",
        "Canvas",
        revision,
        state,
    )?;
    connection
        .execute(
            r#"
            INSERT INTO canvas_documents (
              resource_id, format_version, state_revision, state_hash, state_json
            ) VALUES (?, 1, ?, ?, ?)
            ON CONFLICT(resource_id) DO UPDATE SET
              state_revision = excluded.state_revision,
              state_hash = excluded.state_hash,
              state_json = excluded.state_json
            "#,
            params![resource_id, revision, state_hash, state_json],
        )
        .map_err(to_cli_error)?;
    Ok(revision)
}

fn hydrate_canvas_state(
    connection: &Connection,
    project_id: &str,
    _panel_id: &str,
    state: &mut Value,
) -> Result<bool, CliError> {
    let raw = connection
        .query_row(
            r#"
            SELECT c.state_json FROM canvas_documents c
            JOIN resources r ON r.id = c.resource_id
            WHERE r.project_id = ? AND r.kind = 'canvas' AND r.deleted_at IS NULL
            ORDER BY r.updated_at DESC, r.id ASC LIMIT 1
            "#,
            [project_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(to_cli_error)?;
    if let Some(raw) = raw {
        *state = serde_json::from_str(&raw).map_err(to_cli_error)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn persist_wiki_state(
    connection: &Connection,
    project_id: &str,
    panel_id: &str,
    state: &Value,
) -> Result<i64, CliError> {
    let raw_documents = state
        .get("rawDocuments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let my_documents = state
        .get("myDocuments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let spaces = state
        .get("wikiSpaces")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut revision = 0;
    let mut active_documents = BTreeSet::new();
    for (position, document) in raw_documents.iter().enumerate() {
        if let Some(id) = document.get("id").and_then(Value::as_str) {
            active_documents.insert(id.to_owned());
            revision = revision.max(persist_document(
                connection,
                project_id,
                panel_id,
                "wiki_source",
                position,
                document,
            )?);
        }
    }
    for (position, document) in my_documents.iter().enumerate() {
        if let Some(id) = document.get("id").and_then(Value::as_str) {
            active_documents.insert(id.to_owned());
            revision = revision.max(persist_document(
                connection,
                project_id,
                panel_id,
                "my_document",
                position,
                document,
            )?);
        }
    }
    let mut statement = connection
        .prepare(
            r#"
            SELECT r.id
            FROM resources r
            JOIN documents d ON d.resource_id = r.id
            WHERE r.project_id = ? AND r.deleted_at IS NULL
              AND d.document_kind = 'my_document'
            "#,
        )
        .map_err(to_cli_error)?;
    for id in statement
        .query_map([project_id], |row| row.get::<_, String>(0))
        .map_err(to_cli_error)?
    {
        active_documents.insert(id.map_err(to_cli_error)?);
    }
    drop(statement);
    revision = revision.max(soft_delete_missing_resources(
        connection,
        project_id,
        panel_id,
        "document",
        &active_documents,
    )?);

    let mut active_spaces = BTreeSet::new();
    let mut space_versions = BTreeMap::new();
    for (position, space) in spaces.iter().enumerate() {
        let Some(id) = space.get("id").and_then(Value::as_str) else {
            continue;
        };
        active_spaces.insert(id.to_owned());
        let (space_revision, content_version) = persist_wiki_space(
            connection,
            project_id,
            panel_id,
            position,
            space,
        )?;
        revision = revision.max(space_revision);
        space_versions.insert(id.to_owned(), content_version);
    }
    revision = revision.max(soft_delete_missing_resources(
        connection,
        project_id,
        panel_id,
        "wiki_space",
        &active_spaces,
    )?);

    for document in &raw_documents {
        let Some(document_id) = document.get("id").and_then(Value::as_str) else {
            continue;
        };
        let document_version = document
            .get("markdownVersion")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let ingestions = document
            .get("ingestionByWikiSpace")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for (space_id, ingestion) in ingestions {
            if !active_spaces.contains(&space_id) {
                continue;
            }
            let disposition = match ingestion.get("status").and_then(Value::as_str) {
                Some("ingested") => "included",
                Some("covered") => "already_covered",
                Some("filtered") => "excluded",
                _ => continue,
            };
            let task_id = ingestion
                .get("taskId")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    CliError::with_code(
                        "invalid_ingestion_result",
                        "Completed Wiki ingestion requires a Task id.",
                    )
                })?;
            let processed_document_version = ingestion
                .get("markdownVersion")
                .and_then(Value::as_i64)
                .unwrap_or(document_version);
            let wiki_version_at_processing =
                space_versions.get(&space_id).copied().unwrap_or(0);
            let reason_code = ingestion.get("reasonCode").and_then(Value::as_str);
            let summary = ingestion
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("");
            let updated_at = ingestion
                .get("updatedAt")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(crate::control::now_iso);
            connection
                .execute(
                    r#"
                    INSERT INTO wiki_source_ingestions (
                      project_id, wiki_space_id, document_id, processed_document_version,
                      wiki_version_at_processing, disposition, task_id,
                      reason_code, summary, created_at, updated_at
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    ON CONFLICT(project_id, wiki_space_id, document_id) DO UPDATE SET
                      processed_document_version = excluded.processed_document_version,
                      wiki_version_at_processing = excluded.wiki_version_at_processing,
                      disposition = excluded.disposition,
                      task_id = excluded.task_id,
                      reason_code = excluded.reason_code,
                      summary = excluded.summary,
                      updated_at = excluded.updated_at
                    "#,
                    params![
                        project_id,
                        space_id,
                        document_id,
                        processed_document_version,
                        wiki_version_at_processing,
                        disposition,
                        task_id,
                        reason_code,
                        summary,
                        &updated_at,
                        &updated_at,
                    ],
                )
                .map_err(to_cli_error)?;
        }
    }
    Ok(revision)
}

fn persist_document(
    connection: &Connection,
    project_id: &str,
    panel_id: &str,
    document_kind: &str,
    position: usize,
    document: &Value,
) -> Result<i64, CliError> {
    let id = document
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::new("Document id is required."))?;
    let title = document
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("");
    let media_type = document
        .get("mimeType")
        .and_then(Value::as_str)
        .unwrap_or("application/octet-stream");
    let source = document
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or(if document_kind == "my_document" {
            "agent"
        } else {
            "user"
        });
    let original_file_name = document
        .get("originalFileName")
        .and_then(Value::as_str)
        .unwrap_or("");
    let is_my_document = document_kind == "my_document";
    let original_content_ref = if is_my_document {
        document.pointer("/importSource/originalRef")
    } else {
        document.get("originalRef")
    }
    .and_then(Value::as_str);
    let active_content_ref = if is_my_document {
        document.get("contentRef")
    } else {
        document.get("markdownRef")
    }
    .and_then(Value::as_str);
    let logical_content_version = if is_my_document {
        document.get("contentVersion")
    } else {
        document.get("markdownVersion")
    }
    .and_then(Value::as_i64)
    .unwrap_or(0);
    let character_count = document.get("wordCount").and_then(Value::as_i64);
    let metadata = strip_fields(
        document,
        &[
            "id",
            "title",
            "mimeType",
            "source",
            "originalFileName",
            "markdownVersion",
            "contentVersion",
            "contentRevisionId",
            "contentManifestHash",
            "contentHash",
            "wordCount",
            "createdAt",
            "updatedAt",
            "conversion",
            "ingestionByWikiSpace",
        ],
    );
    let metadata_json = serde_json::to_string(&metadata).map_err(to_cli_error)?;
    let current_shape = connection
        .query_row(
            r#"
            SELECT r.title, d.document_kind, d.media_type, d.source, d.original_file_name,
                   d.original_content_ref, d.active_content_ref,
                   d.logical_content_version, d.character_count, d.position,
                   d.metadata_json, r.deleted_at
            FROM documents d JOIN resources r ON r.id = d.resource_id WHERE d.resource_id = ?
            "#,
            [id],
            |row| {
                Ok(json!([
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, Option<String>>(11)?,
                ]))
            },
        )
        .optional()
        .map_err(to_cli_error)?;
    let next_shape = json!([
        title,
        document_kind,
        media_type,
        source,
        original_file_name,
        original_content_ref,
        active_content_ref,
        logical_content_version,
        character_count,
        position as i64,
        metadata_json,
        Value::Null,
    ]);
    if current_shape.as_ref() == Some(&next_shape) {
        return connection
            .query_row("SELECT revision FROM resources WHERE id = ?", [id], |row| {
                row.get(0)
            })
            .map_err(to_cli_error);
    }

    let revision = record_resource_scope(connection, project_id, panel_id, id)?;
    upsert_resource(
        connection,
        id,
        project_id,
        panel_id,
        "document",
        title,
        revision,
        document,
    )?;
    connection
        .execute(
            r#"
            INSERT INTO documents (
              resource_id, document_kind, media_type, source, original_file_name,
              original_content_ref, active_content_ref,
              logical_content_version, character_count, position, metadata_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(resource_id) DO UPDATE SET
              document_kind = excluded.document_kind,
              media_type = excluded.media_type,
              source = excluded.source,
              original_file_name = excluded.original_file_name,
              original_content_ref = excluded.original_content_ref,
              active_content_ref = excluded.active_content_ref,
              logical_content_version = excluded.logical_content_version,
              character_count = excluded.character_count,
              position = excluded.position,
              metadata_json = excluded.metadata_json
            "#,
            params![
                id,
                document_kind,
                media_type,
                source,
                original_file_name,
                original_content_ref,
                active_content_ref,
                logical_content_version,
                character_count,
                position as i64,
                metadata_json,
            ],
        )
        .map_err(to_cli_error)?;
    Ok(revision)
}

impl Storage {
    pub(crate) fn delete_my_document_resource(
        &self,
        project_id: &str,
        panel_id: &str,
        document_id: &str,
    ) -> Result<(), CliError> {
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        let now = crate::control::now_iso();
        Self::delete_my_document_in_transaction(&tx, project_id, panel_id, document_id)?;
        if cancel_tasks_for_resource_in_transaction(
            &tx,
            project_id,
            document_id,
            "prerequisite_deleted",
            &now,
        )? {
            record_scope(&tx, "tasks", Some(project_id), None)?;
        }
        let selection = tx
            .query_row(
                "SELECT selection_json FROM panel_selections WHERE project_id = ? AND panel_id = ?",
                params![project_id, panel_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(to_cli_error)?
            .map(|raw| serde_json::from_str::<Value>(&raw).map_err(to_cli_error))
            .transpose()?;
        if let Some(mut selection) = selection {
            if let Some(selected_ids) = selection
                .get_mut("selectedMyDocumentIds")
                .and_then(Value::as_array_mut)
            {
                let previous_len = selected_ids.len();
                selected_ids.retain(|value| value.as_str() != Some(document_id));
                if selected_ids.len() != previous_len {
                    selection["updatedAt"] = json!(now);
                    Self::write_panel_selection_in_transaction(
                        &tx,
                        project_id,
                        panel_id,
                        &selection,
                    )?;
                }
            }
        }
        sync_task_resources_for_project(&tx, project_id)?;
        tx.commit().map_err(to_cli_error)
    }

    pub(crate) fn write_my_document_content_in_transaction(
        connection: &Connection,
        project_id: &str,
        expected_content_version: u64,
        document: &Value,
    ) -> Result<i64, CliError> {
        let document_id = document
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| CliError::new("My Document id is required."))?;
        let current = connection
            .query_row(
                r#"
                SELECT r.project_id, r.kind, d.document_kind,
                       d.logical_content_version, d.position
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
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(to_cli_error)?
            .ok_or_else(|| {
                CliError::with_code(
                    "target_not_found",
                    format!("My Document not found: {document_id}"),
                )
            })?;
        if current.0 != project_id || current.1 != "document" || current.2 != "my_document" {
            return Err(CliError::with_code(
                "resource_identity_conflict",
                "The My Document target belongs to another Project or module.",
            ));
        }
        if current.3 != expected_content_version as i64 {
            return Err(CliError::with_code(
                "content_conflict",
                format!(
                    "My Document changed from version {expected_content_version} to {}",
                    current.3
                ),
            ));
        }
        if document
            .get("contentVersion")
            .and_then(Value::as_u64)
            != Some(expected_content_version + 1)
        {
            return Err(CliError::with_code(
                "content_conflict",
                "Prepared My Document metadata has an unexpected version.",
            ));
        }
        persist_document(
            connection,
            project_id,
            "",
            "my_document",
            current.4.max(0) as usize,
            document,
        )
    }
}

fn persist_wiki_space(
    connection: &Connection,
    project_id: &str,
    panel_id: &str,
    position: usize,
    space: &Value,
) -> Result<(i64, i64), CliError> {
    let id = space
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::new("Wiki space id is required."))?;
    let title = space.get("title").and_then(Value::as_str).unwrap_or("Wiki");
    let root_ref = space.get("rootRef").and_then(Value::as_str);
    let selected_skill_id = space.get("agentSkillId").and_then(Value::as_str);
    let metadata = strip_fields(
        space,
        &[
            "id",
            "title",
            "contentVersion",
            "contentRevisionId",
            "contentManifestHash",
            "contentHash",
            "createdAt",
            "updatedAt",
        ],
    );
    let metadata_json = serde_json::to_string(&metadata).map_err(to_cli_error)?;
    let current = connection
        .query_row(
            r#"
            SELECT r.content_version, w.root_ref, w.position, w.metadata_json,
                   r.title, r.deleted_at, r.revision
            FROM wiki_spaces w JOIN resources r ON r.id = w.resource_id
            WHERE w.resource_id = ?
            "#,
            [id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        )
        .optional()
        .map_err(to_cli_error)?;
    if current.as_ref().is_some_and(|value| {
        value.1.as_deref() == root_ref
            && value.2 == position as i64
            && value.3 == metadata_json
            && value.4 == title
            && value.5.is_none()
    }) {
        let value = current.expect("checked");
        return Ok((value.6, value.0));
    }
    let content_version = current.as_ref().map(|value| value.0).unwrap_or(0);
    let revision = record_resource_scope(connection, project_id, panel_id, id)?;
    upsert_resource(
        connection,
        id,
        project_id,
        panel_id,
        "wiki_space",
        title,
        revision,
        space,
    )?;
    connection
        .execute(
            r#"
            INSERT INTO wiki_spaces (
              resource_id, root_ref, selected_skill_id, position, metadata_json
            ) VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(resource_id) DO UPDATE SET
              root_ref = excluded.root_ref,
              selected_skill_id = excluded.selected_skill_id,
              position = excluded.position,
              metadata_json = excluded.metadata_json
            "#,
            params![
                id,
                root_ref,
                selected_skill_id,
                position as i64,
                metadata_json,
            ],
        )
        .map_err(to_cli_error)?;
    Ok((revision, content_version))
}
