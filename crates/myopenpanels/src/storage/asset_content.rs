#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetMigrationJournal {
    format_version: u32,
    migration: String,
    processed_assets: BTreeSet<String>,
    #[serde(default)]
    missing_assets: BTreeMap<String, String>,
    updated_at: String,
    complete: bool,
}

const ASSET_OBJECTS_MIGRATION: &str = "0005_asset_objects";

fn asset_resource_dir(root: &Path, project_id: &str, asset_id: &str) -> PathBuf {
    resource_storage_dir(root, project_id, "asset", asset_id)
}

fn asset_revision_dir(
    root: &Path,
    project_id: &str,
    asset_id: &str,
    revision_id: &str,
) -> PathBuf {
    asset_resource_dir(root, project_id, asset_id).join(sanitize_path_part(revision_id))
}

fn asset_revision_ref(
    project_id: &str,
    asset_id: &str,
    revision_id: &str,
    logical_path: &str,
) -> String {
    format!(
        "projects/{}/content/asset/{}/{}/{}",
        sanitize_logical_path_part(project_id),
        sanitize_logical_path_part(asset_id),
        sanitize_logical_path_part(revision_id),
        logical_path
    )
}

fn prepare_asset_revision(
    root: &Path,
    project_id: &str,
    asset_id: &str,
    revision_id: &str,
    content_version: i64,
    parent_revision_id: Option<String>,
    logical_path: &str,
    mime_type: &str,
    bytes: &[u8],
) -> Result<(PathBuf, String, String, PathBuf), CliError> {
    crate::content::validate_logical_path(logical_path)?;
    ensure_resource_storage_dir(root, project_id, "asset", asset_id)?;
    let revision = asset_revision_dir(root, project_id, asset_id, revision_id);
    let content_hash = crate::content::hash_bytes(bytes);
    let object_ref = format!("objects/{content_hash}");
    let object_path = revision.join(&object_ref);
    crate::content::write_materialized_file(&object_path, bytes)?;
    let manifest = crate::content::RevisionManifest {
        format_version: crate::content::CONTENT_FORMAT_VERSION,
        revision_id: revision_id.to_owned(),
        content_version,
        parent_revision_id,
        created_at: crate::control::now_iso(),
        files: vec![crate::content::RevisionFile {
            logical_path: logical_path.to_owned(),
            object_ref,
            content_hash: content_hash.clone(),
            size_bytes: bytes.len() as i64,
            mime_type: mime_type.to_owned(),
        }],
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(to_cli_error)?;
    crate::content::write_materialized_file(&revision.join("manifest.json"), &manifest_bytes)?;
    let materialized = revision
        .join("materialized")
        .join(crate::content::logical_path_buf(logical_path)?);
    crate::content::write_materialized_file(&materialized, bytes)?;
    Ok((
        revision,
        crate::content::hash_bytes(&manifest_bytes),
        content_hash,
        materialized,
    ))
}

fn resolve_manifest_asset_path(root: &Path, asset_ref: &str) -> Result<Option<PathBuf>, CliError> {
    let parts = asset_ref.split('/').collect::<Vec<_>>();
    if parts.len() < 7
        || parts[0] != "projects"
        || parts[1].is_empty()
        || parts[2] != "content"
        || parts[3] != "asset"
        || parts[4].is_empty()
        || parts[5].is_empty()
        || parts[6..].iter().any(|part| part.is_empty())
    {
        return Err(CliError::with_code(
            "invalid_asset_ref",
            "Asset reference has an invalid project content path.",
        ));
    }
    if parts[5].parse::<u64>().is_ok_and(|version| version > 0) {
        return Ok(None);
    }
    let revision = asset_resource_dir(root, parts[1], parts[4])
        .join(sanitize_path_part(parts[5]));
    let manifest_path = revision.join("manifest.json");
    let manifest: crate::content::RevisionManifest =
        crate::content::read_json(&manifest_path)?;
    if manifest.format_version != crate::content::CONTENT_FORMAT_VERSION {
        return Err(CliError::with_code(
            "content_format_too_new",
            "Asset revision uses an unsupported content format.",
        ));
    }
    let logical_path = parts[6..].join("/");
    let file = manifest
        .files
        .iter()
        .find(|file| file.logical_path == logical_path)
        .ok_or_else(|| CliError::with_code("not_found", "Asset file not found."))?;
    let object = crate::content::revision_object_path(&revision, file)?;
    let bytes = fs::read(&object).map_err(to_cli_error)?;
    if bytes.len() as i64 != file.size_bytes || crate::content::hash_bytes(&bytes) != file.content_hash
    {
        return Err(CliError::with_code(
            "content_integrity_failed",
            "Asset object does not match its manifest.",
        ));
    }
    let materialized = revision
        .join("materialized")
        .join(crate::content::logical_path_buf(&logical_path)?);
    if fs::read(&materialized).ok().as_deref() != Some(bytes.as_slice()) {
        crate::content::write_materialized_file(&materialized, &bytes)?;
    }
    Ok(Some(materialized))
}

pub(crate) fn resolve_asset_path(root: &Path, asset_ref: &str) -> Result<PathBuf, CliError> {
    if let Some(path) = resolve_manifest_asset_path(root, asset_ref)? {
        Ok(path)
    } else {
        legacy_asset_path(root, asset_ref)
    }
}

fn legacy_asset_path(root: &Path, asset_ref: &str) -> Result<PathBuf, CliError> {
    let parts = asset_ref.split('/').collect::<Vec<_>>();
    if parts.len() < 7
        || parts[0] != "projects"
        || parts[1].is_empty()
        || parts[2] != "content"
        || parts[3] != "asset"
        || parts[4].is_empty()
        || !parts[5]
            .parse::<u64>()
            .is_ok_and(|version| version > 0)
        || parts[6..].iter().any(|part| part.is_empty())
    {
        return Err(CliError::with_code(
            "invalid_asset_ref",
            "Legacy Asset reference must use projects/<project>/content/asset/<asset>/<version>/<file>.",
        ));
    }
    let mut path = root.to_path_buf();
    for part in parts {
        path.push(sanitize_path_part(part));
    }
    Ok(path)
}

fn migrate_asset_content_v2(
    paths: &MyOpenPanelsPaths,
    connection: &Transaction<'_>,
) -> Result<(), CliError> {
    let journal_path = paths
        .storage_dir
        .join(".migrations")
        .join(format!("{ASSET_OBJECTS_MIGRATION}.json"));
    let mut journal = if journal_path.is_file() {
        crate::content::read_json(&journal_path)?
    } else {
        AssetMigrationJournal {
            format_version: 1,
            migration: ASSET_OBJECTS_MIGRATION.to_owned(),
            ..AssetMigrationJournal::default()
        }
    };
    if journal.format_version != 1 || journal.migration != ASSET_OBJECTS_MIGRATION {
        return Err(CliError::with_code(
            "invalid_migration_journal",
            "The Asset migration journal has an unsupported identity or format.",
        ));
    }
    journal.complete = false;
    let assets = {
        let mut statement = connection
            .prepare(
                r#"
                SELECT r.project_id, r.id, r.active_content_revision_id,
                       r.content_version, r.content_hash, a.active_file_ref,
                       a.file_name, a.media_type
                FROM assets a
                JOIN resources r ON r.id = a.resource_id
                WHERE r.deleted_at IS NULL AND a.active_file_ref IS NOT NULL
                ORDER BY r.project_id, r.id
                "#,
            )
            .map_err(to_cli_error)?;
        let collected = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .map_err(to_cli_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_cli_error)?;
        collected
    };
    for (
        project_id,
        asset_id,
        revision_id,
        content_version,
        stored_hash,
        active_ref,
        file_name,
        media_type,
    ) in assets
    {
        let revision_id =
            revision_id.unwrap_or_else(|| format!("asset-revision:{content_version}"));
        let revision =
            asset_revision_dir(&paths.storage_dir, &project_id, &asset_id, &revision_id);
        let next_ref = asset_revision_ref(&project_id, &asset_id, &revision_id, &file_name);
        let existing_revision = revision.join("manifest.json").is_file();
        let source = if existing_revision {
            resolve_manifest_asset_path(&paths.storage_dir, &next_ref)?.ok_or_else(|| {
                CliError::with_code("content_not_found", "Migrated Asset revision is missing.")
            })?
        } else if let Some(path) =
            resolve_manifest_asset_path(&paths.storage_dir, &active_ref)?
        {
            path
        } else {
            legacy_asset_path(&paths.storage_dir, &active_ref)?
        };
        if !source.is_file() {
            let now = crate::control::now_iso();
            connection
                .execute(
                    "UPDATE resources SET deleted_at = ?, updated_at = ? WHERE project_id = ? AND id = ?",
                    params![now, now, project_id, asset_id],
                )
                .map_err(to_cli_error)?;
            journal
                .missing_assets
                .insert(format!("{project_id}/{asset_id}"), active_ref);
            journal.updated_at = crate::control::now_iso();
            crate::content::write_json_atomic(&journal_path, &journal)?;
            continue;
        }
        let bytes = fs::read(&source).map_err(to_cli_error)?;
        let actual_hash = crate::content::hash_bytes(&bytes);
        if !stored_hash.is_empty() && stored_hash != actual_hash {
            return Err(CliError::with_code(
                "content_integrity_failed",
                format!("Asset {asset_id} does not match its database content hash."),
            ));
        }
        let (revision, manifest_hash, content_hash) = if existing_revision {
            (
                revision.clone(),
                crate::content::hash_file(&revision.join("manifest.json"))?,
                actual_hash,
            )
        } else {
            let (revision, manifest_hash, content_hash, _) = prepare_asset_revision(
                &paths.storage_dir,
                &project_id,
                &asset_id,
                &revision_id,
                content_version,
                None,
                &file_name,
                &media_type,
                &bytes,
            )?;
            (revision, manifest_hash, content_hash)
        };
        connection
            .execute(
                r#"
                UPDATE resources
                SET active_content_revision_id = ?, content_manifest_hash = ?,
                    content_hash = ?
                WHERE project_id = ? AND id = ?
                "#,
                params![
                    revision_id,
                    manifest_hash,
                    content_hash,
                    project_id,
                    asset_id
                ],
            )
            .map_err(to_cli_error)?;
        connection
            .execute(
                "UPDATE assets SET active_file_ref = ? WHERE resource_id = ?",
                params![next_ref, asset_id],
            )
            .map_err(to_cli_error)?;
        crate::content::write_json_atomic(
            &asset_resource_dir(&paths.storage_dir, &project_id, &asset_id).join("active.json"),
            &crate::content::ActivePointer {
                revision_id,
                content_version,
                manifest_hash,
                content_hash,
                archived: false,
            },
        )?;
        let relative = revision
            .strip_prefix(&paths.storage_dir)
            .map_err(to_cli_error)?
            .to_string_lossy()
            .into_owned();
        journal.processed_assets.insert(relative);
        journal.updated_at = crate::control::now_iso();
        crate::content::write_json_atomic(&journal_path, &journal)?;
    }
    journal.complete = true;
    journal.updated_at = crate::control::now_iso();
    crate::content::write_json_atomic(&journal_path, &journal)
}
