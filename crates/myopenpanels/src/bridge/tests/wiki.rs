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
        let wiki_space_id = active_wiki_space_id(&paths);
        crate::wiki::write_page(
            &paths,
            &wiki_space_id,
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
            Some(&wiki_space_id),
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
        assert!(prompt.contains("selected portable Authoring Skill"));

        assert!(prompt.contains("# Runtime Contract"));
        assert!(prompt.contains("# Source material"));
        assert!(prompt.contains("custom/home.md"));
        assert!(prompt.contains("Default Wiki"));
        assert!(!prompt.contains("wiki page create"));
        assert!(prompt.contains("outputs/wiki/<path.md>"));
        assert!(prompt.contains(&format!(
            "--project-dir {}",
            shell_quote_prompt_arg(&paths.project_dir.display().to_string())
        )));
        assert!(prompt.contains(&format!("--space-id {wiki_space_id}")));
        assert!(prompt.contains(task["id"].as_str().unwrap()));
        assert!(!prompt.contains("Task JSON"));
        assert!(!prompt.contains("mutationSequence"));
        assert!(!prompt.contains("executionGeneration"));
        assert!(!prompt.contains("agent skill read --skill-id"));
        assert!(workspace.join("inputs/source.md").is_file());
        assert!(workspace.join("wiki-paths.txt").is_file());
        assert!(workspace
            .join("skills/wiki-default/references/wiki-conventions.md")
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
        let wiki_space_id = active_wiki_space_id(&paths);
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
            Some(&wiki_space_id),
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
        let wiki_space_id = active_wiki_space_id(&paths);
        let task = json!({
            "id": "task:maintain",
            "projectId": bootstrap.project.id,
            "queue": "wiki",
            "type": "maintain_wiki",
            "mutationKey": "wiki:noise",
            "mutationSequence": 42,
            "executionGeneration": 7,
            "source": {
                "wikiSpaceId": wiki_space_id,
                "agentSkillId": "wiki-default"
            },
            "input": {
                "wikiSpaceId": wiki_space_id,
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
        assert!(workspace.join("skills/wiki-default/SKILL.md").is_file());

        let invalid = json!({
            "id": "task:missing-events",
            "projectId": bootstrap.project.id,
            "queue": "wiki",
            "type": "maintain_wiki",
            "source": {
                "wikiSpaceId": wiki_space_id,
                "agentSkillId": "wiki-default"
            },
            "input": { "wikiSpaceId": wiki_space_id }
        });
        let invalid_workspace = temp.path().join("invalid-execution");
        fs::create_dir_all(&invalid_workspace).expect("workspace");
        let error = wiki_authoring_task_prompt(&paths, &invalid, &invalid_workspace)
            .expect_err("missing change events must fail");
        assert_eq!(error.code(), Some("invalid_task_input"));
    }
