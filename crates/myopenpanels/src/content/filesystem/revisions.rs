use super::*;

pub(crate) fn commit_staged_resource(
    paths: &MyOpenPanelsPaths,
    staged: &StagedResource,
    stage_dir: &Path,
) -> Result<ActivePointer, CliError> {
    commit_staged_resource_with_mime(paths, staged, stage_dir, None)
}

pub(crate) fn commit_staged_resource_with_mime(
    paths: &MyOpenPanelsPaths,
    staged: &StagedResource,
    stage_dir: &Path,
    mime_override: Option<(&str, &str)>,
) -> Result<ActivePointer, CliError> {
    let (active_path, pointer) = prepare_staged_resource(paths, staged, stage_dir, mime_override)?;
    write_json_atomic(&active_path, &pointer)?;
    Ok(pointer)
}

pub(crate) fn prepare_staged_resource(
    paths: &MyOpenPanelsPaths,
    staged: &StagedResource,
    stage_dir: &Path,
    mime_override: Option<(&str, &str)>,
) -> Result<(PathBuf, ActivePointer), CliError> {
    let kind = ResourceKind::parse(&staged.resource_kind)?;
    let resource = resource_dir(paths, &staged.project_id, kind, &staged.resource_key);
    fs::create_dir_all(&resource).map_err(to_cli_error)?;
    write_json_atomic(
        &resource.join("resource.json"),
        &json!({ "resourceKey": staged.resource_key }),
    )?;
    let revision_id = crate::ids::random_id("revision");
    let temporary = resource.join(format!(".{}.tmp", sanitize_path_part(&revision_id)));
    let files_dir = temporary.join("files");
    let replace_all = staged
        .metadata
        .get("replaceAll")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || read_json::<bool>(&stage_dir.join("replace-all.json")).unwrap_or(false);
    if !replace_all {
        if let Some(parent) = staged.base_revision_id.as_deref() {
            copy_tree(&revision_dir(&resource, parent).join("files"), &files_dir)?;
        }
    }
    copy_tree(&stage_dir.join("files"), &files_dir)?;
    let mut files = Vec::new();
    for (logical_path, path) in revision_files(&files_dir)? {
        if logical_path.ends_with(".mopmeta") {
            continue;
        }
        let bytes = fs::read(&path).map_err(to_cli_error)?;
        let mime_type = mime_override
            .filter(|value| value.0 == logical_path)
            .map(|value| value.1.to_owned())
            .unwrap_or_else(|| mime_for_path(&path));
        files.push(RevisionFile {
            logical_path,
            content_hash: hash_bytes(&bytes),
            size_bytes: bytes.len() as i64,
            mime_type,
        });
    }
    files.sort_by(|left, right| left.logical_path.cmp(&right.logical_path));
    if kind == ResourceKind::WikiSpace && files.len() > MAX_WIKI_FILES {
        return Err(CliError::with_code(
            "content_too_large",
            "Wiki contains too many files.",
        ));
    }
    let manifest = RevisionManifest {
        revision_id: revision_id.clone(),
        content_version: staged.base_content_version + 1,
        parent_revision_id: staged.base_revision_id.clone(),
        created_at: now_iso(),
        files,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(to_cli_error)?;
    fs::create_dir_all(&temporary).map_err(to_cli_error)?;
    fs::write(temporary.join("manifest.json"), &manifest_bytes).map_err(to_cli_error)?;
    fs::rename(&temporary, revision_dir(&resource, &revision_id)).map_err(to_cli_error)?;
    let pointer = ActivePointer {
        revision_id,
        content_version: manifest.content_version,
        manifest_hash: hash_bytes(&manifest_bytes),
        archived: false,
    };
    Ok((resource.join("active.json"), pointer))
}

pub(crate) fn read_active_pointer(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<Option<ActivePointer>, CliError> {
    let path = resource_dir(paths, project_id, kind, resource_key).join("active.json");
    if !path.is_file() {
        return Ok(None);
    }
    let pointer: ActivePointer = read_json(&path)?;
    Ok((!pointer.archived).then_some(pointer))
}

pub(crate) fn resource_dir(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> PathBuf {
    paths
        .storage_dir
        .join("projects")
        .join(sanitize_path_part(project_id))
        .join("content")
        .join(kind.as_str())
        .join(sanitize_path_part(resource_key))
}

pub(crate) fn revision_dir(resource: &Path, revision_id: &str) -> PathBuf {
    resource.join(sanitize_path_part(revision_id))
}

pub(crate) fn staging_task_dir(paths: &MyOpenPanelsPaths, context: &ExecutionContext) -> PathBuf {
    staging_root(
        paths,
        &context.project_id,
        &context.task_id,
        context.generation,
    )
}

pub(crate) fn staging_root(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    task_id: &str,
    generation: i64,
) -> PathBuf {
    paths
        .storage_dir
        .join("projects")
        .join(sanitize_path_part(project_id))
        .join("content/.staging")
        .join(sanitize_path_part(task_id))
        .join(generation.to_string())
}

pub(crate) fn staging_resource_dir(
    paths: &MyOpenPanelsPaths,
    context: &ExecutionContext,
    kind: ResourceKind,
    resource_key: &str,
) -> PathBuf {
    staging_task_dir(paths, context)
        .join(kind.as_str())
        .join(sanitize_path_part(resource_key))
}

pub(crate) fn staging_root_for_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
) -> Result<Option<PathBuf>, CliError> {
    let storage = Storage::open(paths)?;
    storage
        .connection()
        .query_row(
            "SELECT project_id, execution_generation FROM tasks WHERE id = ?",
            [task_id],
            |row| {
                Ok(staging_root(
                    paths,
                    &row.get::<_, String>(0)?,
                    task_id,
                    row.get::<_, i64>(1)?,
                ))
            },
        )
        .optional()
        .map_err(to_cli_error)
}

pub(crate) fn task_input_from_storage(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
) -> Result<(String, Value), CliError> {
    if let Some((project_id, input_json)) = Storage::open(paths)?
        .connection()
        .query_row(
            "SELECT project_id, input_json FROM tasks WHERE id = ?",
            [task_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(to_cli_error)?
    {
        return Ok((
            project_id,
            serde_json::from_str(&input_json).unwrap_or_else(|_| json!({})),
        ));
    }
    Ok((
        crate::control::read_project_bootstrap(paths, crate::control::BootstrapRequest::new())?
            .project
            .id,
        json!({}),
    ))
}
