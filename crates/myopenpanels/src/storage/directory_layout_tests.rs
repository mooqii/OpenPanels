#[cfg(test)]
mod directory_layout_tests {
    use super::*;
    use crate::paths::MyOpenPanelsPaths;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn paths_for(storage_dir: PathBuf) -> MyOpenPanelsPaths {
        let studio_dir = storage_dir.join("studio");
        MyOpenPanelsPaths {
            context_dir: storage_dir.join("contexts/test"),
            context_id: "test".to_owned(),
            context_id_source: "test".to_owned(),
            focus_dir: studio_dir.join("focus"),
            project_dir: storage_dir.join("project"),
            studio_dir,
            storage_dir,
        }
    }

    fn version_seven_storage(paths: &MyOpenPanelsPaths) -> Connection {
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
        for migration in &MIGRATIONS[1..7] {
            apply_migration(paths, &mut connection, *migration).expect("migration to v7");
        }
        connection
    }

    #[test]
    fn version_seven_directories_migrate_with_descriptors() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let connection = version_seven_storage(&paths);
        connection
            .execute_batch(
                r#"
                INSERT INTO projects (id, title, root_path, created_at, updated_at)
                VALUES ('project:legacy', 'Legacy', '/legacy',
                        '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z');
                INSERT INTO resources (
                  id, project_id, kind, title, created_at, updated_at
                ) VALUES (
                  'document:legacy', 'project:legacy', 'document', 'Legacy',
                  '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'
                );
                INSERT INTO documents (resource_id, document_kind)
                VALUES ('document:legacy', 'my_document');
                "#,
            )
            .expect("legacy records");
        let legacy_resource = legacy_project_storage_dir(&paths.storage_dir, "project:legacy")
            .join("content/my_document")
            .join(sanitize_path_part("document:legacy"));
        fs::create_dir_all(&legacy_resource).expect("legacy Resource");
        fs::write(legacy_resource.join("keep.txt"), "preserve").expect("legacy content");
        crate::content::write_json_atomic(
            &legacy_resource.join("resource.json"),
            &json!({ "resourceKey": "document:legacy" }),
        )
        .expect("legacy descriptor");
        drop(connection);

        let storage = Storage::open(&paths).expect("migration");
        let project = storage.project_dir("project:legacy");
        let resource = resource_storage_dir(
            &paths.storage_dir,
            "project:legacy",
            "my_document",
            "document:legacy",
        );
        assert_eq!(
            project.file_name().and_then(|value| value.to_str()),
            Some(crate::paths::stable_path_key("project:legacy").as_str())
        );
        assert_eq!(
            fs::read_to_string(resource.join("keep.txt")).expect("migrated content"),
            "preserve"
        );
        let project_descriptor: Value =
            crate::content::read_json(&project.join("project.json")).expect("Project descriptor");
        let resource_descriptor: Value =
            crate::content::read_json(&resource.join("resource.json"))
                .expect("Resource descriptor");
        assert_eq!(project_descriptor["projectId"], json!("project:legacy"));
        assert_eq!(resource_descriptor["projectId"], json!("project:legacy"));
        assert_eq!(resource_descriptor["resourceKind"], json!("my_document"));
        assert_eq!(resource_descriptor["resourceKey"], json!("document:legacy"));

        assert!(!paths.storage_dir.join(".migrations").exists());
        assert_eq!(
            fs::read_to_string(resource.join("keep.txt")).expect("migrated content"),
            "preserve"
        );
    }

    #[test]
    fn migration_rejects_legacy_project_key_collisions_before_moving_data() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let connection = version_seven_storage(&paths);
        connection
            .execute_batch(
                r#"
                INSERT INTO projects (id, title, root_path, created_at, updated_at)
                VALUES
                  ('project:a/b', 'A', '/a',
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'),
                  ('project:a?b', 'B', '/b',
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z');
                "#,
            )
            .expect("colliding Projects");
        let legacy = legacy_project_storage_dir(&paths.storage_dir, "project:a/b");
        fs::create_dir_all(&legacy).expect("legacy Project");
        fs::write(legacy.join("keep.txt"), "preserve").expect("legacy content");
        drop(connection);

        let error = Storage::open(&paths).expect_err("collision");
        assert_eq!(error.code(), Some("storage_migration_failed"));
        assert_eq!(
            fs::read_to_string(legacy.join("keep.txt")).expect("preserved content"),
            "preserve"
        );
        assert!(!stable_project_storage_dir(&paths.storage_dir, "project:a/b").exists());
        assert!(!stable_project_storage_dir(&paths.storage_dir, "project:a?b").exists());
    }

    #[test]
    fn migration_rejects_legacy_resource_key_collisions_before_moving_data() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let connection = version_seven_storage(&paths);
        connection
            .execute_batch(
                r#"
                INSERT INTO projects (id, title, root_path, created_at, updated_at)
                VALUES ('project:collision', 'Collision', '/collision',
                        '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z');
                INSERT INTO resources (
                  id, project_id, kind, title, created_at, updated_at
                ) VALUES
                  ('document:a/b', 'project:collision', 'document', 'A',
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z'),
                  ('document:a?b', 'project:collision', 'document', 'B',
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z');
                INSERT INTO documents (resource_id, document_kind)
                VALUES
                  ('document:a/b', 'my_document'),
                  ('document:a?b', 'my_document');
                "#,
            )
            .expect("colliding Resources");
        let legacy_resource =
            legacy_project_storage_dir(&paths.storage_dir, "project:collision")
                .join("content/my_document")
                .join(sanitize_path_part("document:a/b"));
        fs::create_dir_all(&legacy_resource).expect("legacy Resource");
        fs::write(legacy_resource.join("keep.txt"), "preserve").expect("legacy content");
        drop(connection);

        let error = Storage::open(&paths).expect_err("collision");
        assert_eq!(error.code(), Some("storage_migration_failed"));
        assert_eq!(
            fs::read_to_string(legacy_resource.join("keep.txt")).expect("preserved content"),
            "preserve"
        );
        assert!(!stable_project_storage_dir(&paths.storage_dir, "project:collision").exists());
    }
}
