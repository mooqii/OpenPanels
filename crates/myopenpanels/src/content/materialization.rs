#[derive(Debug, Clone)]
pub struct ActiveResourceFile {
    pub logical_path: String,
    pub object_hash: String,
    pub size_bytes: i64,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ActiveResourceSnapshot {
    pub revision_id: String,
    pub content_version: i64,
    pub manifest_hash: String,
    pub files: Vec<ActiveResourceFile>,
}

pub fn active_resource_descriptor(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<Option<Value>, CliError> {
    let storage = Storage::open(paths)?;
    storage
        .connection()
        .query_row(
            r#"
            SELECT resource.active_revision_id, resource.content_version, revision.manifest_hash
            FROM content_resources resource
            JOIN content_revisions revision ON revision.id = resource.active_revision_id
            WHERE resource.project_id = ? AND resource.resource_kind = ?
              AND resource.resource_key = ? AND resource.archived_at IS NULL
            "#,
            params![project_id, kind.as_str(), resource_key],
            |row| {
                Ok(json!({
                    "revisionId": row.get::<_, String>(0)?,
                    "contentVersion": row.get::<_, i64>(1)?,
                    "manifestHash": row.get::<_, String>(2)?,
                }))
            },
        )
        .optional()
        .map_err(to_cli_error)
}

pub fn active_resource_snapshot(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<Option<ActiveResourceSnapshot>, CliError> {
    let storage = Storage::open(paths)?;
    let header = storage
        .connection()
        .query_row(
            r#"
            SELECT resource.active_revision_id, resource.content_version, revision.manifest_hash
            FROM content_resources resource
            JOIN content_revisions revision ON revision.id = resource.active_revision_id
            WHERE resource.project_id = ? AND resource.resource_kind = ?
              AND resource.resource_key = ? AND resource.archived_at IS NULL
            "#,
            params![project_id, kind.as_str(), resource_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(to_cli_error)?;
    let Some((revision_id, content_version, manifest_hash)) = header else {
        return Ok(None);
    };
    let mut statement = storage
        .connection()
        .prepare(
            r#"
            SELECT logical_path, object_hash, size_bytes, mime_type
            FROM content_revision_files
            WHERE revision_id = ?
            ORDER BY logical_path
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([&revision_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    let files = rows
        .into_iter()
        .map(|(logical_path, object_hash, size_bytes, mime_type)| {
            Ok(ActiveResourceFile {
                logical_path,
                bytes: read_object(paths, &object_hash)?,
                object_hash,
                size_bytes,
                mime_type,
            })
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    Ok(Some(ActiveResourceSnapshot {
        revision_id,
        content_version,
        manifest_hash,
        files,
    }))
}

pub fn materialize_active_file(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
    destination: &Path,
) -> Result<Option<Value>, CliError> {
    let Some(snapshot) = active_resource_snapshot(paths, project_id, kind, resource_key)? else {
        return Ok(None);
    };
    let Some(file) = snapshot
        .files
        .iter()
        .find(|file| file.logical_path == logical_path)
    else {
        return Err(CliError::with_code(
            "content_unavailable",
            format!(
                "Active content revision does not contain the expected file: {logical_path}"
            ),
        ));
    };
    write_materialized_file(destination, &file.bytes)?;
    Ok(Some(json!({
        "revisionId": snapshot.revision_id,
        "contentVersion": snapshot.content_version,
        "manifestHash": snapshot.manifest_hash,
        "logicalPath": file.logical_path,
        "objectHash": file.object_hash,
        "sizeBytes": file.size_bytes,
        "mimeType": file.mime_type,
        "localPath": destination,
    })))
}

pub(crate) fn write_materialized_file(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let parent = path
        .parent()
        .ok_or_else(|| CliError::new("Materialized content path has no parent."))?;
    fs::create_dir_all(parent).map_err(to_cli_error)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(to_cli_error)?;
    temporary.write_all(bytes).map_err(to_cli_error)?;
    temporary.as_file().sync_all().map_err(to_cli_error)?;
    temporary.persist(path).map_err(|error| to_cli_error(error.error))?;
    Ok(())
}
