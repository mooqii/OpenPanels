fn hydrate_wiki_state(
    connection: &Connection,
    project_id: &str,
    panel_id: &str,
    state: &mut Value,
) -> Result<bool, CliError> {
    let raw_documents = read_documents(
        connection,
        project_id,
        panel_id,
        "wiki_source",
    )?;
    let my_documents = read_documents(
        connection,
        project_id,
        panel_id,
        "my_document",
    )?;
    let spaces = read_wiki_spaces(connection, project_id, panel_id)?;
    let tasks = read_resource_task_projections(connection, project_id)?;
    let mut projected_raw = Vec::with_capacity(raw_documents.len());
    for mut document in raw_documents {
        project_wiki_source_status(
            connection,
            project_id,
            &mut document,
            &spaces,
            &tasks,
        )?;
        projected_raw.push(document);
    }
    let mut projected_my_documents = Vec::with_capacity(my_documents.len());
    for mut document in my_documents {
        project_my_document_status(&mut document, &tasks)?;
        projected_my_documents.push(document);
    }
    state["rawDocuments"] = Value::Array(projected_raw);
    state["myDocuments"] = Value::Array(projected_my_documents);
    state["wikiSpaces"] = Value::Array(spaces);
    Ok(!state["rawDocuments"]
        .as_array()
        .is_some_and(Vec::is_empty)
        || !state["myDocuments"]
            .as_array()
            .is_some_and(Vec::is_empty)
        || !state["wikiSpaces"].as_array().is_some_and(Vec::is_empty))
}

fn read_documents(
    connection: &Connection,
    project_id: &str,
    _panel_id: &str,
    document_kind: &str,
) -> Result<Vec<Value>, CliError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT r.id, r.title, r.created_at, r.updated_at, d.media_type, d.source,
                   d.original_file_name, d.original_revision_id, d.active_revision_id,
                   d.content_version, d.character_count, d.metadata_json
            FROM documents d JOIN resources r ON r.id = d.resource_id
            WHERE r.project_id = ? AND r.deleted_at IS NULL AND d.document_kind = ?
            ORDER BY d.position ASC, r.id ASC
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map(params![project_id, document_kind], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, Option<i64>>(10)?,
                row.get::<_, String>(11)?,
            ))
        })
        .map_err(to_cli_error)?;
    rows.map(|row| {
        let (
            id,
            title,
            created_at,
            updated_at,
            media_type,
            source,
            file_name,
            original_ref,
            active_ref,
            version,
            character_count,
            metadata_json,
        ) = row.map_err(to_cli_error)?;
        let mut value =
            serde_json::from_str::<Value>(&metadata_json).map_err(to_cli_error)?;
        value["id"] = json!(id);
        value["title"] = json!(title);
        value["mimeType"] = json!(media_type);
        value["source"] = json!(source);
        value["originalFileName"] = json!(file_name);
        value["createdAt"] = json!(created_at);
        value["updatedAt"] = json!(updated_at);
        if document_kind == "wiki_source" {
            value["originalRef"] = original_ref.map_or(Value::Null, Value::String);
            value["markdownRef"] = active_ref.map_or(Value::Null, Value::String);
            value["markdownVersion"] = json!(version);
        } else {
            value["contentRef"] = active_ref.map_or(Value::Null, Value::String);
            value["contentVersion"] = json!(version);
        }
        value["wordCount"] = character_count.map_or(Value::Null, |count| json!(count));
        Ok(value)
    })
    .collect()
}

fn read_wiki_spaces(
    connection: &Connection,
    project_id: &str,
    _panel_id: &str,
) -> Result<Vec<Value>, CliError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT r.id, r.title, r.created_at, r.updated_at, w.active_revision_id,
                   w.content_version, w.selected_skill_id, w.metadata_json
            FROM wiki_spaces w JOIN resources r ON r.id = w.resource_id
            WHERE r.project_id = ? AND r.deleted_at IS NULL
            ORDER BY w.position ASC, r.id ASC
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
            ))
        })
        .map_err(to_cli_error)?;
    rows.map(|row| {
        let (id, title, created_at, updated_at, active_ref, version, skill, metadata) =
            row.map_err(to_cli_error)?;
        let mut value = serde_json::from_str::<Value>(&metadata).map_err(to_cli_error)?;
        value["id"] = json!(id);
        value["title"] = json!(title);
        value["createdAt"] = json!(created_at);
        value["updatedAt"] = json!(updated_at);
        value["contentVersion"] = json!(version);
        if value.get("rootRef").is_none() {
            value["rootRef"] = active_ref.map_or(Value::Null, Value::String);
        }
        if let Some(skill) = skill {
            value["selectedSkillId"] = json!(skill);
        }
        Ok(value)
    })
    .collect()
}

