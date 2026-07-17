struct StoredObject {
    object_hash: String,
    size_bytes: i64,
}

fn write_object(paths: &MyOpenPanelsPaths, bytes: &[u8]) -> Result<StoredObject, CliError> {
    let object_hash = format!("{:x}", Sha256::digest(bytes));
    let relative = format!(
        "content/objects/sha256/{}/{}",
        &object_hash[..2],
        object_hash
    );
    let path = paths.storage_dir.join(&relative);
    if !path.is_file() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(to_cli_error)?;
        }
        let mut temp =
            tempfile::NamedTempFile::new_in(path.parent().unwrap()).map_err(to_cli_error)?;
        temp.write_all(bytes).map_err(to_cli_error)?;
        temp.as_file().sync_all().map_err(to_cli_error)?;
        match temp.persist_noclobber(&path) {
            Ok(_) => {}
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(to_cli_error(error.error)),
        }
    }
    let storage = Storage::open(paths)?;
    storage.connection().execute(
        "INSERT OR IGNORE INTO content_objects (hash, size_bytes, storage_ref, created_at) VALUES (?, ?, ?, ?)",
        params![object_hash, bytes.len() as i64, relative, now_iso()],
    ).map_err(to_cli_error)?;
    Ok(StoredObject {
        object_hash,
        size_bytes: bytes.len() as i64,
    })
}

fn read_object(paths: &MyOpenPanelsPaths, object_hash: &str) -> Result<Vec<u8>, CliError> {
    if object_hash.len() != 64 || !object_hash.chars().all(|value| value.is_ascii_hexdigit()) {
        return Err(CliError::with_code(
            "invalid_content",
            "Stored object hash is invalid.",
        ));
    }
    fs::read(
        paths
            .storage_dir
            .join("content/objects/sha256")
            .join(&object_hash[..2])
            .join(object_hash),
    )
    .map_err(to_cli_error)
}

fn validate_logical_path(path: &str) -> Result<(), CliError> {
    if path.is_empty() || Path::new(path).is_absolute() || path.contains('\\') {
        return Err(CliError::with_code(
            "invalid_content_path",
            "Content path must be a relative POSIX path.",
        ));
    }
    for component in Path::new(path).components() {
        match component {
            Component::Normal(value) if !value.is_empty() => {}
            _ => {
                return Err(CliError::with_code(
                    "invalid_content_path",
                    "Content path contains an unsafe component.",
                ))
            }
        }
    }
    Ok(())
}

fn is_content_capability(capability: &str) -> bool {
    matches!(
        capability,
        "wiki.convertDocument"
            | "wiki.ingestMarkdown"
            | "wiki.maintain"
            | "writing.generateDocument"
            | "writing.refineSkill"
    )
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
