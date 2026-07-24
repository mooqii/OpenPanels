fn imported_my_document_format(file_name: &str, mime_type: Option<&str>) -> (&'static str, &'static str) {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "md" | "markdown" | "mdx")
        || mime_type.is_some_and(|value| value.contains("markdown"))
    {
        ("markdown", "text/markdown")
    } else {
        ("text", "text/plain")
    }
}

pub fn import_my_document(
    paths: &MyOpenPanelsPaths,
    file_name: &str,
    title: Option<&str>,
    mime_type: Option<&str>,
    content: &[u8],
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let now = now_iso();
    let document_id = create_id("my-document");
    let safe_source_file_name = sanitize_file_name(file_name);
    let source_mime_type = mime_type.unwrap_or_else(|| mime_type_for_file(file_name));
    let source_ref = format!("original/{safe_source_file_name}");
    crate::content::commit_immediate_file(
        paths,
        &wiki.project.id,
        Some(&wiki.panel.id),
        crate::content::ResourceKind::MyDocument,
        &document_id,
        &source_ref,
        content,
        source_mime_type,
        true,
    )?;

    let direct_text = if is_plain_text_file(file_name, mime_type) {
        std::str::from_utf8(content).ok()
    } else {
        None
    };
    let (format, normalized_mime_type) = direct_text
        .map(|_| imported_my_document_format(file_name, mime_type))
        .unwrap_or(("markdown", "text/markdown"));
    let extension = if format == "markdown" { "md" } else { "txt" };
    let output_file_name = format!(
        "{}.{}",
        sanitize_file_name(title_from_file_name(file_name)),
        extension
    );
    let content_ref = format!("content.{extension}");
    crate::content::commit_immediate_text(
        paths,
        &wiki.project.id,
        Some(&wiki.panel.id),
        crate::content::ResourceKind::MyDocument,
        &document_id,
        &content_ref,
        direct_text.map(str::as_bytes).unwrap_or_default(),
        normalized_mime_type,
        false,
    )?;

    let mut conversion_task = if direct_text.is_none() {
        let mut task = create_wiki_task(
            &wiki.state,
            &mut wiki.tasks,
            "convert_document_to_markdown",
            &document_id,
            Some(&document_id),
            Some(0),
            None,
            None,
        )?;
        task["documentKind"] = json!("my_document");
        task["idempotencyKey"] = json!(format!(
            "convert-my-document:{document_id}:{}",
            sha256_hex(content)
        ));
        if let Some(stored) = wiki
            .tasks
            .iter_mut()
            .find(|stored| stored.get("id") == task.get("id"))
        {
            *stored = task.clone();
        }
        Some(task)
    } else {
        None
    };
    let document = json!({
        "id": document_id,
        "title": title
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| title_from_file_name(file_name)),
        "originalFileName": output_file_name,
        "format": format,
        "mimeType": normalized_mime_type,
        "contentRef": content_ref,
        "contentVersion": if direct_text.is_some() { 1 } else { 0 },
        "taskId": null,
        "threadId": null,
        "publishHistory": [],
        "wordCount": direct_text.map(character_count),
        "importSource": {
            "fileName": safe_source_file_name,
            "mimeType": source_mime_type,
            "sizeBytes": content.len(),
            "sha256": sha256_hex(content),
            "originalRef": source_ref,
        },
        "conversion": {
            "status": if direct_text.is_some() { "not_required" } else { "queued" },
            "taskId": conversion_task.as_ref().map(|task| task["id"].clone()),
            "error": null,
            "updatedAt": now,
        },
        "createdAt": now,
        "updatedAt": now,
    });
    let tasks = conversion_task
        .as_ref()
        .map(|task| vec![task.clone()])
        .unwrap_or_default();
    crate::content::create_my_document_with_pending_content(
        paths,
        &wiki.project.id,
        &wiki.panel.id,
        &tasks,
        &document,
    )?;
    let state = Storage::open(paths)?
        .read_panel_state(&wiki.project.id, &wiki.panel.id)?
        .unwrap_or_else(|| json!({}));
    let mut payload = json!({ "document": document, "state": state });
    if let Some(task) = conversion_task.take() {
        payload["task"] = task;
    }
    Ok(payload)
}

