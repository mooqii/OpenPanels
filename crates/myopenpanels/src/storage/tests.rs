#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::MyOpenPanelsPaths;
    use crate::types::{Panel, PanelKind, Project};
    use serde_json::json;
    use std::sync::{Arc, Barrier};
    use tempfile::tempdir;

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

    #[test]
    fn storage_writes_advance_change_seq() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");

        assert_eq!(storage.read_change_seq().expect("initial seq"), 0);

        let session = Project {
            id: "session:test".to_owned(),
            title: "Test".to_owned(),
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            updated_at: "2026-01-01T00:00:00.000Z".to_owned(),
            panel_ids: vec!["panel:canvas".to_owned()],
        };
        storage.write_project(&session).expect("write session");
        let after_session = storage.read_change_seq().expect("session seq");
        assert!(after_session > 0);

        let panel = Panel {
            id: "panel:canvas".to_owned(),
            project_id: session.id.clone(),
            kind: PanelKind::Canvas,
            title: "Canvas".to_owned(),
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            updated_at: "2026-01-01T00:00:00.000Z".to_owned(),
            state_ref: None,
        };
        storage.write_panel(&panel).expect("write panel");
        let after_panel = storage.read_change_seq().expect("panel seq");
        assert!(after_panel > after_session);

        storage
            .write_panel_state(
                &session.id,
                &panel.id,
                &json!({ "schema": { "schemaVersion": 1 }, "store": {} }),
            )
            .expect("write state");
        let after_state = storage.read_change_seq().expect("state seq");
        assert!(after_state > after_panel);
        storage
            .write_panel_state(
                &session.id,
                &panel.id,
                &json!({ "schema": { "schemaVersion": 1 }, "store": {} }),
            )
            .expect("repeat identical state");
        assert_eq!(
            storage.read_change_seq().expect("unchanged seq"),
            after_state
        );
        assert_eq!(
            storage
                .read_panel_state_revision(&session.id, &panel.id)
                .expect("state revision"),
            after_state
        );

        let stale_write = storage
            .write_panel_state_if_current(
                &session.id,
                &panel.id,
                &json!({ "schema": { "schemaVersion": 1 }, "store": { "stale": true } }),
                Some(after_panel),
            )
            .expect("stale write");
        assert_eq!(
            stale_write,
            Err(PanelStateWriteConflict {
                base_revision: after_panel,
                current_revision: after_state,
            })
        );

        storage
            .write_panel_selection(&session.id, &panel.id, &json!({ "selectedShapeIds": [] }))
            .expect("write selection");
        let after_selection = storage.read_change_seq().expect("selection seq");
        assert!(after_selection > after_state);
        storage
            .write_panel_selection(&session.id, &panel.id, &json!({ "selectedShapeIds": [] }))
            .expect("repeat identical selection");
        assert_eq!(
            storage.read_change_seq().expect("unchanged selection seq"),
            after_selection
        );
        let selection_scope_count: i64 = storage
            .connection
            .query_row(
                "SELECT COUNT(*) FROM change_scopes WHERE kind = 'panel_selection' AND project_id = ? AND panel_id = ?",
                params![session.id, panel.id],
                |row| row.get(0),
            )
            .expect("selection scope count");
        assert_eq!(selection_scope_count, 1);

        let schema_version: i64 = storage
            .connection
            .query_row(
                "SELECT schema_version FROM panel_states WHERE project_id = ? AND panel_id = ?",
                params![session.id, panel.id],
                |row| row.get(0),
            )
            .expect("schema version");
        assert_eq!(schema_version, 1);
    }

    #[test]
    fn concurrent_panel_state_cas_allows_exactly_one_writer() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");
        let project = Project {
            id: "project:cas".to_owned(),
            title: "CAS".to_owned(),
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            updated_at: "2026-01-01T00:00:00.000Z".to_owned(),
            panel_ids: vec!["panel:canvas".to_owned()],
        };
        storage.write_project(&project).expect("project");
        storage
            .write_panel(&Panel {
                id: "panel:canvas".to_owned(),
                project_id: project.id.clone(),
                kind: PanelKind::Canvas,
                title: "Canvas".to_owned(),
                created_at: project.created_at.clone(),
                updated_at: project.updated_at.clone(),
                state_ref: None,
            })
            .expect("panel");
        let base_revision = storage
            .write_panel_state(
                &project.id,
                "panel:canvas",
                &json!({ "schema": { "schemaVersion": 1 }, "store": { "value": 0 } }),
            )
            .expect("initial state");
        drop(storage);

        let barrier = Arc::new(Barrier::new(2));
        let handles = [1, 2].map(|value| {
            let paths = paths.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let storage = Storage::open(&paths).expect("concurrent storage");
                barrier.wait();
                storage
                    .write_panel_state_if_current(
                        "project:cas",
                        "panel:canvas",
                        &json!({
                            "schema": { "schemaVersion": 1 },
                            "store": { "value": value }
                        }),
                        Some(base_revision),
                    )
                    .expect("CAS write")
            })
        });
        let results = handles.map(|handle| handle.join().expect("writer"));
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
    }

    #[test]
    fn fresh_storage_has_one_complete_baseline() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");

        let table_count: i64 = storage
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )
            .expect("table count");
        assert_eq!(table_count, 28);

        let migrations: Vec<String> = storage
            .connection
            .prepare("SELECT id FROM schema_migrations ORDER BY id")
            .expect("migration query")
            .query_map([], |row| row.get(0))
            .expect("migration rows")
            .collect::<Result<_, _>>()
            .expect("migrations");
        assert_eq!(migrations, ["0001_initial"]);

        for removed in [
            "task_deliveries",
            "task_delivery_attempts",
            "dispatch_outbox",
            "content_migration_state",
        ] {
            let exists: bool = storage
                .connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?)",
                    [removed],
                    |row| row.get(0),
                )
                .expect("removed table check");
            assert!(!exists, "{removed} must not exist");
        }

        let trigger_count: i64 = storage
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'trigger'",
                [],
                |row| row.get(0),
            )
            .expect("trigger count");
        assert_eq!(trigger_count, 4);

        let removed_column_count: i64 = storage
            .connection
            .query_row(
                r#"
                SELECT
                  (SELECT COUNT(*) FROM pragma_table_info('storage_meta') WHERE name = 'layout_version') +
                  (SELECT COUNT(*) FROM pragma_table_info('agent_targets') WHERE name = 'endpoint')
                "#,
                [],
                |row| row.get(0),
            )
            .expect("removed column count");
        assert_eq!(removed_column_count, 0);

        let task_schema: String = storage
            .connection
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'tasks'",
                [],
                |row| row.get(0),
            )
            .expect("task schema");
        assert!(task_schema.contains("max_attempts INTEGER NOT NULL DEFAULT 8"));
        assert!(task_schema.contains("required_protocol_version = 3"));
        assert!(task_schema.contains("dispatch_mode IN ('auto', 'prefer')"));

        let foreign_key_errors: i64 = storage
            .connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("foreign key check");
        assert_eq!(foreign_key_errors, 0);
    }

    #[test]
    fn reopening_current_baseline_is_idempotent() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("first open");
        let initial_revision = storage.read_change_seq().expect("initial revision");
        let applied_at: String = storage
            .connection
            .query_row(
                "SELECT applied_at FROM schema_migrations WHERE id = '0001_initial'",
                [],
                |row| row.get(0),
            )
            .expect("applied at");
        drop(storage);

        let reopened = Storage::open(&paths).expect("second open");
        assert_eq!(
            reopened.read_change_seq().expect("reopened revision"),
            initial_revision
        );
        let reopened_applied_at: String = reopened
            .connection
            .query_row(
                "SELECT applied_at FROM schema_migrations WHERE id = '0001_initial'",
                [],
                |row| row.get(0),
            )
            .expect("reopened applied at");
        assert_eq!(reopened_applied_at, applied_at);
    }

    #[test]
    fn old_migration_history_is_rejected_without_modification() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        fs::create_dir_all(&paths.storage_dir).expect("storage dir");
        let database_path = paths.storage_dir.join(DATABASE_FILE_NAME);
        let connection = Connection::open(&database_path).expect("legacy database");
        connection
            .execute_batch(
                r#"
                CREATE TABLE schema_migrations (
                  id TEXT PRIMARY KEY NOT NULL,
                  description TEXT NOT NULL,
                  checksum TEXT NOT NULL,
                  applied_at TEXT NOT NULL
                );
                CREATE TABLE projects (id TEXT PRIMARY KEY NOT NULL, payload TEXT NOT NULL);
                INSERT INTO schema_migrations VALUES ('0018_wiki_mutation_lanes', 'old', 'old', 'old');
                INSERT INTO projects VALUES ('project:legacy', 'keep me');
                "#,
            )
            .expect("legacy fixture");
        drop(connection);
        let before = fs::read(&database_path).expect("database before");

        let error = Storage::open(&paths).expect_err("old baseline must be rejected");
        assert_eq!(error.code(), Some("incompatible_storage_baseline"));
        assert_eq!(fs::read(&database_path).expect("database after"), before);
    }

    #[test]
    fn unversioned_business_schema_is_rejected_without_modification() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        fs::create_dir_all(&paths.storage_dir).expect("storage dir");
        let database_path = paths.storage_dir.join(DATABASE_FILE_NAME);
        let connection = Connection::open(&database_path).expect("legacy database");
        connection
            .execute_batch(
                "CREATE TABLE projects (id TEXT PRIMARY KEY NOT NULL); INSERT INTO projects VALUES ('project:legacy');",
            )
            .expect("legacy fixture");
        drop(connection);
        let before = fs::read(&database_path).expect("database before");

        let error = Storage::open(&paths).expect_err("unversioned schema must be rejected");
        assert_eq!(error.code(), Some("incompatible_storage_baseline"));
        assert_eq!(fs::read(&database_path).expect("database after"), before);
    }

    #[test]
    fn baseline_checksum_mismatch_is_incompatible() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");
        storage
            .connection
            .execute(
                "UPDATE schema_migrations SET checksum = 'different' WHERE id = '0001_initial'",
                [],
            )
            .expect("checksum fixture");
        drop(storage);

        let error = Storage::open(&paths).expect_err("checksum mismatch");
        assert_eq!(error.code(), Some("incompatible_storage_baseline"));
    }

    #[test]
    fn project_task_sync_preserves_existing_times_when_task_omits_them() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");
        let session = Project {
            id: "session:test".to_owned(),
            title: "Test".to_owned(),
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            updated_at: "2026-01-01T00:00:00.000Z".to_owned(),
            panel_ids: vec!["panel:wiki".to_owned()],
        };
        storage.write_project(&session).expect("write session");
        let panel = Panel {
            id: "panel:wiki".to_owned(),
            project_id: session.id.clone(),
            kind: PanelKind::Wiki,
            title: "Wiki".to_owned(),
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            updated_at: "2026-01-01T00:00:00.000Z".to_owned(),
            state_ref: None,
        };
        storage.write_panel(&panel).expect("write panel");
        let state = json!({
            "tasks": [{
                "id": "task:missing-times",
                "type": "demo",
                "status": "queued",
                "targetId": "target",
            }],
        });

        storage
            .upsert_tasks(
                &session.id,
                &panel.id,
                "wiki",
                state["tasks"].as_array().unwrap(),
            )
            .expect("initial sync");
        storage
            .connection
            .execute(
                r#"
                UPDATE tasks
                SET
                  created_at = 'created:stable',
                  updated_at = 'updated:stable',
                  attempts = 2,
                  max_attempts = 5,
                  lease_owner = 'agent:test',
                  lease_expires_at = 'expires:stable',
                  last_heartbeat_at = 'heartbeat:stable',
                  retry_after = 'retry:stable'
                WHERE id = 'task:missing-times'
                "#,
                [],
            )
            .expect("seed stable times");
        storage
            .upsert_tasks(
                &session.id,
                &panel.id,
                "wiki",
                state["tasks"].as_array().unwrap(),
            )
            .expect("repeat sync");

        let tasks = storage.list_tasks(&session.id).expect("project tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["createdAt"], json!("created:stable"));
        assert_eq!(tasks[0]["updatedAt"], json!("updated:stable"));
        assert_eq!(tasks[0]["attempt"], json!(2));
        assert_eq!(tasks[0]["maxAttempts"], json!(5));
        assert_eq!(tasks[0]["lease"]["owner"], json!("agent:test"));
        assert_eq!(tasks[0]["lease"]["expiresAt"], json!("expires:stable"));
        assert_eq!(tasks[0]["lease"]["heartbeatAt"], json!("heartbeat:stable"));
        assert_eq!(tasks[0]["retryAfter"], json!("retry:stable"));
        let created_event_count: i64 = storage
            .connection
            .query_row(
                "SELECT COUNT(*) FROM task_events WHERE task_id = 'task:missing-times' AND event_type = 'created'",
                [],
                |row| row.get(0),
            )
            .expect("created event count");
        assert_eq!(created_event_count, 1);
    }
}
