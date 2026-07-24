use super::*;

pub(crate) fn validate_logical_path(value: &str) -> Result<(), CliError> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(CliError::with_code(
            "invalid_content_path",
            "Content path must be relative and cannot traverse directories.",
        ));
    }
    Ok(())
}

pub(crate) fn logical_path_buf(value: &str) -> Result<PathBuf, CliError> {
    validate_logical_path(value)?;
    Ok(value.split('/').fold(PathBuf::new(), |path, part| {
        path.join(sanitize_path_part(part))
    }))
}

pub(crate) fn revision_files(root: &Path) -> Result<Vec<(String, PathBuf)>, CliError> {
    fn visit(
        root: &Path,
        current: &Path,
        output: &mut Vec<(String, PathBuf)>,
    ) -> Result<(), CliError> {
        if !current.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(current).map_err(to_cli_error)? {
            let path = entry.map_err(to_cli_error)?.path();
            if path.is_dir() {
                visit(root, &path, output)?;
            } else {
                let relative = path
                    .strip_prefix(root)
                    .map_err(to_cli_error)?
                    .components()
                    .filter_map(|part| part.as_os_str().to_str())
                    .collect::<Vec<_>>()
                    .join("/");
                output.push((relative, path));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(root, root, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}

pub(crate) fn copy_tree(source: &Path, destination: &Path) -> Result<(), CliError> {
    if !source.is_dir() {
        return Ok(());
    }
    for (relative, path) in revision_files(source)? {
        if relative.ends_with(".mopmeta") {
            continue;
        }
        let target = destination.join(logical_path_buf(&relative)?);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(to_cli_error)?;
        }
        if target.exists() {
            fs::remove_file(&target).map_err(to_cli_error)?;
        }
        if fs::hard_link(&path, &target).is_err() {
            fs::copy(&path, &target).map_err(to_cli_error)?;
        }
    }
    Ok(())
}

pub(crate) fn directory_size(path: &Path) -> Result<u64, CliError> {
    revision_files(path).map(|files| {
        files
            .into_iter()
            .filter_map(|(_, path)| fs::metadata(path).ok().map(|metadata| metadata.len()))
            .sum()
    })
}

pub(crate) fn read_dirs(path: &Path) -> Result<Vec<PathBuf>, CliError> {
    if !path.is_dir() {
        return Ok(Vec::new());
    }
    let mut values = fs::read_dir(path)
        .map_err(to_cli_error)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    values.sort();
    Ok(values)
}

pub(crate) fn read_resource_key(resource_dir: &Path) -> Option<String> {
    fs::read(resource_dir.join("resource.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| {
            value
                .get("resourceKey")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
}

pub(crate) fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<(), CliError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(to_cli_error)?;
    write_materialized_file(path, &bytes)
}

pub(crate) fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, CliError> {
    serde_json::from_slice(&fs::read(path).map_err(to_cli_error)?).map_err(to_cli_error)
}

pub(crate) fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn mime_for_path(path: &Path) -> String {
    mime_guess::from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_owned()
}

pub(crate) fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
