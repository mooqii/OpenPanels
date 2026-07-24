    #[test]
    fn writing_revision_uses_the_complete_captured_target_with_prompt_fallback() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("bridge-writing-revision-test"),
        )
        .expect("paths");
        crate::control::ensure_project_bootstrap(&paths, crate::control::BootstrapRequest::new())
            .expect("bootstrap");
        let captured = format!(
            "# Original\n{}\nREVISION-END-MARKER",
            "x".repeat(MAX_AGENT_PROMPT_BYTES)
        );
        let my_document = crate::wiki::create_my_document(
            &paths,
            "draft.md",
            Some("Draft"),
            Some("text/markdown"),
            None,
            None,
            captured.as_bytes(),
        )
        .expect("My Document");
        let document_id = my_document["document"]["id"].as_str().expect("document id");
        crate::writing::write_selection(&paths, false, &[]).expect("selection");
        let created = crate::writing::create_requests(
            &paths,
            "Revise this document",
            "revise",
            Some(document_id),
            &["writing-default".to_owned()],
        )
        .expect("revision request");
        crate::wiki::write_my_document(
            &paths,
            document_id,
            "draft.md",
            Some("text/markdown"),
            b"# Newer content\n",
        )
        .expect("newer document");
        let task = &created["tasks"][0];
        let workspace = temp.path().join("execution");
        fs::create_dir_all(&workspace).expect("workspace");

        let prompt = my_document_write_task_prompt(&paths, task, &workspace).expect("prompt");

        assert!(prompt.len() <= MAX_AGENT_PROMPT_BYTES);
        assert!(prompt.contains("inputs/target/"));
        assert!(!prompt.contains("REVISION-END-MARKER"));
        assert_eq!(
            fs::read_to_string(workspace.join("inputs/target/content.md"))
                .expect("captured target"),
            captured
        );
        assert!(!workspace.join("wiki-paths.txt").exists());
    }

    #[test]
    fn writing_distillation_prompt_materializes_all_sources_without_wiki_or_scheduler_noise() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("bridge-writing-distillation-test"),
        )
        .expect("paths");
        crate::control::ensure_project_bootstrap(&paths, crate::control::BootstrapRequest::new())
            .expect("bootstrap");
        let wiki_space_id = active_wiki_space_id(&paths);
        let raw = crate::wiki::add_raw_document(
            &paths,
            "style-source.md",
            Some("Raw style source"),
            Some("text/markdown"),
            "user",
            Some(&wiki_space_id),
            b"# Raw style\n\nShort rhythmic paragraphs.\n",
        )
        .expect("raw source");
        let raw_id = raw["document"]["id"].as_str().expect("raw id");
        let my_document = crate::wiki::create_my_document(
            &paths,
            "generated-source.md",
            Some("Generated style source"),
            Some("text/markdown"),
            None,
            None,
            b"# Generated style\n\nConcrete examples and a crisp ending.\n",
        )
        .expect("generated source");
        let my_document_id = my_document["document"]["id"].as_str().expect("My Document id");
        crate::wiki::write_page(
            &paths,
            &wiki_space_id,
            "private/wiki-page.md",
            "# Must not be exposed\n",
            None,
            None,
        )
        .expect("wiki page");
        crate::writing::write_selection(
            &paths,
            true,
            &[my_document_id.to_owned()],
        )
        .expect("selection");
        let created = crate::writing::create_distillation_request(&paths, "House Style")
            .expect("distillation request");
        let mut task = created["task"].clone();
        let raw_content = "# Raw style\n\nShort rhythmic paragraphs.\n";
        let mut raw_snapshot = raw["document"].clone();
        raw_snapshot["snapshotContent"] = json!(raw_content);
        raw_snapshot["snapshotHash"] = json!(format!(
            "{:x}",
            Sha256::digest(raw_content.as_bytes())
        ));
        task["input"]["contextSnapshot"]["rawDocuments"] =
            json!([raw_snapshot]);
        task["executionGeneration"] = json!(23);
        let workspace = temp.path().join("execution");
        fs::create_dir_all(&workspace).expect("workspace");

        let prompt = writing_distillation_task_prompt(&task, &workspace).expect("prompt");

        let skill_id = task["input"]["skillId"].as_str().expect("skill id");
        assert!(prompt.contains("House Style"));
        assert!(prompt.contains(skill_id));
        assert!(prompt.contains("Create one reusable Writing Skill"));
        assert!(prompt.contains("captured portable Distiller Skill"));
        assert!(prompt.contains(&format!("name: {skill_id}")));
        assert!(!prompt.contains("appliesTo: writing"));
        assert!(prompt.contains("# Raw style"));
        assert!(prompt.contains("# Generated style"));
        assert!(!prompt.contains("writing skill install --task-id"));
        assert!(prompt.contains("outputs/SKILL.md"));
        assert!(!prompt.contains("schemaVersion"));
        assert!(!prompt.contains("private/wiki-page.md"));
        assert!(!prompt.contains("Must not be exposed"));
        assert!(!prompt.contains("workflow:noise"));
        assert!(!prompt.contains("executionGeneration"));
        assert!(!prompt.contains("Task JSON:"));
        assert!(workspace.join("task-context.json").is_file());
        assert!(workspace
            .join("skills/writing-distillation-default/SKILL.md")
            .is_file());
        assert!(workspace
            .join("inputs/raw")
            .join(sanitize_path_part(raw_id))
            .join("source.md")
            .is_file());
        assert!(workspace
            .join("inputs/my-documents")
            .join(sanitize_path_part(my_document_id))
            .join("content.md")
            .is_file());
        assert!(workspace.join("outputs").is_dir());
        assert!(!workspace.join("wiki-paths.txt").exists());
        assert!(prompt.len() <= MAX_AGENT_PROMPT_BYTES);

        let custom_command_input = serde_json::to_string_pretty(&task).expect("raw Task JSON");
        assert!(custom_command_input.contains("distillerSkillSnapshot"));
        assert!(custom_command_input.contains("contextSnapshot"));

        let mut incomplete_task = task.clone();
        incomplete_task["input"]
            .as_object_mut()
            .expect("input")
            .remove("distillerSkillSnapshot");
        let incomplete_workspace = temp.path().join("incomplete-execution");
        fs::create_dir_all(&incomplete_workspace).expect("incomplete workspace");
        let incomplete = writing_distillation_task_prompt(&incomplete_task, &incomplete_workspace)
            .expect_err("old distillation Task must not be accepted");
        assert_eq!(incomplete.code(), Some("invalid_task_input"));
    }
