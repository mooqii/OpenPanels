pub fn add_raw_document(
    paths: &MyOpenPanelsPaths,
    file_name: &str,
    title: Option<&str>,
    mime_type: Option<&str>,
    source: &str,
    wiki_space_id: Option<&str>,
    content: &[u8],
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let document = prepare_raw_document(
        paths,
        &mut wiki,
        file_name,
        title,
        mime_type,
        source,
        wiki_space_id,
        content,
    )?;
    save_wiki_state_if_revision(paths, &wiki)?;
    trace::record_simple(
        "task",
        "wiki",
        Some("document"),
        format!(
            "Imported {}",
            document["title"].as_str().unwrap_or("document")
        ),
        Some(format!(
            "Imported {}",
            document["title"].as_str().unwrap_or("document")
        )),
        Some(json!({ "document": document.clone() })),
    );
    Ok(json!({ "document": document, "state": wiki.state }))
}

#[allow(clippy::too_many_arguments)]
fn prepare_raw_document(
    paths: &MyOpenPanelsPaths,
    wiki: &mut WikiBootstrapValue,
    file_name: &str,
    title: Option<&str>,
    mime_type: Option<&str>,
    source: &str,
    wiki_space_id: Option<&str>,
    content: &[u8],
) -> Result<Value, CliError> {
    let now = now_iso();
    let wiki_space = resolve_wiki_space(&wiki.state, wiki_space_id)?;
    let mutation_key = wiki_mutation_key(&wiki.project.id);
    let safe_file_name = sanitize_file_name(file_name);
    let document_id = create_id("raw");
    let original_ref = format!("original/{safe_file_name}");
    let markdown_ref = "source.md".to_owned();
    let is_text = is_plain_text_file(&safe_file_name, mime_type);
    let word_count = if is_text {
        crate::content::validate_immediate_text_content(content)?;
        std::str::from_utf8(content).ok().map(character_count)
    } else {
        None
    };
    let original_mime_type = mime_type.unwrap_or_else(|| mime_type_for_file(file_name));
    let mut files = vec![crate::content::ImmediateFile {
        logical_path: &original_ref,
        content,
        mime_type: original_mime_type,
    }];
    if is_text {
        files.push(crate::content::ImmediateFile {
            logical_path: &markdown_ref,
            content,
            mime_type: "text/markdown",
        });
    }
    crate::content::commit_immediate_files(
        paths,
        &wiki.project.id,
        Some(&wiki.panel.id),
        crate::content::ResourceKind::WikiMarkdown,
        &document_id,
        &files,
        true,
    )?;

    let content_hash = sha256_hex(content);
    let mut conversion_task = if is_text {
        None
    } else {
        Some(create_wiki_task(
            &wiki.state,
            &mut wiki.tasks,
            "convert_document_to_markdown",
            &document_id,
            Some(&document_id),
            Some(0),
            Some(wiki_space.id.as_str()),
            None,
        )?)
    };
    if let Some(task) = conversion_task.as_mut() {
        task["idempotencyKey"] = json!(format!("convert:{document_id}:{content_hash}"));
        if let Some(stored) = wiki
            .tasks
            .iter_mut()
            .find(|stored| stored["id"] == task["id"])
        {
            stored["idempotencyKey"] = task["idempotencyKey"].clone();
        }
    }
    let mut ingest_task = if is_text {
        Some(create_wiki_task(
            &wiki.state,
            &mut wiki.tasks,
            "ingest_markdown_into_wiki",
            &document_id,
            Some(&document_id),
            Some(1),
            Some(wiki_space.id.as_str()),
            Some(&mutation_key),
        )?)
    } else {
        let mut task = create_wiki_task(
            &wiki.state,
            &mut wiki.tasks,
            "ingest_markdown_into_wiki",
            &document_id,
            Some(&document_id),
            Some(1),
            Some(wiki_space.id.as_str()),
            Some(&mutation_key),
        )?;
        let conversion_id = conversion_task
            .as_ref()
            .and_then(|value| value["id"].as_str())
            .unwrap_or_default();
        task["status"] = json!("queued");
        task["dependsOnTaskIds"] = json!([conversion_id]);
        task["idempotencyKey"] = json!(format!("ingest:{document_id}:1"));
        if let Some(stored) = wiki
            .tasks
            .iter_mut()
            .find(|stored| stored["id"] == task["id"])
        {
            *stored = task.clone();
        }
        Some(task)
    };
    if let Some(task) = ingest_task.as_mut() {
        task["idempotencyKey"] = json!(format!("ingest:{document_id}:1"));
        if let Some(stored) = wiki
            .tasks
            .iter_mut()
            .find(|stored| stored["id"] == task["id"])
        {
            stored["idempotencyKey"] = task["idempotencyKey"].clone();
        }
    }
    let mut ingestion = Map::new();
    ingestion.insert(
        wiki_space.id.clone(),
        if let Some(task) = &ingest_task {
            json!({
                "status": task.get("status").and_then(Value::as_str).unwrap_or("queued"),
                "taskId": task["id"],
                "markdownVersion": 1,
                "error": null,
                "updatedAt": task["updatedAt"],
            })
        } else {
            json!({
                "status": "waiting",
                "taskId": ingest_task.as_ref().map(|task| task["id"].clone()),
                "markdownVersion": 1,
                "error": null,
                "updatedAt": now,
            })
        },
    );
    let document = json!({
        "id": document_id,
        "title": title
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| title_from_file_name(file_name)),
        "originalFileName": safe_file_name,
        "mimeType": mime_type.unwrap_or_else(|| mime_type_for_file(file_name)),
        "sizeBytes": content.len(),
        "sha256": content_hash,
        "source": if source == "agent" { "agent" } else { "user" },
        "originalRef": original_ref,
        "markdownRef": if is_text { Value::String(markdown_ref.clone()) } else { Value::Null },
        "markdownVersion": if is_text { 1 } else { 0 },
        "wordCount": word_count,
        "conversion": {
            "status": if is_text { "not_required" } else { "queued" },
            "taskId": conversion_task.as_ref().map(|task| task["id"].clone()),
            "error": null,
            "updatedAt": now,
        },
        "ingestionByWikiSpace": Value::Object(ingestion),
        "createdAt": now,
        "updatedAt": now,
    });
    state_array_mut(&mut wiki.state, "rawDocuments")?.insert(0, document.clone());
    state_object_mut(&mut wiki.state)?
        .insert("activeRawDocumentId".to_owned(), document["id"].clone());
    Ok(document)
}

