    #[test]
    fn writing_generation_prompt_materializes_captured_context_without_scheduler_noise() {
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
            Some("bridge-writing-test"),
        )
        .expect("paths");
        crate::control::ensure_project_bootstrap(&paths, crate::control::BootstrapRequest::new())
            .expect("bootstrap");
        let raw = crate::wiki::add_raw_document(
            &paths,
            "source.md",
            Some("Captured source"),
            Some("text/markdown"),
            "user",
            Some("wiki:default"),
            b"# Captured raw source\n",
        )
        .expect("raw document");
        let raw_id = raw["document"]["id"].as_str().expect("raw id");
        let generated = crate::wiki::create_generated_document(
            &paths,
            "prior.md",
            Some("Prior draft"),
            Some("text/markdown"),
            None,
            None,
            b"# Captured generated source\n",
        )
        .expect("generated document");
        let generated_id = generated["document"]["id"].as_str().expect("generated id");
        crate::wiki::write_page(
            &paths,
            "wiki:default",
            "guides/reference.md",
            "# Captured Wiki page\n",
            Some("Reference"),
            None,
        )
        .expect("wiki page");
        crate::writing::write_selection(
            &paths,
            true,
            &[raw_id.to_owned()],
            &[generated_id.to_owned()],
        )
        .expect("writing selection");
        let created = crate::writing::create_requests(
            &paths,
            "Write a concise report",
            "create",
            None,
            &["writing-xiaohongshu-note".to_owned()],
        )
        .expect("writing request");
        let mut task = created["tasks"][0].clone();
        task["workflowRunId"] = json!("workflow:noise");
        task["mutationKey"] = json!("writing:noise");
        task["executionGeneration"] = json!(17);
        let workspace = temp.path().join("execution");
        fs::create_dir_all(&workspace).expect("workspace");

        let prompt = document_generation_task_prompt(&paths, &task, &workspace).expect("prompt");

        assert!(prompt.contains("Write a concise report"));
        assert!(prompt.contains("Write a polished Xiaohongshu-style note"));
        assert!(prompt.contains("selected portable Writing Skill"));
        assert!(prompt.contains("# Captured raw source"));
        assert!(prompt.contains("# Captured generated source"));
        assert!(prompt.contains("guides/reference.md"));
        assert!(!prompt.contains("writing generate --task-id"));
        assert!(!prompt.contains("operation complete --operation-id"));
        assert!(prompt.contains("outputs/document.md"));
        assert!(prompt.contains("\"schemaVersion\": 2"));
        assert!(!prompt.contains("workflow:noise"));
        assert!(!prompt.contains("writing:noise"));
        assert!(!prompt.contains("executionGeneration"));
        assert!(!prompt.contains("Task JSON:"));
        assert!(workspace.join("task-context.json").is_file());
        assert!(workspace
            .join("skills/writing-xiaohongshu-note/SKILL.md")
            .is_file());
        assert!(workspace
            .join("inputs/raw")
            .join(sanitize_path_part(raw_id))
            .join("source.md")
            .is_file());
        assert!(workspace
            .join("inputs/generated")
            .join(sanitize_path_part(generated_id))
            .join("content.md")
            .is_file());
        assert_eq!(
            fs::read_to_string(workspace.join("wiki-paths.txt")).expect("wiki paths"),
            "guides/reference.md"
        );
        assert!(workspace.join("outputs").is_dir());
        assert!(prompt.len() <= MAX_AGENT_PROMPT_BYTES);

        let custom_command_input = serde_json::to_string_pretty(&task).expect("raw task JSON");
        assert!(custom_command_input.contains("workflow:noise"));
        assert!(custom_command_input.contains("contextSnapshot"));

        crate::wiki::write_page(
            &paths,
            "wiki:default",
            "guides/reference.md",
            "# Newer Wiki page\n",
            Some("Reference"),
            None,
        )
        .expect("update Wiki page");
        let task_id = task["id"].as_str().expect("task id");
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
                name: "writing-wiki-reader",
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
        let pinned = crate::content::read_file(
            &paths,
            claim["executionToken"].as_str().expect("execution token"),
            &crate::content::ReadFileRequest {
                resource_kind: crate::content::ResourceKind::WikiSpace.as_str().to_owned(),
                resource_key: "wiki:default".to_owned(),
                logical_path: "guides/reference.md".to_owned(),
            },
        )
        .expect("read pinned Wiki page");
        use base64::Engine as _;
        let pinned_text = String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(pinned["contentBase64"].as_str().expect("content"))
                .expect("base64"),
        )
        .expect("UTF-8");
        assert_eq!(pinned_text, "# Captured Wiki page\n");
    }
