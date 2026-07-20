pub fn read_markdown(paths: &MyOpenPanelsPaths, document_id: &str) -> Result<Value, CliError> {
    if crate::content::broker_execution_available() {
        let payload = crate::content::broker_read_file(&crate::content::ReadFileRequest {
            resource_kind: crate::content::ResourceKind::WikiMarkdown
                .as_str()
                .to_owned(),
            resource_key: document_id.to_owned(),
            logical_path: "source.md".to_owned(),
        })?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload["contentBase64"].as_str().unwrap_or_default())
            .map_err(to_cli_error)?;
        return Ok(
            json!({ "document": { "id": document_id }, "markdown": String::from_utf8(bytes).map_err(to_cli_error)?, "staged": true }),
        );
    }
    crate::content::require_broker_for_task_execution()?;
    let wiki = get_wiki_bootstrap(paths)?;
    let document = find_document(&wiki.state, document_id)?.clone();
    let panel_dir = Storage::open(paths)?.panel_dir(&wiki.project.id, &wiki.panel.id);
    let (original_path, original_access) = raw_original_access(&panel_dir, &document);
    let (markdown_path, markdown_access) = materialize_raw_markdown(paths, &wiki, &document);
    let markdown = markdown_path
        .as_ref()
        .map(fs::read_to_string)
        .transpose()
        .map_err(to_cli_error)?
        .unwrap_or_default();
    Ok(json!({
        "document": document,
        "markdown": markdown,
        "originalFilePath": original_path,
        "markdownFilePath": markdown_path,
        "originalAccess": original_access,
        "markdownAccess": markdown_access,
    }))
}

pub fn write_markdown(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
    content: &str,
    task_id: Option<&str>,
) -> Result<Value, CliError> {
    if task_id.is_some() && crate::content::broker_execution_available() {
        return crate::content::broker_stage_file(&crate::content::StageFileRequest {
            resource_kind: crate::content::ResourceKind::WikiMarkdown
                .as_str()
                .to_owned(),
            resource_key: document_id.to_owned(),
            logical_path: "source.md".to_owned(),
            content_base64: base64::engine::general_purpose::STANDARD.encode(content.as_bytes()),
            mime_type: "text/markdown".to_owned(),
            metadata: json!({ "documentId": document_id }),
        });
    }
    crate::content::require_broker_for_task_execution()?;
    if let Some(task_id) = task_id {
        crate::tasks::verify_task_write_access(paths, task_id)?;
    }
    let mut wiki = match task_id {
        Some(task_id) => get_wiki_task_target(paths, task_id)?,
        None => get_wiki_bootstrap(paths)?,
    };
    let storage = Storage::open(paths)?;
    let now = now_iso();
    let parent_task = task_id
        .map(|id| task_value(&wiki.tasks, id).cloned())
        .transpose()?;
    let should_queue_ingest = parent_task
        .as_ref()
        .and_then(|task| task.get("type").and_then(Value::as_str))
        != Some("convert_document_to_markdown");
    let wiki_space_id = resolve_wiki_space(
        &wiki.state,
        parent_task
            .as_ref()
            .and_then(|task| task.get("wikiSpaceId").and_then(Value::as_str)),
    )?
    .id;
    let document = find_document_mut(&mut wiki.state, document_id)?;
    let markdown_ref = document
        .get("markdownRef")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| wiki_ref(&["raw", document_id, "source.md"]));
    let path = wiki_panel_path(
        &storage.panel_dir(&wiki.project.id, &wiki.panel.id),
        &markdown_ref,
    )?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(path, content).map_err(to_cli_error)?;
    if task_id.is_none() {
        crate::content::commit_immediate_text(
            paths,
            &wiki.project.id,
            Some(&wiki.panel.id),
            crate::content::ResourceKind::WikiMarkdown,
            document_id,
            "source.md",
            content.as_bytes(),
            "text/markdown",
            true,
        )?;
    }
    let version = document
        .get("markdownVersion")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        + 1;
    document["markdownRef"] = json!(markdown_ref);
    document["markdownVersion"] = json!(version);
    document["wordCount"] = json!(character_count(content));
    document["updatedAt"] = json!(now);
    let conversion_status = if document
        .get("conversion")
        .and_then(|conversion| conversion.get("status"))
        .and_then(Value::as_str)
        == Some("not_required")
    {
        "not_required"
    } else {
        "ready"
    };
    document["conversion"]["status"] = json!(conversion_status);
    document["conversion"]["error"] = Value::Null;
    document["conversion"]["updatedAt"] = json!(now);
    let task = if should_queue_ingest {
        crate::tasks::supersede_tasks_for_changed_resource(paths, &wiki.project.id, document_id)?;
        for existing in &mut wiki.tasks {
            if existing.get("type").and_then(Value::as_str) == Some("ingest_markdown_into_wiki")
                && existing.get("documentId").and_then(Value::as_str) == Some(document_id)
                && matches!(
                    existing.get("status").and_then(Value::as_str),
                    Some("waiting" | "queued" | "failed" | "running" | "indexing")
                )
            {
                existing["status"] = json!("superseded");
                existing["error"] = json!({ "code": "content_conflict" });
                existing["updatedAt"] = json!(now);
            }
        }
        let mutation_key = wiki_mutation_key(&wiki.project.id, &wiki.panel.id, &wiki_space_id);
        let task = create_wiki_task(
            &wiki.state,
            &mut wiki.tasks,
            "ingest_markdown_into_wiki",
            document_id,
            Some(document_id),
            Some(version),
            Some(&wiki_space_id),
            Some(&mutation_key),
        )?;
        let document = find_document_mut(&mut wiki.state, document_id)?;
        let ingestion = document
            .as_object_mut()
            .ok_or_else(|| CliError::new("Wiki raw document is invalid."))?
            .entry("ingestionByWikiSpace")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| CliError::new("Wiki raw document ingestion state is invalid."))?;
        ingestion.insert(
            wiki_space_id.clone(),
            create_ingestion_state(&task, version),
        );
        Some(task)
    } else {
        None
    };
    save_wiki_state(paths, &wiki)?;
    let document = find_document(&wiki.state, document_id)?.clone();
    Ok(json!({ "document": document, "task": task, "state": wiki.state }))
}

