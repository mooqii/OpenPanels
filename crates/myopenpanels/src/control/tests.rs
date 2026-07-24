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
        assert!(bootstrap.state.get("schemaVersion").is_none());
        assert_eq!(bootstrap.state["wikiSpaces"][0]["title"], json!("Wiki"));
        assert_eq!(bootstrap.state["activeWikiPagePath"], Value::Null);
        let writing = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Writing)
            .expect("writing panel");
        assert!(writing.state.get("schemaVersion").is_none());
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
            json!({ "publications": [] })
        );
        let publishing = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == PanelKind::Publishing)
            .expect("publishing panel");
        assert_eq!(publishing.panel.title, "发布");
        assert_eq!(publishing.state, crate::release::empty_state());
        assert!(paths.focus_dir.join("active-project.json").exists());
        assert!(paths.focus_dir.join("active-panel.json").exists());
        assert!(storage_dir
            .join(crate::storage::DATABASE_FILE_NAME)
            .exists());
    }

    #[test]
    fn new_projects_receive_distinct_wiki_space_identities() {
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
        let first_project =
            create_project_record(&storage, "First".to_owned()).expect("first project");
        let first_project =
            ensure_panel_for_project(&storage, &first_project, PanelKind::Wiki)
                .expect("first Wiki panel");
        let second_project =
            create_project_record(&storage, "Second".to_owned()).expect("second project");
        let second_project =
            ensure_panel_for_project(&storage, &second_project, PanelKind::Wiki)
                .expect("second Wiki panel");
        let first_panel_id = first_project.panel_ids.first().expect("first panel");
        let second_panel_id = second_project.panel_ids.first().expect("second panel");
        let first = storage
            .read_panel_state(&first_project.id, first_panel_id)
            .expect("first state")
            .expect("first Wiki");
        let second = storage
            .read_panel_state(&second_project.id, second_panel_id)
            .expect("second state")
            .expect("second Wiki");
        let first_id = first["activeWikiSpaceId"]
            .as_str()
            .expect("first Wiki Space");
        let second_id = second["activeWikiSpaceId"]
            .as_str()
            .expect("second Wiki Space");

        assert_ne!(first_id, second_id);
        assert!(first_id.starts_with("wiki:"));
        assert_eq!(first["wikiSpaces"][0]["id"], first_id);
        assert_eq!(
            first["wikiSpaces"][0]["rootRef"],
            format!("wikis/{first_id}")
        );
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM resources WHERE kind = 'wiki_space'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("Wiki Space resources"),
            2
        );
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
    fn publishing_state_rejects_malformed_values() {
        let malformed = match resolve_publishing_state(Some(json!([]))) {
            Ok(_) => panic!("malformed state"),
            Err(error) => error,
        };
        assert!(malformed.message().contains("Malformed publishing"));
    }

    #[test]
    fn typesetting_state_rejects_malformed_documents() {
        let malformed = match resolve_typesetting_state(Some(json!({
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

        let retired = resolve_typesetting_state(Some(json!({
            "publications": [{
                "id": "publication:1",
                "title": "Existing article",
                "covers": [{
                    "assetRef": "projects/project:1/content/asset/asset:cover/1/cover.png",
                    "fileName": "cover.png",
                    "mimeType": "image/png",
                    "sourceAssetRef": "projects/project:1/content/asset/asset:source/1/source.png",
                    "sourceCanvasPanelId": "panel:canvas",
                    "sourceProjectId": "project:1",
                    "src": "/api/assets/cover.png"
                }],
                "content": { "type": "doc", "content": [{ "type": "paragraph" }] },
                "createdAt": "2026-07-14T00:00:00Z",
                "updatedAt": "2026-07-14T00:00:00Z"
            }]
        })));
        assert!(retired.is_err());

        let uploaded = resolve_typesetting_state(Some(json!({
            "publications": [{
                "id": "publication:upload",
                "title": "Uploaded cover",
                "covers": [{
                    "assetRef": "projects/project:1/content/asset/asset:cover/1/cover.png",
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
    fn writing_state_validates_required_fields() {
        let current = empty_writing_state();
        let accepted = resolve_writing_state(Some(current.clone())).expect("current state");
        assert!(!accepted.changed);
        assert_eq!(accepted.state, current);

        let incomplete = resolve_writing_state(Some(json!({
            "draft": "Revise this paragraph",
            "mode": "revise",
            "distillationName": "",
            "targetMyDocumentId": null,
            "selectedCreateWritingSkillIds": ["writing-default"],
            "selectedRevisionWritingSkillId": "writing-default",
            "selectedDistillationSkillId": "writing-distillation-default"
        })));
        assert!(incomplete.is_err());

        let retired = resolve_writing_state(Some(json!({
            "createDraft": "Create this",
            "draft": "",
            "mode": "refine",
            "refinementName": "House style",
            "revisionDraft": "Revise this",
            "targetMyDocumentId": null,
            "selectedCreateWritingSkillIds": ["writing-default"],
            "selectedRevisionWritingSkillId": "writing-default",
            "selectedRefinementSkillId": "writing-refinement-default"
        })));
        assert!(retired.is_err());
    }

    #[test]
    fn wiki_state_validates_required_fields() {
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
    fn bootstrap_rejects_retired_wiki_shape() {
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
            .expect("write retired state");

        let error = match read_project_bootstrap(&paths, BootstrapRequest::new()) {
            Ok(_) => panic!("old wiki state"),
            Err(error) => error,
        };
        assert!(error.message().contains("Malformed wiki panel state"));
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
                &json!({ "rawDocuments": [] }),
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
    fn bootstrap_rejects_malformed_canvas_state() {
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
                &json!({ "store": [] }),
            )
            .expect("write malformed canvas state");
        drop(storage);

        let error = match read_project_bootstrap(&paths, BootstrapRequest::new()) {
            Ok(_) => panic!("malformed canvas"),
            Err(error) => error,
        };
        assert!(error.message().contains("Malformed canvas panel state"));
    }
}
