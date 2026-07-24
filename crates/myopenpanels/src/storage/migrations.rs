const CURRENT_SCHEMA_VERSION: i64 = 1;
const MIGRATION_0001_SQL: &str = include_str!("../../migrations/0001_initial.sql");

fn archive_legacy_storage_if_needed(
    paths: &MyOpenPanelsPaths,
    database_path: &Path,
) -> Result<(), CliError> {
    if !database_path.is_file() {
        return Ok(());
    }
    let source = Connection::open(database_path).map_err(to_cli_error)?;
    if schema_version(&source)? != 0 || application_table_count(&source)? == 0 {
        return Ok(());
    }

    let backup_parent = paths
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
        ));
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(to_cli_error)?
        .as_millis();
    let backup_dir = backup_parent.join(format!("pre-1.0-{timestamp}-{}", std::process::id()));
    fs::create_dir_all(&backup_dir).map_err(to_cli_error)?;
    copy_legacy_storage_files(&paths.storage_dir, &backup_dir)?;

    let backup_database_path = backup_dir.join(DATABASE_FILE_NAME);
    let mut destination = Connection::open(&backup_database_path).map_err(to_cli_error)?;
    rusqlite::backup::Backup::new(&source, &mut destination)
        .map_err(to_cli_error)?
        .run_to_completion(64, Duration::from_millis(25), None)
        .map_err(to_cli_error)?;
    drop(destination);
    drop(source);

    for path in [
        database_path.to_path_buf(),
        PathBuf::from(format!("{}-wal", database_path.display())),
        PathBuf::from(format!("{}-shm", database_path.display())),
    ] {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(to_cli_error(error)),
        }
    }
    Ok(())
}

fn copy_legacy_storage_files(source: &Path, destination: &Path) -> Result<(), CliError> {
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
            copy_legacy_storage_files(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(source_path, destination_path).map_err(to_cli_error)?;
        }
    }
    Ok(())
}

fn current_schema_fingerprint() -> String {
    format!("{:x}", Sha256::digest(MIGRATION_0001_SQL.as_bytes()))
}

fn read_schema_fingerprint(connection: &Connection) -> Result<Option<String>, CliError> {
    let has_column = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('storage_meta') WHERE name = 'schema_fingerprint')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(to_cli_error)?;
    if !has_column {
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

fn initialize_storage_schema(connection: &mut Connection) -> Result<(), CliError> {
    let version = schema_version(connection)?;
    if version == 0 {
        if application_table_count(connection)? > 0 {
            return Err(CliError::with_code(
                "incompatible_storage_baseline",
                "The incompatible database was not archived safely.",
            ));
        }
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        tx.execute_batch(MIGRATION_0001_SQL).map_err(to_cli_error)?;
        tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
            .map_err(to_cli_error)?;
        tx.execute(
            "UPDATE storage_meta SET schema_fingerprint = ? WHERE id = 1",
            [current_schema_fingerprint()],
        )
        .map_err(to_cli_error)?;
        tx.commit().map_err(to_cli_error)?;
        return Ok(());
    }
    if version != CURRENT_SCHEMA_VERSION {
        return Err(CliError::with_code(
            "storage_version_mismatch",
            "The database schema does not match this build.",
        ));
    }
    if read_schema_fingerprint(connection)?.as_deref()
        != Some(current_schema_fingerprint().as_str())
    {
        return Err(CliError::with_code(
            "storage_schema_mismatch",
            "The database schema does not match this build. Start with an empty storage directory.",
        ));
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
