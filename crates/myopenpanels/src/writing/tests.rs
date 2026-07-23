#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::ensure_project_bootstrap;
    use crate::paths::resolve_myopenpanels_paths;
    use base64::Engine as _;
    use std::fs;

    fn test_paths() -> (tempfile::TempDir, MyOpenPanelsPaths) {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("writing-test"),
        )
        .expect("paths");
        (temp, paths)
    }

    fn default_writing_skill_ids() -> Vec<String> {
        vec!["writing-default".to_owned()]
    }

    fn write_custom_writing_skill(paths: &MyOpenPanelsPaths) {
        let skill_id = "writing-custom-test-style";
        let directory = paths.storage_dir.join("skills").join(skill_id);
        fs::create_dir_all(&directory).expect("custom Writing Skill dir");
        fs::write(
            directory.join("SKILL.md"),
            "---\nname: writing-custom-test-style\ndescription: Write concise test prose.\n---\n\nLead with the main point.\n",
        )
        .expect("custom Writing Skill");
        fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec_pretty(&json!({
                "source": "custom",
                "skillId": skill_id,
                "name": "Test Style",
                "binding": {
                    "moduleKinds": ["writing"],
                },
            }))
            .expect("custom Writing Skill manifest"),
        )
        .expect("custom Writing Skill manifest file");
    }

    #[test]
    fn writing_selection_is_independent_and_request_captures_it() {
        let (_temp, paths) = test_paths();
        let initial = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        write_custom_writing_skill(&paths);
        crate::wiki::write_agent_selection(&paths, &[]).expect("wiki selection");
        let writing = ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id.clone()),
            },
        )
        .expect("writing panel");
        write_selection(&paths, false, &[]).expect("writing selection");

        let storage = Storage::open(&paths).expect("storage");
        let wiki_panel = writing
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
            .expect("wiki panel");
        let wiki_selection = storage
            .read_panel_selection(&writing.project.id, &wiki_panel.panel.id)
            .expect("read wiki selection")
            .expect("stored wiki selection");
        let writing_selection = storage
            .read_panel_selection(&writing.project.id, &writing.panel.id)
            .expect("read writing selection")
            .expect("stored writing selection");
        assert!(wiki_selection.get("isWikiSelected").is_none());
        assert_eq!(writing_selection["isWikiSelected"], json!(false));

        let skill_ids = vec![
            "writing-custom-test-style".to_owned(),
            "writing-default".to_owned(),
        ];
        let created = create_requests(&paths, "Write a concise report", "create", None, &skill_ids)
            .expect("writing requests");
        assert_eq!(created["tasks"].as_array().unwrap().len(), 2);
        assert_eq!(created["documents"].as_array().unwrap().len(), 2);
        assert_eq!(created["documents"][0]["title"], json!(""));
        assert_eq!(created["documents"][0]["contentVersion"], json!(0));
        assert_eq!(
            created["tasks"][0]["input"]["targetMyDocumentId"],
            created["documents"][0]["id"]
        );
        assert_eq!(
            created["tasks"][0]["targetId"],
            created["documents"][0]["id"]
        );
        assert_eq!(created["tasks"][0]["queue"], json!("writing"));
        assert_eq!(created["tasks"][0]["capability"], json!(writing_task_capability(WRITING_TASK_CAPABILITY_KEY, "write_my_document")));
        assert_eq!(
            created["tasks"][0]["input"]["instruction"],
            json!("Write a concise report")
        );
        assert_eq!(
            created["tasks"][0]["input"]["writingSkillId"],
            json!("writing-custom-test-style")
        );
        assert!(
            created["tasks"][0]["input"]["writingSkillSnapshot"]["markdown"]
                .as_str()
                .is_some_and(|markdown| !markdown.is_empty())
        );
        assert_eq!(
            created["tasks"][0]["input"]["contextSnapshot"]["wikiRevision"],
            wiki_panel.revision
        );
        assert_eq!(
            created["state"]["selectedCreateWritingSkillIds"],
            json!(skill_ids)
        );
        assert_eq!(
            created["state"]["selectedRevisionWritingSkillId"],
            json!("writing-default")
        );
        assert_eq!(created["state"]["draft"], json!(""));

        let request = read_request(&paths, created["tasks"][0]["id"].as_str().unwrap())
            .expect("read request");
        assert_eq!(request["writingSkill"]["name"], json!("Test Style"));
        assert_eq!(
            request["actions"]["required"][0]["intent"],
            "agent.skill.read"
        );
        let loaded = crate::agent::read_agent_skill(
            &paths,
            "writing-custom-test-style",
            Some(created["tasks"][0]["id"].as_str().unwrap()),
        )
        .expect("task Writing Skill");
        assert!(loaded
            .markdown
            .contains("writing skill: writing-custom-test-style"));
    }

    #[test]
    fn writing_selection_exposes_materialized_my_document_access() {
        let (_temp, paths) = test_paths();
        let initial = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let my_document = crate::wiki::create_my_document(
            &paths,
            "reference.md",
            Some("Reference"),
            Some("text/markdown"),
            None,
            None,
            b"# Reference\n",
        )
        .expect("My Document");
        let writing = ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id),
            },
        )
        .expect("writing panel");
        write_selection(
            &paths,
            false,
            &[my_document["document"]["id"].as_str().unwrap().to_owned()],
        )
        .expect("selection");

        let selection = panel_selection(&paths, &writing).expect("agent selection");
        assert_eq!(
            selection["selectedMyDocuments"][0]["title"],
            "Reference"
        );
        assert_eq!(
            selection["selectedMyDocuments"][0]["contentAccess"]["status"],
            "ready"
        );
        assert!(selection["selectedMyDocuments"][0]["contentFilePath"]
            .as_str()
            .is_some_and(|path| std::path::Path::new(path).is_file()));
        assert_eq!(selection["wiki"]["localAccess"]["status"], "on_demand");
    }

    #[test]
    fn writing_skill_registry_and_submission_validation_are_authoritative() {
        let (_temp, paths) = test_paths();
        let initial = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let revision_target = crate::wiki::create_my_document(
            &paths,
            "target.md",
            Some("Target"),
            Some("text/markdown"),
            None,
            None,
            b"Target",
        )
        .expect("revision target");
        let revision_target_id = revision_target["document"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id.clone()),
            },
        )
        .expect("writing panel");
        write_custom_writing_skill(&paths);
        let skills = crate::agent::list_writing_agent_skills(&paths).expect("writing skills");
        assert_eq!(
            skills
                .iter()
                .map(|item| item.skill.id.as_str())
                .collect::<Vec<_>>(),
            vec!["writing-default", "writing-custom-test-style"]
        );
        assert!(skills
            .iter()
            .all(|item| item.skill.id != crate::agent::PANELS_SKILL_ID));

        let empty =
            create_requests(&paths, "Write", "create", None, &[]).expect_err("skill required");
        assert_eq!(empty.code(), Some("writing_skill_required"));
        let duplicate_ids = vec![
            "writing-default".to_owned(),
            "writing-default".to_owned(),
        ];
        let duplicate = create_requests(&paths, "Write", "create", None, &duplicate_ids)
            .expect_err("duplicate skill");
        assert_eq!(duplicate.code(), Some("duplicate_writing_skill"));
        let unknown = create_requests(
            &paths,
            "Write",
            "create",
            None,
            &["writing-unknown".to_owned()],
        )
        .expect_err("unknown skill");
        assert_eq!(unknown.code(), Some("writing_skill_not_found"));
        let multi_revision = create_requests(
            &paths,
            "Revise",
            "revise",
            Some(&revision_target_id),
            &[
                "writing-default".to_owned(),
                "writing-custom-test-style".to_owned(),
            ],
        )
        .expect_err("revision limit");
        assert_eq!(multi_revision.code(), Some("writing_revision_skill_limit"));
        assert!(Storage::open(&paths)
            .expect("storage")
            .list_tasks(&initial.project.id)
            .expect("tasks")
            .is_empty());
    }

    #[test]
    fn request_transaction_failure_removes_placeholder_files_and_state() {
        let (_temp, paths) = test_paths();
        let initial = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let writing = ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id.clone()),
            },
        )
        .expect("writing panel");
        let wiki_panel = writing
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
            .expect("wiki panel");
        let storage = Storage::open(&paths).expect("storage");
        let my_documents_dir = storage
            .panel_dir(&initial.project.id, &wiki_panel.panel.id)
            .join("my-documents");
        storage
            .connection()
            .execute_batch(
                "CREATE TRIGGER reject_writing_tasks BEFORE INSERT ON tasks
                 WHEN NEW.queue = 'writing'
                 BEGIN SELECT RAISE(ABORT, 'forced writing insert failure'); END;",
            )
            .expect("failure trigger");
        drop(storage);

        create_requests(
            &paths,
            "This request must roll back",
            "create",
            None,
            &default_writing_skill_ids(),
        )
        .expect_err("forced task failure");

        let storage = Storage::open(&paths).expect("storage after failure");
        assert!(storage
            .list_tasks(&initial.project.id)
            .expect("tasks")
            .is_empty());
        assert_eq!(
            storage
                .read_panel_state(&initial.project.id, &wiki_panel.panel.id)
                .expect("wiki state")
                .expect("wiki state exists")["myDocuments"]
                .as_array()
                .map(Vec::len),
            Some(0)
        );
        assert_eq!(
            fs::read_dir(my_documents_dir)
                .map(|entries| entries.count())
                .unwrap_or(0),
            0
        );
    }

    #[test]
    fn revision_requires_a_known_my_document() {
        let (_temp, paths) = test_paths();
        let initial = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id),
            },
        )
        .expect("writing panel");
        let skill_ids = default_writing_skill_ids();
        let draft = save_draft(
            &paths,
            "Revise this",
            "revise",
            "",
            None,
            &["writing-default".to_owned()],
            skill_ids.first().map(String::as_str),
        )
        .expect("incomplete revision draft");
        assert_eq!(draft["state"]["mode"], json!("revise"));
        assert_eq!(draft["state"]["targetMyDocumentId"], Value::Null);
        assert_eq!(
            draft["state"]["selectedCreateWritingSkillIds"],
            json!(["writing-default"])
        );
        assert_eq!(
            draft["state"]["selectedRevisionWritingSkillId"],
            json!("writing-default")
        );
        let error = create_requests(
            &paths,
            "Revise it",
            "revise",
            Some("my-document:missing"),
            &skill_ids,
        )
        .expect_err("missing target");
        assert_eq!(error.code(), Some("writing_target_not_found"));
    }

    #[test]
    fn create_and_revision_drafts_are_persisted_independently() {
        let (_temp, paths) = test_paths();
        let initial = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id),
            },
        )
        .expect("writing panel");
        let create_skills = vec!["writing-default".to_owned()];
        let revision_skill = "writing-default";

        save_draft(
            &paths,
            "Create a product brief",
            "create",
            "",
            None,
            &create_skills,
            Some(revision_skill),
        )
        .expect("create draft");
        let revised = save_draft(
            &paths,
            "Tighten the opening paragraph",
            "revise",
            "",
            None,
            &create_skills,
            Some(revision_skill),
        )
        .expect("revision draft");

        assert_eq!(
            revised["state"]["createDraft"],
            json!("Create a product brief")
        );
        assert_eq!(
            revised["state"]["revisionDraft"],
            json!("Tighten the opening paragraph")
        );
    }

    #[test]
    fn claimed_request_generates_into_the_linked_wiki_panel() {
        let _broker = crate::content::enable_test_task_broker();
        let (temp, paths) = test_paths();
        let initial = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id.clone()),
            },
        )
        .expect("writing panel");
        let skill_ids = default_writing_skill_ids();
        let created = create_requests(&paths, "Write the report", "create", None, &skill_ids)
            .expect("writing request");
        let task_id = created["tasks"][0]["id"].as_str().unwrap();
        let placeholder_id = created["documents"][0]["id"].as_str().unwrap().to_owned();
        assert_eq!(
            crate::wiki::read_my_document(&paths, &placeholder_id)
                .expect("pending document")["document"]["title"],
            json!("")
        );
        let registered = crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "writer",
                host: None,
                project_id: None,
                capabilities: vec![writing_task_capability(WRITING_TASK_CAPABILITY_KEY, "write_my_document").to_owned()],
                priority: 0,
                max_concurrency: 1,
                model_gateway_connection_id: None,
            },
        )
        .expect("target");
        let target_id = registered["target"]["id"].as_str().unwrap();
        let claimed = crate::tasks::claim_task(&paths, task_id, target_id).expect("claim");
        let lease_token = claimed["leaseToken"].as_str().unwrap();
        let execution_token = claimed["executionToken"].as_str().unwrap();
        let missing_output = crate::tasks::complete_task(&paths, task_id, lease_token, None)
            .expect_err("missing generation output");
        assert_eq!(missing_output.code(), Some("invalid_output"));
        let started = crate::content::begin_operation(
            &paths,
            execution_token,
            &crate::content::BeginOperationRequest {
                task_id: task_id.to_owned(),
                title: "Report".to_owned(),
                document_format: "markdown".to_owned(),
            },
        )
        .expect("generation");
        assert_eq!(started["document"]["id"], json!(placeholder_id));
        assert_eq!(started["document"]["title"], json!("Report"));
        let operation_id = started["operation"]["id"].as_str().unwrap();
        let early = crate::tasks::complete_task(&paths, task_id, lease_token, None)
            .expect_err("active operation");
        assert_eq!(early.code(), Some("writing_operation_active"));
        let artifact = temp.path().join("report.md");
        fs::write(&artifact, "# Report\n\nDone.\n").expect("artifact");
        crate::content::prepare_operation(
            &paths,
            execution_token,
            &crate::content::PrepareOperationRequest {
                operation_id: operation_id.to_owned(),
                file_name: "report.md".to_owned(),
                content_base64: base64::engine::general_purpose::STANDARD
                    .encode(fs::read(&artifact).expect("artifact bytes")),
            },
        )
        .expect("prepare operation");
        let task = crate::tasks::complete_task(
            &paths,
            task_id,
            lease_token,
            Some(json!({ "myDocumentId": placeholder_id })),
        )
        .expect("complete task");
        assert_eq!(task["task"]["status"], json!("succeeded"));

        let cancelled = create_requests(&paths, "Write another report", "create", None, &skill_ids)
            .expect("second request");
        let cancelled_task_id = cancelled["tasks"][0]["id"].as_str().unwrap();
        crate::tasks::claim_task(&paths, cancelled_task_id, target_id).expect("second claim");
        let cancelled_operation = crate::operations::begin_writing(
            &paths,
            cancelled_task_id,
            "Cancelled report",
            "markdown",
        )
        .expect("second generation");
        let cancelled_operation_id = cancelled_operation["operation"]["id"].as_str().unwrap();
        let cancelled_task =
            crate::tasks::cancel_task(&paths, cancelled_task_id).expect("cancel task");
        assert_eq!(cancelled_task["task"]["status"], json!("cancelled"));
        assert_eq!(
            crate::operations::inspect(&paths, cancelled_operation_id)
                .expect("cancelled operation")["status"],
            json!("cancelled")
        );
        assert!(crate::wiki::read_my_document(
            &paths,
            cancelled["documents"][0]["id"].as_str().unwrap()
        )
        .is_err());

        let released = create_requests(&paths, "Write after release", "create", None, &skill_ids)
            .expect("released request");
        let released_task_id = released["tasks"][0]["id"].as_str().unwrap();
        let released_document_id = released["documents"][0]["id"].as_str().unwrap();
        let released_claim =
            crate::tasks::claim_task(&paths, released_task_id, target_id).expect("release claim");
        crate::operations::begin_writing(&paths, released_task_id, "Released report", "markdown")
            .expect("released generation");
        crate::tasks::release_task(
            &paths,
            released_task_id,
            released_claim["leaseToken"].as_str().unwrap(),
        )
        .expect("release task");
        assert!(crate::wiki::read_my_document(&paths, released_document_id).is_ok());
        let failed_claim =
            crate::tasks::claim_task(&paths, released_task_id, target_id).expect("failed claim");
        crate::operations::begin_writing(&paths, released_task_id, "Failed report", "markdown")
            .expect("failed generation");
        let retry_now = crate::control::now_iso();
        let failed_task = crate::tasks::fail_task(
            &paths,
            released_task_id,
            failed_claim["leaseToken"].as_str().unwrap(),
            "Model failed",
            Some(&retry_now),
        )
        .expect("fail task");
        assert_eq!(failed_task["task"]["status"], json!("queued"));
        assert!(crate::wiki::read_my_document(&paths, released_document_id).is_ok());
        let final_claim =
            crate::tasks::claim_task(&paths, released_task_id, target_id).expect("final claim");
        let final_task = crate::tasks::fail_task(
            &paths,
            released_task_id,
            final_claim["leaseToken"].as_str().unwrap(),
            "Model failed again",
            None,
        )
        .expect("final failure");
        assert_eq!(final_task["task"]["status"], json!("failed"));
        assert!(crate::wiki::read_my_document(&paths, released_document_id).is_ok());
    }

    #[test]
    fn revision_rejects_a_target_changed_after_submission() {
        let (_temp, paths) = test_paths();
        let initial = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let document = crate::wiki::create_my_document(
            &paths,
            "draft.md",
            Some("Draft"),
            Some("text/markdown"),
            None,
            None,
            b"Initial",
        )
        .expect("document");
        let document_id = document["document"]["id"].as_str().unwrap().to_owned();
        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id.clone()),
            },
        )
        .expect("writing panel");
        let created = create_requests(
            &paths,
            "Revise the draft",
            "revise",
            Some(&document_id),
            &default_writing_skill_ids(),
        )
        .expect("writing request");
        assert_eq!(created["documents"].as_array().unwrap().len(), 1);
        assert_eq!(created["documents"][0]["id"], json!(document_id));
        assert_eq!(
            created["tasks"][0]["input"]["targetMyDocumentId"],
            created["documents"][0]["id"]
        );
        assert_eq!(
            created["tasks"][0]["input"]["targetDocumentSnapshot"]["snapshotContent"],
            json!("Initial")
        );
        assert_eq!(
            created["tasks"][0]["input"]["targetDocumentSnapshot"]["contentVersion"],
            json!(1)
        );
        let task_id = created["tasks"][0]["id"].as_str().unwrap();

        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Wiki),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id),
            },
        )
        .expect("wiki panel");
        crate::wiki::write_my_document(
            &paths,
            &document_id,
            "draft.md",
            Some("text/markdown"),
            b"Changed after submission",
        )
        .expect("concurrent edit");
        let registered = crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "writer",
                host: None,
                project_id: None,
                capabilities: vec![writing_task_capability(WRITING_TASK_CAPABILITY_KEY, "write_my_document").to_owned()],
                priority: 0,
                max_concurrency: 1,
                model_gateway_connection_id: None,
            },
        )
        .expect("target");
        let claimed = crate::tasks::claim_task(
            &paths,
            task_id,
            registered["target"]["id"].as_str().unwrap(),
        )
        .expect("claim");
        let error = crate::operations::begin_writing(&paths, task_id, "Draft", "markdown")
            .expect_err("content conflict");
        assert_eq!(error.code(), Some("content_conflict"));
        let superseded = crate::tasks::inspect_task(&paths, task_id).expect("superseded task");
        assert_eq!(superseded["task"]["status"], json!("superseded"));
        assert_eq!(
            superseded["task"]["terminalReason"]["code"],
            json!("content_conflict")
        );
        let fenced = crate::tasks::complete_task(
            &paths,
            task_id,
            claimed["leaseToken"].as_str().unwrap(),
            None,
        )
        .expect_err("old execution fenced");
        assert_eq!(fenced.code(), Some("execution_fenced"));
    }

    #[test]
    fn distillation_ignores_wiki_and_requires_ready_selected_documents() {
        let (_temp, paths) = test_paths();
        let initial = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let my_document = crate::wiki::create_my_document(
            &paths,
            "ready.md",
            Some("Ready"),
            Some("text/markdown"),
            None,
            None,
            b"# Ready\n",
        )
        .expect("ready My Document");
        let my_document_id = my_document["document"]["id"].as_str().unwrap().to_owned();
        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id),
            },
        )
        .expect("writing panel");
        write_selection(&paths, true, &[]).expect("wiki-only selection");
        let missing = create_distillation_request(&paths, "My style").expect_err("source required");
        assert_eq!(missing.code(), Some("writing_distillation_source_required"));

        write_selection(&paths, true, std::slice::from_ref(&my_document_id))
            .expect("ready source selected");
        assert_eq!(
            create_distillation_request(&paths, "  ")
                .expect_err("empty name")
                .code(),
            Some("writing_skill_name_required")
        );
        assert_eq!(
            create_distillation_request(&paths, &"x".repeat(81))
                .expect_err("long name")
                .code(),
            Some("writing_skill_name_too_long")
        );
        assert_eq!(
            create_distillation_request(&paths, "默认写作")
                .expect_err("built-in conflict")
                .code(),
            Some("writing_skill_name_conflict")
        );
        create_distillation_request(&paths, "My Style").expect("valid distillation");
        assert_eq!(
            create_distillation_request(&paths, " my style ")
                .expect_err("pending conflict")
                .code(),
            Some("writing_skill_name_conflict")
        );
    }

    #[test]
    fn distillation_installs_a_shared_custom_writing_skill() {
        let _broker = crate::content::enable_test_task_broker();
        let (temp, paths) = test_paths();
        let initial = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let my_document = crate::wiki::create_my_document(
            &paths,
            "sample.md",
            Some("Sample"),
            Some("text/markdown"),
            None,
            None,
            b"# Sample\n\nShort, direct paragraphs.",
        )
        .expect("My Document");
        let my_document_id = my_document["document"]["id"].as_str().unwrap().to_owned();
        let second_my_document = crate::wiki::create_my_document(
            &paths,
            "second-sample.md",
            Some("Second sample"),
            Some("text/markdown"),
            None,
            None,
            b"# Second sample\n\nA second reusable example.",
        )
        .expect("second My Document");
        let second_my_document_id = second_my_document["document"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(initial.project.id.clone()),
            },
        )
        .expect("writing panel");
        write_selection(
            &paths,
            true,
            &[my_document_id.clone(), second_my_document_id.clone()],
        )
        .expect("writing selection");

        let created =
            create_distillation_request(&paths, "Concise House Style").expect("distillation request");
        let task = &created["task"];
        assert_eq!(task["type"], json!("distill_writing_skill"));
        assert_eq!(task["capability"], json!(writing_task_capability(WRITING_DISTILLATION_TASK_CAPABILITY_KEY, "distill_writing_skill")));
        assert!(task["input"]["context"].get("isWikiSelected").is_none());
        assert_eq!(
            task["input"]["context"]["selectedMyDocumentIds"],
            json!([my_document_id, second_my_document_id])
        );
        assert!(task["input"]["contextSnapshot"]["myDocuments"]
            .as_array()
            .is_some_and(|documents| documents.iter().any(|document| {
                document["snapshotContent"] == json!("# Sample\n\nShort, direct paragraphs.")
            })));
        assert!(task["input"]["contextSnapshot"]
            .get("wikiSelection")
            .is_none());
        assert_eq!(
            task["input"]["distillerSkillSnapshot"]["id"],
            json!(DEFAULT_WRITING_DISTILLATION_SKILL_ID)
        );
        assert!(task["input"]["distillerSkillSnapshot"]["markdown"]
            .as_str()
            .is_some_and(|markdown| markdown.contains("Create one reusable Writing Skill")));
        let duplicate = create_distillation_request(&paths, " concise house style ")
            .expect_err("pending name conflict");
        assert_eq!(duplicate.code(), Some("writing_skill_name_conflict"));

        let task_id = task["id"].as_str().unwrap();
        let skill_id = task["input"]["skillId"].as_str().unwrap();
        let registered = crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "distiller",
                host: None,
                project_id: None,
                capabilities: vec![writing_task_capability(WRITING_DISTILLATION_TASK_CAPABILITY_KEY, "distill_writing_skill").to_owned()],
                priority: 0,
                max_concurrency: 1,
                model_gateway_connection_id: None,
            },
        )
        .expect("target");
        let claimed = crate::tasks::claim_task(
            &paths,
            task_id,
            registered["target"]["id"].as_str().unwrap(),
        )
        .expect("claim");
        let lease = claimed["leaseToken"].as_str().unwrap();
        let execution_token = claimed["executionToken"].as_str().unwrap();
        let early = crate::tasks::complete_task(&paths, task_id, lease, None)
            .expect_err("skill must be installed");
        assert_eq!(early.code(), Some("writing_skill_not_installed"));

        let skill_file = temp.path().join("SKILL.md");
        fs::write(
            &skill_file,
            format!(
                "---\nname: {skill_id}\ndescription: Write with concise, direct paragraphs.\n---\n\nUse short, direct paragraphs and remove redundant setup.\n"
            ),
        )
        .expect("skill file");
        let source = fs::read_to_string(&skill_file).expect("skill source");
        crate::content::prepare_skill(
            &paths,
            execution_token,
            &crate::content::PrepareSkillRequest {
                skill_id: skill_id.to_owned(),
                source: source.clone(),
                manifest: json!({}),
            },
        )
        .expect("prepare skill");
        crate::content::prepare_skill(
            &paths,
            execution_token,
            &crate::content::PrepareSkillRequest {
                skill_id: skill_id.to_owned(),
                source,
                manifest: json!({}),
            },
        )
        .expect("idempotent prepare");
        crate::tasks::complete_task(&paths, task_id, lease, None).expect("complete task");

        let custom_skill =
            crate::agent::writing_agent_skill(&paths, skill_id).expect("custom Writing Skill");
        assert!(Path::new(&custom_skill.local_path)
            .components()
            .any(|component| component.as_os_str() == "skills"));
        create_requests(
            &paths,
            "Write with the extracted method",
            "create",
            None,
            &[skill_id.to_owned()],
        )
        .expect("use custom skill");
        crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "writer",
                host: None,
                project_id: None,
                capabilities: vec![writing_task_capability(WRITING_TASK_CAPABILITY_KEY, "write_my_document").to_owned()],
                priority: 0,
                max_concurrency: 1,
                model_gateway_connection_id: None,
            },
        )
        .expect("writer target");
        let pending =
            crate::agent_control::pending_entry_skill_update(&paths)
                .expect("entry skill requirement")
                .expect("pending entry skill update");
        crate::agent_control::acknowledge_entry_skill_update(
            &paths,
            &pending.event_id,
            crate::agent_control::ENTRY_SKILL_VERSION,
        )
        .expect("acknowledge entry skill");
        let agent_bootstrap = crate::agent::agent_bootstrap(&paths, None)
            .expect("agent bootstrap");
        let bootstrap_loads_custom_skill = agent_bootstrap["skills"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|skill| skill["id"].as_str() == Some(skill_id));
        assert!(
            bootstrap_loads_custom_skill,
            "bootstrap did not load the custom skill: {agent_bootstrap:#}"
        );

        let other = crate::control::create_project(&paths, Some("Other")).expect("other project");
        ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_kind: Some(PanelKind::Writing),
                requested_panel_id: None,
                requested_project_id: Some(other.project.id),
            },
        )
        .expect("other writing panel");
        assert!(crate::agent::list_writing_agent_skills(&paths)
            .expect("other skills")
            .iter()
            .any(|item| item.skill.id == skill_id));

        let files = read_skill_files(&paths, skill_id).expect("read custom skill files");
        let source = files["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["path"] == "SKILL.md")
            .and_then(|file| file["content"].as_str())
            .unwrap();
        let edited = source.replace(
            "Use short, direct paragraphs",
            "Use crisp, direct paragraphs",
        );
        write_custom_skill_file(&paths, skill_id, "SKILL.md", &edited).expect("edit custom skill");
        assert!(
            read_skill_files(&paths, skill_id).expect("read edited skill")["files"][0]["content"]
                .as_str()
                .unwrap()
                .contains("crisp, direct")
        );
        assert_eq!(
            delete_custom_skill(&paths, "writing-default")
                .expect_err("built-in delete must fail")
                .code(),
            Some("writing_skill_read_only")
        );
        delete_custom_skill(&paths, skill_id).expect("delete custom skill");
        assert!(crate::agent::writing_agent_skill(&paths, skill_id).is_err());
    }

}
