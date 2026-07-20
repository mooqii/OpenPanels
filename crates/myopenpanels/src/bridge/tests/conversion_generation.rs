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
        let original_bytes = b"PROMPT-SECRET-BINARY\0\xff";
        crate::wiki::add_raw_document(
            &paths,
            "unsafe name.pdf",
            Some("Conversion source"),
            Some("application/pdf"),
            "user",
            Some("wiki:default"),
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
        task["workflowRunId"] = json!("workflow:noise");
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
        assert!(workspace.join("instructions/convert-document.md").is_file());
        assert!(workspace.join("outputs").is_dir());
        assert!(prompt.contains("# Conversion Instructions"));
        assert!(prompt.contains("Convert A Raw Document To Markdown"));
        assert!(prompt.contains(original_path.to_str().unwrap()));
        assert!(!prompt.contains("wiki raw update"));
        assert!(prompt.contains("\"schemaVersion\": 2"));
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
        assert!(custom_command_input.contains("workflow:noise"));
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
        let uploaded = crate::wiki::add_raw_document(
            &paths,
            "automatic.pdf",
            Some("Automatic conversion"),
            Some("application/pdf"),
            "user",
            Some("wiki:default"),
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
                protocol_version: 3,
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
        let command = r#"printf '# Automatic Runtime\n' > outputs/source.md && printf '%s' '{"schemaVersion":2,"outcome":"converted","summary":"Converted automatically.","artifacts":[{"role":"source-markdown","relativePath":"outputs/source.md"}]}' > execution-result.json"#;
        #[cfg(windows)]
        let command = r#"> outputs\source.md echo # Automatic Runtime && > execution-result.json echo {^"schemaVersion^":2,^"outcome^":^"converted^",^"summary^":^"Converted automatically.^",^"artifacts^":[{^"role^":^"source-markdown^",^"relativePath^":^"outputs/source.md^"}]}"#;
        let result = run_task_command(
            &paths,
            command,
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
    }

    #[test]
    fn conversion_result_must_match_the_single_staged_markdown() {
        use base64::Engine as _;

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
            Some("bridge-conversion-result-test"),
        )
        .expect("paths");
        let bootstrap = crate::control::ensure_project_bootstrap(
            &paths,
            crate::control::BootstrapRequest::new(),
        )
        .expect("bootstrap");
        let uploaded = crate::wiki::add_raw_document(
            &paths,
            "source.pdf",
            Some("Source"),
            Some("application/pdf"),
            "user",
            Some("wiki:default"),
            b"pdf fixture",
        )
        .expect("raw document");
        let document_id = uploaded["document"]["id"].as_str().expect("document id");
        let task = crate::storage::Storage::open(&paths)
            .expect("storage")
            .list_tasks(&bootstrap.project.id)
            .expect("tasks")
            .into_iter()
            .find(|task| task["type"] == "convert_document_to_markdown")
            .expect("conversion task");
        crate::storage::Storage::open(&paths)
            .expect("storage")
            .connection()
            .execute(
                "UPDATE tasks SET required_protocol_version = 3 WHERE id = ?",
                [task["id"].as_str().expect("task id")],
            )
            .expect("protocol");
        let _broker = crate::content::enable_test_task_broker();
        let target = crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "conversion-validator",
                host: Some("test"),
                project_id: None,
                capabilities: vec!["wiki.convertDocument".to_owned()],
                priority: 0,
                protocol_version: 3,
                max_concurrency: 1,
                model_gateway_connection_id: None,
            },
        )
        .expect("target");
        let claim = crate::tasks::claim_task(
            &paths,
            task["id"].as_str().expect("task id"),
            target["target"]["id"].as_str().expect("target id"),
        )
        .expect("claim");
        let workspace = temp.path().join("execution");
        fs::create_dir_all(&workspace).expect("workspace");
        let denied = crate::content::authorize_agent_broker_capability(
            &paths,
            claim["executionToken"].as_str().expect("execution token"),
            "content.write",
        )
        .expect_err("conversion Agent cannot stage through the Broker");
        assert_eq!(denied.code(), Some("task_handler_command_not_allowed"));
        let missing = validate_conversion_execution_result(&paths, &task, &workspace)
            .expect_err("missing result must fail");
        assert_eq!(missing.code(), Some("invalid_output"));
        fs::write(workspace.join(EXECUTION_RESULT_FILE), b"not json").expect("invalid result");
        let malformed = validate_conversion_execution_result(&paths, &task, &workspace)
            .expect_err("malformed result must fail");
        assert_eq!(malformed.code(), Some("invalid_output"));

        let valid_result = json!({
            "schemaVersion": 1,
            "outcome": "converted",
            "summary": "Converted the source faithfully.",
            "output": {
                "documentId": document_id,
                "logicalPath": "source.md"
            }
        });
        fs::write(
            workspace.join(EXECUTION_RESULT_FILE),
            serde_json::to_vec(&valid_result).expect("serialize"),
        )
        .expect("result");
        let unstaged = validate_conversion_execution_result(&paths, &task, &workspace)
            .expect_err("a declared result without staged Markdown must fail");
        assert_eq!(unstaged.code(), Some("invalid_output"));

        crate::content::stage_file(
            &paths,
            claim["executionToken"].as_str().expect("execution token"),
            &crate::content::StageFileRequest {
                resource_kind: crate::content::ResourceKind::WikiMarkdown
                    .as_str()
                    .to_owned(),
                resource_key: document_id.to_owned(),
                logical_path: "source.md".to_owned(),
                content_base64: base64::engine::general_purpose::STANDARD.encode(b"# Converted\n"),
                mime_type: "text/markdown".to_owned(),
                metadata: json!({ "documentId": document_id }),
            },
        )
        .expect("stage markdown");
        fs::write(
            workspace.join(EXECUTION_RESULT_FILE),
            serde_json::to_vec(&valid_result).expect("serialize"),
        )
        .expect("result");

        let result =
            validate_conversion_execution_result(&paths, &task, &workspace).expect("valid result");
        assert_eq!(result["outcome"], "converted");

        fs::write(
            workspace.join(EXECUTION_RESULT_FILE),
            serde_json::to_vec(&json!({
                "schemaVersion": 1,
                "outcome": "converted",
                "summary": "Converted the wrong source.",
                "output": {
                    "documentId": "raw:wrong",
                    "logicalPath": "source.md"
                }
            }))
            .expect("serialize"),
        )
        .expect("result");
        let mismatch = validate_conversion_execution_result(&paths, &task, &workspace)
            .expect_err("mismatched result must fail");
        assert_eq!(mismatch.code(), Some("invalid_output"));

        let non_utf8 = crate::content::stage_file(
            &paths,
            claim["executionToken"].as_str().expect("execution token"),
            &crate::content::StageFileRequest {
                resource_kind: crate::content::ResourceKind::WikiMarkdown
                    .as_str()
                    .to_owned(),
                resource_key: document_id.to_owned(),
                logical_path: "invalid.md".to_owned(),
                content_base64: base64::engine::general_purpose::STANDARD.encode([0xff]),
                mime_type: "text/markdown".to_owned(),
                metadata: json!({ "documentId": document_id }),
            },
        )
        .expect_err("non-UTF-8 Markdown must fail at staging");
        assert_eq!(non_utf8.code(), Some("invalid_output"));

        crate::content::stage_file(
            &paths,
            claim["executionToken"].as_str().expect("execution token"),
            &crate::content::StageFileRequest {
                resource_kind: crate::content::ResourceKind::WikiMarkdown
                    .as_str()
                    .to_owned(),
                resource_key: document_id.to_owned(),
                logical_path: "source.md".to_owned(),
                content_base64: base64::engine::general_purpose::STANDARD.encode(b""),
                mime_type: "text/markdown".to_owned(),
                metadata: json!({ "documentId": document_id }),
            },
        )
        .expect("stage empty Markdown");
        fs::write(
            workspace.join(EXECUTION_RESULT_FILE),
            serde_json::to_vec(&valid_result).expect("serialize"),
        )
        .expect("result");
        let empty = validate_conversion_execution_result(&paths, &task, &workspace)
            .expect_err("empty Markdown must fail");
        assert_eq!(empty.code(), Some("invalid_output"));

        crate::content::stage_file(
            &paths,
            claim["executionToken"].as_str().expect("execution token"),
            &crate::content::StageFileRequest {
                resource_kind: crate::content::ResourceKind::WikiMarkdown
                    .as_str()
                    .to_owned(),
                resource_key: document_id.to_owned(),
                logical_path: "source.md".to_owned(),
                content_base64: base64::engine::general_purpose::STANDARD.encode(b"# Converted\n"),
                mime_type: "text/markdown".to_owned(),
                metadata: json!({ "documentId": document_id }),
            },
        )
        .expect("restore Markdown");
        crate::content::stage_file(
            &paths,
            claim["executionToken"].as_str().expect("execution token"),
            &crate::content::StageFileRequest {
                resource_kind: crate::content::ResourceKind::WikiMarkdown
                    .as_str()
                    .to_owned(),
                resource_key: document_id.to_owned(),
                logical_path: "extra.md".to_owned(),
                content_base64: base64::engine::general_purpose::STANDARD.encode(b"# Extra\n"),
                mime_type: "text/markdown".to_owned(),
                metadata: json!({ "documentId": document_id }),
            },
        )
        .expect("stage extra Markdown");
        let multiple = validate_conversion_execution_result(&paths, &task, &workspace)
            .expect_err("multiple staged Markdown files must fail");
        assert_eq!(multiple.code(), Some("invalid_output"));
    }

    #[test]
    fn generation_result_requires_one_matching_prepared_operation() {
        use base64::Engine as _;

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
            Some("bridge-generation-result-test"),
        )
        .expect("paths");
        crate::control::ensure_project_bootstrap(&paths, crate::control::BootstrapRequest::new())
            .expect("bootstrap");
        crate::writing::write_selection(&paths, false, &[], &[]).expect("selection");
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
        let document_id = task["input"]["targetGeneratedDocumentId"]
            .as_str()
            .expect("document id");
        crate::storage::Storage::open(&paths)
            .expect("storage")
            .connection()
            .execute(
                "UPDATE tasks SET required_protocol_version = 3 WHERE id = ?",
                [task_id],
            )
            .expect("protocol");
        let target = crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "generation-validator",
                host: Some("test"),
                project_id: None,
                capabilities: vec!["writing.generateDocument".to_owned()],
                priority: 0,
                protocol_version: 3,
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
        fs::create_dir_all(&workspace).expect("workspace");
        let valid_result = |operation_id: &str| {
            json!({
                "schemaVersion": 1,
                "outcome": "generated",
                "summary": "Generated the requested plain-text document.",
                "output": {
                    "documentId": document_id,
                    "operationId": operation_id,
                    "logicalPath": "content.txt"
                }
            })
        };

        let missing = validate_generation_execution_result(&paths, &task, &workspace)
            .expect_err("missing result must fail");
        assert_eq!(missing.code(), Some("invalid_output"));

        let started =
            crate::operations::begin_writing_for_broker(&paths, task_id, "Plain result", "text")
                .expect("begin operation");
        let operation_id = started["operation"]["id"].as_str().expect("operation id");
        fs::write(
            workspace.join(EXECUTION_RESULT_FILE),
            serde_json::to_vec(&valid_result(operation_id)).expect("serialize"),
        )
        .expect("result");
        let unstaged = validate_generation_execution_result(&paths, &task, &workspace)
            .expect_err("unprepared result must fail");
        assert_eq!(unstaged.code(), Some("invalid_output"));

        crate::content::prepare_operation(
            &paths,
            execution_token,
            &crate::content::PrepareOperationRequest {
                operation_id: operation_id.to_owned(),
                file_name: "document.txt".to_owned(),
                content_base64: base64::engine::general_purpose::STANDARD
                    .encode(b"Generated plain text\n"),
            },
        )
        .expect("prepare operation");
        let result =
            validate_generation_execution_result(&paths, &task, &workspace).expect("valid result");
        assert_eq!(result["outcome"], "generated");

        fs::write(
            workspace.join(EXECUTION_RESULT_FILE),
            serde_json::to_vec(&valid_result("operation:wrong")).expect("serialize"),
        )
        .expect("mismatched result");
        let mismatch = validate_generation_execution_result(&paths, &task, &workspace)
            .expect_err("mismatched Operation must fail");
        assert_eq!(mismatch.code(), Some("invalid_output"));

        fs::write(
            workspace.join(EXECUTION_RESULT_FILE),
            serde_json::to_vec(&valid_result(operation_id)).expect("serialize"),
        )
        .expect("restore result");
        crate::tasks::complete_task(
            &paths,
            task_id,
            claim["leaseToken"].as_str().expect("lease token"),
            Some(result),
        )
        .expect("complete task");
        let completed =
            crate::wiki::read_generated_document(&paths, document_id).expect("completed document");
        assert_eq!(completed["document"]["contentVersion"], 1);
        assert_eq!(completed["document"]["format"], "text");
        assert_eq!(completed["content"], "Generated plain text\n");
    }
