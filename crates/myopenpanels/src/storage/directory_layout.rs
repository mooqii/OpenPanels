const PROJECT_DESCRIPTOR_FILE: &str = "project.json";
const DIRECTORY_KEYS_MIGRATION: &str = "0008_stable_directory_keys";

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DirectoryKeysMigrationJournal {
    format_version: u32,
    migration: String,
    processed_projects: std::collections::BTreeSet<String>,
    updated_at: String,
    complete: bool,
}

fn legacy_project_storage_dir(root: &Path, project_id: &str) -> PathBuf {
    root.join("projects").join(sanitize_path_part(project_id))
}

fn stable_project_storage_dir(root: &Path, project_id: &str) -> PathBuf {
    root.join("projects")
        .join(crate::paths::stable_path_key(project_id))
}

pub(crate) fn project_storage_dir(root: &Path, project_id: &str) -> PathBuf {
    let stable = stable_project_storage_dir(root, project_id);
    if project_descriptor_matches(&stable, project_id) {
        stable
    } else {
        legacy_project_storage_dir(root, project_id)
    }
}

pub(crate) fn resource_storage_dir(
    root: &Path,
    project_id: &str,
    resource_kind: &str,
    resource_key: &str,
) -> PathBuf {
    let project = project_storage_dir(root, project_id);
    let key = if project_descriptor_matches(&project, project_id) {
        crate::paths::stable_path_key(resource_key)
    } else {
        sanitize_path_part(resource_key)
    };
    project.join("content").join(resource_kind).join(key)
}

fn ensure_project_storage_dir(root: &Path, project_id: &str) -> Result<PathBuf, CliError> {
    let directory = stable_project_storage_dir(root, project_id);
    let descriptor = directory.join(PROJECT_DESCRIPTOR_FILE);
    if descriptor.is_file() {
        if project_descriptor_matches(&directory, project_id) {
            return Ok(directory);
        }
        return Err(storage_path_collision("Project", project_id));
    }
    if directory.is_dir() && fs::read_dir(&directory).map_err(to_cli_error)?.next().is_some() {
        return Err(storage_path_collision("Project", project_id));
    }
    fs::create_dir_all(&directory).map_err(to_cli_error)?;
    write_project_descriptor(&directory, project_id)?;
    Ok(directory)
}

pub(crate) fn ensure_resource_storage_dir(
    root: &Path,
    project_id: &str,
    resource_kind: &str,
    resource_key: &str,
) -> Result<PathBuf, CliError> {
    let mut project = project_storage_dir(root, project_id);
    if !project.is_dir() {
        project = ensure_project_storage_dir(root, project_id)?;
    }
    let stable_layout = project_descriptor_matches(&project, project_id);
    let directory = project
        .join("content")
        .join(resource_kind)
        .join(if stable_layout {
            crate::paths::stable_path_key(resource_key)
        } else {
            sanitize_path_part(resource_key)
        });
    fs::create_dir_all(&directory).map_err(to_cli_error)?;
    write_resource_descriptor(&directory, project_id, resource_kind, resource_key)?;
    Ok(directory)
}

