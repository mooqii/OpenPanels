    #[test]
    fn wiki_ingestion_prompt_is_compact_and_materializes_complete_inputs() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("bridge-wiki-test"),
        )
        .expect("paths");
        let bootstrap = crate::control::ensure_project_bootstrap(
            &paths,
            crate::control::BootstrapRequest::new(),
        )
        .expect("bootstrap");
        crate::wiki::write_page(
            &paths,
            "wiki:default",
            "custom/home.md",
            "# Home\n",
            None,
            None,
        )
        .expect("existing page");
        crate::wiki::add_raw_document(
            &paths,
            "source.md",
            Some("Source"),
            Some("text/markdown"),
            "user",
            Some("wiki:default"),
            b"# Source material\n\nUseful facts.\n",
        )
        .expect("raw document");
        let task = crate::storage::Storage::open(&paths)
            .expect("storage")
            .list_tasks(&bootstrap.project.id)
            .expect("tasks")
            .into_iter()
            .find(|task| task["type"] == "ingest_markdown_into_wiki")
            .expect("ingest task");
        let workspace = temp.path().join("execution");
        fs::create_dir_all(&workspace).expect("workspace");
        let prompt = wiki_authoring_task_prompt(&paths, &task, &workspace).expect("prompt");

        assert!(prompt.contains("# Runtime Contract"));
        assert!(prompt.contains("# Source material"));
        assert!(prompt.contains("custom/home.md"));
        assert!(prompt.contains("Karpathy LLM Wiki"));
        assert!(prompt.contains("wiki page create"));
        assert!(prompt.contains("--space-id wiki:default"));
        assert!(prompt.contains(task["id"].as_str().unwrap()));
        assert!(!prompt.contains("Task JSON"));
        assert!(!prompt.contains("workflowId"));
        assert!(!prompt.contains("mutationSequence"));
        assert!(!prompt.contains("executionGeneration"));
        assert!(!prompt.contains("agent skill read --skill-id"));
        assert!(workspace.join("inputs/source.md").is_file());
        assert!(workspace.join("wiki-paths.txt").is_file());
        assert!(workspace
            .join("skills/karpathy-llm-wiki/references/wiki-conventions.md")
            .is_file());
        assert!(prompt.len() <= MAX_AGENT_PROMPT_BYTES);
    }

    #[test]
    fn oversized_wiki_source_falls_back_as_a_complete_section() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("bridge-large-wiki-test"),
        )
        .expect("paths");
        let bootstrap = crate::control::ensure_project_bootstrap(
            &paths,
            crate::control::BootstrapRequest::new(),
        )
        .expect("bootstrap");
        let source = format!(
            "# Large\n\n{}END-MARKER",
            "x".repeat(MAX_AGENT_PROMPT_BYTES)
        );
        crate::wiki::add_raw_document(
            &paths,
            "large.md",
            Some("Large"),
            Some("text/markdown"),
            "user",
            Some("wiki:default"),
            source.as_bytes(),
        )
        .expect("raw document");
        let task = crate::storage::Storage::open(&paths)
            .expect("storage")
            .list_tasks(&bootstrap.project.id)
            .expect("tasks")
            .into_iter()
            .find(|task| task["type"] == "ingest_markdown_into_wiki")
            .expect("ingest task");
        let workspace = temp.path().join("execution");
        fs::create_dir_all(&workspace).expect("workspace");

        let prompt = wiki_authoring_task_prompt(&paths, &task, &workspace).expect("prompt");

        assert!(prompt.len() <= MAX_AGENT_PROMPT_BYTES);
        assert!(prompt.contains("inputs/source.md"));
        assert!(!prompt.contains("END-MARKER"));
        assert!(fs::read_to_string(workspace.join("inputs/source.md"))
            .expect("source file")
            .ends_with("END-MARKER"));
    }

    #[test]
    fn maintenance_prompts_expose_events_without_scheduler_metadata() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("bridge-maintenance-test"),
        )
        .expect("paths");
        let bootstrap = crate::control::ensure_project_bootstrap(
            &paths,
            crate::control::BootstrapRequest::new(),
        )
        .expect("bootstrap");
        let task = json!({
            "id": "task:maintain",
            "projectId": bootstrap.project.id,
            "queue": "wiki",
            "type": "maintain_wiki",
            "workflowId": "workflow:noise",
            "mutationKey": "wiki:noise",
            "mutationSequence": 42,
            "executionGeneration": 7,
            "source": {
                "wikiSpaceId": "wiki:default",
                "agentSkillId": "karpathy-llm-wiki"
            },
            "input": {
                "wikiSpaceId": "wiki:default",
                "changeEvents": [{
                    "kind": "wiki_page_renamed",
                    "fromPath": "old/place.md",
                    "toPath": "new/place.md"
                }]
            }
        });
        let workspace = temp.path().join("maintain-execution");
        fs::create_dir_all(&workspace).expect("workspace");

        let prompt = wiki_authoring_task_prompt(&paths, &task, &workspace).expect("prompt");

        assert!(prompt.contains("wiki_page_renamed"));
        assert!(prompt.contains("old/place.md"));
        assert!(prompt.contains("new/place.md"));
        assert!(!prompt.contains("workflow:noise"));
        assert!(!prompt.contains("wiki:noise"));
        assert!(!prompt.contains("mutationSequence"));
        assert!(!prompt.contains("executionGeneration"));

        let invalid = json!({
            "id": "task:missing-events",
            "projectId": bootstrap.project.id,
            "queue": "wiki",
            "type": "maintain_wiki",
            "source": {
                "wikiSpaceId": "wiki:default",
                "agentSkillId": "karpathy-llm-wiki"
            },
            "input": { "wikiSpaceId": "wiki:default" }
        });
        let invalid_workspace = temp.path().join("invalid-execution");
        fs::create_dir_all(&invalid_workspace).expect("workspace");
        let error = wiki_authoring_task_prompt(&paths, &invalid, &invalid_workspace)
            .expect_err("missing change events must fail");
        assert_eq!(error.code(), Some("invalid_task_input"));
    }

    #[test]
    fn wiki_execution_result_requires_an_exact_staged_path_set() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("bridge-result-test"),
        )
        .expect("paths");
        crate::control::ensure_project_bootstrap(&paths, crate::control::BootstrapRequest::new())
            .expect("bootstrap");
        let workspace = temp.path().join("execution");
        fs::create_dir_all(&workspace).expect("workspace");
        let task = json!({ "id": "task:result", "queue": "wiki" });

        let missing = validate_wiki_execution_result(&paths, &task, &workspace)
            .expect_err("missing result must fail");
        assert_eq!(missing.code(), Some("invalid_output"));

        fs::write(
            workspace.join(EXECUTION_RESULT_FILE),
            serde_json::to_vec(&json!({
                "schemaVersion": 1,
                "outcome": "no_change",
                "summary": "The Skill requires no update.",
                "changedPaths": [],
            }))
            .expect("serialize"),
        )
        .expect("write result");
        let valid =
            validate_wiki_execution_result(&paths, &task, &workspace).expect("valid no change");
        assert_eq!(valid["outcome"], "no_change");
        assert_eq!(valid["bridgeValidated"], true);

        fs::write(
            workspace.join(EXECUTION_RESULT_FILE),
            serde_json::to_vec(&json!({
                "schemaVersion": 1,
                "outcome": "changed",
                "summary": "Updated a page.",
                "changedPaths": ["invented/page.md"],
            }))
            .expect("serialize"),
        )
        .expect("write result");
        let mismatch = validate_wiki_execution_result(&paths, &task, &workspace)
            .expect_err("unstaged paths must fail");
        assert_eq!(mismatch.code(), Some("invalid_output"));
    }