pub fn my_document_import_original(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
) -> Result<WikiOriginalFile, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    my_document_import_original_from_wiki(paths, wiki, document_id)
}

pub fn reveal_my_document_import_original(
    paths: &MyOpenPanelsPaths,
    document_id: &str,
) -> Result<Value, CliError> {
    reveal_wiki_original(my_document_import_original(paths, document_id)?)
}

pub fn my_document_import_original_for_target(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: &str,
    document_id: &str,
) -> Result<WikiOriginalFile, CliError> {
    let wiki = get_wiki_target(paths, project_id, panel_id)?;
    my_document_import_original_from_wiki(paths, wiki, document_id)
}

fn my_document_import_original_from_wiki(
    paths: &MyOpenPanelsPaths,
    wiki: WikiBootstrapValue,
    document_id: &str,
) -> Result<WikiOriginalFile, CliError> {
    let document = find_my_document(&wiki.state, document_id)?.clone();
    let import_source = document
        .get("importSource")
        .ok_or_else(|| CliError::new("My Document has no imported source."))?;
    let original_ref = import_source
        .get("originalRef")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::new("My Document imported source is missing."))?;
    let file_path = crate::content::active_file_path(
        paths,
        &wiki.project.id,
        crate::content::ResourceKind::MyDocument,
        document_id,
        original_ref,
    )?
    .ok_or_else(|| CliError::new("My Document imported source is unavailable."))?;
    let metadata = fs::metadata(&file_path).map_err(to_cli_error)?;
    if !metadata.is_file() {
        return Err(CliError::new("My Document imported source is unavailable."));
    }
    let mime_type = import_source
        .get("mimeType")
        .and_then(Value::as_str)
        .unwrap_or("application/octet-stream")
        .to_owned();
    Ok(WikiOriginalFile {
        document,
        file_path,
        mime_type,
        size_bytes: metadata.len(),
    })
}

#[cfg(test)]
mod my_document_import_tests {
    use super::*;
    use crate::control::{ensure_project_bootstrap, BootstrapRequest};
    use crate::paths::resolve_myopenpanels_paths;