fn project_descriptor_matches(directory: &Path, project_id: &str) -> bool {
    fs::read(directory.join(PROJECT_DESCRIPTOR_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| {
            value
                .get("projectId")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .as_deref()
        == Some(project_id)
}

fn write_project_descriptor(directory: &Path, project_id: &str) -> Result<(), CliError> {
    if directory.join(PROJECT_DESCRIPTOR_FILE).is_file()
        && !project_descriptor_matches(directory, project_id)
    {
        return Err(storage_path_collision("Project", project_id));
    }
    crate::content::write_json_atomic(
        &directory.join(PROJECT_DESCRIPTOR_FILE),
        &json!({
            "formatVersion": 1,
            "projectId": project_id,
        }),
    )
}

fn write_resource_descriptor(
    directory: &Path,
    project_id: &str,
    resource_kind: &str,
    resource_key: &str,
) -> Result<(), CliError> {
    let descriptor = directory.join("resource.json");
    if descriptor.is_file() {
        let current: Value = crate::content::read_json(&descriptor)?;
        if current.get("resourceKey").and_then(Value::as_str) != Some(resource_key)
            || current
                .get("projectId")
                .and_then(Value::as_str)
                .is_some_and(|value| value != project_id)
            || current
                .get("resourceKind")
                .and_then(Value::as_str)
                .is_some_and(|value| value != resource_kind)
        {
            return Err(storage_path_collision("Resource", resource_key));
        }
    }
    crate::content::write_json_atomic(
        &descriptor,
        &json!({
            "formatVersion": 1,
            "projectId": project_id,
            "resourceKind": resource_kind,
            "resourceKey": resource_key,
        }),
    )
}

fn storage_path_collision(kind: &str, logical_id: &str) -> CliError {
    CliError::with_recovery(
        "storage_path_collision",
        format!("{kind} {logical_id} conflicts with an existing storage directory."),
        false,
        "Restore the last storage backup and resolve the conflicting logical IDs before retrying.",
    )
}

fn migrate_stable_directory_keys(
    paths: &MyOpenPanelsPaths,
    connection: &Transaction<'_>,
) -> Result<(), CliError> {
    let journal_path = paths
        .storage_dir
        .join(".migrations")
        .join(format!("{DIRECTORY_KEYS_MIGRATION}.json"));
    let mut journal = if journal_path.is_file() {
        crate::content::read_json(&journal_path)?
    } else {
        DirectoryKeysMigrationJournal {
            format_version: 1,
            migration: DIRECTORY_KEYS_MIGRATION.to_owned(),
            ..DirectoryKeysMigrationJournal::default()
        }
    };
    if journal.format_version != 1 || journal.migration != DIRECTORY_KEYS_MIGRATION {
        return Err(CliError::with_code(
            "invalid_migration_journal",
            "The directory-key migration journal has an unsupported identity or format.",
        ));
    }
    journal.complete = false;

    let projects = migration_project_ids(connection)?;
    let resources = migration_content_resources(connection)?;
    preflight_legacy_directory_collisions(paths, &projects, &resources)?;

    for project_id in projects {
        migrate_project_directory(paths, &project_id, &resources)?;
        journal.processed_projects.insert(project_id);
        journal.updated_at = crate::control::now_iso();
        crate::content::write_json_atomic(&journal_path, &journal)?;
    }
    journal.complete = true;
    journal.updated_at = crate::control::now_iso();
    crate::content::write_json_atomic(&journal_path, &journal)
}

fn migration_project_ids(connection: &Transaction<'_>) -> Result<Vec<String>, CliError> {
    let mut statement = connection
        .prepare("SELECT id FROM projects ORDER BY id")
        .map_err(to_cli_error)?;
    let projects = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    Ok(projects)
}

fn migration_content_resources(
    connection: &Transaction<'_>,
) -> Result<Vec<(String, String, String)>, CliError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT r.project_id, r.id, r.kind, d.document_kind
            FROM resources r
            LEFT JOIN documents d ON d.resource_id = r.id
            ORDER BY r.project_id, r.id
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
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
    Ok(rows
        .into_iter()
        .filter_map(
            |(project_id, resource_id, kind, document_kind)| {
                let directory_kind = match (kind.as_str(), document_kind.as_deref()) {
                    ("asset", _) => "asset",
                    ("wiki_space", _) => "wiki_space",
                    ("document", Some("wiki_source")) => "wiki_markdown",
                    ("document", Some("my_document")) => "my_document",
                    _ => return None,
                };
                Some((project_id, directory_kind.to_owned(), resource_id))
            },
        )
        .collect())
}

fn preflight_legacy_directory_collisions(
    paths: &MyOpenPanelsPaths,
    projects: &[String],
    resources: &[(String, String, String)],
) -> Result<(), CliError> {
    let mut project_keys = std::collections::BTreeMap::<String, Vec<&str>>::new();
    for project_id in projects {
        project_keys
            .entry(sanitize_path_part(project_id))
            .or_default()
            .push(project_id);
    }
    for (key, ids) in project_keys {
        if ids.len() > 1 && paths.storage_dir.join("projects").join(key).is_dir() {
            return Err(storage_path_collision("Projects", &ids.join(", ")));
        }
    }

    let mut resource_keys =
        std::collections::BTreeMap::<(String, String, String), Vec<&str>>::new();
    for (project_id, kind, resource_id) in resources {
        resource_keys
            .entry((
                project_id.clone(),
                kind.clone(),
                sanitize_path_part(resource_id),
            ))
            .or_default()
            .push(resource_id);
    }
    for ((project_id, kind, key), ids) in resource_keys {
        let legacy_project = legacy_project_storage_dir(&paths.storage_dir, &project_id);
        let project = if legacy_project.is_dir() {
            legacy_project
        } else {
            stable_project_storage_dir(&paths.storage_dir, &project_id)
        };
        if ids.len() > 1 && project.join("content").join(kind).join(key).is_dir() {
            return Err(storage_path_collision("Resources", &ids.join(", ")));
        }
    }
    Ok(())
}

fn migrate_project_directory(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    resources: &[(String, String, String)],
) -> Result<(), CliError> {
    let legacy = legacy_project_storage_dir(&paths.storage_dir, project_id);
    let stable = stable_project_storage_dir(&paths.storage_dir, project_id);
    if legacy != stable && legacy.is_dir() && stable.is_dir() {
        return Err(storage_path_collision("Project", project_id));
    }
    if legacy.is_dir() && legacy != stable {
        fs::create_dir_all(stable.parent().unwrap_or(&paths.storage_dir)).map_err(to_cli_error)?;
        fs::rename(&legacy, &stable).map_err(to_cli_error)?;
    } else if !stable.is_dir() {
        fs::create_dir_all(&stable).map_err(to_cli_error)?;
    }

    let expected = resources
        .iter()
        .filter(|(owner, _, _)| owner == project_id)
        .map(|(_, kind, key)| (kind.as_str(), key.as_str()))
        .collect::<Vec<_>>();
    for kind in [
        "asset",
        "wiki_markdown",
        "wiki_space",
        "my_document",
        "writing_skill",
    ] {
        let kind_dir = stable.join("content").join(kind);
        for source in crate::content::read_dirs(&kind_dir)? {
            let name = source
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            let resource_key = crate::content::read_resource_key(&source).or_else(|| {
                expected
                    .iter()
                    .filter(|(expected_kind, key)| {
                        *expected_kind == kind
                            && (sanitize_path_part(key) == name
                                || crate::paths::stable_path_key(key) == name)
                    })
                    .map(|(_, key)| (*key).to_owned())
                    .next()
            });
            let Some(resource_key) = resource_key else {
                continue;
            };
            let target = kind_dir.join(crate::paths::stable_path_key(&resource_key));
            move_directory(&source, &target, "Resource", &resource_key)?;
            write_resource_descriptor(&target, project_id, kind, &resource_key)?;
        }
    }

    let wiki_root = stable.join("materialized").join("wikis");
    for (_, _, wiki_space_id) in resources
        .iter()
        .filter(|(owner, kind, _)| owner == project_id && kind == "wiki_space")
    {
        let source = wiki_root.join(sanitize_path_part(wiki_space_id));
        let target = wiki_root.join(crate::paths::stable_path_key(wiki_space_id));
        move_directory(&source, &target, "Wiki Space", wiki_space_id)?;
    }
    write_project_descriptor(&stable, project_id)
}

fn move_directory(
    source: &Path,
    target: &Path,
    kind: &str,
    logical_id: &str,
) -> Result<(), CliError> {
    if source == target || !source.exists() {
        return Ok(());
    }
    if target.exists() {
        return Err(storage_path_collision(kind, logical_id));
    }
    fs::rename(source, target).map_err(to_cli_error)
}
