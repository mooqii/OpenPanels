use super::*;

pub(crate) fn prepare_staged_resource(
    paths: &MyOpenPanelsPaths,
    staged: &StagedResource,
    stage_dir: &Path,
    mime_override: Option<(&str, &str)>,
) -> Result<(PathBuf, ActivePointer), CliError> {
    let kind = ResourceKind::parse(&staged.resource_kind)?;
    let resource = crate::storage::ensure_resource_storage_dir(
        &paths.storage_dir,
        &staged.project_id,
        kind.as_str(),
        &staged.resource_key,
    )?;
    let revision_id = crate::ids::random_id("revision");
    let temporary = resource.join(format!(".{}.tmp", sanitize_path_part(&revision_id)));
    let replace_all = staged
        .metadata
        .get("replaceAll")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || read_json::<bool>(&stage_dir.join("replace-all.json")).unwrap_or(false);
    struct ContentSource {
        path: PathBuf,
        mime_type: String,
        content_hash: String,
        size_bytes: i64,
    }
    let mut content = BTreeMap::<String, ContentSource>::new();
    if !replace_all {
        if let Some(parent) = staged.base_revision_id.as_deref() {
            let parent_revision = revision_dir(&resource, parent);
            let parent_manifest: RevisionManifest =
                read_json(&parent_revision.join("manifest.json"))?;
            for file in parent_manifest.files {
                let path = revision_object_path(&parent_revision, &file)?;
                let size_bytes = fs::metadata(&path).map_err(to_cli_error)?.len() as i64;
                if size_bytes != file.size_bytes || hash_file(&path)? != file.content_hash {
                    return Err(CliError::with_code(
                        "content_integrity_failed",
                        format!("Parent content file {} is corrupt.", file.logical_path),
                    ));
                }
                content.insert(
                    file.logical_path,
                    ContentSource {
                        path,
                        mime_type: file.mime_type,
                        content_hash: file.content_hash,
                        size_bytes,
                    },
                );
            }
        }
    }
    for file in read_staged_files(stage_dir)? {
        let path = staged_file_path(stage_dir, &file)?;
        let size_bytes = fs::metadata(&path).map_err(to_cli_error)?.len() as i64;
        let mime_type = mime_override
            .filter(|value| value.0 == file.logical_path)
            .map(|value| value.1.to_owned())
            .unwrap_or(file.mime_type);
        content.insert(
            file.logical_path,
            ContentSource {
                content_hash: hash_file(&path)?,
                path,
                mime_type,
                size_bytes,
            },
        );
    }
    let total_size = content
        .values()
        .try_fold(0_i64, |total, source| total.checked_add(source.size_bytes))
        .ok_or_else(|| CliError::with_code("content_too_large", "Content size overflowed."))?;
    if total_size > MAX_STAGING_BYTES {
        return Err(CliError::with_code(
            "content_too_large",
            "A content revision cannot exceed the per-resource size limit.",
        ));
    }
    let mut files = Vec::with_capacity(content.len());
    for (logical_path, source) in content {
        let content_hash = source.content_hash;
        let object_ref = format!("objects/{content_hash}");
        let object_path = temporary.join(&object_ref);
        if !object_path.is_file() {
            let parent = object_path
                .parent()
                .ok_or_else(|| CliError::new("Content object path has no parent."))?;
            fs::create_dir_all(parent).map_err(to_cli_error)?;
            if fs::hard_link(&source.path, &object_path).is_err() {
                fs::copy(&source.path, &object_path).map_err(to_cli_error)?;
            }
        }
        if hash_file(&object_path)? != content_hash {
            return Err(CliError::with_code(
                "content_integrity_failed",
                format!("Prepared content file {logical_path} changed while committing."),
            ));
        }
        files.push(RevisionFile {
            logical_path,
            object_ref,
            content_hash,
            size_bytes: source.size_bytes,
            mime_type: source.mime_type,
        });
    }
    if kind == ResourceKind::WikiSpace && files.len() > MAX_WIKI_FILES {
        return Err(CliError::with_code(
            "content_too_large",
            "Wiki contains too many files.",
        ));
    }
    let manifest = RevisionManifest {
        format_version: 2,
        revision_id: revision_id.clone(),
        content_version: staged.base_content_version + 1,
        parent_revision_id: staged.base_revision_id.clone(),
        created_at: now_iso(),
        files,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(to_cli_error)?;
    let content_hash = revision_content_hash(&manifest.files)?;
    fs::create_dir_all(&temporary).map_err(to_cli_error)?;
    fs::write(temporary.join("manifest.json"), &manifest_bytes).map_err(to_cli_error)?;
    fs::rename(&temporary, revision_dir(&resource, &revision_id)).map_err(to_cli_error)?;
    let pointer = ActivePointer {
        revision_id,
        content_version: manifest.content_version,
        manifest_hash: hash_bytes(&manifest_bytes),
        content_hash,
        archived: false,
    };
    Ok((resource.join("active.json"), pointer))
}

pub(super) fn revision_content_hash(files: &[RevisionFile]) -> Result<String, CliError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ContentHashFile<'a> {
        logical_path: &'a str,
        content_hash: &'a str,
        size_bytes: i64,
        mime_type: &'a str,
    }
    let canonical = files
        .iter()
        .map(|file| ContentHashFile {
            logical_path: &file.logical_path,
            content_hash: &file.content_hash,
            size_bytes: file.size_bytes,
            mime_type: &file.mime_type,
        })
        .collect::<Vec<_>>();
    Ok(hash_bytes(
        &serde_json::to_vec(&canonical).map_err(to_cli_error)?,
    ))
}

