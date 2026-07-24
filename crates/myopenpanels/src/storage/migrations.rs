#[derive(Debug, Clone, Copy)]
struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
    changes_filesystem: bool,
    after_sql: Option<fn(&MyOpenPanelsPaths, &Transaction<'_>) -> Result<(), CliError>>,
}

const MIGRATION_HISTORY_VERSION: i64 = 2;
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial",
        sql: include_str!("../../migrations/0001_initial.sql"),
        changes_filesystem: false,
        after_sql: None,
    },
    Migration {
        version: 2,
        name: "migration_registry",
        sql: include_str!("../../migrations/0002_migration_registry.sql"),
        changes_filesystem: false,
        after_sql: None,
    },
    Migration {
        version: 3,
        name: "canonical_content_authority",
        sql: include_str!("../../migrations/0003_canonical_content_authority.sql"),
        changes_filesystem: false,
        after_sql: Some(backfill_canonical_content_authority),
    },
    Migration {
        version: 4,
        name: "content_objects",
        sql: include_str!("../../migrations/0004_content_objects.sql"),
        changes_filesystem: true,
        after_sql: Some(crate::content::migrate_content_format_v2),
    },
    Migration {
        version: 5,
        name: "asset_objects",
        sql: include_str!("../../migrations/0005_asset_objects.sql"),
        changes_filesystem: true,
        after_sql: Some(migrate_asset_content_v2),
    },
    Migration {
        version: 6,
        name: "same_project_relationships",
        sql: include_str!("../../migrations/0006_same_project_relationships.sql"),
        changes_filesystem: false,
        after_sql: None,
    },
    Migration {
        version: 7,
        name: "release_snapshots",
        sql: include_str!("../../migrations/0007_release_snapshots.sql"),
        changes_filesystem: false,
        after_sql: None,
    },
    Migration {
        version: 8,
        name: "stable_directory_keys",
        sql: include_str!("../../migrations/0008_stable_directory_keys.sql"),
        changes_filesystem: true,
        after_sql: Some(migrate_stable_directory_keys),
    },
];
const CURRENT_SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

#[derive(Debug)]
struct StorageBackup {
    directory: PathBuf,
}

fn initialize_storage_schema(
    paths: &MyOpenPanelsPaths,
    connection: &mut Connection,
) -> Result<(), CliError> {
    validate_migration_registry(MIGRATIONS)?;
    let version = validated_schema_version(paths, connection)?;
    cleanup_applied_migration_journals(paths, version)?;
    if version == CURRENT_SCHEMA_VERSION {
        return Ok(());
    }

    let pending = MIGRATIONS
        .iter()
        .filter(|migration| migration.version > version)
        .copied()
        .collect::<Vec<_>>();
    let backup = if version > 0 {
        Some(create_storage_backup(
            paths,
            connection,
            "schema",
            version,
            CURRENT_SCHEMA_VERSION,
            pending.iter().any(|migration| migration.changes_filesystem),
        )?)
    } else {
        None
    };

    for migration in pending {
        if let Err(error) = apply_migration(paths, connection, migration) {
            let recovery = backup.as_ref().map_or_else(
                || {
                    "The new database was not initialized. Correct the migration and retry."
                        .to_owned()
                },
                |backup| {
                    format!(
                        "The last valid database is unchanged and its pre-upgrade backup is available at {}.",
                        backup.directory.display()
                    )
                },
            );
            return Err(CliError::with_recovery(
                "storage_migration_failed",
                format!(
                    "Storage migration {:04}_{} failed: {error}",
                    migration.version, migration.name
                ),
                false,
                recovery,
            ));
        }
        cleanup_applied_migration_journals(paths, migration.version)?;
    }
    let final_version = validated_schema_version(paths, connection)?;
    if final_version != CURRENT_SCHEMA_VERSION {
        return Err(CliError::with_code(
            "storage_migration_incomplete",
            format!(
                "Storage migration stopped at schema version {final_version}; expected {CURRENT_SCHEMA_VERSION}."
            ),
        ));
    }
    Ok(())
}

