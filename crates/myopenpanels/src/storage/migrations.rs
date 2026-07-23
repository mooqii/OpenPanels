const CURRENT_SCHEMA_VERSION: i64 = 1;
const MIGRATION_0001_SQL: &str = include_str!("../../migrations/0001_initial.sql");

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
