#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::MyOpenPanelsPaths;
    use crate::types::{Panel, PanelKind, Project};
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Arc, Barrier};
    use tempfile::tempdir;

    const TABLES: [&str; 16] = [
        "assets",
        "canvas_documents",
        "change_scopes",
        "documents",
        "panel_selections",
        "panels",
        "projects",
        "publications",
        "releases",
        "resources",
        "settings",
        "storage_meta",
        "task_resources",
        "tasks",
        "wiki_source_ingestions",
        "wiki_spaces",
    ];

    fn paths_for(storage_dir: PathBuf) -> MyOpenPanelsPaths {
        let studio_dir = storage_dir.join("studio");
        MyOpenPanelsPaths {
            context_dir: storage_dir.join("contexts").join("test"),
            context_id: "test".to_owned(),
            context_id_source: "test".to_owned(),
            focus_dir: studio_dir.join("focus"),
            project_dir: storage_dir.join("project"),
            studio_dir,
            storage_dir,
        }
    }

    fn project_and_panel(storage: &Storage, kind: PanelKind) -> (Project, Panel) {
        let project = Project {
            id: "project:test".to_owned(),
            title: "Test".to_owned(),
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            updated_at: "2026-01-01T00:00:00.000Z".to_owned(),
            panel_ids: vec![format!("panel:{}", kind.as_str())],
        };
        storage.write_project(&project).expect("project");
        let panel = Panel {
            id: format!("panel:{}", kind.as_str()),
            project_id: project.id.clone(),
            kind,
            title: "Panel".to_owned(),
            created_at: project.created_at.clone(),
            updated_at: project.updated_at.clone(),
            state_ref: None,
        };
        storage.write_panel(&panel).expect("panel");
        (project, panel)
    }

    #[test]
    fn fresh_storage_contains_only_the_current_tables() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");
        let mut tables = storage
            .connection()
            .prepare(
                "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .expect("table query")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("table rows")
            .collect::<Result<Vec<_>, _>>()
            .expect("table names");
        tables.sort();
        assert_eq!(tables, TABLES);
        assert_eq!(
            schema_version(storage.connection()).expect("version"),
            CURRENT_SCHEMA_VERSION
        );
        let database_id: String = storage
            .connection()
            .query_row("SELECT database_id FROM storage_meta WHERE id = 1", [], |row| {
                row.get(0)
            })
            .expect("database identity");
        assert_eq!(database_id.len(), 32);
        let schema_fingerprint: String = storage
            .connection()
            .query_row(
                "SELECT schema_fingerprint FROM storage_meta WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .expect("schema fingerprint");
        assert_eq!(schema_fingerprint, current_schema_fingerprint());
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('panels') WHERE name = 'state_json'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("panel columns"),
            0
        );
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('panels') WHERE name = 'ui_state_json'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("panel UI state column"),
            1
        );
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('panels') WHERE name = 'position'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("panel position"),
            1
        );
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('panels') WHERE name = 'title'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("panel title column"),
            0
        );
        assert_eq!(
            storage
                .connection()
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| row.get::<_, i64>(0))
                .expect("foreign keys"),
            0
        );
    }

    #[test]
    fn retry_limit_is_code_policy_not_database_schema() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");
        let schema: String = storage
            .connection()
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'tasks'",
                [],
                |row| row.get(0),
            )
            .expect("tasks schema");
        assert!(!schema.contains("max_attempt"));
        assert!(!schema.contains("attempt_count BETWEEN"));
        assert!(!schema.contains("dispatch_mode"));
        assert!(!schema.contains("preferred_runner_key"));
        assert_eq!(crate::tasks::TASK_EXECUTION_LIMIT, 3);

        let (project, panel) = project_and_panel(&storage, PanelKind::Wiki);
        let task = storage
            .insert_task(
                &project.id,
                &panel.id,
                "wiki",
                "maintain_wiki",
                "wiki.maintain",
                "wiki:default",
                &json!({}),
                &json!({ "agentSkillId": "wiki-default" }),
            )
            .expect("task");
        assert_eq!(task["attemptLimit"], 3);
        assert!(task.get("maxAttempts").is_none());
    }

    #[test]
    fn selection_revision_is_independent_from_panel_state() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");
        let (project, panel) = project_and_panel(&storage, PanelKind::Canvas);
        let state_revision = storage
            .write_panel_state(&project.id, &panel.id, &json!({ "store": {} }))
            .expect("state");
        storage
            .write_panel_selection(
                &project.id,
                &panel.id,
                &json!({ "selectedShapeIds": ["shape:1"] }),
            )
            .expect("selection");
        assert_eq!(
            storage
                .read_panel_state_revision(&project.id, &panel.id)
                .expect("state revision"),
            state_revision
        );
        let kinds = storage
            .read_changes_after(state_revision, Some(&project.id))
            .expect("changes")
            .1
            .into_iter()
            .map(|change| change.kind)
            .collect::<Vec<_>>();
        assert_eq!(kinds, ["panel_selection"]);
    }

    #[test]
    fn panel_state_compare_and_swap_allows_one_writer() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");
        let (project, panel) = project_and_panel(&storage, PanelKind::Canvas);
        let base = storage
            .write_panel_state(&project.id, &panel.id, &json!({ "store": {} }))
            .expect("base state");
        drop(storage);
        let barrier = Arc::new(Barrier::new(2));
        let handles = [1, 2].map(|value| {
            let paths = paths.clone();
            let barrier = Arc::clone(&barrier);
            let project_id = project.id.clone();
            let panel_id = panel.id.clone();
            std::thread::spawn(move || {
                let storage = Storage::open(&paths).expect("storage");
                barrier.wait();
                storage
                    .write_panel_state_if_current(
                        &project_id,
                        &panel_id,
                        &json!({ "store": { "value": value } }),
                        Some(base),
                    )
                    .expect("write")
            })
        });
        let results = handles.map(|handle| handle.join().expect("writer"));
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
    }

    #[test]
    fn wiki_resources_are_relational_and_deletion_keeps_revision_monotonic() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");
        let (project, panel) = project_and_panel(&storage, PanelKind::Wiki);
        let state = json!({
            "rawDocuments": [{
                "id": "raw:1",
                "title": "Source",
                "originalFileName": "source.md",
                "mimeType": "text/markdown",
                "source": "user",
                "markdownRef": "raw/raw:1/source.md",
                "markdownVersion": 1,
                "createdAt": "2026-01-01T00:00:00.000Z",
                "updatedAt": "2026-01-01T00:00:00.000Z"
            }],
            "myDocuments": [],
            "wikiSpaces": [{
                "id": "wiki:default",
                "title": "Wiki",
                "rootRef": "wikis/wiki:default",
                "pageIndex": [],
                "createdAt": "2026-01-01T00:00:00.000Z",
                "updatedAt": "2026-01-01T00:00:00.000Z"
            }],
            "activeRawDocumentId": "raw:1",
            "activeWikiSpaceId": "wiki:default"
        });
        let created_revision = storage
            .write_panel_state(&project.id, &panel.id, &state)
            .expect("state");
        let ui: Value = storage
            .connection()
            .query_row(
                "SELECT ui_state_json FROM panels WHERE id = ?",
                [&panel.id],
                |row| row.get::<_, String>(0),
            )
            .map(|raw| serde_json::from_str(&raw).expect("UI JSON"))
            .expect("UI state");
        assert!(ui.get("rawDocuments").is_none());
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT document_kind FROM documents WHERE resource_id = 'raw:1'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("document"),
            "wiki_source"
        );

        let mut deleted = state;
        deleted["rawDocuments"] = json!([]);
        deleted["activeRawDocumentId"] = Value::Null;
        let deleted_revision = storage
            .write_panel_state(&project.id, &panel.id, &deleted)
            .expect("delete");
        assert!(deleted_revision > created_revision);
        assert_eq!(
            storage
                .read_panel_state_revision(&project.id, &panel.id)
                .expect("revision"),
            deleted_revision
        );
        assert!(storage
            .connection()
            .query_row(
                "SELECT deleted_at FROM resources WHERE id = 'raw:1'",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .expect("deleted resource")
            .is_some());
    }

    #[test]
    fn schema_fingerprint_mismatch_is_rejected_without_changes() {
        let temp = tempdir().expect("tempdir");
        let storage_dir = temp.path().join(".myopenpanels");
        let paths = paths_for(storage_dir.clone());
        let storage = Storage::open(&paths).expect("storage");
        storage
            .write_setting("fixture", "value", r#""keep me""#)
            .expect("setting");
        storage
            .connection()
            .execute(
                "UPDATE storage_meta SET schema_fingerprint = 'old-schema' WHERE id = 1",
                [],
            )
            .expect("old fingerprint");
        fs::write(storage_dir.join("content-marker.txt"), "keep me").expect("content marker");
        drop(storage);

        let error = Storage::open(&paths).expect_err("schema mismatch");
        assert_eq!(error.code(), Some("storage_schema_mismatch"));
        assert_eq!(
            fs::read_to_string(storage_dir.join("content-marker.txt")).expect("preserved content"),
            "keep me"
        );
    }

    #[test]
    fn newer_database_is_refused_without_changes() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");
        storage
            .connection()
            .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 1)
            .expect("future version");
        drop(storage);
        let before = fs::read(paths.storage_dir.join(DATABASE_FILE_NAME)).expect("before");
        let error = Storage::open(&paths).expect_err("newer database");
        assert_eq!(error.code(), Some("storage_version_mismatch"));
        assert_eq!(
            fs::read(paths.storage_dir.join(DATABASE_FILE_NAME)).expect("after"),
            before
        );
    }

}