pub(crate) fn read_active_pointer(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<Option<ActivePointer>, CliError> {
    read_pointer(&resource_dir(paths, project_id, kind, resource_key).join("active.json"))
}

pub(crate) fn read_pending_pointer(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<Option<ActivePointer>, CliError> {
    read_pointer(&resource_dir(paths, project_id, kind, resource_key).join("pending.json"))
}

fn read_pointer(path: &Path) -> Result<Option<ActivePointer>, CliError> {
    if !path.is_file() {
        return Ok(None);
    }
    let pointer: ActivePointer = read_json(path)?;
    Ok((!pointer.archived).then_some(pointer))
}

pub(crate) fn read_authoritative_pointer(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<Option<ActivePointer>, CliError> {
    if kind == ResourceKind::WritingSkill {
        return read_active_pointer(paths, project_id, kind, resource_key);
    }
    let storage = Storage::open(paths)?;
    let pointer = read_authoritative_pointer_from_connection(
        storage.connection(),
        project_id,
        kind,
        resource_key,
    )?;
    if pointer.is_some() {
        return Ok(pointer);
    }
    let exists = storage
        .connection()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM resources WHERE project_id = ? AND id = ?)",
            params![project_id, resource_key],
            |row| row.get::<_, bool>(0),
        )
        .map_err(to_cli_error)?;
    if exists {
        Ok(None)
    } else {
        match read_pending_pointer(paths, project_id, kind, resource_key)? {
            Some(pointer) => Ok(Some(pointer)),
            None => read_active_pointer(paths, project_id, kind, resource_key),
        }
    }
}

pub(crate) fn read_authoritative_pointer_from_connection(
    connection: &rusqlite::Connection,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<Option<ActivePointer>, CliError> {
    if kind == ResourceKind::WritingSkill {
        return Err(CliError::with_code(
            "invalid_content_authority",
            "Writing Skills are filesystem-owned and have no database content pointer.",
        ));
    }
    let row = connection
        .query_row(
            r#"
            SELECT r.kind, d.document_kind, r.active_content_revision_id,
                   r.content_version, r.content_manifest_hash, r.content_hash
            FROM resources r
            LEFT JOIN documents d ON d.resource_id = r.id
            WHERE r.project_id = ? AND r.id = ? AND r.deleted_at IS NULL
            "#,
            params![project_id, resource_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()
        .map_err(to_cli_error)?;
    let Some((resource_kind, document_kind, revision_id, version, manifest_hash, content_hash)) =
        row
    else {
        return Ok(None);
    };
    let matches_kind = match kind {
        ResourceKind::Asset => resource_kind == "asset",
        ResourceKind::WikiMarkdown => {
            resource_kind == "document" && document_kind.as_deref() == Some("wiki_source")
        }
        ResourceKind::MyDocument => {
            resource_kind == "document" && document_kind.as_deref() == Some("my_document")
        }
        ResourceKind::WikiSpace => resource_kind == "wiki_space",
        ResourceKind::WritingSkill => false,
    };
    if !matches_kind {
        return Err(CliError::with_code(
            "content_resource_mismatch",
            format!(
                "Resource {resource_key} does not match content kind {}.",
                kind.as_str()
            ),
        ));
    }
    Ok(revision_id.map(|revision_id| ActivePointer {
        revision_id,
        content_version: version,
        manifest_hash,
        content_hash,
        archived: false,
    }))
}

pub(crate) fn resource_dir(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> PathBuf {
    crate::storage::resource_storage_dir(
        &paths.storage_dir,
        project_id,
        kind.as_str(),
        resource_key,
    )
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
    crate::storage::project_storage_dir(&paths.storage_dir, project_id)
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
