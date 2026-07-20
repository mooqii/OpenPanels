pub struct WrittenAsset {
    pub asset_ref: String,
    pub file_name: String,
    pub file_path: PathBuf,
}

fn project_task_capability(queue: &str, task_type: &str) -> String {
    match (queue, task_type) {
        ("wiki", "convert_document_to_markdown") => "wiki.convertDocument".to_owned(),
        ("wiki", "ingest_markdown_into_wiki") => "wiki.ingestMarkdown".to_owned(),
        ("wiki", "maintain_wiki") => "wiki.maintain".to_owned(),
        ("writing", "refine_writing_skill") => "writing.refineSkill".to_owned(),
        _ => format!("{}.{}", queue, task_type.replace('_', ".")),
    }
}

fn workflow_id_for_task(task_id: &str) -> String {
    format!(
        "workflow:{}",
        task_id.strip_prefix("task:").unwrap_or(task_id)
    )
}

fn insert_workflow_if_missing(
    connection: &Connection,
    workflow_id: &str,
    project_id: &str,
    panel_id: &str,
    workflow_type: &str,
    source_workflow_id: Option<&str>,
    source: &Value,
    now: &str,
) -> Result<(), CliError> {
    connection
        .execute(
            r#"
            INSERT OR IGNORE INTO workflows (
              id, project_id, panel_id, type, status, source_workflow_id,
              source_json, created_at, updated_at
            ) VALUES (?, ?, ?, ?, 'active', ?, ?, ?, ?)
            "#,
            params![
                workflow_id,
                project_id,
                panel_id,
                workflow_type,
                source_workflow_id,
                serde_json::to_string(source).map_err(to_cli_error)?,
                now,
                now,
            ],
        )
        .map_err(to_cli_error)?;
    Ok(())
}