    fn test_paths(name: &str) -> (tempfile::TempDir, MyOpenPanelsPaths) {
        let temp = tempfile::tempdir().expect("temp");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some(name),
        )
        .expect("paths");
        ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        (temp, paths)
    }

    #[test]
    fn imports_utf8_text_without_creating_a_wiki_task() {
        let (_temp, paths) = test_paths("generated-text-import");
        let imported = import_my_document(
            &paths,
            "notes.txt",
            Some("Notes"),
            Some("text/plain"),
            b"Imported notes",
        )
        .expect("import");
        let document_id = imported["document"]["id"].as_str().expect("document id");

        assert_eq!(imported["document"]["conversion"]["status"], "not_required");
        assert_eq!(imported["document"]["contentVersion"], 1);
        assert!(imported.get("task").is_none());
        assert_eq!(
            read_my_document(&paths, document_id).expect("read")["content"],
            "Imported notes"
        );
        let context = wiki_context(&paths).expect("context");
        assert_eq!(context["state"]["rawDocuments"], json!([]));
        assert_eq!(context["state"]["wikiSpaces"][0]["pageIndex"], json!([]));
    }

    #[test]
    fn stale_wiki_projection_cannot_delete_a_new_my_document() {
        let (_temp, paths) = test_paths("stale-wiki-my-document");
        let before = get_wiki_bootstrap(&paths).expect("initial Wiki");
        let created = create_my_document(
            &paths,
            "notes.md",
            Some("Notes"),
            Some("text/markdown"),
            None,
            None,
            b"# Notes",
        )
        .expect("create");
        let document_id = created["document"]["id"].as_str().expect("document id");

        Storage::open(&paths)
            .expect("storage")
            .write_panel_state(&before.project.id, &before.panel.id, &before.state)
            .expect("stale Wiki write");

        assert_eq!(
            read_my_document(&paths, document_id).expect("preserved document")["content"],
            "# Notes"
        );
    }

    #[test]
    fn imports_binary_as_a_my_document_conversion_task() {
        let (_temp, paths) = test_paths("generated-binary-import");
        let imported = import_my_document(
            &paths,
            "brief.pdf",
            None,
            Some("application/pdf"),
            b"%PDF binary fixture",
        )
        .expect("import");
        let document_id = imported["document"]["id"].as_str().expect("document id");
        let task = &imported["task"];

        assert_eq!(imported["document"]["conversion"]["status"], "queued");
        assert_eq!(imported["document"]["contentVersion"], 0);
        assert_eq!(task["type"], "convert_document_to_markdown");
        assert_eq!(task["documentKind"], "my_document");
        assert_eq!(task["documentId"], document_id);
        let original = my_document_import_original(&paths, document_id).expect("original");
        assert_eq!(
            fs::read_to_string(original.file_path).expect("source"),
            "%PDF binary fixture"
        );
        let context = wiki_context(&paths).expect("context");
        assert_eq!(context["state"]["rawDocuments"], json!([]));
        assert_eq!(context["state"]["wikiSpaces"][0]["pageIndex"], json!([]));
    }

    #[test]
    fn failed_import_conversion_can_retry_and_delete_cancels_it() {
        let (_temp, paths) = test_paths("generated-import-retry-delete");
        let imported = import_my_document(
            &paths,
            "brief.pdf",
            None,
            Some("application/pdf"),
            b"binary fixture",
        )
        .expect("import");
        let document_id = imported["document"]["id"].as_str().expect("document id");
        let task_id = imported["task"]["id"].as_str().expect("task id");
        let original_path = my_document_import_original(&paths, document_id)
            .expect("original")
            .file_path;

        let _broker = crate::content::enable_test_task_broker();
        let claim =
            crate::tasks::claim_task(&paths, task_id, "agent-cli:codex").expect("claim");
        crate::tasks::fail_task_with_class(
            &paths,
            task_id,
            claim["leaseToken"].as_str().expect("lease"),
            "conversion failed",
            None,
            crate::tasks::TaskFailureClass::TerminalTask,
        )
        .expect("fail");
        let failed = wiki_context(&paths).expect("failed context");
        assert_eq!(
            failed["state"]["myDocuments"][0]["conversion"]["status"],
            "failed"
        );
        let retry = crate::tasks::retry_task(&paths, task_id).expect("retry");
        let retry_task_id = retry["task"]["id"].as_str().expect("retry task id");
        let retried = wiki_context(&paths).expect("retried context");
        assert_eq!(
            retried["state"]["myDocuments"][0]["conversion"]["status"],
            "queued"
        );

        delete_my_document(&paths, document_id).expect("delete");
        let bootstrap = get_wiki_bootstrap(&paths).expect("bootstrap");
        assert!(original_path.exists());
        assert!(crate::content::active_file_path(
            &paths,
            &bootstrap.project.id,
            crate::content::ResourceKind::MyDocument,
            document_id,
            imported["document"]["importSource"]["originalRef"]
                .as_str()
                .expect("original ref"),
        )
        .expect("archived resource")
        .is_none());
        let task = bootstrap
            .tasks
            .iter()
            .find(|task| task["id"] == retry_task_id)
            .expect("task");
        assert_eq!(task["status"], "cancelled");
        let original = bootstrap
            .tasks
            .iter()
            .find(|task| task["id"] == task_id)
            .expect("original task");
        assert_eq!(original["status"], "failed");
        assert_eq!(bootstrap.state["myDocuments"], json!([]));
    }
}
