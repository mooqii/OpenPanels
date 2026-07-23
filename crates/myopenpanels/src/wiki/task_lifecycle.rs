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

pub fn claim_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let mut wiki = get_wiki_task_target(paths, task_id)?;
    let task = task_mut(&mut wiki.tasks, task_id)?;
    let status = task.get("status").and_then(Value::as_str).unwrap_or("");
    if status != "queued" && status != "failed" && status != "reserved" && !task_lease_expired(task)
    {
        return Err(CliError::new(format!(
            "Wiki task is not claimable: {task_id}"
        )));
    }
    let now = now_iso();
    let task_type = task.get("type").and_then(Value::as_str).unwrap_or("");
    task["status"] = json!(if task_type == "convert_document_to_markdown" {
        "running"
    } else {
        "claimed"
    });
    task["error"] = Value::Null;
    task["attempt"] = json!(task.get("attempt").and_then(Value::as_i64).unwrap_or(0) + 1);
    if task.get("attemptLimit").and_then(Value::as_i64).is_none() {
        task["attemptLimit"] = json!(crate::tasks::TASK_EXECUTION_LIMIT);
    }
    task["leaseOwner"] = Value::Null;
    task["leaseExpiresAt"] = json!(lease_expires_at(15));
    task["lastHeartbeatAt"] = json!(now);
    task["retryAfter"] = Value::Null;
    task["updatedAt"] = json!(now);
    let task_snapshot = task.clone();
    if task_snapshot.get("type").and_then(Value::as_str) == Some("convert_document_to_markdown") {
        if let Some(document_id) = task_snapshot.get("documentId").and_then(Value::as_str) {
            let document = if is_my_document_conversion_task(&task_snapshot) {
                find_my_document_mut(&mut wiki.state, document_id)?
            } else {
                find_document_mut(&mut wiki.state, document_id)?
            };
            document["conversion"]["status"] = json!("converting");
            document["conversion"]["updatedAt"] = json!(now);
            document["updatedAt"] = json!(now);
        }
    }
    if task_snapshot.get("type").and_then(Value::as_str) == Some("ingest_markdown_into_wiki") {
        if let Some(document_id) = task_snapshot.get("documentId").and_then(Value::as_str) {
            let wiki_space_id = task_wiki_space_id(&task_snapshot)?.to_owned();
            let task_id = task_snapshot
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or(task_id)
                .to_owned();
            let markdown_version = task_snapshot
                .get("markdownVersion")
                .and_then(Value::as_i64)
                .or_else(|| {
                    find_document(&wiki.state, document_id)
                        .ok()
                        .and_then(|document| {
                            document.get("markdownVersion").and_then(Value::as_i64)
                        })
                })
                .unwrap_or(0);
            let document = find_document_mut(&mut wiki.state, document_id)?;
            let ingestion = document
                .as_object_mut()
                .ok_or_else(|| CliError::new("Wiki raw document is invalid."))?
                .entry("ingestionByWikiSpace")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(|| CliError::new("Wiki raw document ingestion state is invalid."))?;
            let mut current = ingestion
                .get(&wiki_space_id)
                .cloned()
                .unwrap_or_else(|| json!({}));
            current["status"] = json!("ingesting");
            current["taskId"] = json!(task_id);
            current["markdownVersion"] = json!(markdown_version);
            current["error"] = Value::Null;
            current["updatedAt"] = json!(now);
            ingestion.insert(wiki_space_id, current);
            document["updatedAt"] = json!(now);
        }
    }
    save_wiki_state(paths, &wiki)?;
    trace_task_event(
        "claimed",
        &task_snapshot,
        format!(
            "Agent claimed {}",
            task_type_label(
                task_snapshot
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            )
        ),
        Some(format!(
            "Started {}",
            task_type_label(
                task_snapshot
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            )
        )),
    );
    Ok(json!({ "task": task_snapshot, "state": wiki.state }))
}

pub fn complete_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    result: Option<Value>,
) -> Result<Value, CliError> {
    complete_task_internal(paths, task_id, result, true)
}

pub(crate) fn prepare_task_completion(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    result: Option<Value>,
) -> Result<Value, CliError> {
    complete_task_internal(paths, task_id, result, false)
}