fn insert_task_created_records(
    connection: &Connection,
    task_id: &str,
    workflow_id: &str,
    project_id: &str,
    status: &str,
    _capability: &str,
    input: &Value,
    now: &str,
) -> Result<(), CliError> {
    connection
        .execute(
            "INSERT INTO task_events (task_id, workflow_id, event_type, from_status, to_status, created_at) VALUES (?, ?, 'created', NULL, ?, ?)",
            params![task_id, workflow_id, status, now],
        )
        .map_err(to_cli_error)?;
    if let Some(document_id) = input.get("documentId").and_then(Value::as_str) {
        connection
            .execute(
                r#"
                INSERT OR IGNORE INTO task_inputs (
                  id, task_id, resource_kind, resource_id, resource_version,
                  content_hash, snapshot_ref, missing_policy, changed_policy, created_at
                ) VALUES (?, ?, 'wiki.rawDocument', ?, ?, ?, ?, 'cancel', 'supersede', ?)
                "#,
                params![
                    crate::ids::random_id("task-input"),
                    task_id,
                    document_id,
                    input
                        .get("markdownVersion")
                        .and_then(Value::as_i64)
                        .map(|value| value.to_string()),
                    input.get("contentHash").and_then(Value::as_str),
                    input.get("snapshotRef").and_then(Value::as_str),
                    now,
                ],
            )
            .map_err(to_cli_error)?;
    }
    if let Some(snapshot) = input.get("contextSnapshot") {
        for (collection, resource_kind) in [
            ("rawDocuments", "wiki.rawDocument"),
            ("generatedDocuments", "wiki.generatedDocument"),
        ] {
            for document in snapshot
                .get(collection)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(resource_id) = document.get("id").and_then(Value::as_str) else {
                    continue;
                };
                connection.execute(
                    r#"INSERT OR IGNORE INTO task_inputs (
                       id, task_id, resource_kind, resource_id, resource_version,
                       content_hash, snapshot_ref, missing_policy, changed_policy, created_at
                       ) VALUES (?, ?, ?, ?, ?, ?, ?, 'continue_snapshot', 'continue_snapshot', ?)"#,
                    params![
                        crate::ids::random_id("task-input"),
                        task_id,
                        resource_kind,
                        resource_id,
                        document.get("contentVersion").or_else(|| document.get("markdownVersion")).and_then(Value::as_i64).map(|value| value.to_string()),
                        document.get("snapshotHash").and_then(Value::as_str),
                        format!("inline:input.contextSnapshot.{collection}.{resource_id}"),
                        now,
                    ],
                ).map_err(to_cli_error)?;
            }
        }
        if snapshot
            .pointer("/wikiSelection/selected")
            .and_then(Value::as_bool)
            == Some(true)
        {
            let wiki_space_id = snapshot
                .pointer("/wikiSelection/wikiSpaceId")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    CliError::with_code(
                        "invalid_task_input",
                        "Selected Wiki context has no Wiki space id.",
                    )
                })?;
            let revision_id = snapshot
                .pointer("/wikiSelection/contentRevisionId")
                .and_then(Value::as_str);
            connection
                .execute(
                    r#"INSERT OR IGNORE INTO task_inputs (
                       id, task_id, resource_kind, resource_id, resource_version,
                       snapshot_ref, missing_policy, changed_policy, created_at
                       ) VALUES (?, ?, 'wiki.space', ?, ?, ?,
                         'continue_snapshot', 'continue_snapshot', ?)"#,
                    params![
                        crate::ids::random_id("task-input"),
                        task_id,
                        wiki_space_id,
                        snapshot
                            .get("wikiRevision")
                            .and_then(Value::as_i64)
                            .map(|value| value.to_string()),
                        revision_id,
                        now,
                    ],
                )
                .map_err(to_cli_error)?;
            if let Some(revision_id) = revision_id {
                connection
                    .execute(
                        "INSERT OR IGNORE INTO content_pins (task_id, revision_id, created_at) VALUES (?, ?, ?)",
                        params![task_id, revision_id, now],
                    )
                    .map_err(to_cli_error)?;
            }
        }
    }
    if let Some(skill) = input.get("writingSkillSnapshot") {
        if let Some(resource_id) = skill.get("id").and_then(Value::as_str) {
            connection
                .execute(
                    r#"INSERT OR IGNORE INTO task_inputs (
                   id, task_id, resource_kind, resource_id, content_hash, snapshot_ref,
                   missing_policy, changed_policy, created_at
                   ) VALUES (?, ?, 'writing.skill', ?, ?, 'inline:input.writingSkillSnapshot',
                     'continue_snapshot', 'continue_snapshot', ?)"#,
                    params![
                        crate::ids::random_id("task-input"),
                        task_id,
                        resource_id,
                        skill.get("contentHash").and_then(Value::as_str),
                        now,
                    ],
                )
                .map_err(to_cli_error)?;
        }
    }
    if let Some(target_id) = input
        .get("targetGeneratedDocumentId")
        .and_then(Value::as_str)
    {
        connection
            .execute(
                r#"INSERT OR IGNORE INTO task_inputs (
               id, task_id, resource_kind, resource_id, resource_version,
               missing_policy, changed_policy, created_at
               ) VALUES (?, ?, 'writing.targetDocument', ?, ?, 'cancel', 'supersede', ?)"#,
                params![
                    crate::ids::random_id("task-input"),
                    task_id,
                    target_id,
                    input
                        .get("targetContentVersion")
                        .and_then(Value::as_i64)
                        .map(|value| value.to_string()),
                    now,
                ],
            )
            .map_err(to_cli_error)?;
    }
    crate::content::pin_task_inputs_in_transaction(connection, task_id, now)?;
    let _ = project_id;
    Ok(())
}

fn sanitize_asset_path(value: &str) -> String {
    let parts = value
        .split('/')
        .map(sanitize_path_part)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        "asset.png".to_owned()
    } else {
        parts.join("/")
    }
}

fn unique_file_name(assets_dir: &std::path::Path, requested_name: &str) -> String {
    let requested = sanitize_asset_path(requested_name);
    if !assets_dir.join(&requested).exists() {
        return requested;
    }
    let path = std::path::Path::new(&requested);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("asset");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    for index in 1.. {
        let candidate = format!("{stem}-{index}{extension}");
        if !assets_dir.join(&candidate).exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn extract_panel_state_schema_version(kind: PanelKind, state: &Value) -> Option<i64> {
    match kind {
        PanelKind::Canvas => state
            .get("schema")
            .and_then(|schema| schema.get("schemaVersion"))
            .and_then(Value::as_i64),
        PanelKind::Wiki => state.get("schemaVersion").and_then(Value::as_i64),
        PanelKind::Writing => state.get("schemaVersion").and_then(Value::as_i64),
        PanelKind::Typesetting => state.get("schemaVersion").and_then(Value::as_i64),
        PanelKind::Publishing => state.get("schemaVersion").and_then(Value::as_i64),
    }
}