fn cleanup_applied_migration_journals(
    paths: &MyOpenPanelsPaths,
    schema_version: i64,
) -> Result<(), CliError> {
    let directory = paths.storage_dir.join(".migrations");
    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.changes_filesystem && migration.version <= schema_version)
    {
        let journal = directory.join(format!(
            "{:04}_{}.json",
            migration.version, migration.name
        ));
        match fs::remove_file(journal) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(to_cli_error(error)),
        }
    }

    match fs::remove_dir(directory) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(to_cli_error(error)),
    }
}

fn validated_schema_version(
    paths: &MyOpenPanelsPaths,
    connection: &Connection,
) -> Result<i64, CliError> {
    loop {
        let version = schema_version(connection)?;
        if version > CURRENT_SCHEMA_VERSION {
            return Err(CliError::with_recovery(
                "storage_version_mismatch",
                format!(
                    "The database schema version {version} is newer than this CLI supports ({CURRENT_SCHEMA_VERSION})."
                ),
                false,
                "Open this storage with the same or a newer MyOpenPanels CLI. The database was not modified.",
            ));
        }
        if version == 0 && application_table_count(connection)? > 0 {
            if schema_version(connection)? != 0 {
                continue;
            }
            let backup = create_storage_backup(
                paths,
                connection,
                "unversioned",
                0,
                CURRENT_SCHEMA_VERSION,
                true,
            )?;
            return Err(CliError::with_recovery(
                "incompatible_storage_baseline",
                "The storage contains an unversioned database that cannot be migrated safely.",
                false,
                format!(
                    "The original storage was left unchanged and a recovery backup was created at {}.",
                    backup.directory.display()
                ),
            ));
        }

        if let Err(error) = validate_applied_migrations(connection, version) {
            if schema_version(connection)? != version {
                continue;
            }
            return Err(error);
        }
        if schema_version(connection)? == version {
            return Ok(version);
        }
    }
}

fn validate_migration_registry(migrations: &[Migration]) -> Result<(), CliError> {
    if migrations.is_empty() {
        return Err(invalid_migration_registry(
            "The migration registry is empty.",
        ));
    }
    for (index, migration) in migrations.iter().enumerate() {
        let expected = index as i64 + 1;
        if migration.version != expected {
            return Err(invalid_migration_registry(format!(
                "Expected migration version {expected}, found {}.",
                migration.version
            )));
        }
        if migration.name.trim().is_empty() || migration.sql.trim().is_empty() {
            return Err(invalid_migration_registry(format!(
                "Migration version {} has an empty name or SQL body.",
                migration.version
            )));
        }
    }
    if migrations.last().map(|migration| migration.version) != Some(CURRENT_SCHEMA_VERSION) {
        return Err(invalid_migration_registry(format!(
            "The migration registry does not end at CURRENT_SCHEMA_VERSION {CURRENT_SCHEMA_VERSION}."
        )));
    }
    Ok(())
}

fn invalid_migration_registry(message: impl Into<String>) -> CliError {
    CliError::with_code("invalid_migration_registry", message)
}

fn validate_applied_migrations(
    connection: &Connection,
    version: i64,
) -> Result<(), CliError> {
    if version == 0 {
        return Ok(());
    }
    if version == 1 {
        if read_schema_fingerprint(connection)?.as_deref()
            != Some(migration_checksum(&MIGRATIONS[0]).as_str())
        {
            return Err(CliError::with_recovery(
                "storage_schema_mismatch",
                "The version 1 database does not match the released baseline migration.",
                false,
                "Restore a valid backup or open the storage with the CLI that created it. The database was not modified.",
            ));
        }
        return Ok(());
    }
    if !table_exists(connection, "schema_migrations")? {
        return Err(CliError::with_recovery(
            "storage_schema_mismatch",
            "The database is missing its migration history.",
            false,
            "Restore a valid backup or open the storage with the CLI that created it. The database was not modified.",
        ));
    }

    let mut statement = connection
        .prepare("SELECT version, name, checksum FROM schema_migrations ORDER BY version")
        .map_err(to_cli_error)?;
    let applied = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    let expected = MIGRATIONS
        .iter()
        .take(version as usize)
        .copied()
        .collect::<Vec<_>>();
    if applied.len() != expected.len() {
        return Err(migration_history_mismatch(format!(
            "Expected {} applied migrations for schema version {version}, found {}.",
            expected.len(),
            applied.len()
        )));
    }
    for (actual, migration) in applied.iter().zip(expected) {
        let checksum = migration_checksum(&migration);
        if actual.0 != migration.version || actual.1 != migration.name || actual.2 != checksum {
            return Err(migration_history_mismatch(format!(
                "Migration history differs at version {}.",
                migration.version
            )));
        }
    }
    Ok(())
}