fn complete_task_internal(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    result: Option<Value>,
    persist: bool,
) -> Result<Value, CliError> {
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
    let task_snapshot = {
        let task = task_mut(&mut wiki.tasks, task_id)?;
        task["status"] = json!("succeeded");
        task["error"] = Value::Null;
        task["result"] = result.unwrap_or(Value::Null);
        task["leaseOwner"] = Value::Null;
        task["leaseExpiresAt"] = Value::Null;
        task["lastHeartbeatAt"] = Value::Null;
        task["retryAfter"] = Value::Null;
        task["updatedAt"] = json!(now);
        task.clone()
    };
    let mut follow_up_task_ids = Vec::new();
    if task_snapshot.get("type").and_then(Value::as_str) == Some("convert_document_to_markdown") {
        if let Some(document_id) = task_snapshot.get("documentId").and_then(Value::as_str) {
            if is_my_document_conversion_task(&task_snapshot) {
                let document = find_my_document_mut(&mut wiki.state, document_id)?;
                document["conversion"]["status"] = json!("ready");
                document["conversion"]["error"] = Value::Null;
                document["conversion"]["updatedAt"] = json!(now);
                document["updatedAt"] = json!(now);
            } else {
                let markdown_version = {
                    let document = find_document_mut(&mut wiki.state, document_id)?;
                    document["conversion"]["status"] = json!("ready");
                    document["conversion"]["error"] = Value::Null;
                    document["conversion"]["updatedAt"] = json!(now);
                    document["updatedAt"] = json!(now);
                    document
                        .get("markdownVersion")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                };
                let existing_index = wiki.tasks.iter().position(|candidate| {
                    candidate.get("type").and_then(Value::as_str)
                        == Some("ingest_markdown_into_wiki")
                        && candidate.get("documentId").and_then(Value::as_str)
                            == Some(document_id)
                        && matches!(
                            candidate.get("status").and_then(Value::as_str),
                            Some("waiting" | "queued")
                        )
                        && (candidate.get("dependsOnTaskId").and_then(Value::as_str)
                            == Some(task_id)
                            || candidate
                                .get("dependsOnTaskIds")
                                .and_then(Value::as_array)
                                .is_some_and(|ids| {
                                    ids.iter().any(|id| id.as_str() == Some(task_id))
                                }))
                });
                let ingest_task = if let Some(index) = existing_index {
                    wiki.tasks[index]["status"] = json!("queued");
                    wiki.tasks[index]["markdownVersion"] = json!(markdown_version);
                    wiki.tasks[index]["updatedAt"] = json!(now);
                    wiki.tasks[index].clone()
                } else {
                    let wiki_space_id = task_wiki_space_id(&task_snapshot)?;
                    let mutation_key =
                        wiki_mutation_key(&wiki.project.id, &wiki.panel.id, wiki_space_id);
                    create_wiki_task(
                        &wiki.state,
                        &mut wiki.tasks,
                        "ingest_markdown_into_wiki",
                        document_id,
                        Some(document_id),
                        Some(markdown_version),
                        Some(wiki_space_id),
                        Some(&mutation_key),
                    )?
                };
                follow_up_task_ids.push(
                    ingest_task
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                );
                let wiki_space_id = task_wiki_space_id(&ingest_task)?.to_owned();
                let document = find_document_mut(&mut wiki.state, document_id)?;
                let ingestion = document
                    .as_object_mut()
                    .ok_or_else(|| CliError::new("Wiki raw document is invalid."))?
                    .entry("ingestionByWikiSpace")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .ok_or_else(|| {
                        CliError::new("Wiki raw document ingestion state is invalid.")
                    })?;
                ingestion.insert(
                    wiki_space_id,
                    create_ingestion_state(&ingest_task, markdown_version),
                );
                document["updatedAt"] = json!(now);
            }
        }
    }
    if task_snapshot.get("type").and_then(Value::as_str) == Some("ingest_markdown_into_wiki") {
        if let Some(document_id) = task_snapshot.get("documentId").and_then(Value::as_str) {
            let wiki_space_id = task_wiki_space_id(&task_snapshot)?.to_owned();
            let markdown_version = task_snapshot
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
    if persist {
        save_wiki_state(paths, &wiki)?;
        trace_task_event(
            "completed",
            &task_snapshot,
            format!(
                "Completed {}",
                task_type_label(
                    task_snapshot
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                )
            ),
            Some(format!(
                "Completed {}",
                task_type_label(
                    task_snapshot
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                )
            )),
        );
    }
    Ok(json!({ "task": task_snapshot, "state": wiki.state }))
}

pub fn fail_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    message: &str,
) -> Result<Value, CliError> {
    fail_task_with_retry(paths, task_id, message, None)
}

pub fn fail_task_with_retry(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    message: &str,
    retry_after: Option<&str>,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_task_target(paths, task_id)?;
    let now = now_iso();
    let task_snapshot = {
        let task = task_mut(&mut wiki.tasks, task_id)?;
        task["status"] = json!("failed");
        task["error"] = json!(message);
        task["leaseOwner"] = Value::Null;
        task["leaseExpiresAt"] = Value::Null;
        task["lastHeartbeatAt"] = Value::Null;
        task["retryAfter"] = retry_after.map_or(Value::Null, Value::from);
        task["updatedAt"] = json!(now);
        task.clone()
    };
    if task_snapshot.get("type").and_then(Value::as_str) == Some("convert_document_to_markdown") {
        if let Some(document_id) = task_snapshot.get("documentId").and_then(Value::as_str) {
            let document = if is_my_document_conversion_task(&task_snapshot) {
                find_my_document_mut(&mut wiki.state, document_id)?
            } else {
                find_document_mut(&mut wiki.state, document_id)?
            };
            document["conversion"]["status"] = json!("failed");
            document["conversion"]["error"] = json!(message);
            document["conversion"]["updatedAt"] = json!(now);
            document["updatedAt"] = json!(now);
        }
    }
    if task_snapshot.get("type").and_then(Value::as_str) == Some("ingest_markdown_into_wiki") {
        if let Some(document_id) = task_snapshot.get("documentId").and_then(Value::as_str) {
            let wiki_space_id = task_wiki_space_id(&task_snapshot)?.to_owned();
            let document = find_document_mut(&mut wiki.state, document_id)?;
            let ingestion = document
                .as_object_mut()
                .ok_or_else(|| CliError::new("Wiki raw document is invalid."))?
                .entry("ingestionByWikiSpace")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(|| CliError::new("Wiki raw document ingestion state is invalid."))?;
            let mut current = ingestion
                .get(&wiki_space_id)
                .cloned()
                .unwrap_or_else(|| create_ingestion_state(&task_snapshot, 0));
            current["status"] = json!("failed");
            current["error"] = json!(message);
            current["updatedAt"] = json!(now);
            ingestion.insert(wiki_space_id, current);
            document["updatedAt"] = json!(now);
        }
    }
    save_wiki_state(paths, &wiki)?;
    trace_task_event(
        "failed",
        &task_snapshot,
        format!("Task failed: {message}"),
        Some(format!("Task failed: {message}")),
    );
    Ok(json!({ "task": task_snapshot, "state": wiki.state }))
}

pub fn heartbeat_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    lease_expires_at: &str,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_task_target(paths, task_id)?;
    let now = now_iso();
    let task_snapshot = {
        let task = task_mut(&mut wiki.tasks, task_id)?;
        if !matches!(
            task.get("status").and_then(Value::as_str),
            Some("running" | "claimed" | "converting" | "indexing")
        ) {
            return Err(CliError::new(format!(
                "Wiki task is not running: {task_id}"
            )));
        }
        task["leaseExpiresAt"] = json!(lease_expires_at);
        task["lastHeartbeatAt"] = json!(now);
        task["updatedAt"] = json!(now);
        task.clone()
    };
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "task": task_snapshot, "state": wiki.state }))
}

