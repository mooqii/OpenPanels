const MAX_SKILL_IMPORT_ARCHIVE_BYTES: usize = 20 * 1024 * 1024;
const MAX_SKILL_NAME_BYTES: usize = 64;
const MAX_SKILL_DESCRIPTION_BYTES: usize = 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillImportFile {
    pub path: String,
    pub content_base64: String,
}

#[derive(Debug)]
struct GithubSkillSource {
    owner: String,
    repo: String,
    revision: String,
    subpath: Option<String>,
}

#[derive(Debug)]
struct RemoteSkillSource {
    archive_url: String,
    label: &'static str,
    subpath: Option<String>,
}

pub fn import_skill_from_files(
    paths: &MyOpenPanelsPaths,
    files: &[SkillImportFile],
    module_kind: &str,
    replace_existing: bool,
) -> Result<Value, CliError> {
    validate_custom_module(module_kind)?;
    if files.is_empty() {
        return Err(invalid_skill_import("Choose a Skill folder to install."));
    }
    let temporary = tempfile::tempdir().map_err(to_cli_error)?;
    materialize_import_files(temporary.path(), files)?;
    install_imported_skill(
        paths,
        temporary.path(),
        None,
        module_kind,
        replace_existing,
        "local-folder",
    )
}

pub fn import_skill_from_zip(
    paths: &MyOpenPanelsPaths,
    archive: &[u8],
    module_kind: &str,
    replace_existing: bool,
) -> Result<Value, CliError> {
    validate_custom_module(module_kind)?;
    if archive.is_empty() || archive.len() > MAX_SKILL_IMPORT_ARCHIVE_BYTES {
        return Err(CliError::with_code(
            "skill_package_too_large",
            "The Skill zip is empty or exceeds the 20 MB upload limit.",
        ));
    }
    let temporary = tempfile::tempdir().map_err(to_cli_error)?;
    extract_skill_zip(archive, temporary.path())?;
    install_imported_skill(
        paths,
        temporary.path(),
        None,
        module_kind,
        replace_existing,
        "local-zip",
    )
}

pub fn import_skill_from_url(
    paths: &MyOpenPanelsPaths,
    source_url: &str,
    module_kind: &str,
    replace_existing: bool,
) -> Result<Value, CliError> {
    validate_custom_module(module_kind)?;
    let source = parse_remote_skill_source(source_url)?;
    let response = ureq::get(&source.archive_url)
        .set("User-Agent", "MyOpenPanels-Skill-Importer")
        .call()
        .map_err(|error| {
            CliError::with_code(
                "skill_source_unavailable",
                format!("Could not download the {} Skill: {error}", source.label),
            )
        })?;
    if response
        .header("Content-Length")
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|size| size > MAX_SKILL_IMPORT_ARCHIVE_BYTES)
    {
        return Err(CliError::with_code(
            "skill_package_too_large",
            format!(
                "The {} Skill archive exceeds the 20 MB download limit.",
                source.label
            ),
        ));
    }
    use std::io::Read;
    let mut archive = Vec::new();
    response
        .into_reader()
        .take((MAX_SKILL_IMPORT_ARCHIVE_BYTES + 1) as u64)
        .read_to_end(&mut archive)
        .map_err(to_cli_error)?;
    if archive.len() > MAX_SKILL_IMPORT_ARCHIVE_BYTES {
        return Err(CliError::with_code(
            "skill_package_too_large",
            format!(
                "The {} Skill archive exceeds the 20 MB download limit.",
                source.label
            ),
        ));
    }
    let temporary = tempfile::tempdir().map_err(to_cli_error)?;
    extract_skill_zip(&archive, temporary.path())?;
    install_imported_skill(
        paths,
        temporary.path(),
        source.subpath.as_deref(),
        module_kind,
        replace_existing,
        source_url,
    )
}

