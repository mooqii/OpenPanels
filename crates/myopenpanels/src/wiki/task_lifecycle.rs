fn is_my_document_conversion_task(task: &Value) -> bool {
    task.get("type").and_then(Value::as_str) == Some("convert_document_to_markdown")
        && task.get("documentKind").and_then(Value::as_str) == Some("my_document")
}

fn task_wiki_space_id(task: &Value) -> Result<&str, CliError> {
    task.get("wikiSpaceId")
        .and_then(Value::as_str)
        .or_else(|| task.pointer("/source/wikiSpaceId").and_then(Value::as_str))
        .or_else(|| task.pointer("/input/wikiSpaceId").and_then(Value::as_str))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            CliError::with_code(
                "invalid_task_input",
                "Wiki Task has no target Wiki Space.",
            )
        })
}

pub(crate) fn prepare_task_completion(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    result: Option<Value>,
) -> Result<Option<crate::tasks::PreparedPanelState>, CliError> {
    complete_task_internal(paths, task_id, result)
}

fn complete_task_internal(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    result: Option<Value>,
) -> Result<Option<crate::tasks::PreparedPanelState>, CliError> {
    let mut wiki = get_wiki_task_target(paths, task_id)?;
    let now = now_iso();
    let current_task = task_value(&wiki.tasks, task_id)?.clone();
    let ingestion_result = if current_task.get("type").and_then(Value::as_str)
        == Some("ingest_markdown_into_wiki")
    {
        let result = result.as_ref().ok_or_else(|| {
            CliError::with_code(
                "invalid_output",
                "Wiki ingestion completed without a result.",
            )
        })?;
        let disposition = result
            .get("disposition")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                CliError::with_code(
                    "invalid_output",
                    "Wiki ingestion completed without a disposition.",
                )
            })?;
        let outcome = result.get("outcome").and_then(Value::as_str).unwrap_or("");
        let reason_code = result.get("reasonCode").cloned().unwrap_or(Value::Null);
        let status = match disposition {
            "included" if outcome == "changed" && reason_code.is_null() => "ingested",
            "already_covered" if outcome == "no_change" && reason_code.is_null() => "covered",
            "excluded"
                if outcome == "no_change"
                    && matches!(
                        reason_code.as_str(),
                        Some(
                            "not_relevant"
                                | "insufficient_content"
                                | "unsupported_by_wiki_skill"
                                | "policy_excluded"
                        )
                    ) =>
            {
                "filtered"
            }
            _ => {
                return Err(CliError::with_code(
                    "invalid_output",
                    "Wiki ingestion outcome, disposition, or reasonCode is invalid.",
                ))
            }
        };
        Some((
            status,
            reason_code,
            result
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
        ))
    } else {
        None
    };
    let staged_markdown = crate::content::staged_files_for_task(
        paths,
        task_id,
        crate::content::ResourceKind::WikiMarkdown,
    )?;
    if let Some((document_id, _, bytes, _)) = staged_markdown.first() {
        let markdown = std::str::from_utf8(bytes).map_err(|_| {
            CliError::with_code("invalid_output", "Converted Markdown must be valid UTF-8.")
        })?;
        if markdown.trim().is_empty() {
            return Err(CliError::with_code(
                "invalid_output",
                "Converted Markdown cannot be empty.",
            ));
        }
        let document = find_document_mut(&mut wiki.state, document_id)?;
        let version = document
            .get("markdownVersion")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            + 1;
        document["markdownRef"] = json!("source.md");
        document["markdownVersion"] = json!(version);
        document["wordCount"] = json!(character_count(markdown));
        document["updatedAt"] = json!(now);
    }
    let staged_my_documents = crate::content::staged_files_for_task(
        paths,
        task_id,
        crate::content::ResourceKind::MyDocument,
    )?;
    if is_my_document_conversion_task(&current_task) {
        let (document_id, logical_path, bytes, _) = staged_my_documents.first().ok_or_else(|| {
            CliError::with_code(
                "invalid_output",
                "Conversion completed without a My Document Markdown artifact.",
            )
        })?;
        if logical_path != "content.md" {
            return Err(CliError::with_code(
                "invalid_output",
                "Imported document conversion must produce content.md.",
            ));
        }
        let markdown = std::str::from_utf8(bytes).map_err(|_| {
            CliError::with_code("invalid_output", "Converted Markdown must be valid UTF-8.")
        })?;
        if markdown.trim().is_empty() {
            return Err(CliError::with_code(
                "invalid_output",
                "Converted Markdown cannot be empty.",
            ));
        }
        let document = find_my_document_mut(&mut wiki.state, document_id)?;
        let version = document
            .get("contentVersion")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            + 1;
        document["contentRef"] = json!("content.md");
        document["contentVersion"] = json!(version);
        document["format"] = json!("markdown");
        document["mimeType"] = json!("text/markdown");
        document["wordCount"] = json!(character_count(markdown));
        document["updatedAt"] = json!(now);
    }
    let staged_wiki = crate::content::staged_files_for_task(
        paths,
        task_id,
        crate::content::ResourceKind::WikiSpace,
    )?;
    for (wiki_space_id, page_path, bytes, metadata) in &staged_wiki {
        let markdown = std::str::from_utf8(bytes).map_err(|_| {
            CliError::with_code("invalid_output", "Wiki pages must be valid UTF-8.")
        })?;
        upsert_page_index(
            &mut wiki.state,
            wiki_space_id,
            page_path,
            markdown,
            metadata.get("title").and_then(Value::as_str),
            &now,
        )?;
        update_wiki_space_timestamp(&mut wiki.state, wiki_space_id, &now)?;
    }
    if let Some((wiki_space_id, page_path, _, _)) = staged_wiki.first() {
        state_object_mut(&mut wiki.state)?
            .insert("activeWikiSpaceId".to_owned(), json!(wiki_space_id));
        state_object_mut(&mut wiki.state)?
            .insert("activeWikiPagePath".to_owned(), json!(page_path));
    }
    if current_task.get("type").and_then(Value::as_str) == Some("convert_document_to_markdown") {
        let document_id = current_task
            .get("documentId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                CliError::with_code("invalid_output", "Conversion Task has no source document.")
            })?;
        if is_my_document_conversion_task(&current_task) {
            let document = find_my_document(&wiki.state, document_id)?;
            if document
                .get("contentVersion")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                <= current_task
                    .get("markdownVersion")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                || staged_my_documents.is_empty()
            {
                return Err(CliError::with_code(
                    "invalid_output",
                    "Conversion completed without advancing the imported document.",
                ));
            }
        } else {
            let document = find_document(&wiki.state, document_id)?;
            let markdown_version = document
                .get("markdownVersion")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let markdown_ref = document
                .get("markdownRef")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    CliError::with_code(
                        "invalid_output",
                        "Conversion completed without a Markdown artifact.",
                    )
                })?;
            let markdown_exists = crate::content::active_file_path(
                paths,
                &wiki.project.id,
                crate::content::ResourceKind::WikiMarkdown,
                document_id,
                markdown_ref,
            )?
            .is_some();
            if markdown_version
                <= current_task
                    .get("markdownVersion")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                || (staged_markdown.is_empty() && !markdown_exists)
            {
                return Err(CliError::with_code(
                    "invalid_output",
                    "Conversion completed without advancing a valid Markdown artifact.",
                ));
            }
        }
    }
    if current_task.get("type").and_then(Value::as_str) == Some("ingest_markdown_into_wiki") {
        if let Some(document_id) = current_task.get("documentId").and_then(Value::as_str) {
            let wiki_space_id = task_wiki_space_id(&current_task)?.to_owned();
            let markdown_version = current_task
                .get("markdownVersion")
                .and_then(Value::as_i64)
                .unwrap_or_else(|| {
                    find_document(&wiki.state, document_id)
                        .ok()
                        .and_then(|document| {
                            document.get("markdownVersion").and_then(Value::as_i64)
                        })
                        .unwrap_or(0)
                });
            let (status, reason_code, summary) = ingestion_result
                .as_ref()
                .ok_or_else(|| {
                    CliError::with_code(
                        "invalid_output",
                        "Wiki ingestion result was not prepared.",
                    )
                })?;
            let document = find_document_mut(&mut wiki.state, document_id)?;
            let ingestions = document
                .as_object_mut()
                .ok_or_else(|| CliError::new("Wiki raw document is invalid."))?
                .entry("ingestionByWikiSpace")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(|| CliError::new("Wiki raw document ingestion state is invalid."))?;
            ingestions.insert(
                wiki_space_id,
                json!({
                    "status": status,
                    "taskId": task_id,
                    "markdownVersion": markdown_version,
                    "error": null,
                    "reasonCode": reason_code,
                    "summary": summary,
                    "updatedAt": now,
                }),
            );
            document["updatedAt"] = json!(now);
        }
    }
    Ok(Some(crate::tasks::PreparedPanelState::new(
        &wiki.panel.id,
        wiki.revision,
        wiki.state,
    )))
}