fn read_resource_task_projections(
    connection: &Connection,
    project_id: &str,
) -> Result<BTreeMap<String, Vec<StoredTaskProjection>>, CliError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT DISTINCT tr.resource_id, t.id, t.handler_key, t.status, t.depends_on_task_id,
                   t.input_json, t.source_json, t.error_json, t.updated_at
            FROM task_resources tr JOIN tasks t ON t.id = tr.task_id
            WHERE t.project_id = ?
            ORDER BY t.updated_at DESC, t.id ASC
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, String>(8)?,
            ))
        })
        .map_err(to_cli_error)?;
    let mut tasks = BTreeMap::<String, Vec<StoredTaskProjection>>::new();
    for row in rows {
        let (resource_id, id, handler_key, status, dependency, input, source, error, updated_at) =
            row.map_err(to_cli_error)?;
        let task_type = crate::capabilities::task_route_for_handler(&handler_key)?
            .map(|route| route.task_type.clone())
            .unwrap_or(handler_key);
        tasks
            .entry(resource_id)
            .or_default()
            .push(StoredTaskProjection {
                id,
                task_type,
                status,
                depends_on_task_id: dependency,
                input: serde_json::from_str(&input).unwrap_or_else(|_| json!({})),
                source: serde_json::from_str(&source).unwrap_or_else(|_| json!({})),
                error: error
                    .and_then(|value| serde_json::from_str(&value).ok())
                    .unwrap_or(Value::Null),
                updated_at,
            });
    }
    Ok(tasks)
}

