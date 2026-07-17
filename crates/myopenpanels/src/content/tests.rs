#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{ensure_project_bootstrap, BootstrapRequest};
    use crate::paths::resolve_myopenpanels_paths;
    use crate::types::PanelKind;

    #[test]
    fn converted_markdown_is_invisible_until_atomic_task_commit() {
        let _env_lock = crate::TASK_BROKER_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::tempdir().expect("temp");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("content-broker-test"),
        )
        .expect("paths");
        let bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let uploaded = crate::wiki::add_raw_document(
            &paths,
            "source.pdf",
            Some("Source"),
            Some("application/pdf"),
            "user",
            Some("wiki:default"),
            b"pdf fixture",
        )
        .expect("upload");
        let document_id = uploaded["document"]["id"].as_str().unwrap();
        let storage = Storage::open(&paths).expect("storage");
        let tasks = storage.list_tasks(&bootstrap.project.id).expect("tasks");
        let task_id = tasks
            .iter()
            .find(|task| task["type"] == "convert_document_to_markdown")
            .and_then(|task| task["id"].as_str())
            .expect("conversion task");
        storage
            .connection()
            .execute(
                "UPDATE tasks SET required_protocol_version = 3 WHERE id = ?",
                [task_id],
            )
            .expect("protocol");
        let _broker = crate::content::enable_test_task_broker();
        let target = crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "v3-converter",
                host: Some("test"),
                transport: "poll",
                capabilities: vec!["wiki.convertDocument".to_owned()],
                priority: 0,
                protocol_version: 3,
                max_concurrency: 1,
                model_gateway_connection_id: None,
            },
        )
        .expect("target");
        let claim =
            crate::tasks::claim_task(&paths, task_id, target["target"]["id"].as_str().unwrap())
                .expect("claim");
        assert_eq!(claim["executionProtocolVersion"], 3);
        let execution_token = claim["executionToken"].as_str().expect("execution token");
        stage_file(
            &paths,
            execution_token,
            &StageFileRequest {
                resource_kind: ResourceKind::WikiMarkdown.as_str().to_owned(),
                resource_key: document_id.to_owned(),
                logical_path: "source.md".to_owned(),
                content_base64: base64::engine::general_purpose::STANDARD.encode(b"# Converted\n"),
                mime_type: "text/markdown".to_owned(),
                metadata: json!({}),
            },
        )
        .expect("stage");
        assert_eq!(
            crate::wiki::read_markdown(&paths, document_id).expect("before")["markdown"],
            ""
        );
        crate::tasks::complete_task(&paths, task_id, claim["leaseToken"].as_str().unwrap(), None)
            .expect("complete");
        let wiki_panel = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
            .expect("wiki panel");
        let markdown_path = Storage::open(&paths)
            .expect("storage")
            .panel_dir(&bootstrap.project.id, &wiki_panel.panel.id)
            .join("raw")
            .join(document_id)
            .join("source.md");
        assert!(!markdown_path.exists());
        let after = crate::wiki::read_markdown(&paths, document_id).expect("after");
        assert_eq!(
            after["markdown"],
            "# Converted\n"
        );
        assert_eq!(after["markdownAccess"]["status"], "ready");
        assert!(after["markdownFilePath"]
            .as_str()
            .is_some_and(|path| Path::new(path).is_file()));
        let fenced = stage_file(
            &paths,
            execution_token,
            &StageFileRequest {
                resource_kind: ResourceKind::WikiMarkdown.as_str().to_owned(),
                resource_key: document_id.to_owned(),
                logical_path: "source.md".to_owned(),
                content_base64: base64::engine::general_purpose::STANDARD.encode(b"late"),
                mime_type: "text/markdown".to_owned(),
                metadata: json!({}),
            },
        )
        .expect_err("completed Attempt must be fenced");
        assert_eq!(fenced.code(), Some("execution_fenced"));
        let session_status: String = storage
            .connection()
            .query_row(
                "SELECT status FROM task_staging_sessions WHERE task_id = ?",
                [task_id],
                |row| row.get(0),
            )
            .expect("staging status");
        assert_eq!(session_status, "committed");
    }

    #[test]
    fn bridge_validated_wiki_no_change_completes_without_a_new_revision() {
        let _env_lock = crate::TASK_BROKER_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::tempdir().expect("temp");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("wiki-no-change-test"),
        )
        .expect("paths");
        let bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let written = crate::wiki::write_page(
            &paths,
            "wiki:default",
            "notes/user-page.md",
            "# User page\n",
            None,
            None,
        )
        .expect("write page");
        let task_id = written["task"]["id"].as_str().expect("task id");
        let storage = Storage::open(&paths).expect("storage");
        storage
            .connection()
            .execute(
                "UPDATE tasks SET required_protocol_version = 3 WHERE id = ?",
                [task_id],
            )
            .expect("protocol");
        let before: (String, i64) = storage
            .connection()
            .query_row(
                "SELECT active_revision_id, content_version FROM content_resources WHERE project_id = ? AND resource_kind = 'wiki_space' AND resource_key = 'wiki:default'",
                [&bootstrap.project.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("before revision");
        let _broker = crate::content::enable_test_task_broker();
        let target = crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "v3-wiki-maintainer",
                host: Some("test"),
                transport: "poll",
                capabilities: vec!["wiki.maintain".to_owned()],
                priority: 0,
                protocol_version: 3,
                max_concurrency: 1,
                model_gateway_connection_id: None,
            },
        )
        .expect("target");
        let claim = crate::tasks::claim_task(
            &paths,
            task_id,
            target["target"]["id"].as_str().expect("target id"),
        )
        .expect("claim");

        let completed = crate::tasks::complete_task(
            &paths,
            task_id,
            claim["leaseToken"].as_str().expect("lease"),
            Some(json!({
                "schemaVersion": 1,
                "outcome": "no_change",
                "summary": "The user page needs no related maintenance.",
                "changedPaths": [],
                "bridgeValidated": true,
            })),
        )
        .expect("complete no-change");

        assert_eq!(completed["task"]["status"], "succeeded");
        let after: (String, i64) = storage
            .connection()
            .query_row(
                "SELECT active_revision_id, content_version FROM content_resources WHERE project_id = ? AND resource_kind = 'wiki_space' AND resource_key = 'wiki:default'",
                [&bootstrap.project.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("after revision");
        assert_eq!(after, before);
    }

    #[test]
    fn prepared_writing_document_is_invisible_until_task_commit() {
        let _env_lock = crate::TASK_BROKER_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::tempdir().expect("temp");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("writing-content-broker-test"),
        )
        .expect("paths");
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
        let created = crate::writing::create_requests(
            &paths,
            "Write an atomic report",
            "create",
            None,
            &["writing-default".to_owned()],
        )
        .expect("request");
        let task_id = created["tasks"][0]["id"].as_str().expect("task id");
        let document_id = created["documents"][0]["id"].as_str().expect("document id");
        Storage::open(&paths)
            .expect("storage")
            .connection()
            .execute(
                "UPDATE tasks SET required_protocol_version = 3 WHERE id = ?",
                [task_id],
            )
            .expect("protocol");
        let _broker = crate::content::enable_test_task_broker();
        let target = crate::tasks::register_target(
            &paths,
            crate::tasks::TargetRegistration {
                name: "v3-writer",
                host: Some("test"),
                transport: "poll",
                capabilities: vec!["writing.generateDocument".to_owned()],
                priority: 0,
                protocol_version: 3,
                max_concurrency: 1,
                model_gateway_connection_id: None,
            },
        )
        .expect("target");
        let claim = crate::tasks::claim_task(
            &paths,
            task_id,
            target["target"]["id"].as_str().expect("target id"),
        )
        .expect("claim");
        let execution_token = claim["executionToken"].as_str().expect("execution token");
        let started = begin_operation(
            &paths,
            execution_token,
            &BeginOperationRequest {
                task_id: task_id.to_owned(),
                title: "Atomic report".to_owned(),
                document_format: "markdown".to_owned(),
            },
        )
        .expect("begin");
        let operation_id = started["operation"]["id"].as_str().expect("operation id");
        prepare_operation(
            &paths,
            execution_token,
            &PrepareOperationRequest {
                operation_id: operation_id.to_owned(),
                file_name: "report.md".to_owned(),
                content_base64: base64::engine::general_purpose::STANDARD
                    .encode(b"# Atomic report\n\nCommitted once.\n"),
            },
        )
        .expect("prepare");
        assert_eq!(
            crate::wiki::read_generated_document(&paths, document_id).expect("before")["content"],
            ""
        );
        crate::tasks::complete_task(
            &paths,
            task_id,
            claim["leaseToken"].as_str().expect("lease"),
            None,
        )
        .expect("complete");
        let after = crate::wiki::read_generated_document(&paths, document_id).expect("after");
        assert_eq!(after["content"], "# Atomic report\n\nCommitted once.\n");
        assert_eq!(after["contentAccess"]["status"], "ready");
        assert!(after["contentFilePath"]
            .as_str()
            .is_some_and(|path| Path::new(path).is_file()));
    }
}