fn migration_history_mismatch(message: impl Into<String>) -> CliError {
    CliError::with_recovery(
        "storage_schema_mismatch",
        message,
        false,
        "Restore a valid backup or open the storage with the CLI that created it. The database was not modified.",
    )
}

fn apply_migration(
    paths: &MyOpenPanelsPaths,
    connection: &mut Connection,
    migration: Migration,
) -> Result<(), CliError> {
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(to_cli_error)?;
    let current_version = schema_version(&tx)?;
    if current_version >= migration.version {
        tx.commit().map_err(to_cli_error)?;
        return Ok(());
    }
    if current_version + 1 != migration.version {
        return Err(CliError::with_code(
            "storage_migration_order_mismatch",
            format!(
                "Cannot apply migration {} after schema version {current_version}.",
                migration.version
            ),
        ));
    }
    tx.execute_batch(migration.sql).map_err(to_cli_error)?;
    if let Some(after_sql) = migration.after_sql {
        after_sql(paths, &tx)?;
    }
    if migration.version == 1 {
        tx.execute(
            "UPDATE storage_meta SET schema_fingerprint = ? WHERE id = 1",
            [migration_checksum(&migration)],
        )
        .map_err(to_cli_error)?;
    }
    if migration.version == MIGRATION_HISTORY_VERSION {
        for previous in MIGRATIONS
            .iter()
            .filter(|previous| previous.version < migration.version)
        {
            insert_migration_history(&tx, previous)?;
        }
    }
    if migration.version >= MIGRATION_HISTORY_VERSION {
        insert_migration_history(&tx, &migration)?;
    }
    tx.pragma_update(None, "user_version", migration.version)
        .map_err(to_cli_error)?;
    tx.commit().map_err(to_cli_error)
}

fn insert_migration_history(
    connection: &Connection,
    migration: &Migration,
) -> Result<(), CliError> {
    connection
        .execute(
            "INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (?, ?, ?, ?)",
            params![
                migration.version,
                migration.name,
                migration_checksum(migration),
                crate::control::now_iso(),
            ],
        )
        .map_err(to_cli_error)?;
    Ok(())
}

fn backfill_canonical_content_authority(
    paths: &MyOpenPanelsPaths,
    connection: &Transaction<'_>,
) -> Result<(), CliError> {
    let resources = {
        let mut statement = connection
            .prepare(
                r#"
                SELECT r.project_id, r.id, r.kind, d.document_kind
                FROM resources r
                LEFT JOIN documents d ON d.resource_id = r.id
                WHERE r.kind IN ('document', 'wiki_space')
                ORDER BY r.project_id, r.id
                "#,
            )
            .map_err(to_cli_error)?;
        let collected = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(to_cli_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_cli_error)?;
        collected
    };
    for (project_id, resource_id, resource_kind, document_kind) in resources {
        let kind = match (resource_kind.as_str(), document_kind.as_deref()) {
            ("document", Some("wiki_source")) => crate::content::ResourceKind::WikiMarkdown,
            ("document", Some("my_document")) => crate::content::ResourceKind::MyDocument,
            ("wiki_space", _) => crate::content::ResourceKind::WikiSpace,
            _ => continue,
        };
        let snapshot = crate::content::projected_active_resource_snapshot(
            paths,
            &project_id,
            kind,
            &resource_id,
        )?;
        if let Some(snapshot) = snapshot {
            connection
                .execute(
                    r#"
                    UPDATE resources
                    SET active_content_revision_id = ?, content_version = ?,
                        content_manifest_hash = ?, content_hash = ?
                    WHERE project_id = ? AND id = ?
                    "#,
                    params![
                        snapshot.revision_id,
                        snapshot.content_version,
                        snapshot.manifest_hash,
                        snapshot.content_hash,
                        project_id,
                        resource_id,
                    ],
                )
                .map_err(to_cli_error)?;
        } else {
            connection
                .execute(
                    r#"
                    UPDATE resources
                    SET active_content_revision_id = NULL, content_version = 0,
                        content_manifest_hash = '', content_hash = ''
                    WHERE project_id = ? AND id = ?
                    "#,
                    params![project_id, resource_id],
                )
                .map_err(to_cli_error)?;
        }
    }
    Ok(())
}