pub fn set_agent_skill(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
    rebuild_confirmed: bool,
) -> Result<Value, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    crate::agent::wiki_agent_skill_for_project(paths, &wiki.project.id, skill_id)?;
    let current_skill_id = selected_agent_skill_id(&wiki.state);
    if current_skill_id == skill_id {
        return Ok(json!({
            "agentSkillId": skill_id,
            "rebuildWorkflowRunId": null,
            "cancelledTaskIds": [],
            "queuedTaskIds": [],
            "rawDocumentCount": wiki.state.get("rawDocuments").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
            "state": wiki.state,
        }));
    }
    if !rebuild_confirmed {
        return Err(CliError::with_code(
            "wiki_rebuild_confirmation_required",
            "Changing the Wiki generation Skill requires confirmed full regeneration.",
        ));
    }

    let cancellable_task_ids = wiki
        .tasks
        .iter()
        .filter(|task| {
            matches!(
                task.get("type").and_then(Value::as_str),
                Some("ingest_markdown_into_wiki" | "maintain_wiki")
            ) && matches!(
                task.get("status").and_then(Value::as_str),
                Some(
                    "waiting"
                        | "queued"
                        | "failed"
                        | "reserved"
                        | "running"
                        | "claimed"
                        | "indexing"
                )
            )
        })
        .filter_map(|task| task.get("id").and_then(Value::as_str).map(str::to_owned))
        .collect::<Vec<_>>();
    let mut cancelled_task_ids = Vec::new();
    for task_id in cancellable_task_ids {
        if crate::tasks::cancel_task(paths, &task_id).is_ok() {
            cancelled_task_ids.push(task_id);
        }
    }

    let mut wiki = get_wiki_bootstrap(paths)?;
    let space = resolve_wiki_space(&wiki.state, None)?;
    let space_id = space.id.clone();
    let storage = Storage::open(paths)?;
    let pages_dir = storage
        .panel_dir(&wiki.project.id, &wiki.panel.id)
        .join("wikis")
        .join(sanitize_path_part(&space_id))
        .join("pages");
    if pages_dir.exists() {
        fs::remove_dir_all(&pages_dir).map_err(to_cli_error)?;
    }
    fs::create_dir_all(&pages_dir).map_err(to_cli_error)?;
    crate::content::archive_resource(
        paths,
        Some(&wiki.project.id),
        crate::content::ResourceKind::WikiSpace,
        &space_id,
    )?;

    let now = now_iso();
    if let Some(space) = state_array_mut(&mut wiki.state, "wikiSpaces")?
        .iter_mut()
        .find(|space| space.get("id").and_then(Value::as_str) == Some(space_id.as_str()))
    {
        space["pageIndex"] = json!([]);
        space["updatedAt"] = json!(now);
    }
    let state = state_object_mut(&mut wiki.state)?;
    state.insert("activeWikiPagePath".to_owned(), Value::Null);
    state.insert("wikiAgentSkillId".to_owned(), json!(skill_id));
    state.insert("wikiAgentSkillConfigured".to_owned(), json!(true));
    let workflow_run_id = create_id("workflow-run");
    let mutation_key = wiki_mutation_key(&wiki.project.id, &wiki.panel.id, &space_id);
    let mut documents = wiki
        .state
        .get("rawDocuments")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    documents.sort_by(|left, right| {
        left.get("createdAt")
            .and_then(Value::as_str)
            .unwrap_or("")
            .cmp(right.get("createdAt").and_then(Value::as_str).unwrap_or(""))
            .then_with(|| {
                left.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .cmp(right.get("id").and_then(Value::as_str).unwrap_or(""))
            })
    });
    let mut queued_task_ids = Vec::new();
    for document in &documents {
        let Some(document_id) = document.get("id").and_then(Value::as_str) else {
            continue;
        };
        let markdown_version = document
            .get("markdownVersion")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let markdown_ready = document
            .get("markdownRef")
            .and_then(Value::as_str)
            .is_some();
        let conversion_task_id = if markdown_ready {
            None
        } else {
            active_conversion_task_id(&wiki.tasks, document_id).or_else(|| {
                let mut conversion = create_wiki_task(
                    &wiki.state,
                    &mut wiki.tasks,
                    "convert_document_to_markdown",
                    document_id,
                    Some(document_id),
                    Some(markdown_version),
                    Some(&space_id),
                    None,
                )
                .ok()?;
                conversion["idempotencyKey"] =
                    json!(format!("rebuild-convert:{document_id}:{markdown_version}"));
                let conversion_id = conversion["id"].as_str()?.to_owned();
                if let Some(stored) = wiki
                    .tasks
                    .iter_mut()
                    .find(|task| task["id"] == conversion["id"])
                {
                    *stored = conversion;
                }
                Some(conversion_id)
            })
        };
        let mut ingest = create_wiki_task(
            &wiki.state,
            &mut wiki.tasks,
            "ingest_markdown_into_wiki",
            document_id,
            Some(document_id),
            Some(markdown_version),
            Some(&space_id),
            Some(&mutation_key),
        )?;
        ingest["workflowRunId"] = json!(workflow_run_id);
        ingest["idempotencyKey"] = json!(format!(
            "rebuild-ingest:{workflow_run_id}:{document_id}:{markdown_version}"
        ));
        if let Some(conversion_task_id) = conversion_task_id {
            ingest["status"] = json!("waiting");
            ingest["dependsOnTaskIds"] = json!([conversion_task_id]);
        }
        let ingest_id = ingest["id"].as_str().unwrap_or_default().to_owned();
        if let Some(stored) = wiki
            .tasks
            .iter_mut()
            .find(|task| task["id"] == ingest["id"])
        {
            *stored = ingest.clone();
        }
        queued_task_ids.push(ingest_id);
        let raw_document = find_document_mut(&mut wiki.state, document_id)?;
        let ingestion = raw_document
            .as_object_mut()
            .ok_or_else(|| CliError::new("Wiki raw document is invalid."))?
            .entry("ingestionByWikiSpace")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| CliError::new("Wiki raw document ingestion state is invalid."))?;
        ingestion.insert(
            space_id.clone(),
            create_ingestion_state(&ingest, markdown_version),
        );
    }
    save_wiki_state(paths, &wiki)?;
    storage.ensure_workflow_run(
        &wiki.project.id,
        &wiki.panel.id,
        &workflow_run_id,
        "wiki.rebuild",
        if queued_task_ids.is_empty() {
            "succeeded"
        } else {
            "active"
        },
        &json!({ "agentSkillId": skill_id, "rawDocumentCount": documents.len() }),
    )?;
    Ok(json!({
        "agentSkillId": skill_id,
        "rebuildWorkflowRunId": workflow_run_id,
        "cancelledTaskIds": cancelled_task_ids,
        "queuedTaskIds": queued_task_ids,
        "rawDocumentCount": documents.len(),
        "state": wiki.state,
    }))
}

pub fn selected_agent_skill_id(state: &Value) -> &str {
    state
        .get("wikiAgentSkillId")
        .and_then(Value::as_str)
        .filter(|skill_id| !skill_id.is_empty())
        .unwrap_or("karpathy-llm-wiki")
}

fn active_conversion_task_id(tasks: &[Value], document_id: &str) -> Option<String> {
    tasks
        .iter()
        .find(|task| {
            task.get("type").and_then(Value::as_str) == Some("convert_document_to_markdown")
                && task.get("documentId").and_then(Value::as_str) == Some(document_id)
                && matches!(
                    task.get("status").and_then(Value::as_str),
                    Some(
                        "waiting"
                            | "queued"
                            | "failed"
                            | "reserved"
                            | "running"
                            | "claimed"
                            | "converting"
                    )
                )
        })
        .and_then(|task| task.get("id").and_then(Value::as_str))
        .map(str::to_owned)
}
