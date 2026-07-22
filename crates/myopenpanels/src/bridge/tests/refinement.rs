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
        let generated = crate::wiki::create_generated_document(
            &paths,
            "draft.md",
            Some("Draft"),
            Some("text/markdown"),
            None,
            None,
            captured.as_bytes(),
        )
        .expect("generated document");
        let document_id = generated["document"]["id"].as_str().expect("document id");
        crate::writing::write_selection(&paths, false, &[]).expect("selection");
        let created = crate::writing::create_requests(
            &paths,
            "Revise this document",
            "revise",
            Some(document_id),
            &["writing-default".to_owned()],
        )
        .expect("revision request");
        crate::wiki::write_generated_document(
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

        let prompt = document_generation_task_prompt(&paths, task, &workspace).expect("prompt");

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
    fn writing_refinement_prompt_materializes_all_sources_without_wiki_or_scheduler_noise() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("bridge-writing-refinement-test"),
        )
        .expect("paths");
        crate::control::ensure_project_bootstrap(&paths, crate::control::BootstrapRequest::new())
            .expect("bootstrap");
        let raw = crate::wiki::add_raw_document(
            &paths,
            "style-source.md",
            Some("Raw style source"),
            Some("text/markdown"),
            "user",
            Some("wiki:default"),
            b"# Raw style\n\nShort rhythmic paragraphs.\n",
        )
        .expect("raw source");
        let raw_id = raw["document"]["id"].as_str().expect("raw id");
        let generated = crate::wiki::create_generated_document(
            &paths,
            "generated-source.md",
            Some("Generated style source"),
            Some("text/markdown"),
            None,
            None,
            b"# Generated style\n\nConcrete examples and a crisp ending.\n",
        )
        .expect("generated source");
        let generated_id = generated["document"]["id"].as_str().expect("generated id");
        crate::wiki::write_page(
            &paths,
            "wiki:default",
            "private/wiki-page.md",
            "# Must not be exposed\n",
            None,
            None,
        )
        .expect("wiki page");
        crate::writing::write_selection(
            &paths,
            true,
            &[generated_id.to_owned()],
        )
        .expect("selection");
        let created = crate::writing::create_refinement_request(&paths, "House Style")
            .expect("refinement request");
        let mut task = created["task"].clone();
        let raw_content = "# Raw style\n\nShort rhythmic paragraphs.\n";
        let mut legacy_raw_snapshot = raw["document"].clone();
        legacy_raw_snapshot["snapshotContent"] = json!(raw_content);
        legacy_raw_snapshot["snapshotHash"] = json!(format!(
            "{:x}",
            Sha256::digest(raw_content.as_bytes())
        ));
        task["input"]["contextSnapshot"]["rawDocuments"] =
            json!([legacy_raw_snapshot]);
        task["workflowRunId"] = json!("workflow:noise");
        task["executionGeneration"] = json!(23);
        let workspace = temp.path().join("execution");
        fs::create_dir_all(&workspace).expect("workspace");

        let prompt = writing_refinement_task_prompt(&task, &workspace).expect("prompt");

        let skill_id = task["input"]["skillId"].as_str().expect("skill id");
        assert!(prompt.contains("House Style"));
        assert!(prompt.contains(skill_id));
        assert!(prompt.contains("Create one reusable Writing Skill"));
        assert!(prompt.contains("captured portable Refiner Skill"));
        assert!(prompt.contains(&format!("name: {skill_id}")));
        assert!(!prompt.contains("appliesTo: writing"));
        assert!(prompt.contains("# Raw style"));
        assert!(prompt.contains("# Generated style"));
        assert!(!prompt.contains("writing skill install --task-id"));
        assert!(prompt.contains("outputs/SKILL.md"));
        assert!(prompt.contains("\"schemaVersion\": 2"));
        assert!(!prompt.contains("private/wiki-page.md"));
        assert!(!prompt.contains("Must not be exposed"));
        assert!(!prompt.contains("workflow:noise"));
        assert!(!prompt.contains("executionGeneration"));
        assert!(!prompt.contains("Task JSON:"));
        assert!(workspace.join("task-context.json").is_file());
        assert!(workspace
            .join("skills/writing-refinement-default/SKILL.md")
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
        assert!(workspace.join("outputs").is_dir());
        assert!(!workspace.join("wiki-paths.txt").exists());
        assert!(prompt.len() <= MAX_AGENT_PROMPT_BYTES);

        let custom_command_input = serde_json::to_string_pretty(&task).expect("raw Task JSON");
        assert!(custom_command_input.contains("workflow:noise"));
        assert!(custom_command_input.contains("refinerSkillSnapshot"));
        assert!(custom_command_input.contains("contextSnapshot"));

        let mut incomplete_task = task.clone();
        incomplete_task["input"]
            .as_object_mut()
            .expect("input")
            .remove("refinerSkillSnapshot");
        let incomplete_workspace = temp.path().join("incomplete-execution");
        fs::create_dir_all(&incomplete_workspace).expect("incomplete workspace");
        let incomplete = writing_refinement_task_prompt(&incomplete_task, &incomplete_workspace)
            .expect_err("old refinement Task must not be accepted");
        assert_eq!(incomplete.code(), Some("invalid_task_input"));
    }

    #[test]
    fn refinement_result_requires_the_exact_staged_writing_skill() {
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
            Some("bridge-refinement-result-test"),
        )
        .expect("paths");
        crate::control::ensure_project_bootstrap(&paths, crate::control::BootstrapRequest::new())
            .expect("bootstrap");
        let generated = crate::wiki::create_generated_document(
            &paths,
            "sample.md",
            Some("Sample"),
            Some("text/markdown"),
            None,
            None,
            b"# Sample\n\nShort, direct paragraphs.\n",
        )
        .expect("source");
        let source_id = generated["document"]["id"].as_str().expect("source id");
        crate::writing::write_selection(&paths, false, &[source_id.to_owned()])
            .expect("selection");
        let created = crate::writing::create_refinement_request(&paths, "Concise House Style")
            .expect("request");
        let task = created["task"].clone();
        let task_id = task["id"].as_str().expect("task id");
        let skill_id = task["input"]["skillId"].as_str().expect("skill id");
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
                name: "refinement-validator",
                host: Some("test"),
                project_id: None,
                capabilities: vec!["writing.refineSkill".to_owned()],
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
        let workspace = temp.path().join("execution");
        fs::create_dir_all(&workspace).expect("workspace");
        let result_value = json!({
            "schemaVersion": 1,
            "outcome": "refined",
            "summary": "Extracted a concise reusable house style.",
            "output": {
                "skillId": skill_id,
                "logicalPath": "SKILL.md"
            }
        });
        fs::write(
            workspace.join(EXECUTION_RESULT_FILE),
            serde_json::to_vec(&result_value).expect("serialize"),
        )
        .expect("result");
        let unstaged = validate_refinement_execution_result(&paths, &task, &workspace)
            .expect_err("unstaged result must fail");
        assert_eq!(unstaged.code(), Some("invalid_output"));

        let skill_source = format!(
            "---\nname: {skill_id}\ndescription: Write concise documents with direct structure.\n---\n\nUse short, direct paragraphs. Lead with the main point and remove redundant setup.\n"
        );
        crate::content::prepare_skill(
            &paths,
            claim["executionToken"].as_str().expect("execution token"),
            &crate::content::PrepareSkillRequest {
                skill_id: skill_id.to_owned(),
                source: skill_source,
                manifest: json!({}),
            },
        )
        .expect("prepare Skill");
        let result = validate_refinement_execution_result(&paths, &task, &workspace)
            .expect("valid refinement result");
        assert_eq!(result["outcome"], "refined");

        let mut mismatch = result_value.clone();
        mismatch["output"]["skillId"] = json!("writing-custom:wrong");
        fs::write(
            workspace.join(EXECUTION_RESULT_FILE),
            serde_json::to_vec(&mismatch).expect("serialize"),
        )
        .expect("mismatched result");
        let mismatch_error = validate_refinement_execution_result(&paths, &task, &workspace)
            .expect_err("mismatched Skill must fail");
        assert_eq!(mismatch_error.code(), Some("invalid_output"));

        crate::tasks::complete_task(
            &paths,
            task_id,
            claim["leaseToken"].as_str().expect("lease token"),
            Some(result),
        )
        .expect("complete Task");
        let installed =
            crate::agent::writing_agent_skill(&paths, skill_id).expect("installed Writing Skill");
        assert_eq!(installed.skill.name, "Concise House Style");
    }