pub fn release_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let mut wiki = get_wiki_task_target(paths, task_id)?;
    let now = now_iso();
    let task_snapshot = {
        let task = task_mut(&mut wiki.tasks, task_id)?;
        task["status"] = json!("queued");
        task["error"] = Value::Null;
        task["leaseOwner"] = Value::Null;
        task["leaseExpiresAt"] = Value::Null;
        task["lastHeartbeatAt"] = Value::Null;
        task["retryAfter"] = Value::Null;
        task["updatedAt"] = json!(now);
        task.clone()
    };
    reset_document_task_state(&mut wiki.state, &task_snapshot, "queued", None, &now)?;
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "task": task_snapshot, "state": wiki.state }))
}

pub fn retry_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let mut wiki = get_wiki_task_target(paths, task_id)?;
    let now = now_iso();
    let task_snapshot = {
        let task = task_mut(&mut wiki.tasks, task_id)?;
        if !matches!(
            task.get("status").and_then(Value::as_str),
            Some("failed" | "queued")
        ) {
            return Err(CliError::new(format!(
                "Wiki task cannot be retried: {task_id}"
            )));
        }
        task["status"] = json!("queued");
        task["attempt"] = json!(0);
        task["error"] = Value::Null;
        task["result"] = Value::Null;
        task["leaseOwner"] = Value::Null;
        task["leaseExpiresAt"] = Value::Null;
        task["lastHeartbeatAt"] = Value::Null;
        task["retryAfter"] = Value::Null;
        task["updatedAt"] = json!(now);
        task.clone()
    };
    reset_document_task_state(&mut wiki.state, &task_snapshot, "queued", None, &now)?;
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "task": task_snapshot, "state": wiki.state }))
}

