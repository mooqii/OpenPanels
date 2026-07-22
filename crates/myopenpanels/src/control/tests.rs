#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::resolve_myopenpanels_paths;

    #[test]
    fn bootstrap_creates_project_with_all_default_panels() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");

        let bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");

        assert_eq!(bootstrap.project.title, "Project 1");
        assert_eq!(bootstrap.active_panel_kind, PanelKind::Wiki);
        assert_eq!(
            bootstrap
                .panels
                .iter()
                .map(|snapshot| snapshot.panel.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["wiki", "writing", "canvas", "typesetting", "publishing"]
        );
        assert_eq!(bootstrap.state["schemaVersion"], json!(4));
        assert_eq!(bootstrap.state["wikiSpaces"][0]["title"], json!("Wiki"));
        assert_eq!(bootstrap.state["activeWikiPagePath"], Value::Null);
        let writing = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Writing)
            .expect("writing panel");
        assert_eq!(writing.state["schemaVersion"], json!(5));
        assert_eq!(
            writing.state["selectedCreateWritingSkillIds"],
            json!(["writing-default"])
        );
        assert_eq!(
            writing.state["selectedRevisionWritingSkillId"],
            json!("writing-default")
        );
        let typesetting = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Typesetting)
            .expect("typesetting panel");
        assert_eq!(typesetting.panel.title, "排版");
        assert_eq!(
            typesetting.state,
            json!({ "schemaVersion": 2, "publications": [] })
        );
        let publishing = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Publishing)
            .expect("publishing panel");
        assert_eq!(publishing.panel.title, "发布");
        assert_eq!(publishing.state, crate::publishing::empty_state());
        assert!(paths.focus_dir.join("active-project.json").exists());
        assert!(paths.focus_dir.join("active-panel.json").exists());
        assert!(storage_dir
            .join(crate::storage::DATABASE_FILE_NAME)
            .exists());
    }

    #[test]
    fn bootstrap_backfills_new_default_panels_for_an_existing_project() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");
        let storage = Storage::open(&paths).expect("storage");
        let mut project =
            create_project_record(&storage, "Existing".to_owned()).expect("existing project");
        for kind in [PanelKind::Wiki, PanelKind::Writing, PanelKind::Canvas] {
            project = ensure_panel_for_project(&storage, &project, kind).expect("default panel");
        }
        drop(storage);

        let first = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let second = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        assert_eq!(
            first
                .panels
                .iter()
                .filter(|snapshot| snapshot.panel.kind == PanelKind::Typesetting)
                .count(),
            1
        );
        assert_eq!(
            first
                .panels
                .iter()
                .filter(|snapshot| snapshot.panel.kind == PanelKind::Publishing)
                .count(),
            1
        );
        assert_eq!(
            second
                .panels
                .iter()
                .map(|snapshot| snapshot.panel.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["wiki", "writing", "canvas", "typesetting", "publishing"]
        );
    }

    #[test]
    fn publishing_state_rejects_future_schema_versions() {
        let future = match resolve_publishing_state(Some(json!({
            "schemaVersion": 2
        }))) {
            Ok(_) => panic!("future state"),
            Err(error) => error,
        };
        assert!(future.message().contains("Unsupported future publishing"));
    }

    #[test]
    fn typesetting_state_rejects_future_and_malformed_documents() {
        let future = match resolve_typesetting_state(Some(json!({
            "schemaVersion": 3,
            "publications": []
        }))) {
            Ok(_) => panic!("future state"),
            Err(error) => error,
        };
        assert!(future.message().contains("Unsupported future typesetting"));

        let malformed = match resolve_typesetting_state(Some(json!({
            "schemaVersion": 2,
            "publications": [{
                "id": "publication:1",
                "title": "Broken",
                "covers": [],
                "content": null,
                "createdAt": "2026-07-14T00:00:00Z",
                "updatedAt": "2026-07-14T00:00:00Z"
            }]
        }))) {
            Ok(_) => panic!("malformed state"),
            Err(error) => error,
        };
        assert!(malformed.message().contains("Malformed typesetting"));

        let migrated = resolve_typesetting_state(Some(json!({
            "schemaVersion": 1,
            "publications": [{
                "id": "publication:1",
                "title": "Existing article",
                "covers": [{
                    "assetRef": "projects/project:1/panels/panel:canvas/assets/cover.png",
                    "fileName": "cover.png",
                    "mimeType": "image/png",
                    "sourceAssetRef": "projects/project:1/panels/panel:canvas/assets/source.png",
                    "sourceCanvasPanelId": "panel:canvas",
                    "sourceProjectId": "project:1",
                    "src": "/api/assets/cover.png"
                }],
                "content": { "type": "doc", "content": [{ "type": "paragraph" }] },
                "createdAt": "2026-07-14T00:00:00Z",
                "updatedAt": "2026-07-14T00:00:00Z"
            }]
        })))
        .expect("v1 state migration");
        assert!(migrated.changed);
        assert_eq!(migrated.state["schemaVersion"], json!(2));
        assert_eq!(
            migrated.state["publications"][0]["covers"][0]["source"],
            json!({
                "kind": "canvas",
                "assetRef": "projects/project:1/panels/panel:canvas/assets/source.png",
                "panelId": "panel:canvas",
                "projectId": "project:1"
            })
        );

        let uploaded = resolve_typesetting_state(Some(json!({
            "schemaVersion": 2,
            "publications": [{
                "id": "publication:upload",
                "title": "Uploaded cover",
                "covers": [{
                    "assetRef": "projects/project:1/panels/panel:typesetting/assets/cover.png",
                    "fileName": "cover.png",
                    "mimeType": "image/png",
                    "source": { "kind": "upload" },
                    "src": "/api/assets/cover.png"
                }],
                "content": { "type": "doc", "content": [{ "type": "paragraph" }] },
                "createdAt": "2026-07-22T00:00:00Z",
                "updatedAt": "2026-07-22T00:00:00Z"
            }]
        })))
        .expect("uploaded cover state");
        assert!(!uploaded.changed);
    }

    #[test]
    fn writing_state_only_accepts_schema_v5() {
        let current = empty_writing_state();
        let accepted = resolve_writing_state(Some(current.clone())).expect("current state");
        assert!(!accepted.changed);
        assert_eq!(accepted.state, current);

        let migrated = resolve_writing_state(Some(json!({
            "schemaVersion": 5,
            "draft": "Revise this paragraph",
            "mode": "revise",
            "refinementName": "",
            "targetGeneratedDocumentId": null,
            "selectedCreateWritingSkillIds": ["writing-default"],
            "selectedRevisionWritingSkillId": "writing-default",
            "selectedRefinementSkillId": "writing-skill-refiner"
        })))
        .expect("legacy draft state");
        assert!(migrated.changed);
        assert_eq!(migrated.state["createDraft"], json!(""));
        assert_eq!(
            migrated.state["revisionDraft"],
            json!("Revise this paragraph")
        );
        assert_eq!(
            migrated.state["selectedRefinementSkillId"],
            json!(crate::writing::DEFAULT_WRITING_REFINEMENT_SKILL_ID)
        );

        let error = match resolve_writing_state(Some(json!({ "schemaVersion": 4 }))) {
            Ok(_) => panic!("old state must be rejected"),
            Err(error) => error,
        };
        assert!(error.message().contains("expected schemaVersion 5"));
    }

    #[test]
    fn wiki_state_only_accepts_schema_v4() {
        let temp = tempfile::tempdir().expect("temp dir");
        let paths = resolve_myopenpanels_paths(
            Some(temp.path().to_str().unwrap()),
            Some(temp.path().join("storage").to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");
        let bootstrap = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("project");
        let panel = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
            .expect("wiki panel")
            .panel
            .clone();
        let storage = Storage::open(&paths).expect("storage");
        let current = empty_wiki_state();
        let accepted =
            resolve_wiki_state(&storage, &bootstrap.project, &panel, Some(current.clone()))
                .expect("current state");
        assert!(!accepted.changed);
        assert_eq!(accepted.state, current);

        let mut removed_skill_state = current;
        removed_skill_state["wikiAgentSkillId"] = json!("karpathy-llm-wiki");
        let migrated = resolve_wiki_state(
            &storage,
            &bootstrap.project,
            &panel,
            Some(removed_skill_state),
        )
        .expect("removed Skill state");
        assert!(migrated.changed);
        assert_eq!(
            migrated.state["wikiAgentSkillId"],
            json!("wiki-default")
        );

        let error = match resolve_wiki_state(
            &storage,
            &bootstrap.project,
            &panel,
            Some(json!({ "schemaVersion": 3 })),
        ) {
            Ok(_) => panic!("old state must be rejected"),
            Err(error) => error,
        };
        assert!(error.message().contains("expected schemaVersion 4"));
    }

    #[test]
    fn bootstrap_uses_the_global_studio_project_for_every_context() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let first_paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("thread-a"),
        )
        .expect("first paths");
        let second_paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("thread-b"),
        )
        .expect("second paths");
        let third_paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("thread-c"),
        )
        .expect("third paths");

        let first = ensure_project_bootstrap(&first_paths, BootstrapRequest::new()).expect("first");
        let latest = create_project(&second_paths, Some("Latest")).expect("latest");
        let third =
            ensure_project_bootstrap(&third_paths, BootstrapRequest::new()).expect("new context");
        let first_again =
            ensure_project_bootstrap(&first_paths, BootstrapRequest::new()).expect("again");

        assert_eq!(first.project.title, "Project 1");
        assert_eq!(first_again.project.id, latest.project.id);
        assert_eq!(third.project.id, latest.project.id);
        assert_eq!(third.projects.len(), 2);
    }

    #[test]
    fn newest_project_prefers_updated_time_then_created_time_and_id() {
        let project = |id: &str, created_at: &str, updated_at: &str| Project {
            id: id.to_owned(),
            title: id.to_owned(),
            created_at: created_at.to_owned(),
            updated_at: updated_at.to_owned(),
            panel_ids: Vec::new(),
        };
        let projects = vec![
            project(
                "project:older",
                "2026-07-11T10:00:00Z",
                "2026-07-11T12:00:00Z",
            ),
            project(
                "project:newer",
                "2026-07-11T11:00:00Z",
                "2026-07-11T13:00:00Z",
            ),
        ];

        assert_eq!(
            most_recently_updated_project(&projects).unwrap().id,
            "project:newer"
        );
    }

    #[test]
    fn bootstrap_errors_for_missing_requested_project_instead_of_creating_one() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");

        create_project(&paths, Some("Demo")).expect("demo project");
        write_active_project_id(&paths, "project:deleted").expect("stale active project");

        let error = ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_panel_id: None,
                requested_panel_kind: None,
                requested_project_id: Some("project:deleted".to_owned()),
            },
        )
        .expect_err("missing requested project should fail");

        assert_eq!(error.code(), Some("project_not_found"));
        let projects = Storage::open(&paths)
            .expect("storage")
            .list_projects()
            .expect("projects");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].title, "Demo");
    }

    #[test]
    fn bootstrap_recovers_stale_active_project_without_creating_one() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");

        let demo = create_project(&paths, Some("Demo")).expect("demo project");
        write_active_project_id(&paths, "project:deleted").expect("stale active project");

        let recovered =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");

        assert_eq!(recovered.project.id, demo.project.id);
        let projects = Storage::open(&paths)
            .expect("storage")
            .list_projects()
            .expect("projects");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].title, "Demo");
        assert_eq!(
            read_active_project_id(&paths).expect("active project"),
            Some(demo.project.id)
        );
    }

    #[test]
    fn bootstrap_rejects_wiki_v1_state() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");
        let bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let wiki_panel = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
            .expect("wiki panel")
            .panel
            .clone();
        Storage::open(&paths)
            .expect("storage")
            .write_panel_state(
                &bootstrap.project.id,
                &wiki_panel.id,
                &json!({
                    "schemaVersion": 1,
                    "pages": [{
                        "id": "page:notes",
                        "title": "Research Notes",
                        "path": "research/notes.md",
                        "markdown": "# Research Notes\n\nKept during migration.",
                        "createdAt": "2026-01-01T00:00:00.000Z",
                        "updatedAt": "2026-01-02T00:00:00.000Z"
                    }],
                    "activePageId": "page:notes"
                }),
            )
            .expect("write v1 state");

        let error = match read_project_bootstrap(&paths, BootstrapRequest::new()) {
            Ok(_) => panic!("old wiki state"),
            Err(error) => error,
        };
        assert!(error.message().contains("expected schemaVersion 4"));
    }

    #[test]
    fn bootstrap_rejects_malformed_wiki_state_instead_of_clearing_it() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");
        let bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let wiki_panel = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
            .expect("wiki panel")
            .panel
            .clone();
        let storage = Storage::open(&paths).expect("storage");
        storage
            .write_panel_state(
                &bootstrap.project.id,
                &wiki_panel.id,
                &json!({ "schemaVersion": 2, "rawDocuments": [] }),
            )
            .expect("write malformed state");
        drop(storage);

        let error = match read_project_bootstrap(&paths, BootstrapRequest::new()) {
            Ok(_) => panic!("malformed"),
            Err(error) => error,
        };
        assert!(error.message().contains("Malformed wiki panel state"));
    }

    #[test]
    fn bootstrap_rejects_future_wiki_state_version() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");
        let bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let wiki_panel = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
            .expect("wiki panel")
            .panel
            .clone();
        let storage = Storage::open(&paths).expect("storage");
        storage
            .write_panel_state(
                &bootstrap.project.id,
                &wiki_panel.id,
                &json!({ "schemaVersion": 5, "rawDocuments": [] }),
            )
            .expect("write future wiki state");
        drop(storage);

        let error = match read_project_bootstrap(&paths, BootstrapRequest::new()) {
            Ok(_) => panic!("future wiki"),
            Err(error) => error,
        };
        assert!(error.message().contains("expected schemaVersion 4"));
    }

    #[test]
    fn bootstrap_rejects_future_canvas_state_version() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");
        let bootstrap =
            ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
        let canvas_panel = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Canvas)
            .expect("canvas panel")
            .panel
            .clone();
        let storage = Storage::open(&paths).expect("storage");
        storage
            .write_panel_state(
                &bootstrap.project.id,
                &canvas_panel.id,
                &json!({ "schema": { "schemaVersion": 2 }, "store": {} }),
            )
            .expect("write future canvas state");
        drop(storage);

        let error = match read_project_bootstrap(&paths, BootstrapRequest::new()) {
            Ok(_) => panic!("future canvas"),
            Err(error) => error,
        };
        assert!(error.message().contains("Unsupported future canvas"));
    }
}
