fn studio_owner_lock_path(paths: &MyOpenPanelsPaths) -> PathBuf {
    paths.studio_dir.join("owner.lock")
}

fn studio_owner_identity_path(paths: &MyOpenPanelsPaths) -> PathBuf {
    paths.studio_dir.join("owner.json")
}

fn studio_owner_guard_lock_path(paths: &MyOpenPanelsPaths) -> PathBuf {
    studio_owner_coordination_dir(paths).join("owner.lock")
}

fn studio_owner_coordination_dir(paths: &MyOpenPanelsPaths) -> PathBuf {
    let storage_key = paths.storage_dir.to_string_lossy();
    let digest = format!("{:x}", Sha256::digest(storage_key.as_bytes()));
    std::env::temp_dir()
        .join("myopenpanels-studio-owners")
        .join(digest)
}

fn write_studio_owner_identity(
    paths: &MyOpenPanelsPaths,
    owner: &StudioOwner,
) -> Result<(), CliError> {
    let mut file = tempfile::NamedTempFile::new_in(&paths.studio_dir).map_err(to_cli_error)?;
    file.write_all(
        format!(
            "{}\n",
            serde_json::to_string_pretty(owner).map_err(to_cli_error)?
        )
        .as_bytes(),
    )
    .map_err(to_cli_error)?;
    file.persist(studio_owner_identity_path(paths))
        .map(|_| ())
        .map_err(to_cli_error)
}

fn studio_owner_state(paths: &MyOpenPanelsPaths) -> Result<StudioOwnerState, CliError> {
    fs::create_dir_all(&paths.studio_dir).map_err(to_cli_error)?;
    let Some(guard_file) = try_acquire_studio_guard_file(paths)? else {
        return Ok(StudioOwnerState::Held(read_studio_owner(paths)));
    };
    if let Some(file) = try_acquire_studio_owner_file(paths)? {
        drop(file);
        drop(guard_file);
        return Ok(StudioOwnerState::Available);
    }
    drop(guard_file);
    Ok(StudioOwnerState::Held(read_studio_owner(paths)))
}

fn read_studio_owner(paths: &MyOpenPanelsPaths) -> Option<StudioOwner> {
    fs::read_to_string(studio_owner_identity_path(paths))
        .ok()
        .and_then(|raw| serde_json::from_str::<StudioOwner>(&raw).ok())
}

fn try_acquire_studio_owner_file(
    paths: &MyOpenPanelsPaths,
) -> Result<Option<fs::File>, CliError> {
    try_acquire_studio_lock_file(&studio_owner_lock_path(paths))
}

fn try_acquire_studio_guard_file(
    paths: &MyOpenPanelsPaths,
) -> Result<Option<fs::File>, CliError> {
    try_acquire_studio_lock_file(&studio_owner_guard_lock_path(paths))
}

#[cfg(unix)]
fn try_acquire_studio_lock_file(path: &Path) -> Result<Option<fs::File>, CliError> {
    use std::os::fd::AsRawFd;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)
        .map_err(to_cli_error)?;
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result == 0 {
        return Ok(Some(file));
    }
    let error = std::io::Error::last_os_error();
    if matches!(error.raw_os_error(), Some(code) if code == libc::EAGAIN || code == libc::EACCES) {
        return Ok(None);
    }
    Err(to_cli_error(error))
}

#[cfg(windows)]
fn try_acquire_studio_lock_file(path: &Path) -> Result<Option<fs::File>, CliError> {
    use std::os::windows::fs::OpenOptionsExt;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    match fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path)
    {
        Ok(file) => Ok(Some(file)),
        Err(error) if matches!(error.raw_os_error(), Some(code) if code == 32 || code == 33) => {
            Ok(None)
        }
        Err(error) => Err(to_cli_error(error)),
    }
}

#[cfg(not(any(unix, windows)))]
fn try_acquire_studio_lock_file(path: &Path) -> Result<Option<fs::File>, CliError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    match fs::OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(path)
    {
        Ok(file) => Ok(Some(file)),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
        Err(error) => Err(to_cli_error(error)),
    }
}