fn install_imported_skill(
    paths: &MyOpenPanelsPaths,
    extracted_root: &Path,
    requested_subpath: Option<&str>,
    module_kind: &str,
    replace_existing: bool,
    origin: &str,
) -> Result<Value, CliError> {
    sync_builtin_agent_skills(paths)?;
    migrate_legacy_custom_agent_skills(paths)?;
    let search_root = resolve_import_search_root(extracted_root, requested_subpath)?;
    let package_root = find_single_imported_skill(&search_root)?;
    let (name, description) = validate_imported_skill(&package_root)?;
    let name_key = normalized_device_skill_name(&name);

    if let Some(listing) = find_installed_skill_by_identity(paths, &name_key)? {
        let kind = managed_skill_kind(&listing)?;
        if kind != "custom" {
            return Err(CliError::with_code(
                "skill_reserved_name",
                format!(
                    "Installation failed because '{name}' has the same name as a built-in Skill. Rename the Skill and try again."
                ),
            ));
        }
        if !replace_existing {
            return Ok(json!({
                "status": "conflict",
                "message": format!("A self-built Skill named '{name}' is already installed."),
                "existingSkill": managed_skill_from_listing(
                    &listing,
                    kind,
                    managed_skill_module_kinds(&listing),
                ),
                "incomingSkill": { "name": name, "description": description },
            }));
        }
        let mut modules = custom_skill_modules(&listing)?;
        if !modules.iter().any(|value| value == module_kind) {
            modules.push(module_kind.to_owned());
        }
        let manifest = imported_skill_manifest(&listing.skill.id, &name, modules, origin);
        atomic_replace_device_skill(&package_root, Path::new(&listing.local_dir), &manifest)?;
        let replaced = managed_skill_listing(paths, &listing.skill.id)?;
        return Ok(json!({
            "status": "installed",
            "replaced": true,
            "skill": managed_skill_from_listing(
                &replaced,
                "custom",
                managed_skill_module_kinds(&replaced),
            ),
        }));
    }

    use sha2::Digest;
    let skill_id = format!(
        "custom-{}",
        &format!("{:x}", sha2::Sha256::digest(name_key.as_bytes()))[..16]
    );
    let target = paths
        .storage_dir
        .join("skills")
        .join(crate::paths::sanitize_path_part(&skill_id));
    if target.exists() {
        return Err(CliError::with_code(
            "skill_name_conflict",
            format!("The Skill install target already exists: {skill_id}"),
        ));
    }
    let manifest = imported_skill_manifest(&skill_id, &name, vec![module_kind.to_owned()], origin);
    atomic_copy_device_skill(&package_root, &target, &manifest)?;
    let installed = managed_skill_listing(paths, &skill_id)?;
    Ok(json!({
        "status": "installed",
        "replaced": false,
        "skill": managed_skill_from_listing(
            &installed,
            "custom",
            managed_skill_module_kinds(&installed),
        ),
    }))
}

fn imported_skill_manifest(
    skill_id: &str,
    name: &str,
    modules: Vec<String>,
    origin: &str,
) -> Value {
    json!({
        "schemaVersion": 3,
        "source": "custom",
        "skillId": skill_id,
        "name": name,
        "binding": { "moduleKinds": modules },
        "createdAt": crate::control::now_iso(),
        "origin": origin,
    })
}

fn materialize_import_files(root: &Path, files: &[SkillImportFile]) -> Result<(), CliError> {
    use base64::Engine;
    let mut paths = BTreeSet::new();
    let mut total = 0usize;
    if files.len() > 512 {
        return Err(CliError::with_code(
            "skill_package_too_large",
            "The Skill folder contains too many files.",
        ));
    }
    for file in files {
        let relative = safe_import_relative_path(&file.path)?;
        if !paths.insert(relative.clone()) {
            return Err(invalid_skill_import(
                "The Skill folder contains duplicate file paths.",
            ));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&file.content_base64)
            .map_err(|_| invalid_skill_import("A Skill file is not valid base64 data."))?;
        total = total.saturating_add(bytes.len());
        if bytes.len() > 10 * 1024 * 1024 || total > MAX_SKILL_IMPORT_ARCHIVE_BYTES {
            return Err(CliError::with_code(
                "skill_package_too_large",
                "The Skill folder exceeds the upload limit.",
            ));
        }
        let target = root.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(to_cli_error)?;
        }
        fs::write(target, bytes).map_err(to_cli_error)?;
    }
    Ok(())
}

