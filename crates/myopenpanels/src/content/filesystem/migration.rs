use super::*;

const CONTENT_OBJECTS_MIGRATION: &str = "0004_content_objects";

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ContentMigrationJournal {
    format_version: u32,
    migration: String,
    processed_revisions: BTreeSet<String>,
    updated_at: String,
    complete: bool,
}

pub(crate) fn migrate_content_format_v2(
    paths: &MyOpenPanelsPaths,
    connection: &Transaction<'_>,
) -> Result<(), CliError> {
    let journal_path = paths
        .storage_dir
        .join(".migrations")
        .join(format!("{CONTENT_OBJECTS_MIGRATION}.json"));
    let mut journal = if journal_path.is_file() {
        read_json(&journal_path)?
    } else {
        ContentMigrationJournal {
            format_version: 1,
            migration: CONTENT_OBJECTS_MIGRATION.to_owned(),
            ..ContentMigrationJournal::default()
        }
    };
    if journal.format_version != 1 || journal.migration != CONTENT_OBJECTS_MIGRATION {
        return Err(CliError::with_code(
            "invalid_migration_journal",
            "The content migration journal has an unsupported identity or format.",
        ));
    }
    journal.complete = false;

    for project in read_dirs(&paths.storage_dir.join("projects"))? {
        let content = project.join("content");
        for kind_dir in read_dirs(&content)? {
            let kind_name = kind_dir
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if ResourceKind::parse(kind_name).is_err() {
                continue;
            }
            for resource in read_dirs(&kind_dir)? {
                for revision in read_dirs(&resource)? {
                    let revision_name = revision
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("");
                    if revision_name.starts_with('.') || revision_name == "materialized" {
                        continue;
                    }
                    let manifest_path = revision.join("manifest.json");
                    if !manifest_path.is_file() {
                        continue;
                    }
                    migrate_revision(&revision, &manifest_path)?;
                    let relative = revision
                        .strip_prefix(&paths.storage_dir)
                        .map_err(to_cli_error)?
                        .to_string_lossy()
                        .into_owned();
                    journal.processed_revisions.insert(relative);
                    journal.updated_at = now_iso();
                    write_json_atomic(&journal_path, &journal)?;
                }
            }
        }
    }

    refresh_database_content_authority(paths, connection)?;
    refresh_writing_skill_pointers(paths)?;
    journal.complete = true;
    journal.updated_at = now_iso();
    write_json_atomic(&journal_path, &journal)
}

fn migrate_revision(revision: &Path, manifest_path: &Path) -> Result<(), CliError> {
    let mut manifest: RevisionManifest = read_json(manifest_path)?;
    if manifest.format_version > CONTENT_FORMAT_VERSION {
        return Err(CliError::with_code(
            "content_format_too_new",
            format!(
                "Content revision {} uses format version {}, but this CLI supports version {}.",
                manifest.revision_id, manifest.format_version, CONTENT_FORMAT_VERSION
            ),
        ));
    }
    let needs_rewrite = manifest.format_version != CONTENT_FORMAT_VERSION
        || manifest.files.iter().any(|file| file.object_ref.is_empty());

    let mut logical_paths = BTreeSet::new();
    for file in &mut manifest.files {
        validate_logical_path(&file.logical_path)?;
        if !logical_paths.insert(file.logical_path.clone()) {
            return Err(CliError::with_code(
                "invalid_content_manifest",
                format!(
                    "Content revision {} contains duplicate logical path {}.",
                    manifest.revision_id, file.logical_path
                ),
            ));
        }
        let object_ref = format!("objects/{}", file.content_hash);
        let object_path = revision.join(&object_ref);
        let source = if object_path.is_file() {
            object_path.clone()
        } else {
            revision_object_path(revision, file)?
        };
        let bytes = fs::read(&source).map_err(to_cli_error)?;
        if hash_bytes(&bytes) != file.content_hash || bytes.len() as i64 != file.size_bytes {
            return Err(CliError::with_code(
                "content_integrity_failed",
                format!(
                    "Content object {} in revision {} does not match its manifest.",
                    file.logical_path, manifest.revision_id
                ),
            ));
        }
        if !object_path.is_file() {
            write_materialized_file(&object_path, &bytes)?;
        }
        file.object_ref = object_ref;
    }
    if needs_rewrite {
        manifest.format_version = CONTENT_FORMAT_VERSION;
        write_json_atomic(manifest_path, &manifest)?;
    }
    Ok(())
}

fn refresh_database_content_authority(
    paths: &MyOpenPanelsPaths,
    connection: &Transaction<'_>,
) -> Result<(), CliError> {
    let resources = {
        let mut statement = connection
            .prepare(
                r#"
                SELECT r.project_id, r.id, r.kind, d.document_kind,
                       r.active_content_revision_id, r.content_version
                FROM resources r
                LEFT JOIN documents d ON d.resource_id = r.id
                WHERE r.deleted_at IS NULL AND r.active_content_revision_id IS NOT NULL
                  AND r.kind IN ('document', 'wiki_space')
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
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .map_err(to_cli_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_cli_error)?;
        collected
    };

    for (project_id, resource_key, resource_kind, document_kind, revision_id, content_version) in
        resources
    {
        let kind = match (resource_kind.as_str(), document_kind.as_deref()) {
            ("document", Some("wiki_source")) => ResourceKind::WikiMarkdown,
            ("document", Some("my_document")) => ResourceKind::MyDocument,
            ("wiki_space", _) => ResourceKind::WikiSpace,
            _ => continue,
        };
        let resource = resource_dir(paths, &project_id, kind, &resource_key);
        let revision = revision_dir(&resource, &revision_id);
        let manifest_path = revision.join("manifest.json");
        let manifest_bytes = fs::read(&manifest_path).map_err(to_cli_error)?;
        let manifest: RevisionManifest =
            serde_json::from_slice(&manifest_bytes).map_err(to_cli_error)?;
        let manifest_hash = hash_bytes(&manifest_bytes);
        let content_hash = revision_content_hash(&manifest.files)?;
        connection
            .execute(
                r#"
                UPDATE resources
                SET content_manifest_hash = ?, content_hash = ?
                WHERE project_id = ? AND id = ?
                "#,
                params![manifest_hash, content_hash, project_id, resource_key],
            )
            .map_err(to_cli_error)?;
        write_json_atomic(
            &resource.join("active.json"),
            &ActivePointer {
                revision_id,
                content_version,
                manifest_hash,
                content_hash,
                archived: false,
            },
        )?;
    }
    Ok(())
}

fn refresh_writing_skill_pointers(paths: &MyOpenPanelsPaths) -> Result<(), CliError> {
    for project in read_dirs(&paths.storage_dir.join("projects"))? {
        let skills = project
            .join("content")
            .join(ResourceKind::WritingSkill.as_str());
        for resource in read_dirs(&skills)? {
            let active_path = resource.join("active.json");
            if !active_path.is_file() {
                continue;
            }
            let mut pointer: ActivePointer = read_json(&active_path)?;
            let manifest_path = revision_dir(&resource, &pointer.revision_id).join("manifest.json");
            let manifest_bytes = fs::read(&manifest_path).map_err(to_cli_error)?;
            let manifest: RevisionManifest =
                serde_json::from_slice(&manifest_bytes).map_err(to_cli_error)?;
            pointer.manifest_hash = hash_bytes(&manifest_bytes);
            pointer.content_hash = revision_content_hash(&manifest.files)?;
            write_json_atomic(&active_path, &pointer)?;
        }
    }
    Ok(())
}
