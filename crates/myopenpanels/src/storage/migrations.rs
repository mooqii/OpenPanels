const SCHEMA_MIGRATIONS_SQL: &str = r#"
CREATE TABLE schema_migrations (
  id TEXT PRIMARY KEY NOT NULL,
  description TEXT NOT NULL,
  checksum TEXT NOT NULL,
  applied_at TEXT NOT NULL
);
"#;

const MIGRATION_0001_SQL: &str = include_str!("../../migrations/0001_initial.sql");

struct Migration {
    id: &'static str,
    description: &'static str,
    checksum_material: &'static str,
    up: fn(&Transaction<'_>) -> Result<(), CliError>,
}

#[derive(Debug, Clone)]
struct AppliedMigration {
    checksum: String,
}

fn migrations() -> &'static [Migration] {
    &[Migration {
        id: "0001_initial",
        description: "Create the initial MyOpenPanels storage schema",
        checksum_material: MIGRATION_0001_SQL,
        up: migration_0001,
    }]
}

fn migrate(connection: &mut Connection) -> Result<(), CliError> {
    let migration_table_exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'schema_migrations')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(to_cli_error)?;
    let application_table_count = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name != 'schema_migrations'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(to_cli_error)?;

    if !migration_table_exists {
        if application_table_count > 0 {
            return Err(incompatible_storage_baseline());
        }
        connection
            .execute_batch(SCHEMA_MIGRATIONS_SQL)
            .map_err(to_cli_error)?;
    } else if application_table_count > 0 {
        let applied = read_applied_migrations(connection)?;
        if applied.len() != 1 || !applied.contains_key("0001_initial") {
            return Err(incompatible_storage_baseline());
        }
    }

    run_migrations(connection, migrations())
}

fn incompatible_storage_baseline() -> CliError {
    CliError::with_code(
        "incompatible_storage_baseline",
        "This storage directory belongs to an older MyOpenPanels database baseline. Use an empty storage directory; the existing data was left unchanged.",
    )
}

fn preflight_existing_database(database_path: &std::path::Path) -> Result<(), CliError> {
    let connection = Connection::open_with_flags(database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(to_cli_error)?;
    let migration_table_exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'schema_migrations')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(to_cli_error)?;
    let application_table_count = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name != 'schema_migrations'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(to_cli_error)?;
    if !migration_table_exists {
        return if application_table_count == 0 {
            Ok(())
        } else {
            Err(incompatible_storage_baseline())
        };
    }

    let applied = read_applied_migrations(&connection)?;
    if applied.is_empty() && application_table_count == 0 {
        return Ok(());
    }
    let migration = &migrations()[0];
    if applied.len() != 1
        || applied
            .get(migration.id)
            .is_none_or(|entry| entry.checksum != migration_checksum(migration))
    {
        return Err(incompatible_storage_baseline());
    }
    Ok(())
}

fn run_migrations(connection: &mut Connection, migrations: &[Migration]) -> Result<(), CliError> {
    validate_registry(migrations)?;
    let applied = read_applied_migrations(connection)?;
    let registry_ids = migrations
        .iter()
        .map(|migration| migration.id)
        .collect::<HashSet<_>>();
    if applied.keys().any(|id| !registry_ids.contains(id.as_str())) {
        return Err(incompatible_storage_baseline());
    }

    let mut saw_missing = false;
    for migration in migrations {
        if applied.contains_key(migration.id) {
            if saw_missing {
                return Err(CliError::new(format!(
                    "non-contiguous migration history: {} is applied after a missing earlier migration",
                    migration.id
                )));
            }
        } else {
            saw_missing = true;
        }
    }

    for migration in migrations {
        if let Some(applied_migration) = applied.get(migration.id) {
            if applied_migration.checksum != migration_checksum(migration) {
                return Err(incompatible_storage_baseline());
            }
        } else {
            apply_migration(connection, migration)?;
        }
    }
    Ok(())
}

fn validate_registry(migrations: &[Migration]) -> Result<(), CliError> {
    let mut ids = HashSet::new();
    for migration in migrations {
        if !ids.insert(migration.id) {
            return Err(CliError::new(format!(
                "duplicate migration id in registry: {}",
                migration.id
            )));
        }
    }
    Ok(())
}

fn read_applied_migrations(
    connection: &Connection,
) -> Result<HashMap<String, AppliedMigration>, CliError> {
    let mut statement = connection
        .prepare("SELECT id, checksum FROM schema_migrations ORDER BY id ASC")
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                AppliedMigration {
                    checksum: row.get::<_, String>(1)?,
                },
            ))
        })
        .map_err(to_cli_error)?;
    rows.map(|row| row.map_err(to_cli_error)).collect()
}

fn apply_migration(connection: &mut Connection, migration: &Migration) -> Result<(), CliError> {
    let checksum = migration_checksum(migration);
    let tx = connection.transaction().map_err(to_cli_error)?;
    (migration.up)(&tx)?;
    tx.execute(
        "INSERT INTO schema_migrations (id, description, checksum, applied_at) VALUES (?, ?, ?, ?)",
        params![
            migration.id,
            migration.description,
            checksum,
            crate::control::now_iso()
        ],
    )
    .map_err(to_cli_error)?;
    tx.commit().map_err(to_cli_error)
}

fn migration_checksum(migration: &Migration) -> String {
    format!(
        "{:x}",
        Sha256::digest(migration.checksum_material.as_bytes())
    )
}

fn migration_0001(tx: &Transaction<'_>) -> Result<(), CliError> {
    tx.execute_batch(MIGRATION_0001_SQL).map_err(to_cli_error)
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