fn extract_skill_zip(archive: &[u8], target: &Path) -> Result<(), CliError> {
    use std::io::{Cursor, Read};
    let mut zip = zip::ZipArchive::new(Cursor::new(archive))
        .map_err(|_| invalid_skill_import("The selected file is not a valid zip archive."))?;
    if zip.len() > 512 {
        return Err(CliError::with_code(
            "skill_package_too_large",
            "The Skill zip contains too many files.",
        ));
    }
    let mut total = 0u64;
    for index in 0..zip.len() {
        let entry = zip
            .by_index(index)
            .map_err(|_| invalid_skill_import("The Skill zip is damaged."))?;
        let Some(enclosed) = entry.enclosed_name() else {
            return Err(invalid_skill_import(
                "The Skill zip contains an unsafe file path.",
            ));
        };
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(invalid_skill_import(
                "Symbolic links are not allowed in uploaded Skills.",
            ));
        }
        if entry.is_dir() {
            fs::create_dir_all(target.join(enclosed)).map_err(to_cli_error)?;
            continue;
        }
        if entry.size() > 10 * 1024 * 1024 {
            return Err(CliError::with_code(
                "skill_package_too_large",
                "The extracted Skill package exceeds the size limit.",
            ));
        }
        let destination = target.join(enclosed);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(to_cli_error)?;
        }
        let mut output = fs::File::create(destination).map_err(to_cli_error)?;
        let copied = std::io::copy(&mut entry.take(10 * 1024 * 1024 + 1), &mut output)
            .map_err(to_cli_error)?;
        total = total.saturating_add(copied);
        if copied > 10 * 1024 * 1024 || total > 50 * 1024 * 1024 {
            return Err(CliError::with_code(
                "skill_package_too_large",
                "The extracted Skill package exceeds the size limit.",
            ));
        }
    }
    Ok(())
}

fn resolve_import_search_root(root: &Path, subpath: Option<&str>) -> Result<PathBuf, CliError> {
    let mut entries = fs::read_dir(root)
        .map_err(to_cli_error)?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    let archive_root = if entries.len() == 1 && entries[0].path().is_dir() {
        entries.remove(0).path()
    } else {
        root.to_path_buf()
    };
    let Some(subpath) = subpath else {
        return Ok(archive_root);
    };
    let relative = safe_import_relative_path(subpath)?;
    let requested = archive_root.join(relative);
    if !requested.is_dir() {
        return Err(invalid_skill_import(
            "The URL does not point to an available Skill directory.",
        ));
    }
    Ok(requested)
}

fn find_single_imported_skill(search_root: &Path) -> Result<PathBuf, CliError> {
    let mut roots = Vec::new();
    find_imported_skill_roots(search_root, 0, &mut roots)?;
    match roots.len() {
        0 => Err(invalid_skill_import(
            "No valid Skill was found. A Skill folder must contain SKILL.md.",
        )),
        1 => Ok(roots.remove(0)),
        _ => Err(CliError::with_code(
            "skill_source_ambiguous",
            "More than one Skill was found. Use a URL or local folder that points to one Skill.",
        )),
    }
}

fn find_imported_skill_roots(
    directory: &Path,
    depth: usize,
    roots: &mut Vec<PathBuf>,
) -> Result<(), CliError> {
    if depth > 6 {
        return Ok(());
    }
    if directory.join("SKILL.md").is_file() {
        roots.push(directory.to_path_buf());
        return Ok(());
    }
    for entry in fs::read_dir(directory).map_err(to_cli_error)? {
        let entry = entry.map_err(to_cli_error)?;
        let file_type = entry.file_type().map_err(to_cli_error)?;
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        if matches!(
            entry.file_name().to_str(),
            Some("node_modules" | ".git" | "dist" | "build" | "target" | "__pycache__")
        ) {
            continue;
        }
        find_imported_skill_roots(&entry.path(), depth + 1, roots)?;
    }
    Ok(())
}

fn validate_imported_skill(root: &Path) -> Result<(String, String), CliError> {
    let source = fs::read_to_string(root.join("SKILL.md"))
        .map_err(|_| invalid_skill_import("SKILL.md must be a valid UTF-8 text file."))?;
    let (name, description) = parse_device_skill(&root.join("SKILL.md"))?.ok_or_else(|| {
        invalid_skill_import(
            "SKILL.md must have YAML frontmatter with string name and description fields.",
        )
    })?;
    external_custom_skill_from_source(&source, "SKILL.md", "import-validation")
        .map_err(|error| invalid_skill_import(error.message()))?;
    if !valid_imported_skill_name(&name) || description.len() > MAX_SKILL_DESCRIPTION_BYTES {
        return Err(invalid_skill_import(
            "Skill name must use 1-64 lowercase letters, digits, or hyphens, start and end with a letter or digit, and include a valid description of at most 1024 bytes.",
        ));
    }
    collect_skill_package_files(root, root, 0, &mut Vec::new())?;
    Ok((name, description))
}

