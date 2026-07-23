pub fn active_file_path(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
) -> Result<Option<PathBuf>, CliError> {
    validate_logical_path(logical_path)?;
    let Some(active) = read_active_pointer(paths, project_id, kind, resource_key)? else {
        return Ok(None);
    };
    let path = resource_dir(paths, project_id, kind, resource_key)
        .join(active.revision_id)
        .join("files")
        .join(logical_path_buf(logical_path)?);
    Ok(path.is_file().then_some(path))
}

#[allow(clippy::too_many_arguments)]
pub fn commit_immediate_text(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: Option<&str>,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
    content: &[u8],
    mime_type: &str,
    replace_all: bool,
) -> Result<Value, CliError> {
    validate_logical_path(logical_path)?;
    if content.len() > MAX_TEXT_FILE_BYTES || std::str::from_utf8(content).is_err() {
        return Err(CliError::with_code(
            "invalid_output",
            "Content must be bounded UTF-8 text.",
        ));
    }
    commit_immediate_file(
        paths,
        project_id,
        panel_id,
        kind,
        resource_key,
        logical_path,
        content,
        mime_type,
        replace_all,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn commit_immediate_file(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: Option<&str>,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
    content: &[u8],
    mime_type: &str,
    replace_all: bool,
) -> Result<Value, CliError> {
    validate_logical_path(logical_path)?;
    if content.len() as i64 > MAX_STAGING_BYTES {
        return Err(CliError::with_code(
            "content_too_large",
            "Content exceeds the per-resource staging limit.",
        ));
    }
    let active = read_active_pointer(paths, project_id, kind, resource_key)?;
    let staging = tempfile::tempdir_in(&paths.storage_dir).map_err(to_cli_error)?;
    let staged = StagedResource {
        project_id: project_id.to_owned(),
        panel_id: panel_id.unwrap_or_default().to_owned(),
        resource_kind: kind.as_str().to_owned(),
        resource_key: resource_key.to_owned(),
        base_revision_id: active.as_ref().map(|value| value.revision_id.clone()),
        base_content_version: active.as_ref().map_or(0, |value| value.content_version),
        metadata: json!({ "replaceAll": replace_all }),
    };
    write_json_atomic(&staging.path().join("resource.json"), &staged)?;
    let file = staging
        .path()
        .join("files")
        .join(logical_path_buf(logical_path)?);
    write_materialized_file(&file, content)?;
    write_json_atomic(&staging.path().join("replace-all.json"), &replace_all)?;
    let pointer = commit_staged_resource_with_mime(
        paths,
        &staged,
        staging.path(),
        Some((logical_path, mime_type)),
    )?;
    Ok(json!({
        "revisionId": pointer.revision_id,
        "contentVersion": pointer.content_version,
        "manifestHash": pointer.manifest_hash,
    }))
}
