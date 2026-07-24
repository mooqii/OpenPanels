#[cfg(test)]
mod migration_tests {
    use super::*;
    use crate::paths::MyOpenPanelsPaths;
    use serde_json::{json, Value};
    use std::{fs, path::PathBuf, sync::{Arc, Barrier}};
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

    fn create_version_one_storage(paths: &MyOpenPanelsPaths) -> Connection {
        fs::create_dir_all(&paths.storage_dir).expect("storage dir");
        let mut connection =
            Connection::open(paths.storage_dir.join(DATABASE_FILE_NAME)).expect("database");
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("transaction");
        tx.execute_batch(MIGRATIONS[0].sql).expect("initial schema");
        tx.execute(
            "UPDATE storage_meta SET schema_fingerprint = ? WHERE id = 1",
            [migration_checksum(&MIGRATIONS[0])],
        )
        .expect("baseline fingerprint");
        tx.pragma_update(None, "user_version", 1)
            .expect("schema version");
        tx.commit().expect("initial commit");
        connection
    }

    fn backup_directories(paths: &MyOpenPanelsPaths) -> Vec<PathBuf> {
        let parent = storage_backup_parent(paths);
        let mut directories = fs::read_dir(parent)
            .expect("backup parent")
            .map(|entry| entry.expect("backup entry").path())
            .collect::<Vec<_>>();
        directories.sort();
        directories
    }

    #[test]
    fn applied_migration_journals_are_removed() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        drop(Storage::open(&paths).expect("storage"));
        assert!(!paths.storage_dir.join(".migrations").exists());

        let journal_dir = paths.storage_dir.join(".migrations");
        fs::create_dir_all(&journal_dir).expect("journal directory");
        for name in [
            "0004_content_objects.json",
            "0005_asset_objects.json",
            "0008_stable_directory_keys.json",
        ] {
            fs::write(journal_dir.join(name), "{}").expect("stale journal");
        }

