    #[test]
    fn conversion_prompt_is_compact_and_materializes_the_binary_original() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("bridge-conversion-test"),
        )
        .expect("paths");
        let bootstrap = crate::control::ensure_project_bootstrap(
            &paths,
            crate::control::BootstrapRequest::new(),
        )
        .expect("bootstrap");
        let wiki_space_id = active_wiki_space_id(&paths);
        let original_bytes = b"PROMPT-SECRET-BINARY\0\xff";
        crate::wiki::add_raw_document(
            &paths,
            "unsafe name.pdf",
            Some("Conversion source"),
            Some("application/pdf"),
            "user",
            Some(&wiki_space_id),
            original_bytes,
        )
        .expect("raw document");
        let mut task = crate::storage::Storage::open(&paths)
            .expect("storage")
            .list_tasks(&bootstrap.project.id)
            .expect("tasks")
            .into_iter()
            .find(|task| task["type"] == "convert_document_to_markdown")
            .expect("conversion task");
        task["executionGeneration"] = json!(9);
        task["leaseOwner"] = json!("target:noise");
        let workspace = temp.path().join("execution");
        fs::create_dir_all(&workspace).expect("workspace");
        let materialized = materialize_task_inputs(&paths, &task, &workspace).expect("inputs");
        let prompt =
            document_conversion_task_prompt(&paths, &materialized, &workspace).expect("prompt");

        let original_path = PathBuf::from(
            materialized
                .pointer("/executionInputs/originalDocument/filePath")
                .and_then(Value::as_str)
                .expect("original path"),
        );
        assert!(original_path.is_absolute());
        assert!(original_path.starts_with(workspace.join("inputs/original")));
        assert_eq!(
            fs::read(&original_path).expect("original bytes"),
            original_bytes
        );
        assert!(workspace.join("task-context.json").is_file());
        assert!(workspace.join("outputs").is_dir());
        assert!(prompt.contains(original_path.to_str().unwrap()));
        assert!(!prompt.contains("wiki raw update"));
        assert!(!prompt.contains("schemaVersion"));
        assert!(prompt.contains("outputs/source.md"));
        assert!(!prompt.contains("PROMPT-SECRET-BINARY"));
        assert!(!prompt.contains("workflow:noise"));
        assert!(!prompt.contains("executionGeneration"));
        assert!(!prompt.contains("target:noise"));
        assert!(!prompt.contains("Task JSON"));
        assert!(!prompt.contains("agent skill read --skill-id"));
        assert!(!prompt.contains("agent catalog --"));
        assert!(!prompt.contains("Authoring Skill"));
        assert!(prompt.len() <= MAX_AGENT_PROMPT_BYTES);

        let custom_command_input = serde_json::to_string_pretty(&materialized).expect("task json");
        assert!(custom_command_input.contains("executionInputs"));
        let serialized_task: Value =
            serde_json::from_str(&custom_command_input).expect("serialized task");
        assert_eq!(
            serialized_task
                .pointer("/executionInputs/originalDocument/filePath")
                .and_then(Value::as_str),
            original_path.to_str()
        );
    }

    #[test]
    fn automatic_agent_uses_the_runtime_finalizer_for_conversion() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("automatic-runtime-finalizer"),
        )
        .expect("paths");
        let bootstrap = crate::control::ensure_project_bootstrap(
            &paths,
            crate::control::BootstrapRequest::new(),
        )
        .expect("bootstrap");
        let wiki_space_id = active_wiki_space_id(&paths);
        let uploaded = crate::wiki::add_raw_document(
            &paths,
            "automatic.pdf",
            Some("Automatic conversion"),
            Some("application/pdf"),
            "user",
            Some(&wiki_space_id),
            b"binary fixture",
        )
        .expect("raw document");
        let document_id = uploaded["document"]["id"].as_str().unwrap();
        let task = crate::storage::Storage::open(&paths)
            .expect("storage")
            .list_tasks(&bootstrap.project.id)
            .expect("tasks")
            .into_iter()
            .find(|task| task["type"] == "convert_document_to_markdown")
            .expect("conversion task");
        let target = crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "automatic-runtime-finalizer",
                host: Some("test"),
                project_id: Some(&bootstrap.project.id),
                capabilities: vec!["wiki.convertDocument".to_owned()],
                priority: 0,
                max_concurrency: 1,
                model_gateway_connection_id: None,
            },
        )
        .expect("target");
        let _broker = crate::content::enable_test_task_broker();
        let claim = crate::tasks::claim_task(
            &paths,
            task["id"].as_str().unwrap(),
            target["target"]["id"].as_str().unwrap(),
        )
        .expect("claim");
        #[cfg(unix)]
        let command = r#"printf '# Automatic Runtime\n' > outputs/source.md && printf '%s' '{"outcome":"converted","summary":"Converted automatically.","artifacts":[{"role":"source-markdown","relativePath":"outputs/source.md"}]}' > execution-result.json"#.to_owned();
        #[cfg(windows)]
        let command = {
            use base64::Engine as _;

            let script = r##"[IO.File]::WriteAllText('outputs/source.md', "# Automatic Runtime`n"); [IO.File]::WriteAllText('execution-result.json', '{"outcome":"converted","summary":"Converted automatically.","artifacts":[{"role":"source-markdown","relativePath":"outputs/source.md"}]}')"##;
            let utf16 = script
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            let encoded = base64::engine::general_purpose::STANDARD.encode(utf16);
            format!("powershell.exe -NoProfile -NonInteractive -EncodedCommand {encoded}")
        };
        let result = run_task_command(
            &paths,
            &command,
            10_000,
            &claim["task"],
            target["target"]["id"].as_str().unwrap(),
            claim["leaseToken"].as_str().unwrap(),
            true,
            claim["attemptId"].as_str(),
            claim["executionGeneration"].as_i64(),
            claim["taskBrokerUrl"].as_str(),
            claim["executionToken"].as_str(),
            None,
        )
        .expect("automatic execution");
        assert_eq!(result["success"], true, "{result}");
        assert_eq!(result["runtimeFinalized"], true);
        assert_eq!(result["runtimeFinalization"]["status"], "succeeded");
        assert_eq!(
            result["runtimeFinalization"]["finalizationState"]["phase"],
            "completed"
        );
        assert_eq!(
            result["runtimeFinalization"]["result"]["runtimeFinalization"]["phase"],
            "completed"
        );
        let markdown = crate::wiki::read_markdown(&paths, document_id).unwrap();
        assert_eq!(
            markdown["markdown"].as_str().unwrap().replace("\r\n", "\n"),
            "# Automatic Runtime\n"
        );
        let storage = crate::storage::Storage::open(&paths).expect("storage");
        let tasks = storage
            .list_tasks(&bootstrap.project.id)
            .expect("tasks after conversion");
        let ingestion_tasks = tasks
            .iter()
            .filter(|candidate| {
                candidate["type"] == "ingest_markdown_into_wiki"
                    && candidate["input"]["documentId"] == document_id
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ingestion_tasks.len(),
            1,
            "conversion must reuse the dependent ingestion Task"
        );
        assert_eq!(
            ingestion_tasks[0]["dependsOnTaskId"], task["id"],
            "claiming the conversion Task must not clear the ingestion dependency"
        );
        let state = storage
            .read_panel_state(&bootstrap.project.id, &bootstrap.panel.id)
            .expect("panel state")
            .expect("Wiki state");
        let ingestion =
            &state["rawDocuments"][0]["ingestionByWikiSpace"][wiki_space_id.as_str()];
        assert_eq!(ingestion["status"], "queued");
        assert_eq!(
            ingestion["taskId"], ingestion_tasks[0]["id"],
            "the document projection must reference a persisted Task"
        );
    }

    #[test]
    fn imported_document_conversion_stays_out_of_the_wiki() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("imported-document-conversion"),
        )
        .expect("paths");
        let bootstrap = crate::control::ensure_project_bootstrap(
            &paths,
            crate::control::BootstrapRequest::new(),
        )
        .expect("bootstrap");
        let imported = crate::wiki::import_my_document(
            &paths,
            "brief.pdf",
            Some("Brief"),
            Some("application/pdf"),
            b"binary fixture",
        )
        .expect("import");
        let document_id = imported["document"]["id"].as_str().unwrap();
        let task_id = imported["task"]["id"].as_str().unwrap();
        let task = crate::storage::Storage::open(&paths)
            .expect("storage")
            .list_tasks(&bootstrap.project.id)
            .expect("tasks")
            .into_iter()
            .find(|task| task["id"] == task_id)
            .expect("conversion task");
        assert_eq!(task["input"]["documentKind"], "my_document");

        let target = crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "imported-document-conversion",
                host: Some("test"),
                project_id: Some(&bootstrap.project.id),
                capabilities: vec!["wiki.convertDocument".to_owned()],
                priority: 0,
                max_concurrency: 1,
                model_gateway_connection_id: None,
            },
        )
        .expect("target");
        let _broker = crate::content::enable_test_task_broker();
        let claim = crate::tasks::claim_task(
            &paths,
            task_id,
            target["target"]["id"].as_str().unwrap(),
        )
        .expect("claim");
        #[cfg(unix)]
        let command = r#"printf '# Imported Document\n' > outputs/source.md && printf '%s' '{"outcome":"converted","summary":"Converted imported document.","artifacts":[{"role":"source-markdown","relativePath":"outputs/source.md"}]}' > execution-result.json"#.to_owned();
        #[cfg(windows)]
        let command = {
            use base64::Engine as _;

            let script = r##"[IO.File]::WriteAllText('outputs/source.md', "# Imported Document`n"); [IO.File]::WriteAllText('execution-result.json', '{"outcome":"converted","summary":"Converted imported document.","artifacts":[{"role":"source-markdown","relativePath":"outputs/source.md"}]}')"##;
            let utf16 = script
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            format!(
                "powershell.exe -NoProfile -NonInteractive -EncodedCommand {}",
                base64::engine::general_purpose::STANDARD.encode(utf16)
            )
        };
        let result = run_task_command(
            &paths,
            &command,
            10_000,
            &claim["task"],
            target["target"]["id"].as_str().unwrap(),
            claim["leaseToken"].as_str().unwrap(),
            true,
            claim["attemptId"].as_str(),
            claim["executionGeneration"].as_i64(),
            claim["taskBrokerUrl"].as_str(),
            claim["executionToken"].as_str(),
            None,
        )
        .expect("automatic execution");
        assert_eq!(result["success"], true, "{result}");

        let completed = crate::wiki::read_my_document(&paths, document_id)
            .expect("completed document");
        assert_eq!(completed["content"], "# Imported Document\n");
        assert_eq!(completed["document"]["contentVersion"], 1);
        assert_eq!(completed["document"]["conversion"]["status"], "ready");
        let context = crate::wiki::wiki_context(&paths).expect("wiki context");
        assert_eq!(context["state"]["rawDocuments"], json!([]));
        assert_eq!(context["state"]["wikiSpaces"][0]["pageIndex"], json!([]));
        let tasks = crate::storage::Storage::open(&paths)
            .expect("storage")
            .list_tasks(&bootstrap.project.id)
            .expect("tasks");
        assert!(!tasks.iter().any(|task| {
            matches!(
                task["type"].as_str(),
                Some("ingest_markdown_into_wiki" | "maintain_wiki")
            )
        }));
    }

    #[test]
    fn my_document_write_stages_one_validated_artifact_without_an_operation() {
        let _env_lock = crate::TASK_BROKER_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("bridge-my-document-write-result-test"),
        )
        .expect("paths");
        crate::control::ensure_project_bootstrap(&paths, crate::control::BootstrapRequest::new())
            .expect("bootstrap");
        crate::writing::write_selection(&paths, false, &[]).expect("selection");
        let created = crate::writing::create_requests(
            &paths,
            "Return plain text",
            "create",
            None,
            &["writing-default".to_owned()],
        )
        .expect("request");
        let task = created["tasks"][0].clone();
        let task_id = task["id"].as_str().expect("task id");
        let document_id = task["input"]["targetMyDocumentId"]
            .as_str()
            .expect("document id");
        let target = crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "my-document-write-validator",
                host: Some("test"),
                project_id: None,
                capabilities: vec!["writing.writeMyDocument".to_owned()],
                priority: 0,
                max_concurrency: 1,
                model_gateway_connection_id: None,
            },
        )
        .expect("target");
        let _broker = crate::content::enable_test_task_broker();
        let claim = crate::tasks::claim_task(
            &paths,
            task_id,
            target["target"]["id"].as_str().expect("target id"),
        )
        .expect("claim");
        let execution_token = claim["executionToken"].as_str().expect("execution token");
        let workspace = temp.path().join("execution");
        fs::create_dir_all(workspace.join("outputs")).expect("workspace");
        let claimed_task = claim["task"].clone();
        let missing = build_my_document_write_output_plan(
            &paths,
            &claimed_task,
            &workspace,
            claim["attemptId"].as_str().expect("attempt id"),
            claim["executionGeneration"]
                .as_i64()
                .expect("execution generation"),
            &json!({}),
        )
        .expect_err("missing result must fail");
        assert_eq!(missing.code(), Some("invalid_output"));

        fs::write(
            workspace.join("outputs/document.txt"),
            "Written plain text\n",
        )
        .expect("artifact");
        fs::write(
            workspace.join(EXECUTION_RESULT_FILE),
            serde_json::to_vec(&json!({
                "outcome": "written",
                "summary": "Wrote the requested plain-text document.",
                "title": "Plain result",
                "artifacts": [{
                    "role": "my-document",
                    "relativePath": "outputs/document.txt"
                }]
            }))
            .expect("serialize"),
        )
        .expect("result");
        let prepared = prepare_task_output_plan(
            &paths,
            &claimed_task,
            &workspace,
            claimed_task["handlerKey"].as_str().expect("handler"),
            "bundle:test",
            claim["attemptId"].as_str().expect("attempt id"),
            claim["executionGeneration"]
                .as_i64()
                .expect("execution generation"),
        )
        .expect("output plan");
        let applied = apply_task_output_plan(&paths, execution_token, &prepared.plan)
            .expect("apply output plan");
        assert_eq!(applied["artifacts"].as_array().map(Vec::len), Some(1));
        assert!(crate::storage::Storage::open(&paths)
            .expect("storage")
            .list_direct_operations(None, None)
            .expect("operations")
            .is_empty());
        crate::tasks::complete_task(
            &paths,
            task_id,
            claim["leaseToken"].as_str().expect("lease token"),
            Some(prepared.result),
        )
        .expect("complete task");
        let completed =
            crate::wiki::read_my_document(&paths, document_id).expect("completed document");
        assert_eq!(completed["document"]["contentVersion"], 1);
        assert_eq!(completed["document"]["format"], "text");
        assert_eq!(completed["content"], "Written plain text\n");
    }
