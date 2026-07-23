#[test]
fn wiki_selection_and_query_context_are_agent_facing_without_panel_state_churn() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let wiki_space_id = active_wiki_space_id(&project_dir, &storage_dir);

    let (code, stdout, stderr) = run(&[
        "wiki",
        "raw",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--title",
        "Product brief",
        "--content",
        "# Product brief\n\nMyOpenPanels keeps project knowledge local.",
        "--space-id",
        wiki_space_id.as_str(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let document = serde_json::from_str::<Value>(&stdout).expect("json")["document"].clone();

    let page_file = project_dir.join("product.md");
    fs::write(
        &page_file,
        "# MyOpenPanels\n\nMyOpenPanels provides a local indexed Wiki for project knowledge.\n",
    )
    .expect("page file");
    let (code, _stdout, stderr) = run(&[
        "wiki",
        "page",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--space-id",
        wiki_space_id.as_str(),
        "--path",
        "concepts/myopenpanels.md",
        "--content-file",
        page_file.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");

    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let storage = Storage::open(&paths).expect("storage");
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let wiki_panel = bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");
    let revision_before = storage
        .read_panel_state_revision(&bootstrap.project.id, &wiki_panel.panel.id)
        .expect("panel revision");
    storage
        .write_panel_selection(
            &bootstrap.project.id,
            &wiki_panel.panel.id,
            &json!({
                "isWikiSelected": true,
                "selectedRawDocumentIds": [document["id"]],
                "selectedMyDocumentIds": [],
            }),
        )
        .expect("legacy selection");
    let revision_after = Storage::open(&paths)
        .expect("storage")
        .read_panel_state_revision(&bootstrap.project.id, &wiki_panel.panel.id)
        .expect("panel revision");
    assert_eq!(revision_after, revision_before);

    let (code, stdout, stderr) = run(&[
        "panel",
        "selection",
        "read",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let selection = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(document["id"].as_str().is_some());
    assert!(selection["value"]["selection"]
        .get("isWikiSelected")
        .is_none());
    assert!(selection["value"]["selection"]
        .get("selectedRawDocumentIds")
        .is_none());
    assert!(selection["value"].get("selectedRawDocuments").is_none());
    assert_eq!(
        selection["value"]["wiki"]["localAccess"]["status"],
        "on_demand"
    );
    assert!(selection["value"]["wiki"].get("loadAction").is_none());
    assert_action_parses(&selection["actions"]["suggested"][0]);
    assert!(selection["value"].get("actions").is_none());

    let (code, stdout, stderr) = run(&[
        "agent",
        "bootstrap",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let context = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(context["panel"]["selection"]["isExplicit"], false);
    assert_eq!(
        context["panel"]["selection"]["summary"]["myDocumentCount"],
        0
    );
    assert!(context["panel"]["selection"]["summary"]
        .get("rawDocumentCount")
        .is_none());
    assert!(context["panel"]["selection"]["summary"]
        .get("wikiSelected")
        .is_none());
    assert!(context.get("knowledgeContext").is_none());
    let loader = fs::read_to_string(
        context["skills"][0]["contextPath"]
            .as_str()
            .expect("Wiki loader path"),
    )
    .expect("Wiki loader");
    assert!(!loader.contains("selected raw document"));
    assert!(!loader.contains("wiki selected as context"));
    assert!(loader.contains("wiki local access: status=on_demand"));

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "read",
        "--skill-id",
        "myopenpanels-panels",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let skill = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(skill["markdown"]
        .as_str()
        .unwrap_or("")
        .contains("`wiki.page.search`"));

    let (code, stdout, stderr) = run(&[
        "wiki",
        "page",
        "search",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--space-id",
        wiki_space_id.as_str(),
        "--query",
        "local indexed Wiki",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let search = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(search["matches"][0]["path"], "concepts/myopenpanels.md");
}

#[test]
fn my_documents_support_versions_selection_publication_and_deletion() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let created = wiki::create_my_document(
        &paths,
        "report.md",
        Some("Report"),
        Some("text/markdown"),
        None,
        Some("thread:1"),
        b"# Report\n\nVersion one.",
    )
    .expect("create My Document");
    let document_id = created["document"]["id"]
        .as_str()
        .expect("document id")
        .to_owned();
    assert_eq!(created["document"]["contentVersion"], 1);
    assert_eq!(created["document"]["wordCount"], 18);
    assert!(wiki::create_my_document(
        &paths,
        "report.pdf",
        None,
        Some("application/pdf"),
        None,
        None,
        b"not a pdf",
    )
    .is_err());

    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let wiki_panel = bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");
    let revision_before = Storage::open(&paths)
        .expect("storage")
        .read_panel_state_revision(&bootstrap.project.id, &wiki_panel.panel.id)
        .expect("revision");
    let selection =
        wiki::write_agent_selection(&paths, &[document_id.clone()]).expect("selection");
    assert_eq!(
        selection["selectedMyDocuments"][0]["id"],
        document_id
    );
    assert_eq!(
        selection["selectedMyDocuments"][0]["contentAccess"]["status"],
        "ready"
    );
    assert!(selection["selectedMyDocuments"][0]["contentFilePath"]
        .as_str()
        .is_some_and(|path| Path::new(path).is_file()));
    assert_eq!(selection["wiki"]["localAccess"]["status"], "on_demand");
    assert_eq!(
        selection["wiki"]["localAccess"]["materializeAction"]["intent"],
        "wiki.space.materialize"
    );
    let revision_after = Storage::open(&paths)
        .expect("storage")
        .read_panel_state_revision(&bootstrap.project.id, &wiki_panel.panel.id)
        .expect("revision");
    assert_eq!(revision_after, revision_before);

    let read = wiki::read_my_document(&paths, &document_id).expect("read");
    assert_eq!(read["content"], "# Report\n\nVersion one.");
    let first_publish =
        wiki::publish_my_document(&paths, &document_id, None).expect("first publish");
    assert_eq!(first_publish["rawDocument"]["source"], "agent");
    assert!(wiki::publish_my_document(&paths, &document_id, None).is_err());

    let updated = wiki::write_my_document(
        &paths,
        &document_id,
        "report.md",
        Some("text/markdown"),
        b"# Report\n\nVersion two.",
    )
    .expect("update");
    assert_eq!(updated["document"]["contentVersion"], 2);
    assert_eq!(updated["document"]["wordCount"], 18);
    let second_publish =
        wiki::publish_my_document(&paths, &document_id, None).expect("second publish");
    assert_eq!(
        second_publish["document"]["publishHistory"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );

    wiki::delete_my_document(&paths, &document_id).expect("delete");
    let context = wiki::wiki_context(&paths).expect("context");
    assert_eq!(
        context["state"]["rawDocuments"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        wiki::read_agent_selection(&paths).expect("selection")["selectedMyDocuments"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
}

#[test]
fn wiki_document_file_names_can_be_renamed_without_changing_extensions() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let wiki_space_id = active_wiki_space_id(&project_dir, &storage_dir);

    let raw = wiki::add_raw_document(
        &paths,
        "draft.md",
        None,
        Some("text/markdown"),
        "user",
        Some(&wiki_space_id),
        b"# Draft",
    )
    .expect("raw document");
    let raw_id = raw["document"]["id"].as_str().expect("raw id");
    let renamed_raw =
        wiki::rename_raw_document(&paths, raw_id, "final.md").expect("rename raw document");
    assert_eq!(renamed_raw["document"]["originalFileName"], "final.md");
    assert_eq!(renamed_raw["document"]["title"], "final");
    assert!(wiki::raw_document_original(&paths, raw_id)
        .expect("raw original")
        .file_path
        .ends_with("final.md"));
    let renamed_raw = wiki::rename_raw_document(&paths, raw_id, "Final 版本 (v2).md")
        .expect("rename raw document with human-readable characters");
    assert_eq!(
        renamed_raw["document"]["originalFileName"],
        "Final 版本 (v2).md"
    );
    assert_eq!(renamed_raw["document"]["title"], "Final 版本 (v2)");

    let my_document = wiki::create_my_document(
        &paths,
        "my-document.md",
        None,
        Some("text/markdown"),
        None,
        None,
        b"# My Document",
    )
    .expect("My Document");
    let my_document_id = my_document["document"]["id"].as_str().expect("My Document id");
    let renamed_my_document =
        wiki::rename_my_document_file(&paths, my_document_id, "article.md")
            .expect("rename My Document");
    assert_eq!(
        renamed_my_document["document"]["originalFileName"],
        "article.md"
    );
    assert_eq!(
        wiki::read_my_document(&paths, my_document_id).expect("My Document content")["content"],
        "# My Document"
    );
    let renamed_my_document =
        wiki::rename_my_document_file(&paths, my_document_id, "Article 测试 [v2].md")
            .expect("rename My Document with human-readable characters");
    assert_eq!(
        renamed_my_document["document"]["originalFileName"],
        "Article 测试 [v2].md"
    );
    assert_eq!(
        renamed_my_document["document"]["title"],
        "Article 测试 [v2]"
    );

    wiki::write_page(
        &paths,
        &wiki_space_id,
        "notes/draft.md",
        "# Page",
        None,
        None,
    )
    .expect("write page");
    let renamed_page =
        wiki::rename_page(&paths, &wiki_space_id, "notes/draft.md", "notes/final.md")
            .expect("rename page");
    assert_eq!(renamed_page["pagePath"], "notes/final.md");
    assert_eq!(
        renamed_page["task"]["changeEvents"],
        json!([
            {
                "kind": "wiki_page_written",
                "path": "notes/draft.md",
                "operation": "created"
            },
            {
                "kind": "wiki_page_renamed",
                "fromPath": "notes/draft.md",
                "toPath": "notes/final.md"
            }
        ])
    );
    assert_eq!(
        wiki::read_page(&paths, &wiki_space_id, "notes/final.md").expect("renamed page")
            ["markdown"],
        "# Page"
    );
}

#[test]
fn wiki_document_titles_preserve_spaces_and_unicode() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let wiki_space_id = active_wiki_space_id(&project_dir, &storage_dir);

    let raw = wiki::add_raw_document(
        &paths,
        "Pixcall 基础概念.md",
        None,
        Some("text/markdown"),
        "user",
        Some(&wiki_space_id),
        b"# Pixcall",
    )
    .expect("raw document");
    assert_eq!(raw["document"]["title"], "Pixcall 基础概念");
    let raw_id = raw["document"]["id"].as_str().expect("raw id");
    let renamed_raw = wiki::rename_raw_document_title(
        &paths,
        raw_id,
        "  Pixcall 核心 概念  ",
    )
    .expect("renamed raw document");
    assert_eq!(renamed_raw["document"]["title"], "Pixcall 核心 概念");
    assert_eq!(
        renamed_raw["document"]["originalFileName"],
        raw["document"]["originalFileName"]
    );

    let my_document = wiki::create_my_document(
        &paths,
        "Pixcall 基础概念.md",
        None,
        Some("text/markdown"),
        None,
        None,
        b"# Pixcall",
    )
    .expect("My Document");
    assert_eq!(my_document["document"]["title"], "Pixcall 基础概念");
    let my_document_id = my_document["document"]["id"]
        .as_str()
        .expect("My Document id");
    let renamed_my_document =
        wiki::rename_my_document(&paths, my_document_id, "  Pixcall 核心 概念  ")
            .expect("renamed My Document");
    assert_eq!(
        renamed_my_document["document"]["title"],
        "Pixcall 核心 概念"
    );
}

#[test]
fn wiki_mdx_upload_skips_conversion_and_queues_ingest() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let wiki_space_id = active_wiki_space_id(&project_dir, &storage_dir);
    update_wiki_state_field(
        &storage_dir,
        "wikiAgentSkillId",
        json!("wiki-default"),
    );
    let mdx_path = project_dir.join("component.mdx");
    let mdx_content = "# Component\n\n<ComponentPreview name=\"Button\" />\n";
    fs::write(&mdx_path, mdx_content).expect("mdx file");

    let (code, stdout, stderr) = run(&[
        "wiki",
        "raw",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--source-file",
        mdx_path.to_str().unwrap(),
        "--mime-type",
        "application/octet-stream",
        "--space-id",
        wiki_space_id.as_str(),
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "{stderr}");
    let result = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(result["document"]["conversion"]["status"], "not_required");
    assert_eq!(result["document"]["markdownVersion"], 1);
    assert_eq!(
        result["document"]["wordCount"],
        mdx_content
            .chars()
            .filter(|character| !character.is_whitespace())
            .count()
    );
    assert_eq!(
        result["document"]["ingestionByWikiSpace"][wiki_space_id.as_str()]["status"],
        "queued"
    );
    assert!(result["state"].get("tasks").is_none());
    let (code, stdout, stderr) = run(&[
        "task",
        "list",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let tasks = serde_json::from_str::<Value>(&stdout).expect("tasks");
    assert_eq!(tasks["tasks"][0]["type"], "ingest_markdown_into_wiki");
    assert_eq!(
        tasks["tasks"][0]["source"]["agentSkillId"],
        "wiki-default"
    );
}

#[test]
fn agent_bridge_without_command_does_not_process_wiki_tasks() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let wiki_space_id = active_wiki_space_id(&project_dir, &storage_dir);

    let (code, stdout, stderr) = run(&[
        "wiki",
        "raw",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--title",
        "Bridge Source",
        "--file-name",
        "bridge-source.md",
        "--content",
        "# Bridge Source\n\nContent imported by the built-in worker.",
        "--space-id",
        wiki_space_id.as_str(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let created = serde_json::from_str::<Value>(&stdout).expect("json");
    let _task_id = created["document"]["ingestionByWikiSpace"][wiki_space_id.as_str()]["taskId"]
        .as_str()
        .unwrap();

    let (code, stdout, _stderr) = run(&[
        "agent",
        "bridge",
        "run",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--once",
        "--format",
        "json",
    ]);
    assert_eq!(code, 2);
    let bridge = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(bridge["error"]
        .as_str()
        .is_some_and(|message| message.contains("requires --command")));

    let (code, stdout, stderr) = run(&[
        "task",
        "list",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--pending",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let pending = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(pending["pendingCount"], 1);
    assert_eq!(pending["tasks"][0]["status"], "queued");

    let (code, stdout, stderr) = run(&[
        "agent",
        "bridge",
        "status",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let status = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(status["status"], "idle");
    assert_eq!(status["queue"]["unhandledCount"], 0);
}

#[test]
fn broker_page_writes_do_not_read_the_live_wiki_before_staging() {
    assert!(should_check_live_page_existence(None, false));
    assert!(should_check_live_page_existence(Some("task:test"), false));
    assert!(!should_check_live_page_existence(Some("task:test"), true));
}