pub struct WikiOriginalFile {
    pub document: Value,
    pub file_path: PathBuf,
    pub mime_type: String,
    pub size_bytes: u64,
}

pub fn raw_document_original(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
) -> Result<WikiOriginalFile, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    let document = find_document(&wiki.state, document_id)?.clone();
    let original_ref = document
        .get("originalRef")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CliError::new(format!(
                "Wiki raw document has no originalRef: {document_id}"
            ))
        })?;
    let file_path = crate::content::active_file_path(
        paths,
        &wiki.project.id,
        crate::content::ResourceKind::WikiMarkdown,
        document_id,
        original_ref,
    )?
    .ok_or_else(|| CliError::new(format!("Wiki raw document original is unavailable: {document_id}")))?;
    let metadata = fs::metadata(&file_path).map_err(to_cli_error)?;
    if !metadata.is_file() {
        return Err(CliError::new(format!(
            "Wiki raw document original is not a file: {document_id}"
        )));
    }
    let mime_type = document
        .get("mimeType")
        .and_then(Value::as_str)
        .unwrap_or_else(|| mime_type_for_file(file_path.to_string_lossy().as_ref()))
        .to_owned();
    Ok(WikiOriginalFile {
        document,
        file_path,
        mime_type,
        size_bytes: metadata.len(),
    })
}

pub fn reveal_raw_document_original(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
) -> Result<Value, CliError> {
    let original = raw_document_original(paths, document_id)?;
    reveal_wiki_original(original)
}

fn reveal_wiki_original(original: WikiOriginalFile) -> Result<Value, CliError> {
    let command = if cfg!(target_os = "macos") {
        Some((
            "open",
            vec!["-R".to_owned(), original.file_path.display().to_string()],
        ))
    } else if cfg!(target_os = "windows") {
        Some((
            "explorer.exe",
            vec![format!("/select,{}", original.file_path.display())],
        ))
    } else if cfg!(target_os = "linux") {
        original
            .file_path
            .parent()
            .map(|parent| ("xdg-open", vec![parent.display().to_string()]))
    } else {
        None
    };
    if let Some((program, args)) = command {
        let _ = Command::new(program).args(args).spawn();
    }
    Ok(json!({
        "document": original.document,
        "filePath": original.file_path,
        "revealed": true,
    }))
}

pub fn delete_raw_document(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
    wiki_space_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let wiki_space = resolve_wiki_space(&wiki.state, wiki_space_id)?;
    let documents = state_array_mut(&mut wiki.state, "rawDocuments")?;
    let index = documents
        .iter()
        .position(|document| document.get("id").and_then(Value::as_str) == Some(document_id))
        .ok_or_else(|| CliError::new(format!("Wiki raw document not found: {document_id}")))?;
    let document = documents.remove(index);
    let mutation_key = wiki_mutation_key(&wiki.project.id);
    let task = create_wiki_maintenance_task(
        &wiki.state,
        &mut wiki.tasks,
        Some(wiki_space.id.as_str()),
        &mutation_key,
        json!({
            "kind": "raw_document_deleted",
            "documentId": document_id,
            "title": document.get("title").cloned().unwrap_or(Value::Null),
        }),
    )?;
    if wiki
        .state
        .get("activeRawDocumentId")
        .and_then(Value::as_str)
        == Some(document_id)
    {
        let next_id = wiki
            .state
            .get("rawDocuments")
            .and_then(Value::as_array)
            .and_then(|documents| documents.first())
            .and_then(|document| document.get("id"))
            .cloned()
            .unwrap_or(Value::Null);
        state_object_mut(&mut wiki.state)?.insert("activeRawDocumentId".to_owned(), next_id);
    }
    save_wiki_state(paths, &wiki)?;
    crate::content::archive_resource(
        paths,
        Some(&wiki.project.id),
        crate::content::ResourceKind::WikiMarkdown,
        document_id,
    )?;
    Ok(json!({ "document": document, "task": task, "state": wiki.state }))
}

