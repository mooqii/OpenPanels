use super::*;
use std::io::Read;

pub(crate) fn validate_logical_path(value: &str) -> Result<(), CliError> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || value.contains('\\')
        || value.chars().any(char::is_control)
        || path.is_absolute()
        || value
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."))
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

pub(super) fn staged_files_manifest(stage_dir: &Path) -> PathBuf {
    stage_dir.join("staged-files.json")
}

pub(super) fn read_staged_files(stage_dir: &Path) -> Result<Vec<StagedFile>, CliError> {
    let manifest = staged_files_manifest(stage_dir);
    if manifest.is_file() {
        return read_json(&manifest);
    }
    revision_files(&stage_dir.join("files"))?
        .into_iter()
        .filter(|(path, _)| !path.ends_with(".mopmeta"))
        .map(|(logical_path, path)| {
            let mime_type = mime_for_path(&path);
            Ok(StagedFile {
                object_name: hash_bytes(logical_path.as_bytes()),
                logical_path,
                mime_type,
                metadata: json!({}),
            })
        })
        .collect()
}

pub(super) fn staged_file_path(stage_dir: &Path, file: &StagedFile) -> Result<PathBuf, CliError> {
    if file.object_name != hash_bytes(file.logical_path.as_bytes())
        || Path::new(&file.object_name)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(CliError::with_code(
            "invalid_content_manifest",
            "Staged content object reference is invalid.",
        ));
    }
    let object_path = stage_dir.join("staged-objects").join(&file.object_name);
    if object_path.is_file() {
        Ok(object_path)
    } else {
        Ok(stage_dir
            .join("files")
            .join(logical_path_buf(&file.logical_path)?))
    }
}

pub(crate) fn write_staged_file(
    stage_dir: &Path,
    logical_path: &str,
    bytes: &[u8],
    mime_type: &str,
    metadata: Value,
) -> Result<PathBuf, CliError> {
    validate_logical_path(logical_path)?;
    let object_name = hash_bytes(logical_path.as_bytes());
    let destination = stage_dir.join("staged-objects").join(&object_name);
    write_materialized_file(&destination, bytes)?;
    let mut files = read_staged_files(stage_dir)?;
    files.retain(|file| file.logical_path != logical_path);
    files.push(StagedFile {
        logical_path: logical_path.to_owned(),
        object_name,
        mime_type: mime_type.to_owned(),
        metadata,
    });
    files.sort_by(|left, right| left.logical_path.cmp(&right.logical_path));
    write_json_atomic(&staged_files_manifest(stage_dir), &files)?;
    Ok(destination)
}

pub(crate) fn revision_object_path(
    revision_dir: &Path,
    file: &RevisionFile,
) -> Result<PathBuf, CliError> {
    if !file.object_ref.is_empty() {
        let expected = format!("objects/{}", file.content_hash);
        if file.object_ref != expected {
            return Err(CliError::with_code(
                "invalid_content_manifest",
                "Content object reference does not match its content hash.",
            ));
        }
        let path = Path::new(&file.object_ref);
        if path.is_absolute()
            || path
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(CliError::with_code(
                "invalid_content_manifest",
                "Content object reference is invalid.",
            ));
        }
        return Ok(revision_dir.join(path));
    }
    Ok(revision_dir
        .join("files")
        .join(logical_path_buf(&file.logical_path)?))
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

pub(crate) fn hash_file(path: &Path) -> Result<String, CliError> {
    let mut file = fs::File::open(path).map_err(to_cli_error)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(to_cli_error)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
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
