use super::*;

pub fn task_has_staged_resource(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    kind: ResourceKind,
) -> Result<bool, CliError> {
    let Some(root) = staging_root_for_task(paths, task_id)? else {
        return Ok(false);
    };
    Ok(root.join(kind.as_str()).is_dir())
}

pub fn staged_files_for_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    kind: ResourceKind,
) -> Result<Vec<(String, String, Vec<u8>, Value)>, CliError> {
    let Some(root) = staging_root_for_task(paths, task_id)? else {
        return Ok(Vec::new());
    };
    let kind_dir = root.join(kind.as_str());
    if !kind_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for resource_dir in read_dirs(&kind_dir)? {
        let metadata: StagedResource = read_json(&resource_dir.join("resource.json"))?;
        for file in read_staged_files(&resource_dir)? {
            result.push((
                metadata.resource_key.clone(),
                file.logical_path.clone(),
                fs::read(staged_file_path(&resource_dir, &file)?).map_err(to_cli_error)?,
                metadata.metadata.clone(),
            ));
        }
    }
    result.sort_by(|left, right| (&left.0, &left.1).cmp(&(&right.0, &right.1)));
    Ok(result)
}

pub(crate) fn prepare_task_staging_in_transaction(
    paths: &MyOpenPanelsPaths,
    tx: &Transaction<'_>,
    task_id: &str,
    _now: &str,
    allow_empty: bool,
) -> Result<PreparedTaskContent, CliError> {
    let (project_id, generation, handler_key) = tx
        .query_row(
            "SELECT project_id, execution_generation, handler_key FROM tasks WHERE id = ? AND status = 'running'",
            [task_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?)),
        )
        .map_err(to_cli_error)?;
    let root = staging_root(paths, &project_id, task_id, generation);
    if !root.is_dir() {
        let route = crate::capabilities::task_route_for_handler(&handler_key)?;
        if allow_empty || route.is_none() {
            return Ok(PreparedTaskContent {
                commits: Vec::new(),
                activations: Vec::new(),
                staging_root: None,
            });
        }
        return Err(CliError::with_code(
            "invalid_output",
            "Task completed without staged content.",
        ));
    }
    let mut staged_resources = Vec::new();
    for kind_dir in read_dirs(&root)? {
        for resource_dir in read_dirs(&kind_dir)? {
            let staged: StagedResource = read_json(&resource_dir.join("resource.json"))?;
            let kind = ResourceKind::parse(&staged.resource_kind)?;
            let current = if kind == ResourceKind::WritingSkill {
                read_active_pointer(paths, &project_id, kind, &staged.resource_key)?
            } else {
                read_authoritative_pointer_from_connection(
                    tx,
                    &project_id,
                    kind,
                    &staged.resource_key,
                )?
            };
            if current.as_ref().map(|value| value.revision_id.as_str())
                != staged.base_revision_id.as_deref()
                || current.as_ref().map_or(0, |value| value.content_version)
                    != staged.base_content_version
            {
                return Err(CliError::with_code(
                    "content_conflict",
                    format!(
                        "Content changed while the Task was running: {}",
                        staged.resource_key
                    ),
                ));
            }
            staged_resources.push((staged, resource_dir));
        }
    }
    let mut commits = Vec::new();
    let mut activations = Vec::new();
    for (staged, resource_dir) in staged_resources {
        let kind = ResourceKind::parse(&staged.resource_kind)?;
        let (active_path, revision) = prepare_staged_resource(paths, &staged, &resource_dir, None)?;
        commits.push(ContentCommit {
            resource_kind: kind.as_str().to_owned(),
            resource_key: staged.resource_key,
            revision_id: revision.revision_id.clone(),
            content_version: revision.content_version,
            manifest_hash: revision.manifest_hash.clone(),
            content_hash: revision.content_hash.clone(),
        });
        activations.push(PreparedActivation {
            active_path,
            pointer: revision,
        });
    }
    tx.execute(
        "UPDATE tasks SET execution_token_hash = NULL WHERE id = ?",
        [task_id],
    )
    .map_err(to_cli_error)?;
    Ok(PreparedTaskContent {
        commits,
        activations,
        staging_root: Some(root),
    })
}

pub(crate) fn publish_prepared_task_content(
    _paths: &MyOpenPanelsPaths,
    prepared: PreparedTaskContent,
) -> Result<(), CliError> {
    for activation in &prepared.activations {
        write_json_atomic(&activation.active_path, &activation.pointer)?;
    }
    if let Some(root) = prepared.staging_root {
        let _ = fs::remove_dir_all(root);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_direct_text_content(
    paths: &MyOpenPanelsPaths,
    operation_id: &str,
    project_id: &str,
    panel_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
    bytes: &[u8],
    mime_type: &str,
    base_content_version: u64,
) -> Result<PreparedDirectContent, CliError> {
    validate_logical_path(logical_path)?;
    if bytes.len() > MAX_TEXT_FILE_BYTES || std::str::from_utf8(bytes).is_err() {
        return Err(CliError::with_code(
            "invalid_my_document",
            "My Document content must be bounded UTF-8 text.",
        ));
    }
    let current = read_authoritative_pointer(paths, project_id, kind, resource_key)?;
    let current_version = current.as_ref().map_or(0, |value| value.content_version);
    if current_version != base_content_version as i64 {
        return Err(CliError::with_code(
            "content_conflict",
            format!("Content changed from version {base_content_version} to {current_version}."),
        ));
    }
    let staging_root = paths
        .storage_dir
        .join("operations")
        .join(sanitize_path_part(operation_id))
        .join("content-staging");
    if staging_root.exists() {
        fs::remove_dir_all(&staging_root).map_err(to_cli_error)?;
    }
    write_staged_file(&staging_root, logical_path, bytes, mime_type, json!({}))?;
    let staged = StagedResource {
        project_id: project_id.to_owned(),
        panel_id: panel_id.to_owned(),
        resource_kind: kind.as_str().to_owned(),
        resource_key: resource_key.to_owned(),
        base_revision_id: current.as_ref().map(|value| value.revision_id.clone()),
        base_content_version: current_version,
        metadata: json!({ "replaceAll": true }),
    };
    let (active_path, pointer) = prepare_staged_resource(paths, &staged, &staging_root, None)?;
    let commit = ContentCommit {
        resource_kind: kind.as_str().to_owned(),
        resource_key: resource_key.to_owned(),
        revision_id: pointer.revision_id.clone(),
        content_version: pointer.content_version,
        manifest_hash: pointer.manifest_hash.clone(),
        content_hash: pointer.content_hash.clone(),
    };
    Ok(PreparedDirectContent {
        commit,
        activation: PreparedActivation {
            active_path,
            pointer,
        },
        staging_root,
    })
}

pub(crate) fn publish_prepared_direct_content(
    prepared: PreparedDirectContent,
) -> Result<(), CliError> {
    write_json_atomic(
        &prepared.activation.active_path,
        &prepared.activation.pointer,
    )?;
    let _ = fs::remove_dir_all(prepared.staging_root);
    Ok(())
}