pub fn rename_raw_document(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
    file_name: &str,
) -> Result<Value, CliError> {
    let safe_file_name = sanitize_file_name(file_name);
    if safe_file_name.is_empty() {
        return Err(CliError::new("Raw document file name cannot be empty."));
    }
    let mut wiki = get_wiki_bootstrap(paths)?;
    let existing = find_document(&wiki.state, document_id)?.clone();
    let old_ref = existing["originalRef"]
        .as_str()
        .ok_or_else(|| CliError::new("Wiki raw document originalRef is missing."))?;
    let new_ref = format!("original/{safe_file_name}");
    if old_ref != new_ref {
        crate::content::rename_active_file(
            paths,
            &wiki.project.id,
            crate::content::ResourceKind::WikiMarkdown,
            document_id,
            old_ref,
            &new_ref,
        )?
        .ok_or_else(|| CliError::new("Wiki raw document content is unavailable."))?;
    }
    let now = now_iso();
    let document = find_document_mut(&mut wiki.state, document_id)?;
    document["originalFileName"] = json!(safe_file_name);
    document["originalRef"] = json!(new_ref);
    document["title"] = json!(title_from_file_name(file_name));
    document["updatedAt"] = json!(now);
    let document = document.clone();
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "document": document, "state": wiki.state }))
}

pub fn rename_raw_document_title(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
    title: &str,
) -> Result<Value, CliError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(CliError::with_code(
            "invalid_raw_document",
            "Raw document title cannot be empty.",
        ));
    }
    let mut wiki = get_wiki_bootstrap(paths)?;
    let now = now_iso();
    let document = find_document_mut(&mut wiki.state, document_id)?;
    document["title"] = json!(title);
    document["updatedAt"] = json!(now);
    let document = document.clone();
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "document": document, "state": wiki.state }))
}

pub fn extract_raw_document_markdown(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
    wiki_space_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let _ = find_document(&wiki.state, document_id)?;
    let wiki_space = resolve_wiki_space(&wiki.state, wiki_space_id)?;
    let now = now_iso();
    for task in &mut wiki.tasks {
        let active_status = matches!(
            task.get("status").and_then(Value::as_str),
            Some("queued" | "running" | "failed")
        );
        if task.get("documentId").and_then(Value::as_str) == Some(document_id)
            && task.get("type").and_then(Value::as_str) == Some("convert_document_to_markdown")
            && active_status
        {
            task["status"] = json!("superseded");
            task["error"] = json!("Superseded by a new extraction request");
            task["updatedAt"] = json!(now);
        }
    }
    let markdown_version = find_document(&wiki.state, document_id)?
        .get("markdownVersion")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let task = create_wiki_task(
        &wiki.state,
        &mut wiki.tasks,
        "convert_document_to_markdown",
        document_id,
        Some(document_id),
        Some(markdown_version),
        Some(wiki_space.id.as_str()),
        None,
    )?;
    let document = find_document_mut(&mut wiki.state, document_id)?;
    document["conversion"] = json!({
        "status": "queued",
        "taskId": task["id"],
        "error": null,
        "updatedAt": task["updatedAt"],
    });
    document["updatedAt"] = json!(now_iso());
    let document = document.clone();
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "document": document, "task": task, "state": wiki.state }))
}

pub fn reindex_raw_document(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
    wiki_space_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let wiki_space = resolve_wiki_space(&wiki.state, wiki_space_id)?;
    let document = find_document(&wiki.state, document_id)?;
    if document
        .get("markdownRef")
        .and_then(Value::as_str)
        .is_none()
    {
        return Err(CliError::new(
            "Source Markdown is required before indexing.",
        ));
    }
    let markdown_version = document
        .get("markdownVersion")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let mutation_key = wiki_mutation_key(&wiki.project.id);
    let task = create_wiki_task(
        &wiki.state,
        &mut wiki.tasks,
        "ingest_markdown_into_wiki",
        document_id,
        Some(document_id),
        Some(markdown_version),
        Some(wiki_space.id.as_str()),
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
        wiki_space.id.clone(),
        json!({
            "status": "queued",
            "taskId": task["id"],
            "markdownVersion": markdown_version,
            "error": null,
            "updatedAt": task["updatedAt"],
        }),
    );
    document["updatedAt"] = task["updatedAt"].clone();
    let document = document.clone();
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "document": document, "task": task, "state": wiki.state }))
}
