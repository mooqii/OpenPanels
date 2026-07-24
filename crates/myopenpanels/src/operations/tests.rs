#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{
        activate_project_panel, create_project, ensure_project_bootstrap, read_active_project_id,
    };
    use crate::paths::resolve_myopenpanels_paths;
    use base64::Engine;

    fn test_paths() -> (tempfile::TempDir, MyOpenPanelsPaths) {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project).expect("project");
        let paths = resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("operation-test"),
        )
        .expect("paths");
        (temp, paths)
    }

    fn operation_record(id: &str, status: &str, completed_at: Option<&str>) -> Value {
        json!({
            "id": id,
            "ownerContextId": "operation-test",
            "intent": "canvas.image.generate",
            "status": status,
            "projectId": "session:test",
            "panelId": "panel:test",
            "panelKind": "canvas",
            "targetId": "shape:test",
            "baseRevision": 0,
            "guideId": null,
            "target": {},
            "input": {},
            "result": null,
            "error": null,
            "createdAt": "2026-01-01T00:00:00.000Z",
            "updatedAt": completed_at.unwrap_or("2026-01-01T00:00:00.000Z"),
            "completedAt": completed_at,
        })
    }

    #[test]
    fn cleanup_removes_only_expired_terminal_operation_artifacts() {
        let (_temp, paths) = test_paths();
        let bootstrap = crate::control::ensure_project_bootstrap(
            &paths,
            crate::control::BootstrapRequest::new(),
        )
        .expect("bootstrap");
        let storage = Storage::open(&paths).expect("storage");
        let cases = [
            (
                "operation:old-completed",
                "completed",
                Some("2026-01-01T00:00:00.000Z"),
            ),
            (
                "operation:old-cancelled",
                "cancelled",
                Some("2026-01-02T00:00:00.000Z"),
            ),
            (
                "operation:recent-completed",
                "completed",
                Some("2026-01-15T23:30:00.001Z"),
            ),
            (
                "operation:old-failed",
                "failed",
                Some("2026-01-01T00:00:00.000Z"),
            ),
            ("operation:active", "active", None),
        ];
        for (id, status, completed_at) in cases {
            let mut operation = operation_record(id, status, completed_at);
            operation["projectId"] = json!(bootstrap.project.id);
            operation["panelId"] = json!(bootstrap.panel.id);
            operation["panelKind"] = json!(bootstrap.panel.kind);
            storage
                .write_direct_operation(&operation)
                .expect("operation");
            let operation_dir = paths
                .storage_dir
                .join("operations")
                .join(sanitize_path_part(id));
            fs::create_dir_all(&operation_dir).expect("operation dir");
            fs::write(operation_dir.join("reference.png"), b"reference").expect("reference");
        }
        let abandoned = paths.storage_dir.join("operations/operation_orphaned");
        fs::create_dir_all(&abandoned).expect("abandoned operation dir");
        fs::write(abandoned.join("artifact.tmp"), b"orphaned").expect("orphaned artifact");

        cleanup_artifacts_with_storage(
            &paths,
            &storage,
            chrono::DateTime::parse_from_rfc3339("2026-01-16T00:00:00.000Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
        );

        let operation_dir = |id: &str| {
            paths
                .storage_dir
                .join("operations")
                .join(sanitize_path_part(id))
        };
        assert!(!operation_dir("operation:old-completed").exists());
        assert!(!operation_dir("operation:old-cancelled").exists());
        assert!(operation_dir("operation:recent-completed").exists());
        assert!(!operation_dir("operation:old-failed").exists());
        assert!(operation_dir("operation:active").exists());
        assert!(!abandoned.exists());
        assert!(storage
            .read_direct_operation("operation:old-completed")
            .expect("read operation")
            .is_some());
    }

    #[test]
    fn canvas_generation_completes_against_original_project_after_focus_switch() {
        let (_temp, paths) = test_paths();
        ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let mut request = BootstrapRequest::new();
        request.requested_panel_kind = Some(PanelKind::Canvas);
        let canvas = read_project_bootstrap(&paths, request).expect("canvas");
        let started = begin_canvas(&paths, Some(128.0), Some(128.0), false, None).expect("begin");
        assert_eq!(started["panelSkill"]["skill"]["id"], PANELS_SKILL_ID);
        assert_eq!(started["operation"]["skillId"], PANELS_SKILL_ID);
        assert!(started["panelSkill"]["referencePaths"][0]
            .as_str()
            .unwrap()
            .ends_with("references/canvas-contract.md"));
        assert!(started["operation"].get("protocolVersion").is_none());
        assert!(started["operation"]["guideId"].is_null());
        let operation_id = started["operation"]["id"].as_str().unwrap();
        let next_project = create_project(&paths, Some("Another")).expect("new project");
        let image = paths.storage_dir.join("operation-result.png");
        fs::write(&image, base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==").unwrap()).expect("image");
        let completed = complete_canvas(
            &paths,
            operation_id,
            image.to_str().unwrap(),
            json!({
                "generatedBy": "agent",
                "generateOptions": { "prompt": "test image", "referenceImages": [] }
            }),
        )
        .expect("complete");
        assert_eq!(completed["operation"]["projectId"], canvas.project.id);
        assert_eq!(
            read_active_project_id(&paths).unwrap(),
            Some(next_project.project.id)
        );
        let state = Storage::open(&paths)
            .unwrap()
            .read_panel_state(&canvas.project.id, &canvas.panel.id)
            .unwrap()
            .unwrap();
        let shape_id = completed["image"]["shapeId"].as_str().unwrap();
        assert_eq!(state["store"][shape_id]["type"], "image");
    }

    #[test]
    fn reference_generation_requires_explicit_selection() {
        let (_temp, paths) = test_paths();
        ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        activate_project_panel(&paths, PanelKind::Canvas).expect("activate canvas");
        let error = begin_canvas(&paths, None, None, true, None).expect_err("selection required");
        assert_eq!(error.code(), Some("explicit_selection_required"));
    }

    #[test]
    fn failed_direct_operations_do_not_leave_pending_targets() {
        let (_temp, paths) = test_paths();
        ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");

        let canvas_started =
            begin_canvas(&paths, Some(128.0), Some(128.0), false, None).expect("canvas begin");
        let canvas_operation_id = canvas_started["operation"]["id"].as_str().unwrap();
        let placeholder_id = canvas_started["operation"]["targetId"].as_str().unwrap();
        finish_canvas(
            &paths,
            canvas_operation_id,
            "failed",
            Some("generation failed"),
        )
        .expect("canvas fail");
        let canvas_project_id = canvas_started["operation"]["projectId"].as_str().unwrap();
        let canvas_panel_id = canvas_started["operation"]["panelId"].as_str().unwrap();
        let canvas_state = Storage::open(&paths)
            .unwrap()
            .read_panel_state(canvas_project_id, canvas_panel_id)
            .unwrap()
            .unwrap();
        assert!(canvas_state["store"][placeholder_id].is_null());
        assert_eq!(
            inspect(&paths, canvas_operation_id).unwrap()["status"],
            "failed"
        );

        let document_started =
            begin_my_document(&paths, "Failed draft", "markdown", None).expect("document begin");
        let document_operation_id = document_started["operation"]["id"].as_str().unwrap();
        let document_id = document_started["operation"]["targetId"].as_str().unwrap();
        finish_my_document(
            &paths,
            document_operation_id,
            "failed",
            Some("writing failed"),
        )
        .expect("document fail");
        let document_project_id = document_started["operation"]["projectId"].as_str().unwrap();
        let document_panel_id = document_started["operation"]["panelId"].as_str().unwrap();
        let document_state = Storage::open(&paths)
            .unwrap()
            .read_panel_state(document_project_id, document_panel_id)
            .unwrap()
            .unwrap();
        assert!(!document_state["myDocuments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|document| document["id"].as_str() == Some(document_id)));
        assert_eq!(
            inspect(&paths, document_operation_id).unwrap()["status"],
            "failed"
        );
    }

    #[test]
    fn my_document_write_completes_against_original_project_after_restart_or_switch() {
        let (_temp, paths) = test_paths();
        let wiki_bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let started = begin_my_document(&paths, "Report", "markdown", None).expect("begin");
        assert_eq!(
            started["panelSkill"]["skill"]["id"],
            PANELS_SKILL_ID
        );
        assert_eq!(started["operation"]["skillId"], PANELS_SKILL_ID);
        assert!(started["panelSkill"]["referencePaths"][0]
            .as_str()
            .unwrap()
            .ends_with("references/wiki-contract.md"));
        assert!(started["operation"].get("protocolVersion").is_none());
        assert!(started["operation"]["guideId"].is_null());
        let operation_id = started["operation"]["id"].as_str().unwrap().to_owned();
        create_project(&paths, Some("Another")).expect("switch project");
        let file = paths.storage_dir.join("report.md");
        fs::write(&file, "# Report\n\nDone.\n").expect("file");
        let completed =
            complete_my_document(&paths, &operation_id, file.to_str().unwrap()).expect("complete");
        assert_eq!(
            completed["operation"]["projectId"],
            wiki_bootstrap.project.id
        );
        assert_eq!(completed["document"]["contentVersion"], 1);
        assert!(completed["document"].get("writeOperation").is_none());
        assert_eq!(
            inspect(&paths, &operation_id).unwrap()["status"],
            "completed"
        );
        let document_id = completed["document"]["id"].as_str().unwrap();
        let active_path = paths
            .storage_dir
            .join("projects")
            .join(sanitize_path_part(&wiki_bootstrap.project.id))
            .join("content/my_document")
            .join(sanitize_path_part(document_id))
            .join("active.json");
        fs::remove_file(&active_path).expect("simulate interrupted activation");
        assert!(crate::content::active_resource_descriptor(
            &paths,
            &wiki_bootstrap.project.id,
            crate::content::ResourceKind::MyDocument,
            document_id,
        )
        .unwrap()
        .is_none());
        crate::content::recover_filesystem(&paths).expect("recover direct operation content");
        assert!(crate::content::active_resource_descriptor(
            &paths,
            &wiki_bootstrap.project.id,
            crate::content::ResourceKind::MyDocument,
            document_id,
        )
        .unwrap()
        .is_some());
    }

    #[test]
    fn my_document_write_detects_concurrent_document_updates() {
        let (_temp, paths) = test_paths();
        ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let created = wiki::create_my_document(
            &paths,
            "report.md",
            Some("Report"),
            None,
            None,
            None,
            b"# First",
        )
        .expect("document");
        let document_id = created["document"]["id"].as_str().unwrap();
        let started = begin_my_document(&paths, "Report", "markdown", Some(document_id)).expect("begin");
        let operation_id = started["operation"]["id"].as_str().unwrap();
        wiki::write_my_document(&paths, document_id, "report.md", None, b"# User edit")
            .expect("concurrent update");
        let file = paths.storage_dir.join("agent-report.md");
        fs::write(&file, "# Agent edit").expect("file");
        let error = complete_my_document(&paths, operation_id, file.to_str().unwrap())
            .expect_err("content conflict");
        assert_eq!(error.code(), Some("content_conflict"));
        assert_eq!(inspect(&paths, operation_id).unwrap()["status"], "active");
    }

    #[test]
    fn my_document_write_rejects_an_active_write_target() {
        let (_temp, paths) = test_paths();
        ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let started = begin_my_document(&paths, "Report", "markdown", None).expect("begin");
        let document_id = started["document"]["id"].as_str().unwrap();

        let error = wiki::write_my_document_for_agent(
            &paths,
            document_id,
            "report.md",
            None,
            b"# Written through the wrong path",
        )
        .expect_err("active generation should reject a direct write");

        assert_eq!(error.code(), Some("my_document_write_in_progress"));
        assert_eq!(
            wiki::read_my_document(&paths, document_id).unwrap()["document"]
                ["contentVersion"],
            0
        );
    }

}