fn safe_import_relative_path(value: &str) -> Result<PathBuf, CliError> {
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);
    if normalized.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
        || path.components().count() > 13
        || path
            .components()
            .any(|component| component.as_os_str().to_string_lossy().as_bytes().len() > 255)
    {
        return Err(invalid_skill_import(
            "The Skill package contains an unsafe or deeply nested path.",
        ));
    }
    Ok(path.to_path_buf())
}

fn parse_github_skill_source(value: &str) -> Result<GithubSkillSource, CliError> {
    let trimmed = normalized_source_url(value);
    if trimmed.len() > 2048 {
        return Err(unsupported_skill_url());
    }
    let rest = trimmed
        .strip_prefix("https://github.com/")
        .ok_or_else(|| unsupported_skill_url())?;
    let segments = rest.split('/').collect::<Vec<_>>();
    if segments.len() < 2
        || !valid_github_segment(segments[0])
        || !valid_github_segment(segments[1].trim_end_matches(".git"))
    {
        return Err(unsupported_skill_url());
    }
    let owner = segments[0].to_owned();
    let repo = segments[1].trim_end_matches(".git").to_owned();
    if segments.len() == 2 {
        return Ok(GithubSkillSource {
            owner,
            repo,
            revision: "HEAD".to_owned(),
            subpath: None,
        });
    }
    if segments.len() >= 4 && segments[2] == "tree" && valid_github_segment(segments[3]) {
        let subpath = if segments.len() > 4 {
            Some(segments[4..].join("/"))
        } else {
            None
        };
        if let Some(value) = &subpath {
            safe_import_relative_path(value)?;
        }
        return Ok(GithubSkillSource {
            owner,
            repo,
            revision: segments[3].to_owned(),
            subpath,
        });
    }
    Err(unsupported_skill_url())
}

fn parse_remote_skill_source(value: &str) -> Result<RemoteSkillSource, CliError> {
    let normalized = normalized_source_url(value);
    if normalized.starts_with("https://github.com/") {
        let source = parse_github_skill_source(normalized)?;
        return Ok(RemoteSkillSource {
            archive_url: format!(
                "https://codeload.github.com/{}/{}/zip/{}",
                source.owner, source.repo, source.revision
            ),
            label: "GitHub",
            subpath: source.subpath,
        });
    }
    if let Some(rest) = normalized.strip_prefix("https://clawhub.ai/") {
        let segments = rest.split('/').collect::<Vec<_>>();
        if segments.len() == 3
            && segments[1] == "skills"
            && valid_registry_segment(segments[0])
            && valid_registry_segment(segments[2])
        {
            return Ok(RemoteSkillSource {
                archive_url: format!(
                    "https://clawhub.ai/api/v1/download?slug={}&ownerHandle={}",
                    segments[2], segments[0]
                ),
                label: "ClawHub",
                subpath: None,
            });
        }
        return Err(CliError::with_code(
            "unsupported_skill_source",
            "Use a ClawHub Skill detail URL such as https://clawhub.ai/owner/skills/skill-name. Plugin detail URLs cannot be installed as Skills.",
        ));
    }
    if let Some(rest) = normalized.strip_prefix("https://skillhub.cn/skills/") {
        if valid_registry_segment(rest) {
            return Ok(RemoteSkillSource {
                archive_url: format!("https://api.skillhub.cn/api/v1/download?slug={rest}"),
                label: "SkillHub",
                subpath: None,
            });
        }
        return Err(CliError::with_code(
            "unsupported_skill_source",
            "Use a SkillHub Skill detail URL such as https://skillhub.cn/skills/skill-name.",
        ));
    }
    if normalized == "https://hermes-ai.net/skills"
        || normalized.starts_with("https://hermes-ai.net/skills/")
    {
        return Err(CliError::with_code(
            "unsupported_skill_source",
            "hermes-ai.net currently provides a Skills guide, not downloadable Skill detail pages. Use the linked GitHub or supported marketplace detail URL instead.",
        ));
    }
    Err(unsupported_skill_url())
}

fn normalized_source_url(value: &str) -> &str {
    value
        .trim()
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('/')
}

fn valid_imported_skill_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_SKILL_NAME_BYTES
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && name
            .bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

fn valid_github_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_registry_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 120
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn unsupported_skill_url() -> CliError {
    CliError::with_code(
        "unsupported_skill_source",
        "Use a supported Skill URL: a GitHub repository or tree URL, a ClawHub Skill detail URL, or a SkillHub Skill detail URL.",
    )
}

fn invalid_skill_import(message: impl Into<String>) -> CliError {
    CliError::with_code("invalid_skill_package", message)
}