fn project_wiki_source_status(
    connection: &Connection,
    project_id: &str,
    document: &mut Value,
    spaces: &[Value],
    tasks: &BTreeMap<String, Vec<StoredTaskProjection>>,
) -> Result<(), CliError> {
    let document_id = document
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let document_tasks = tasks
        .get(&document_id)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let conversion = document_tasks
        .iter()
        .find(|task| task.task_type == "convert_document_to_markdown");
    document["conversion"] = project_conversion(
        conversion,
        document.get("markdownRef").is_some_and(Value::is_string),
        document
            .get("updatedAt")
            .and_then(Value::as_str)
            .unwrap_or(""),
    )?;

    let document_version = document
        .get("markdownVersion")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let mut ingestions = serde_json::Map::new();
    for space in spaces {
        let Some(space_id) = space.get("id").and_then(Value::as_str) else {
            continue;
        };
        let processed = connection
            .query_row(
                r#"
                SELECT processed_document_version, disposition, task_id,
                       reason_code, summary, updated_at
                FROM wiki_source_ingestions
                WHERE project_id = ? AND wiki_space_id = ? AND document_id = ?
                "#,
                params![project_id, space_id, document_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(to_cli_error)?;
        let task = document_tasks.iter().find(|task| {
            let task_version = task.input.get("markdownVersion").and_then(Value::as_i64);
            task.task_type == "ingest_markdown_into_wiki"
                && task.source.get("wikiSpaceId").and_then(Value::as_str) == Some(space_id)
                && (task_version == Some(document_version)
                    || (task_version == Some(document_version + 1)
                        && task.depends_on_task_id.as_deref()
                            == conversion.map(|dependency| dependency.id.as_str())))
        });
        let dependency_waiting = task
            .and_then(|task| task.depends_on_task_id.as_deref())
            .is_some_and(|dependency_id| {
                !document_tasks.iter().any(|candidate| {
                    candidate.id == dependency_id && candidate.status == "succeeded"
                })
            });
        let processed_current = processed
            .as_ref()
            .is_some_and(|record| document_version > 0 && record.0 == document_version);
        let current_record = processed.as_ref().filter(|_| processed_current);
        let status = if processed_current {
            match processed.as_ref().map(|record| record.1.as_str()) {
                Some("included") => "ingested",
                Some("already_covered") => "covered",
                Some("excluded") => "filtered",
                _ => "unrecorded",
            }
        } else {
            match task.map(|task| task.status.as_str()) {
                Some("running") => "ingesting",
                Some("failed") => "failed",
                Some("cancelled" | "superseded") => "cancelled",
                Some("queued") if dependency_waiting => "waiting",
                Some("queued") => "queued",
                Some("succeeded") => "unrecorded",
                Some(status) => {
                    return Err(CliError::with_code(
                        "invalid_task_status",
                        format!(
                            "Unsupported Task status in Wiki ingestion projection: {status}"
                        ),
                    ));
                }
                None => "unscheduled",
            }
        };
        let derived_error = if !processed_current
            && task.is_some_and(|task| task.status == "succeeded")
        {
            json!({
                "code": "ingestion_result_missing",
                "message": "The indexing Task completed without recording its Wiki disposition."
            })
        } else if !processed_current && task.is_none() {
            json!({
                "code": "ingestion_task_missing",
                "message": "This document version has no indexing Task or recorded result."
            })
        } else {
            task.filter(|task| {
                matches!(
                    task.status.as_str(),
                    "failed" | "cancelled" | "superseded"
                )
            })
            .map(|task| task.error.clone())
            .unwrap_or(Value::Null)
        };
        ingestions.insert(
            space_id.to_owned(),
            json!({
                "status": status,
                "taskId": current_record.map(|record| record.2.as_str()).or_else(|| task.map(|task| task.id.as_str())),
                "markdownVersion": document_version,
                "error": derived_error,
                "disposition": current_record.map(|record| record.1.as_str()),
                "reasonCode": current_record.and_then(|record| record.3.as_deref()),
                "summary": current_record.map(|record| record.4.as_str()),
                "updatedAt": current_record.map(|record| record.5.as_str()).or_else(|| task.map(|task| task.updated_at.as_str())).unwrap_or_else(|| document.get("updatedAt").and_then(Value::as_str).unwrap_or("")),
            }),
        );
    }
    document["ingestionByWikiSpace"] = Value::Object(ingestions);
    Ok(())
}

fn project_my_document_status(
    document: &mut Value,
    tasks: &BTreeMap<String, Vec<StoredTaskProjection>>,
) -> Result<(), CliError> {
    let document_id = document.get("id").and_then(Value::as_str).unwrap_or("");
    let document_tasks = tasks.get(document_id).map(Vec::as_slice).unwrap_or(&[]);
    if let Some(task) = document_tasks
        .iter()
        .find(|task| task.task_type == "convert_document_to_markdown")
    {
        document["conversion"] = project_conversion(
            Some(task),
            false,
            document
                .get("updatedAt")
                .and_then(Value::as_str)
                .unwrap_or(""),
        )?;
    }
    Ok(())
}

fn project_conversion(
    task: Option<&StoredTaskProjection>,
    ready_without_task: bool,
    fallback_updated_at: &str,
) -> Result<Value, CliError> {
    let status = match task.map(|task| task.status.as_str()) {
        Some("queued") => "queued",
        Some("running") => "converting",
        Some("succeeded") => "ready",
        Some("failed") => "failed",
        Some("cancelled" | "superseded") => "cancelled",
        Some(status) => {
            return Err(CliError::with_code(
                "invalid_task_status",
                format!("Unsupported Task status in Wiki projection: {status}"),
            ));
        }
        None if ready_without_task => "not_required",
        None => "queued",
    };
    Ok(json!({
        "status": status,
        "taskId": task.map(|task| task.id.as_str()),
        "error": task.filter(|task| matches!(task.status.as_str(), "failed" | "cancelled" | "superseded")).map(|task| task.error.clone()),
        "updatedAt": task.map(|task| task.updated_at.as_str()).unwrap_or(fallback_updated_at),
    }))
}

#[cfg(test)]
mod projection_tests {
    use super::{project_conversion, StoredTaskProjection};
    use serde_json::json;

    #[test]
    fn wiki_conversion_uses_the_canonical_task_status_projection() {
        let cases = [
            ("queued", "queued"),
            ("running", "converting"),
            ("succeeded", "ready"),
            ("failed", "failed"),
            ("cancelled", "cancelled"),
            ("superseded", "cancelled"),
        ];
        for (task_status, expected) in cases {
            let task = task(task_status);
            let projection =
                project_conversion(Some(&task), false, "fallback").expect("projection");
            assert_eq!(projection["status"], expected);
        }
        assert_eq!(
            project_conversion(None, true, "fallback").expect("projection")["status"],
            "not_required"
        );
        assert_eq!(
            project_conversion(None, false, "fallback").expect("projection")["status"],
            "queued"
        );
    }

    #[test]
    fn wiki_conversion_rejects_a_noncanonical_task_status() {
        let task = task("claimed");
        let error = project_conversion(Some(&task), false, "fallback")
            .expect_err("noncanonical status must fail");
        assert_eq!(error.code(), Some("invalid_task_status"));
    }

    fn task(status: &str) -> StoredTaskProjection {
        StoredTaskProjection {
            id: "task:1".to_owned(),
            task_type: "convert_document_to_markdown".to_owned(),
            status: status.to_owned(),
            depends_on_task_id: None,
            input: json!({}),
            source: json!({}),
            error: json!(null),
            updated_at: "2026-07-24T00:00:00Z".to_owned(),
        }
    }
}