        drop(Storage::open(&paths).expect("reopened storage"));
        assert!(!journal_dir.exists());
    }

    #[test]
    fn version_one_fingerprint_mismatch_is_rejected_without_changes() {
        let temp = tempdir().expect("tempdir");
        let storage_dir = temp.path().join(".myopenpanels");
        let paths = paths_for(storage_dir.clone());
        let connection = create_version_one_storage(&paths);
        connection
            .execute(
                "UPDATE storage_meta SET schema_fingerprint = 'old-schema' WHERE id = 1",
                [],
            )
            .expect("old fingerprint");
        fs::write(storage_dir.join("content-marker.txt"), "keep me").expect("content marker");
        drop(connection);

        let error = Storage::open(&paths).expect_err("schema mismatch");
        assert_eq!(error.code(), Some("storage_schema_mismatch"));
        assert_eq!(
            fs::read_to_string(storage_dir.join("content-marker.txt")).expect("preserved content"),
            "keep me"
        );
        let connection =
            Connection::open(storage_dir.join(DATABASE_FILE_NAME)).expect("original database");
        assert_eq!(schema_version(&connection).expect("version"), 1);
    }

    #[test]
    fn version_one_storage_is_backed_up_and_migrated() {
        let temp = tempdir().expect("tempdir");
        let storage_dir = temp.path().join(".myopenpanels");
        let paths = paths_for(storage_dir);
        let connection = create_version_one_storage(&paths);
        connection
            .execute(
                "INSERT INTO settings (key, value_json, updated_at) VALUES ('fixture', '\"keep me\"', '2026-01-01T00:00:00.000Z')",
                [],
            )
            .expect("fixture setting");
        connection
            .execute_batch(
                r#"
                INSERT INTO projects (id, title, root_path, created_at, updated_at)
                VALUES ('project:fixture', 'Fixture', '/fixture',
                        '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z');
                INSERT INTO panels (
                  project_id, id, kind, position, created_at, updated_at
                ) VALUES (
                  'project:fixture', 'panel:fixture', 'typesetting', 0,
                  '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'
                );
                INSERT INTO resources (
                  id, project_id, kind, title, revision, created_at, updated_at
                ) VALUES
                  ('document:fixture', 'project:fixture', 'document', 'Document', 1,
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'),
                  ('publication:fixture', 'project:fixture', 'publication', 'Publication', 2,
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'),
                  ('release:fixture', 'project:fixture', 'release', 'Release', 3,
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z');
                INSERT INTO documents (
                  resource_id, document_kind, active_revision_id, content_version,
                  content_hash, metadata_json
                ) VALUES (
                  'document:fixture', 'my_document', 'content.md', 4, 'legacy-hash', '{}'
                );
                INSERT INTO publications (
                  resource_id, source_document_id, content_version, content_hash,
                  selected_title, config_json
                ) VALUES (
                  'publication:fixture', 'document:fixture', 5, 'legacy-config-hash',
                  'Title', '{"id":"publication:fixture","title":"Publication","contentVersion":5}'
                );
                INSERT INTO releases (
                  resource_id, publication_id, platform_key, release_json
                ) VALUES (
                  'release:fixture', 'publication:fixture', 'xiaohongshu',
                  '{"id":"release:fixture","platform":"xiaohongshu","sourcePublicationId":"publication:fixture","sourceUpdatedAt":"2026-01-01T00:01:00.000Z","snapshot":{"title":"Captured title","bodyText":"Captured body","tags":[],"media":[]},"createdAt":"2026-01-01T00:00:00.000Z","updatedAt":"2026-01-01T00:01:00.000Z"}'
                );
                INSERT INTO direct_operations (
                  id, owner_context_id, intent, status, project_id, panel_id,
                  target_id, base_revision, operation_json, created_at, updated_at
                ) VALUES (
                  'operation:fixture', 'test', 'canvas.image.generate', 'active',
                  'project:fixture', 'panel:fixture', 'shape:fixture', 7,
                  '{"id":"operation:fixture","ownerContextId":"test","intent":"canvas.image.generate","status":"active","projectId":"project:fixture","panelId":"panel:fixture","targetId":"shape:fixture","baseRevision":7,"target":{"placeholderShapeId":"shape:fixture"},"input":{},"result":null,"error":null,"createdAt":"2026-01-01T00:00:00.000Z","updatedAt":"2026-01-01T00:00:00.000Z","completedAt":null}',
                  '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'
                );
                "#,
            )
            .expect("relational fixtures");
        drop(connection);

        let storage = Storage::open(&paths).expect("migrated storage");
        assert_eq!(
            schema_version(storage.connection()).expect("version"),
            CURRENT_SCHEMA_VERSION
        );
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT value_json FROM settings WHERE key = 'fixture'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("preserved setting"),
            "\"keep me\""
        );
        assert_eq!(
            storage
                .connection()
                .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
                .expect("integrity check"),
            "ok"
        );
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM releases WHERE resource_id = 'release:fixture' AND publication_id = 'publication:fixture'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("preserved release"),
            1
        );
        let releases = storage
            .list_releases("project:fixture")
            .expect("migrated releases");
        assert_eq!(releases[0]["id"], "release:fixture");
        assert_eq!(releases[0]["platform"], "xiaohongshu");
        assert_eq!(releases[0]["snapshot"]["title"], "Captured title");
        assert_eq!(releases[0]["snapshot"]["bodyText"], "Captured body");
        let snapshot: String = storage
            .connection()
            .query_row(
                "SELECT snapshot_json FROM releases WHERE resource_id = 'release:fixture'",
                [],
                |row| row.get(0),
            )
            .expect("snapshot JSON");
        let snapshot = serde_json::from_str::<Value>(&snapshot).expect("snapshot");
        assert!(snapshot.get("title").is_none());
        assert!(snapshot.get("id").is_none());
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT config_version FROM publications WHERE resource_id = 'publication:fixture'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("publication config version"),
            5
        );
        let operation = storage
            .read_direct_operation("operation:fixture")
            .expect("operation read")
            .expect("operation");
        assert_eq!(operation["target"]["placeholderShapeId"], "shape:fixture");
        let payload: String = storage
            .connection()
            .query_row(
                "SELECT payload_json FROM direct_operations WHERE id = 'operation:fixture'",
                [],
                |row| row.get(0),
            )
            .expect("operation payload");
        assert!(serde_json::from_str::<Value>(&payload)
            .expect("payload JSON")
            .get("status")
            .is_none());
        assert_eq!(
            storage
                .connection()
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("foreign key check"),
            0
        );
        drop(storage);

        let backups = backup_directories(&paths);
        assert_eq!(backups.len(), 1);
        let backup = &backups[0];
        let backup_database =
            Connection::open(backup.join(DATABASE_FILE_NAME)).expect("backup database");
        assert_eq!(schema_version(&backup_database).expect("backup version"), 1);
        assert_eq!(
            backup_database
                .query_row(
                    "SELECT value_json FROM settings WHERE key = 'fixture'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("backup setting"),
            "\"keep me\""
        );
        let metadata: Value = serde_json::from_slice(
            &fs::read(backup.join("backup.json")).expect("backup metadata"),
        )
        .expect("backup metadata json");
        assert_eq!(metadata["kind"], json!("schema"));
        assert_eq!(metadata["fromSchemaVersion"], json!(1));
        assert_eq!(
            metadata["toSchemaVersion"],
            json!(CURRENT_SCHEMA_VERSION)
        );
        assert_eq!(metadata["includesFilesystem"], json!(true));
    }

    #[test]
    fn version_five_cross_project_links_are_repaired_before_constraints() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let mut connection = create_version_one_storage(&paths);
        for migration in &MIGRATIONS[1..5] {
            apply_migration(&paths, &mut connection, *migration).expect("migration to v5");
        }
        connection
            .execute_batch(
                r#"
                INSERT INTO projects (
                  id, title, root_path, created_at, updated_at
                ) VALUES
                  ('project:a', 'A', '/a',
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'),
                  ('project:b', 'B', '/b',
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z');
                INSERT INTO panels (
                  project_id, id, kind, position, created_at, updated_at
                ) VALUES
                  ('project:a', 'panel:a', 'wiki', 0,
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'),
                  ('project:b', 'panel:b', 'wiki', 0,
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z');
                INSERT INTO resources (
                  id, project_id, kind, title, created_at, updated_at
                ) VALUES
                  ('wiki:a', 'project:a', 'wiki_space', 'Wiki',
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'),
                  ('document:a', 'project:a', 'document', 'Source',
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'),
                  ('document:b', 'project:b', 'document', 'Document',
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'),
                  ('publication:a', 'project:a', 'publication', 'Publication A',
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'),
                  ('publication:b', 'project:b', 'publication', 'Publication B',
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'),
                  ('release:a', 'project:a', 'release', 'Release',
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z');
                INSERT INTO documents (resource_id, document_kind)
                VALUES
                  ('document:a', 'wiki_source'),
                  ('document:b', 'my_document');
                INSERT INTO wiki_spaces (resource_id)
                VALUES ('wiki:a');
                INSERT INTO publications (resource_id, source_document_id)
                VALUES
                  ('publication:a', 'document:b'),
                  ('publication:b', NULL);
                INSERT INTO releases (
                  resource_id, publication_id, platform_key, release_json
                ) VALUES (
                  'release:a', 'publication:b', 'test',
                  '{"sourcePublicationId":"publication:b"}'
                );
                INSERT INTO tasks (
                  id, project_id, origin_panel_id, handler_key, status,
                  target_ref, input_json, source_json, depends_on_task_id,
                  retry_of_task_id, available_at, created_at, updated_at
                ) VALUES
                  ('task:b', 'project:b', 'panel:b', 'handler.wiki.maintain',
                   'queued', 'b', '{}', '{}', NULL, NULL,
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z',
                   '2026-01-01T00:00:00.000Z'),
                  ('task:a', 'project:a', 'panel:b', 'handler.wiki.maintain',
                   'queued', 'a', '{}', '{}', 'task:b', 'task:b',
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z',
                   '2026-01-01T00:00:00.000Z'),
                  ('task:cycle1', 'project:a', 'panel:a',
                   'handler.wiki.maintain', 'queued', 'cycle1', '{}', '{}',
                   NULL, NULL, '2026-01-01T00:00:00.000Z',
                   '2026-01-01T00:00:00.000Z',
                   '2026-01-01T00:00:00.000Z'),
                  ('task:cycle2', 'project:a', 'panel:a',
                   'handler.wiki.maintain', 'queued', 'cycle2', '{}', '{}',
                   NULL, NULL, '2026-01-01T00:00:00.000Z',
                   '2026-01-01T00:00:00.000Z',
                   '2026-01-01T00:00:00.000Z');
                UPDATE tasks SET depends_on_task_id = 'task:cycle2'
                WHERE id = 'task:cycle1';
                UPDATE tasks SET depends_on_task_id = 'task:cycle1'
                WHERE id = 'task:cycle2';
                INSERT INTO wiki_source_ingestions (
                  project_id, wiki_space_id, document_id,
                  processed_document_version, disposition, task_id,
                  created_at, updated_at
                ) VALUES (
                  'project:a', 'wiki:a', 'document:a', 1, 'included', 'task:a',
                  '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'
                );
                INSERT INTO task_resources (
                  task_id, resource_id, role, created_at
                ) VALUES (
                  'task:a', 'document:b', 'input',
                  '2026-01-01T00:00:00.000Z'
                );
                INSERT INTO change_scopes (
                  scope_key, kind, project_id, resource_id, revision, updated_at
                ) VALUES (
                  'resource:cross', 'resource', 'project:a', 'document:b', 1,
                  '2026-01-01T00:00:00.000Z'
                );
                "#,
            )
            .expect("cross-Project v5 fixtures");
        drop(connection);

        let storage = Storage::open(&paths).expect("v6 migration");
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT source_document_id FROM publications WHERE resource_id = 'publication:a'",
                    [],
                    |row| row.get::<_, Option<String>>(0),
                )
                .expect("publication source"),
            None
        );
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM releases WHERE resource_id = 'release:a'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("release publication"),
            0
        );
        assert!(storage
            .connection()
            .query_row(
                "SELECT deleted_at FROM resources WHERE id = 'release:a'",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .expect("release lifecycle")
            .is_some());
        let task = storage
            .connection()
            .query_row(
                "SELECT status, origin_panel_id, depends_on_task_id, retry_of_task_id FROM tasks WHERE id = 'task:a'",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .expect("repaired task");
        assert_eq!(task, ("cancelled".to_owned(), None, None, None));
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM tasks WHERE id IN ('task:cycle1', 'task:cycle2') AND status = 'cancelled' AND depends_on_task_id IS NULL",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("cycle repair"),
            2
        );
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM task_resources WHERE task_id = 'task:a'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("Task resource count"),
            0
        );
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM wiki_source_ingestions WHERE project_id = 'project:a' AND task_id = 'task:a'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("Wiki ingestion count"),
            1
        );
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT COUNT(*) FROM change_scopes WHERE scope_key = 'resource:cross'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("change scope count"),
            0
        );
        assert_eq!(
            storage
                .connection()
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("foreign key check"),
            0
        );
    }

    #[test]
    fn version_three_content_is_migrated_to_opaque_objects() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let mut connection = create_version_one_storage(&paths);
        apply_migration(&paths, &mut connection, MIGRATIONS[1]).expect("migration 2");
        apply_migration(&paths, &mut connection, MIGRATIONS[2]).expect("migration 3");
        connection
            .execute_batch(
                r#"
                INSERT INTO projects (id, title, root_path, created_at, updated_at)
                VALUES ('project:legacy', 'Legacy', '/legacy',
                        '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z');
                INSERT INTO resources (
                  id, project_id, kind, title, revision, created_at, updated_at,
                  active_content_revision_id, content_version,
                  content_manifest_hash, content_hash
                ) VALUES (
                  'document:legacy', 'project:legacy', 'document', 'Legacy', 1,
                  '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z',
                  'revision:legacy', 1, 'legacy-manifest-hash', 'legacy-content-hash'
                );
                INSERT INTO documents (
                  resource_id, document_kind, media_type, source, original_file_name,
                  active_content_ref, logical_content_version, metadata_json
                ) VALUES (
                  'document:legacy', 'my_document', 'text/markdown', 'user', 'legacy.md',
                  'content.md', 1, '{}'
                );
                "#,
            )
            .expect("legacy rows");
        drop(connection);

        let resource = crate::content::resource_dir(
            &paths,
            "project:legacy",
            crate::content::ResourceKind::MyDocument,
            "document:legacy",
        );
        let revision = crate::content::revision_dir(&resource, "revision:legacy");
        let bytes = b"legacy content";
        fs::create_dir_all(revision.join("files/notes")).expect("legacy files");
        fs::write(revision.join("files/notes/legacy.md"), bytes).expect("legacy content");
        let object_hash = crate::content::hash_bytes(bytes);
        crate::content::write_json_atomic(
            &revision.join("manifest.json"),
            &json!({
                "revisionId": "revision:legacy",
                "contentVersion": 1,
                "parentRevisionId": null,
                "createdAt": "2026-01-01T00:00:00.000Z",
                "files": [{
                    "logicalPath": "notes/legacy.md",
                    "contentHash": object_hash,
                    "sizeBytes": bytes.len(),
                    "mimeType": "text/markdown"
                }]
            }),
        )
        .expect("legacy manifest");
        crate::content::write_json_atomic(
            &resource.join("active.json"),
            &json!({
                "revisionId": "revision:legacy",
                "contentVersion": 1,
                "manifestHash": "legacy-manifest-hash",
                "contentHash": "legacy-content-hash",
                "archived": false
            }),
        )
        .expect("legacy pointer");

        let storage = Storage::open(&paths).expect("migrated storage");
        assert_eq!(
            schema_version(storage.connection()).expect("version"),
            CURRENT_SCHEMA_VERSION
        );
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT content_format_version FROM storage_meta WHERE id = 1",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("content format"),
            2
        );
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT directory_layout_version FROM storage_meta WHERE id = 1",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("directory layout"),
            2
        );
        let resource = crate::content::resource_dir(
            &paths,
            "project:legacy",
            crate::content::ResourceKind::MyDocument,
            "document:legacy",
        );
        let revision = crate::content::revision_dir(&resource, "revision:legacy");
        let manifest: Value = serde_json::from_slice(
            &fs::read(revision.join("manifest.json")).expect("migrated manifest"),
        )
        .expect("manifest json");
        assert_eq!(manifest["formatVersion"], json!(2));
        assert_eq!(
            manifest["files"][0]["objectRef"],
            json!(format!("objects/{object_hash}"))
        );
        assert_eq!(
            fs::read(revision.join(format!("objects/{object_hash}")))
                .expect("migrated object"),
            bytes
        );
        assert_eq!(
            fs::read(revision.join("files/notes/legacy.md")).expect("retained source"),
            bytes
        );
        assert!(!paths.storage_dir.join(".migrations").exists());
        let manifest_hash = crate::content::hash_bytes(
            &fs::read(revision.join("manifest.json")).expect("manifest bytes"),
        );
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT content_manifest_hash FROM resources WHERE id = 'document:legacy'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("database authority"),
            manifest_hash
        );
        drop(storage);

        let backup = backup_directories(&paths)
            .into_iter()
            .next()
            .expect("filesystem backup");
        assert!(backup
            .join("projects")
            .join(sanitize_path_part("project:legacy"))
            .join("content/my_document")
            .join(sanitize_path_part("document:legacy"))
            .join(sanitize_path_part("revision:legacy"))
            .join("files/notes/legacy.md")
            .is_file());
    }

    #[test]
    fn version_four_assets_are_migrated_to_manifest_objects() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let mut connection = create_version_one_storage(&paths);
        for migration in &MIGRATIONS[1..4] {
            apply_migration(&paths, &mut connection, *migration).expect("baseline migration");
        }
        let bytes = b"legacy asset bytes";
        let content_hash = crate::content::hash_bytes(bytes);
        let legacy_ref =
            "projects/project:asset/content/asset/asset:legacy/1/legacy.png";
        connection
            .execute_batch(
                &format!(
                    r#"
                    INSERT INTO projects (id, title, root_path, created_at, updated_at)
                    VALUES ('project:asset', 'Asset Project', '/asset',
                            '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z');
                    INSERT INTO panels (
                      project_id, id, kind, position, created_at, updated_at
                    ) VALUES (
                      'project:asset', 'panel:asset', 'canvas', 0,
                      '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'
                    );
                    INSERT INTO resources (
                      id, project_id, kind, title, revision, created_at, updated_at,
                      active_content_revision_id, content_version,
                      content_manifest_hash, content_hash
                    ) VALUES (
                      'asset:legacy', 'project:asset', 'asset', 'legacy.png', 1,
                      '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z',
                      'asset-revision:1', 1, '', '{content_hash}'
                    );
                    INSERT INTO assets (
                      resource_id, media_type, file_name, active_file_ref,
                      byte_size, metadata_json
                    ) VALUES (
                      'asset:legacy', 'image/png', 'legacy.png', '{legacy_ref}',
                      {}, '{{}}'
                    );
                    INSERT INTO resources (
                      id, project_id, kind, title, revision, created_at, updated_at,
                      active_content_revision_id, content_version,
                      content_manifest_hash, content_hash
                    ) VALUES (
                      'asset:missing', 'project:asset', 'asset', 'missing.png', 2,
                      '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z',
                      'asset-revision:1', 1, '', 'missing'
                    );
                    INSERT INTO assets (
                      resource_id, media_type, file_name, active_file_ref,
                      byte_size, metadata_json
                    ) VALUES (
                      'asset:missing', 'image/png', 'missing.png',
                      'projects/project:asset/content/asset/asset:missing/1/missing.png',
                      7, '{{}}'
                    );
                    "#,
                    bytes.len()
                ),
            )
            .expect("legacy asset rows");
        let legacy_path =
            legacy_asset_path(&paths.storage_dir, legacy_ref).expect("legacy asset path");
        crate::content::write_materialized_file(&legacy_path, bytes).expect("legacy asset");
        drop(connection);

        let storage = Storage::open(&paths).expect("migrated storage");
        let active_ref: String = storage
            .connection()
            .query_row(
                "SELECT active_file_ref FROM assets WHERE resource_id = 'asset:legacy'",
                [],
                |row| row.get(0),
            )
            .expect("active Asset ref");
        assert!(active_ref.contains("/asset-revision:1/legacy.png"));
        assert_eq!(storage.read_asset(&active_ref).expect("migrated Asset"), bytes);
        assert_eq!(
            storage
                .connection()
                .query_row(
                    "SELECT content_manifest_hash FROM resources WHERE id = 'asset:legacy'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("manifest hash")
                .len(),
            64
        );
        let revision = asset_revision_dir(
            &paths.storage_dir,
            "project:asset",
            "asset:legacy",
            "asset-revision:1",
        );
        assert!(revision.join("manifest.json").is_file());
        assert!(revision.join(format!("objects/{content_hash}")).is_file());
        assert_eq!(
            fs::read(
                asset_resource_dir(&paths.storage_dir, "project:asset", "asset:legacy")
                    .join("1/legacy.png")
            )
            .expect("retained source"),
            bytes
        );
        assert!(!paths.storage_dir.join(".migrations").exists());
        assert!(storage
            .connection()
            .query_row(
                "SELECT deleted_at FROM resources WHERE id = 'asset:missing'",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .expect("missing Asset lifecycle")
            .is_some());

        drop(storage);

        let backup = backup_directories(&paths)
            .into_iter()
            .next()
            .expect("filesystem backup");
        assert!(backup
            .join(
                legacy_path
                    .strip_prefix(&paths.storage_dir)
                    .expect("relative legacy path")
            )
            .is_file());
    }

    #[test]
    fn concurrent_version_one_openers_converge_on_one_migration_history() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        drop(create_version_one_storage(&paths));
        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let worker_paths = paths.clone();
            let worker_barrier = barrier.clone();
            workers.push(std::thread::spawn(move || {
                worker_barrier.wait();
                Storage::open(&worker_paths).map(drop)
            }));
        }
        barrier.wait();
        for worker in workers {
            worker.join().expect("migration worker").expect("storage");
        }

        let storage = Storage::open(&paths).expect("migrated storage");
        assert_eq!(
            schema_version(storage.connection()).expect("version"),
            CURRENT_SCHEMA_VERSION
        );
        assert_eq!(
            storage
                .connection()
                .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("history count"),
            CURRENT_SCHEMA_VERSION
        );
    }

    #[test]
    fn migration_history_mismatch_is_rejected_without_changes() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");
        storage
            .write_setting("fixture", "value", r#""keep me""#)
            .expect("setting");
        storage
            .connection()
            .execute(
                "UPDATE schema_migrations SET checksum = ? WHERE version = 1",
                ["0".repeat(64)],
            )
            .expect("corrupt migration history");
        drop(storage);

        let error = Storage::open(&paths).expect_err("history mismatch");
        assert_eq!(error.code(), Some("storage_schema_mismatch"));
        let connection =
            Connection::open(paths.storage_dir.join(DATABASE_FILE_NAME)).expect("database");
        assert_eq!(
            connection
                .query_row(
                    "SELECT value_json FROM settings WHERE key = 'fixture.value'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("preserved setting"),
            "\"keep me\""
        );
    }

    #[test]
    fn failed_migration_rolls_back_schema_and_version() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let mut storage = Storage::open(&paths).expect("storage");
        let invalid = Migration {
            version: CURRENT_SCHEMA_VERSION + 1,
            name: "invalid_test",
            sql: "CREATE TABLE migration_should_rollback (id INTEGER); INVALID SQL;",
            changes_filesystem: false,
            after_sql: None,
        };
        apply_migration(&paths, storage.connection_mut(), invalid).expect_err("invalid migration");
        assert!(!table_exists(
            storage.connection(),
            "migration_should_rollback"
        )
        .expect("rolled back table"));
        assert_eq!(
            schema_version(storage.connection()).expect("version"),
            CURRENT_SCHEMA_VERSION
        );
        assert_eq!(
            storage
                .connection()
                .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("history count"),
            CURRENT_SCHEMA_VERSION
        );
    }

    #[test]
    fn migration_registry_rejects_nonconsecutive_versions() {
        let invalid = [
            MIGRATIONS[0],
            Migration {
                version: 3,
                name: "gap",
                sql: "SELECT 1;",
                changes_filesystem: false,
                after_sql: None,
            },
        ];
        let error = validate_migration_registry(&invalid).expect_err("registry gap");
        assert_eq!(error.code(), Some("invalid_migration_registry"));
    }

    #[test]
    fn unversioned_storage_is_backed_up_and_rejected_without_reset() {
        let temp = tempdir().expect("tempdir");
        let storage_dir = temp.path().join(".myopenpanels");
        let paths = paths_for(storage_dir.clone());
        fs::create_dir_all(&storage_dir).expect("storage dir");
        fs::write(storage_dir.join("content-marker.txt"), "keep me").expect("content marker");
        let connection =
            Connection::open(storage_dir.join(DATABASE_FILE_NAME)).expect("legacy database");
        connection
            .execute("CREATE TABLE legacy_records (value TEXT NOT NULL)", [])
            .expect("legacy table");
        connection
            .execute("INSERT INTO legacy_records VALUES ('keep me')", [])
            .expect("legacy row");
        drop(connection);

        let error = Storage::open(&paths).expect_err("unversioned storage");
        assert_eq!(error.code(), Some("incompatible_storage_baseline"));
        let original =
            Connection::open(storage_dir.join(DATABASE_FILE_NAME)).expect("original database");
        assert_eq!(
            original
                .query_row("SELECT value FROM legacy_records", [], |row| {
                    row.get::<_, String>(0)
                })
                .expect("original row"),
            "keep me"
        );
        assert_eq!(
            fs::read_to_string(storage_dir.join("content-marker.txt")).expect("original marker"),
            "keep me"
        );

        let backups = backup_directories(&paths);
        assert_eq!(backups.len(), 1);
        let backup = &backups[0];
        let backup_database =
            Connection::open(backup.join(DATABASE_FILE_NAME)).expect("backup database");
        assert_eq!(
            backup_database
                .query_row("SELECT value FROM legacy_records", [], |row| {
                    row.get::<_, String>(0)
                })
                .expect("backup row"),
            "keep me"
        );
        assert_eq!(
            fs::read_to_string(backup.join("content-marker.txt")).expect("backup marker"),
            "keep me"
        );
    }
}
