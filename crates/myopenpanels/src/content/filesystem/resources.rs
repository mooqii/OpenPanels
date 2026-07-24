use super::*;

pub fn active_resource_descriptor(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<Option<Value>, CliError> {
    Ok(
        read_authoritative_pointer(paths, project_id, kind, resource_key)?.map(|active| {
            json!({
                "revisionId": active.revision_id,
                "contentVersion": active.content_version,
                "manifestHash": active.manifest_hash,
                "contentHash": active.content_hash,
            })
        }),
    )
}

pub fn active_resource_snapshot(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<Option<ActiveResourceSnapshot>, CliError> {
    let Some(active) = read_authoritative_pointer(paths, project_id, kind, resource_key)? else {
        return Ok(None);
    };
    resource_snapshot_at_revision(paths, project_id, kind, resource_key, &active.revision_id)
}

pub(crate) fn resource_snapshot_at_revision(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    revision_id: &str,
) -> Result<Option<ActiveResourceSnapshot>, CliError> {
    let resource = resource_dir(paths, project_id, kind, resource_key);
    let revision = revision_dir(&resource, revision_id);
    let manifest_path = revision.join("manifest.json");
    if !manifest_path.is_file() {
        return Ok(None);
    }
    let manifest: RevisionManifest = read_json(&manifest_path)?;
    if manifest.format_version > CONTENT_FORMAT_VERSION {
        return Err(CliError::with_code(
            "content_format_too_new",
            format!(
                "Content revision {} uses unsupported format version {}.",
                manifest.revision_id, manifest.format_version
            ),
        ));
    }
    let content_hash = revision_content_hash(&manifest.files)?;
    let files = manifest
        .files
        .into_iter()
        .map(|file| {
            let bytes = fs::read(revision_object_path(&revision, &file)?).map_err(to_cli_error)?;
            if bytes.len() as i64 != file.size_bytes || hash_bytes(&bytes) != file.content_hash {
                return Err(CliError::with_code(
                    "content_integrity_failed",
                    format!(
                        "Content file {} does not match revision {}.",
                        file.logical_path, manifest.revision_id
                    ),
                ));
            }
            Ok(ActiveResourceFile {
                logical_path: file.logical_path,
                object_hash: file.content_hash,
                size_bytes: file.size_bytes,
                mime_type: file.mime_type,
                bytes,
            })
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    Ok(Some(ActiveResourceSnapshot {
        revision_id: manifest.revision_id,
        content_version: manifest.content_version,
        manifest_hash: hash_bytes(&fs::read(manifest_path).map_err(to_cli_error)?),
        content_hash,
        files,
    }))
}

pub(crate) fn projected_active_resource_snapshot(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<Option<ActiveResourceSnapshot>, CliError> {
    let Some(active) = read_active_pointer(paths, project_id, kind, resource_key)? else {
        return Ok(None);
    };
    resource_snapshot_at_revision(paths, project_id, kind, resource_key, &active.revision_id)
}

pub(crate) fn resource_snapshot_for_task(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    input: &Value,
) -> Result<Option<ActiveResourceSnapshot>, CliError> {
    if let Some(revision_id) = pinned_revision_id(input, kind, resource_key) {
        return resource_snapshot_at_revision(paths, project_id, kind, resource_key, revision_id);
    }
    active_resource_snapshot(paths, project_id, kind, resource_key)
}

pub(crate) fn pinned_revision_id<'a>(
    input: &'a Value,
    kind: ResourceKind,
    resource_key: &str,
) -> Option<&'a str> {
    match kind {
        ResourceKind::Asset => None,
        ResourceKind::WikiSpace
            if input
                .pointer("/contextSnapshot/wikiSelection/wikiSpaceId")
                .and_then(Value::as_str)
                == Some(resource_key) =>
        {
            input
                .pointer("/contextSnapshot/wikiSelection/contentRevisionId")
                .and_then(Value::as_str)
        }
        _ => None,
    }
}

pub fn read_active_text(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
) -> Result<Option<String>, CliError> {
    let Some(snapshot) = active_resource_snapshot(paths, project_id, kind, resource_key)? else {
        return Ok(None);
    };
    snapshot
        .files
        .into_iter()
        .find(|file| file.logical_path == logical_path)
        .map(|file| {
            String::from_utf8(file.bytes)
                .map_err(|_| CliError::with_code("invalid_content", "Stored content is not UTF-8."))
        })
        .transpose()
}

pub fn rename_active_file(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    current_path: &str,
    next_path: &str,
) -> Result<Option<Value>, CliError> {
    let Some(snapshot) = active_resource_snapshot(paths, project_id, kind, resource_key)? else {
        return Ok(None);
    };
    if snapshot
        .files
        .iter()
        .any(|file| file.logical_path == next_path)
    {
        return Err(CliError::with_code(
            "content_conflict",
            "Destination content already exists.",
        ));
    }
    let stage = tempfile::tempdir_in(&paths.storage_dir).map_err(to_cli_error)?;
    if !snapshot
        .files
        .iter()
        .any(|file| file.logical_path == current_path)
    {
        return Err(CliError::with_code(
            "content_unavailable",
            "Source content does not exist.",
        ));
    }
    for file in &snapshot.files {
        let logical_path = if file.logical_path == current_path {
            next_path
        } else {
            &file.logical_path
        };
        write_staged_file(
            stage.path(),
            logical_path,
            &file.bytes,
            &file.mime_type,
            json!({}),
        )?;
    }
    let staged = StagedResource {
        project_id: project_id.to_owned(),
        panel_id: String::new(),
        resource_kind: kind.as_str().to_owned(),
        resource_key: resource_key.to_owned(),
        base_revision_id: Some(snapshot.revision_id),
        base_content_version: snapshot.content_version,
        metadata: json!({ "replaceAll": true }),
    };
    let (active_path, pointer) = prepare_staged_resource(paths, &staged, stage.path(), None)?;
    crate::content::publish_immediate_pointer_with_authority(
        paths,
        project_id,
        kind,
        resource_key,
        &active_path,
        &pointer,
    )?;
    Ok(Some(json!({
        "revisionId": pointer.revision_id,
        "contentVersion": pointer.content_version,
        "manifestHash": pointer.manifest_hash,
        "contentHash": pointer.content_hash,
    })))
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
    let file = snapshot
        .files
        .iter()
        .find(|file| file.logical_path == logical_path)
        .ok_or_else(|| {
            CliError::with_code(
                "content_unavailable",
                "Active revision does not contain the file.",
            )
        })?;
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
        .ok_or_else(|| CliError::new("Content path has no parent."))?;
    fs::create_dir_all(parent).map_err(to_cli_error)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(to_cli_error)?;
    temporary.write_all(bytes).map_err(to_cli_error)?;
    temporary.as_file().sync_all().map_err(to_cli_error)?;
    temporary
        .persist(path)
        .map_err(|error| to_cli_error(error.error))?;
    Ok(())
}

pub fn active_writing_skill_sources(
    paths: &MyOpenPanelsPaths,
) -> Result<Vec<(String, String, Value, String)>, CliError> {
    let mut result = Vec::new();
    let projects = Storage::open(paths)?
        .list_projects()?
        .into_iter()
        .map(|project| project.id)
        .collect::<Vec<_>>();
    for project_id in projects {
        let project = crate::storage::project_storage_dir(&paths.storage_dir, &project_id);
        let skills = project
            .join("content")
            .join(ResourceKind::WritingSkill.as_str());
        for skill in read_dirs(&skills)? {
            let skill_id = read_resource_key(&skill).unwrap_or_else(|| {
                skill
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
            let Some(snapshot) = active_resource_snapshot(
                paths,
                &project_id,
                ResourceKind::WritingSkill,
                &skill_id,
            )?
            else {
                continue;
            };
            let source = snapshot
                .files
                .iter()
                .find(|file| file.logical_path == "SKILL.md")
                .and_then(|file| String::from_utf8(file.bytes.clone()).ok());
            let manifest = snapshot
                .files
                .iter()
                .find(|file| file.logical_path == "manifest.json")
                .and_then(|file| serde_json::from_slice::<Value>(&file.bytes).ok());
            if let (Some(source), Some(manifest)) = (source, manifest) {
                let dir = skill.join("materialized").join(&snapshot.revision_id);
                for file in &snapshot.files {
                    let path = dir.join(&file.logical_path);
                    let matches = fs::read(&path)
                        .ok()
                        .is_some_and(|bytes| bytes == file.bytes);
                    if !matches {
                        write_materialized_file(&dir.join(&file.logical_path), &file.bytes)?;
                    }
                }
                result.push((skill_id, source, manifest, dir.display().to_string()));
            }
        }
    }
    result.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(result)
}

pub fn active_writing_skill_dir(paths: &MyOpenPanelsPaths, skill_id: &str) -> Option<PathBuf> {
    active_writing_skill_sources(paths)
        .ok()?
        .into_iter()
        .find(|value| value.0 == skill_id)
        .map(|value| PathBuf::from(value.3))
}

pub fn writing_skill_project_id(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
) -> Result<Option<String>, CliError> {
    for project_id in Storage::open(paths)?
        .list_projects()?
        .into_iter()
        .map(|project| project.id)
    {
        if read_active_pointer(paths, &project_id, ResourceKind::WritingSkill, skill_id)?.is_some()
        {
            return Ok(Some(project_id));
        }
    }
    Ok(None)
}

pub fn archive_resource(
    paths: &MyOpenPanelsPaths,
    project_id: Option<&str>,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<(), CliError> {
    let projects = if let Some(project_id) = project_id {
        vec![project_id.to_owned()]
    } else {
        Storage::open(paths)?
            .list_projects()?
            .into_iter()
            .map(|project| project.id)
            .collect()
    };
    for project_id in projects {
        let pointer_path = resource_dir(paths, &project_id, kind, resource_key).join("active.json");
        if pointer_path.is_file() {
            let mut pointer: ActivePointer = read_json(&pointer_path)?;
            pointer.archived = true;
            write_json_atomic(&pointer_path, &pointer)?;
        }
    }
    Ok(())
}

pub fn start_gc_loop(paths: MyOpenPanelsPaths) {
    if cfg!(test) {
        return;
    }
    std::thread::spawn(move || loop {
        let _ = gc_content(&paths);
        std::thread::sleep(std::time::Duration::from_secs(60 * 60));
    });
}

pub fn gc_content(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let storage = Storage::open(paths)?;
    let mut statement = storage
        .connection()
        .prepare("SELECT id, project_id, execution_generation FROM tasks WHERE status = 'running'")
        .map_err(to_cli_error)?;
    let active = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    let mut removed = 0;
    for project in read_dirs(&paths.storage_dir.join("projects"))? {
        let staging = project.join("content/.staging");
        for task_dir in read_dirs(&staging)? {
            let task_id = task_dir.file_name().unwrap_or_default().to_string_lossy();
            let keep = active
                .iter()
                .any(|(id, _, _)| sanitize_path_part(id) == task_id);
            if !keep {
                fs::remove_dir_all(task_dir).map_err(to_cli_error)?;
                removed += 1;
            }
        }
    }
    Ok(json!({ "removedStagingDirectories": removed, "prunedRevisions": 0 }))
}

pub(crate) fn pinned_task_input_text(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
) -> Result<Option<String>, CliError> {
    let (project_id, input) = task_input_from_storage(paths, task_id)?;
    let Some(snapshot) =
        resource_snapshot_for_task(paths, &project_id, kind, resource_key, &input)?
    else {
        return Ok(None);
    };
    snapshot
        .files
        .into_iter()
        .find(|file| file.logical_path == logical_path)
        .map(|file| {
            String::from_utf8(file.bytes)
                .map_err(|_| CliError::with_code("invalid_content", "Stored content is not UTF-8."))
        })
        .transpose()
}

pub(crate) fn pinned_task_input_paths(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<Vec<String>, CliError> {
    let (project_id, input) = task_input_from_storage(paths, task_id)?;
    Ok(
        resource_snapshot_for_task(paths, &project_id, kind, resource_key, &input)?
            .map(|snapshot| {
                snapshot
                    .files
                    .into_iter()
                    .map(|file| file.logical_path)
                    .collect()
            })
            .unwrap_or_default(),
    )
}

pub(crate) fn task_wiki_base_paths(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    wiki_space_id: &str,
) -> Result<Vec<String>, CliError> {
    pinned_task_input_paths(paths, task_id, ResourceKind::WikiSpace, wiki_space_id)
}