pub fn cancel_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let mut wiki = get_wiki_task_target(paths, task_id)?;
    let now = now_iso();
    let task_snapshot = {
        let task = task_mut(&mut wiki.tasks, task_id)?;
        if task.get("status").and_then(Value::as_str) == Some("succeeded") {
            return Err(CliError::new(format!(
                "Completed Wiki task cannot be cancelled: {task_id}"
            )));
        }
        task["status"] = json!("cancelled");
        task["error"] = Value::Null;
        task["leaseOwner"] = Value::Null;
        task["leaseExpiresAt"] = Value::Null;
        task["lastHeartbeatAt"] = Value::Null;
        task["retryAfter"] = Value::Null;
        task["updatedAt"] = json!(now);
        task.clone()
    };
    reset_document_task_state(&mut wiki.state, &task_snapshot, "cancelled", None, &now)?;
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "task": task_snapshot, "state": wiki.state }))
}

fn reset_document_task_state(
    state: &mut Value,
    task: &Value,
    status: &str,
    error: Option<&str>,
    now: &str,
) -> Result<(), CliError> {
    let Some(document_id) = task.get("documentId").and_then(Value::as_str) else {
        return Ok(());
    };
    if task.get("type").and_then(Value::as_str) == Some("convert_document_to_markdown") {
        let document = if is_my_document_conversion_task(task) {
            find_my_document_mut(state, document_id)?
        } else {
            find_document_mut(state, document_id)?
        };
        document["conversion"]["status"] = json!(status);
        document["conversion"]["error"] = error.map_or(Value::Null, Value::from);
        document["conversion"]["updatedAt"] = json!(now);
        document["updatedAt"] = json!(now);
    } else if task.get("type").and_then(Value::as_str) == Some("ingest_markdown_into_wiki") {
        let wiki_space_id = task_wiki_space_id(task)?.to_owned();
        let document = find_document_mut(state, document_id)?;
        if let Some(ingestion) = document
            .get_mut("ingestionByWikiSpace")
            .and_then(Value::as_object_mut)
            .and_then(|ingestion| ingestion.get_mut(&wiki_space_id))
        {
            ingestion["status"] = json!(status);
            ingestion["error"] = error.map_or(Value::Null, Value::from);
            ingestion["updatedAt"] = json!(now);
        }
        document["updatedAt"] = json!(now);
    }
    Ok(())
}

fn lease_expires_at(minutes: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::minutes(minutes))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn task_lease_expired(task: &Value) -> bool {
    let Some(expires_at) = task.get("leaseExpiresAt").and_then(Value::as_str) else {
        return false;
    };
    chrono::DateTime::parse_from_rfc3339(expires_at)
        .map(|expires_at| expires_at.with_timezone(&chrono::Utc) <= chrono::Utc::now())
        .unwrap_or(false)
}