fn migration_checksum(migration: &Migration) -> String {
    format!("{:x}", Sha256::digest(migration.sql.as_bytes()))
}

fn read_schema_fingerprint(connection: &Connection) -> Result<Option<String>, CliError> {
    if !column_exists(connection, "storage_meta", "schema_fingerprint")? {
        return Ok(None);
    }
    connection
        .query_row(
            "SELECT schema_fingerprint FROM storage_meta WHERE id = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(to_cli_error)
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, CliError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?)",
            [table],
            |row| row.get(0),
        )
        .map_err(to_cli_error)
}

fn column_exists(
    connection: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, CliError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?) WHERE name = ?)",
            params![table, column],
            |row| row.get(0),
        )
        .map_err(to_cli_error)
}

fn create_storage_backup(
    paths: &MyOpenPanelsPaths,
    source: &Connection,
    kind: &str,
    from_version: i64,
    to_version: i64,
    include_filesystem: bool,
) -> Result<StorageBackup, CliError> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(to_cli_error)?
        .as_nanos();
    let backup_parent = storage_backup_parent(paths);
    let directory = backup_parent.join(format!(
        "{kind}-v{from_version}-to-v{to_version}-{timestamp}-{}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).map_err(to_cli_error)?;

    let backup_database_path = directory.join(DATABASE_FILE_NAME);
    let mut destination = Connection::open(&backup_database_path).map_err(to_cli_error)?;
    rusqlite::backup::Backup::new(source, &mut destination)
        .map_err(to_cli_error)?
        .run_to_completion(64, Duration::from_millis(25), None)
        .map_err(to_cli_error)?;
    drop(destination);

    if include_filesystem {
        copy_storage_files(&paths.storage_dir, &directory)?;
    }
    let metadata = json!({
        "formatVersion": 1,
        "kind": kind,
        "sourceStorage": paths.storage_dir,
        "fromSchemaVersion": from_version,
        "toSchemaVersion": to_version,
        "includesFilesystem": include_filesystem,
        "createdAtUnixNanos": timestamp.to_string(),
    });
    fs::write(
        directory.join("backup.json"),
        serde_json::to_vec_pretty(&metadata).map_err(to_cli_error)?,
    )
    .map_err(to_cli_error)?;
    Ok(StorageBackup { directory })
}

fn storage_backup_parent(paths: &MyOpenPanelsPaths) -> PathBuf {
    paths
        .storage_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(
            "{}-backups",
            paths
                .storage_dir
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("myopenpanels")
        ))
}

fn copy_storage_files(source: &Path, destination: &Path) -> Result<(), CliError> {
    for entry in fs::read_dir(source).map_err(to_cli_error)? {
        let entry = entry.map_err(to_cli_error)?;
        let file_type = entry.file_type().map_err(to_cli_error)?;
        let source_path = entry.path();
        let file_name = entry.file_name();
        if matches!(
            file_name.to_str(),
            Some(DATABASE_FILE_NAME | "main.sqlite3-wal" | "main.sqlite3-shm")
        ) {
            continue;
        }
        let destination_path = destination.join(file_name);
        if file_type.is_dir() {
            fs::create_dir_all(&destination_path).map_err(to_cli_error)?;
            copy_storage_files(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(source_path, destination_path).map_err(to_cli_error)?;
        }
    }
    Ok(())
}

fn schema_version(connection: &Connection) -> Result<i64, CliError> {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(to_cli_error)
}

fn application_table_count(connection: &Connection) -> Result<i64, CliError> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .map_err(to_cli_error)
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
