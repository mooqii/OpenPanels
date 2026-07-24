const MAX_SKILL_IMPORT_ARCHIVE_BYTES: usize = 20 * 1024 * 1024;
const MAX_SKILL_NAME_BYTES: usize = 64;
const MAX_SKILL_DESCRIPTION_BYTES: usize = 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillImportFile {
    pub path: String,
    pub content_base64: String,
}

#[derive(Debug, Clone)]
struct GithubSkillSource {
    owner: String,
    repo: String,
    revision: String,
    subpath: Option<String>,
}

#[derive(Debug, Clone)]
struct RemoteSkillSource {
    archive_url: String,
    label: &'static str,
    skill_selector: Option<String>,
    subpath: Option<String>,
    provenance: SkillProvenanceSource,
}

#[derive(Debug)]
struct PreparedSkillPackage {
    _temporary: Option<tempfile::TempDir>,
    root: PathBuf,
    name: String,
    description: String,
}

#[derive(Debug)]
struct PreparedRemoteArchive {
    _temporary: tempfile::TempDir,
    archive_root: PathBuf,
    search_root: PathBuf,
    source: RemoteSkillSource,
}

#[derive(Debug)]
struct RemoteSkillCandidate {
    root: PathBuf,
    name: String,
    description: String,
    subpath: String,
}

#[derive(Clone, Copy)]
enum SkillZipSymlinkPolicy {
    Reject,
    Ignore,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlSkillImportSelection {
    pub subpath: String,
    #[serde(default)]
    pub module_kind: Option<String>,
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
        None,
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
    extract_skill_zip(archive, temporary.path(), SkillZipSymlinkPolicy::Reject)?;
    install_imported_skill(
        paths,
        temporary.path(),
        None,
        module_kind,
        replace_existing,
        "local-zip",
        None,
    )
}

pub fn import_skill_from_url(
    paths: &MyOpenPanelsPaths,
    source_url: &str,
    module_kind: &str,
    replace_existing: bool,
) -> Result<Value, CliError> {
    validate_custom_module(module_kind)?;
    let (source, package) = prepare_remote_skill(source_url)?;
    let origin = source.provenance.source_locator.clone();
    let module_kinds = vec![module_kind.to_owned()];
    install_skill_package(
        paths,
        &package.root,
        package.name,
        package.description,
        &module_kinds,
        replace_existing,
        &origin,
        Some(source.provenance),
    )
}

pub fn scan_skills_from_url(source_url: &str) -> Result<Value, CliError> {
    let prepared = prepare_remote_archive(source_url)?;
    let candidates = discover_remote_skill_candidates(
        &prepared.archive_root,
        &prepared.search_root,
        prepared.source.skill_selector.as_deref(),
    )?;
    Ok(json!({
        "sourceUrl": prepared.source.provenance.source_locator,
        "skills": candidates
            .into_iter()
            .map(|candidate| json!({
                "name": candidate.name,
                "description": candidate.description,
                "subpath": candidate.subpath,
            }))
            .collect::<Vec<_>>(),
    }))
}

pub fn import_skills_from_url(
    paths: &MyOpenPanelsPaths,
    source_url: &str,
    selections: &[UrlSkillImportSelection],
    replace_existing: bool,
) -> Result<Value, CliError> {
    if selections.is_empty() {
        return Err(invalid_skill_import("Scan and select at least one Skill to install."));
    }
    let prepared = prepare_remote_archive(source_url)?;
    let candidates = discover_remote_skill_candidates(
        &prepared.archive_root,
        &prepared.search_root,
        prepared.source.skill_selector.as_deref(),
    )?;
    let mut candidates_by_subpath = candidates
        .into_iter()
        .map(|candidate| (candidate.subpath.clone(), candidate))
        .collect::<BTreeMap<_, _>>();
    let mut selected = Vec::with_capacity(selections.len());
    let mut selected_subpaths = BTreeSet::new();
    for selection in selections {
        if !selected_subpaths.insert(selection.subpath.clone()) {
            return Err(invalid_skill_import(
                "The same scanned Skill cannot be installed more than once.",
            ));
        }
        let candidate = candidates_by_subpath
            .remove(&selection.subpath)
            .ok_or_else(|| {
                invalid_skill_import(
                    "The scanned Skill list changed. Scan the URL again before installing.",
                )
            })?;
        let module_kinds = selection
            .module_kind
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(|value| {
                validate_custom_module(value)?;
                Ok(vec![value.to_owned()])
            })
            .transpose()?
            .ok_or_else(|| {
                CliError::with_code(
                    "skill_module_required",
                    "Choose a Skill module before installation.",
                )
            })?;
        selected.push((candidate, module_kinds));
    }

    sync_builtin_agent_skills(paths)?;
    sync_task_created_agent_skills(paths)?;
    for (candidate, _) in &selected {
        let name_key = normalized_device_skill_name(&candidate.name);
        let Some(listing) = find_installed_skill_by_identity(paths, &name_key)? else {
            continue;
        };
        if managed_skill_kind(&listing)? != "custom" {
            return Err(CliError::with_code(
                "skill_reserved_name",
                format!(
                    "Installation failed because '{}' has the same name as a built-in Skill. Rename the Skill and try again.",
                    candidate.name
                ),
            ));
        }
        if !replace_existing {
            return Ok(json!({
                "status": "conflict",
                "message": format!("A self-built Skill named '{}' is already installed.", candidate.name),
                "incomingSkill": {
                    "name": candidate.name,
                    "description": candidate.description,
                },
            }));
        }
    }

    let mut installed = Vec::with_capacity(selected.len());
    for (candidate, module_kinds) in selected {
        let mut provenance = prepared.source.provenance.clone();
        provenance.subpath = (!candidate.subpath.is_empty()).then_some(candidate.subpath);
        let origin = provenance.source_locator.clone();
        let result = install_skill_package(
            paths,
            &candidate.root,
            candidate.name,
            candidate.description,
            &module_kinds,
            replace_existing,
            &origin,
            Some(provenance),
        )?;
        installed.push(result["skill"].clone());
    }
    Ok(json!({
        "status": "installed",
        "skills": installed,
    }))
}

fn prepare_remote_skill(
    source_url: &str,
) -> Result<(RemoteSkillSource, PreparedSkillPackage), CliError> {
    let prepared = prepare_remote_archive(source_url)?;
    let mut candidates = discover_remote_skill_candidates(
        &prepared.archive_root,
        &prepared.search_root,
        prepared.source.skill_selector.as_deref(),
    )?;
    if candidates.len() > 1 {
        return Err(CliError::with_code(
            "skill_source_ambiguous",
            "More than one Skill was found. Scan the URL and choose the Skills to install.",
        ));
    }
    let candidate = candidates.remove(0);
    let mut source = prepared.source;
    if source.skill_selector.is_none() {
        source.provenance.subpath = (!candidate.subpath.is_empty()).then_some(candidate.subpath);
    }
    Ok((
        source,
        PreparedSkillPackage {
            _temporary: Some(prepared._temporary),
            root: candidate.root,
            name: candidate.name,
            description: candidate.description,
        },
    ))
}

fn prepare_remote_archive(source_url: &str) -> Result<PreparedRemoteArchive, CliError> {
    let source = parse_remote_skill_source(source_url)?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(10))
        .timeout_read(std::time::Duration::from_secs(30))
        .build();
    let response = agent
        .get(&source.archive_url)
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
    extract_skill_zip(&archive, temporary.path(), SkillZipSymlinkPolicy::Ignore)?;
    let archive_root = resolve_import_archive_root(temporary.path())?;
    let search_root = resolve_import_subpath(&archive_root, source.subpath.as_deref())?;
    Ok(PreparedRemoteArchive {
        _temporary: temporary,
        archive_root,
        search_root,
        source,
    })
}

fn install_imported_skill(
    paths: &MyOpenPanelsPaths,
    extracted_root: &Path,
    requested_subpath: Option<&str>,
    module_kind: &str,
    replace_existing: bool,
    origin: &str,
    provenance_source: Option<SkillProvenanceSource>,
) -> Result<Value, CliError> {
    sync_builtin_agent_skills(paths)?;
    sync_task_created_agent_skills(paths)?;
    let search_root = resolve_import_search_root(extracted_root, requested_subpath)?;
    let package_root = find_single_imported_skill(&search_root)?;
    let (name, description) = validate_imported_skill(&package_root)?;
    let module_kinds = vec![module_kind.to_owned()];
    install_skill_package(
        paths,
        &package_root,
        name,
        description,
        &module_kinds,
        replace_existing,
        origin,
        provenance_source,
    )
}

fn install_skill_package(
    paths: &MyOpenPanelsPaths,
    package_root: &Path,
    name: String,
    description: String,
    module_kinds: &[String],
    replace_existing: bool,
    origin: &str,
    provenance_source: Option<SkillProvenanceSource>,
) -> Result<Value, CliError> {
    for module_kind in module_kinds {
        validate_custom_module(module_kind)?;
    }
    sync_builtin_agent_skills(paths)?;
    sync_task_created_agent_skills(paths)?;
    let name_key = normalized_device_skill_name(&name);
    let content_hash = skill_package_hash(package_root)?;
    let provenance = provenance_source.map(|source| source.with_content_hash(&content_hash));

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
        for module_kind in module_kinds {
            if !modules.iter().any(|value| value == module_kind) {
                modules.push(module_kind.clone());
            }
        }
        let previous_manifest = read_skill_manifest(&listing)?;
        let mut manifest = imported_skill_manifest(
            &listing.skill.id,
            &name,
            modules,
            origin,
            provenance.as_ref(),
        );
        if let Some(created_at) = previous_manifest.get("createdAt") {
            manifest["createdAt"] = created_at.clone();
        }
        manifest["updatedAt"] = json!(crate::control::now_iso());
        atomic_replace_skill_package(&package_root, Path::new(&listing.local_dir), &manifest)?;
        clear_ignored_device_mismatches(paths, &listing.skill.name)?;
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
    let manifest = imported_skill_manifest(
        &skill_id,
        &name,
        module_kinds.to_vec(),
        origin,
        provenance.as_ref(),
    );
    atomic_install_skill_package(&package_root, &target, &manifest)?;
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
    provenance: Option<&SkillProvenance>,
) -> Value {
    let mut manifest = json!({
        "source": "custom",
        "skillId": skill_id,
        "name": name,
        "binding": { "moduleKinds": modules },
        "createdAt": crate::control::now_iso(),
        "origin": origin,
    });
    if let Some(provenance) = provenance {
        manifest["provenance"] = serde_json::to_value(provenance).unwrap_or(Value::Null);
    }
    manifest
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

fn extract_skill_zip(
    archive: &[u8],
    target: &Path,
    symlink_policy: SkillZipSymlinkPolicy,
) -> Result<(), CliError> {
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
            match symlink_policy {
                SkillZipSymlinkPolicy::Reject => {
                    return Err(invalid_skill_import(
                        "Symbolic links are not allowed in uploaded Skills.",
                    ));
                }
                SkillZipSymlinkPolicy::Ignore => continue,
            }
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
    let archive_root = resolve_import_archive_root(root)?;
    resolve_import_subpath(&archive_root, subpath)
}

fn resolve_import_archive_root(root: &Path) -> Result<PathBuf, CliError> {
    let mut entries = fs::read_dir(root)
        .map_err(to_cli_error)?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    Ok(if entries.len() == 1 && entries[0].path().is_dir() {
        entries.remove(0).path()
    } else {
        root.to_path_buf()
    })
}

fn resolve_import_subpath(
    archive_root: &Path,
    subpath: Option<&str>,
) -> Result<PathBuf, CliError> {
    let Some(subpath) = subpath else {
        return Ok(archive_root.to_path_buf());
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
    find_imported_skill(search_root, None)
}

fn find_imported_skill(
    search_root: &Path,
    skill_selector: Option<&str>,
) -> Result<PathBuf, CliError> {
    let mut roots = Vec::new();
    find_imported_skill_roots(search_root, 0, &mut roots)?;
    if let Some(selector) = skill_selector {
        roots.retain(|root| imported_skill_matches_selector(root, selector));
    }
    match roots.len() {
        0 if skill_selector.is_some() => Err(invalid_skill_import(
            "The selected Skill was not found in its source repository.",
        )),
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

fn discover_remote_skill_candidates(
    archive_root: &Path,
    search_root: &Path,
    skill_selector: Option<&str>,
) -> Result<Vec<RemoteSkillCandidate>, CliError> {
    let mut roots = Vec::new();
    find_imported_skill_roots(search_root, 0, &mut roots)?;
    if let Some(selector) = skill_selector {
        roots.retain(|root| imported_skill_matches_selector(root, selector));
    }
    if roots.is_empty() {
        return Err(invalid_skill_import(if skill_selector.is_some() {
            "The selected Skill was not found in its source repository."
        } else {
            "No valid Skill was found. A Skill folder must contain SKILL.md."
        }));
    }
    roots.sort();
    let mut names = BTreeSet::new();
    let mut candidates = Vec::with_capacity(roots.len());
    let mut first_invalid = None;
    for root in roots {
        let (name, description) = match validate_imported_skill(&root) {
            Ok(metadata) => metadata,
            Err(error) if error.code() == Some("invalid_skill_package") => {
                first_invalid.get_or_insert(error);
                continue;
            }
            Err(error) => return Err(error),
        };
        if !names.insert(normalized_device_skill_name(&name)) {
            return Err(invalid_skill_import(
                "The source contains more than one Skill with the same name.",
            ));
        }
        let relative = root.strip_prefix(archive_root).map_err(to_cli_error)?;
        let subpath = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        candidates.push(RemoteSkillCandidate {
            root,
            name,
            description,
            subpath,
        });
    }
    if candidates.is_empty() {
        return Err(first_invalid.unwrap_or_else(|| {
            invalid_skill_import("No installable Skill was found in the source repository.")
        }));
    }
    Ok(candidates)
}

fn imported_skill_matches_selector(root: &Path, selector: &str) -> bool {
    if root.file_name().and_then(|value| value.to_str()) == Some(selector) {
        return true;
    }
    parse_device_skill(&root.join("SKILL.md"))
        .ok()
        .flatten()
        .is_some_and(|(name, _)| name == selector)
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
    validate_portable_custom_skill_source(&source, "SKILL.md", "import-validation")
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
            skill_selector: None,
            subpath: source.subpath.clone(),
            provenance: SkillProvenanceSource {
                source_type: "github".to_owned(),
                source_locator: normalized.to_owned(),
                revision: Some(source.revision),
                subpath: source.subpath,
            },
        });
    }
    if let Some(rest) = normalized
        .strip_prefix("https://skills.sh/")
        .or_else(|| normalized.strip_prefix("https://www.skills.sh/"))
    {
        let segments = rest.split('/').collect::<Vec<_>>();
        if matches!(segments.len(), 2 | 3)
            && valid_github_segment(segments[0])
            && valid_github_segment(segments[1])
            && !segments[0].contains('.')
            && segments
                .get(2)
                .is_none_or(|value| valid_registry_segment(value))
        {
            let selector = segments.get(2).map(|value| (*value).to_owned());
            return Ok(RemoteSkillSource {
                archive_url: format!(
                    "https://codeload.github.com/{}/{}/zip/HEAD",
                    segments[0], segments[1]
                ),
                label: "skills.sh",
                skill_selector: selector,
                subpath: None,
                provenance: SkillProvenanceSource {
                    source_type: "skills-sh".to_owned(),
                    source_locator: normalized.to_owned(),
                    revision: Some("HEAD".to_owned()),
                    subpath: None,
                },
            });
        }
        return Err(CliError::with_code(
            "unsupported_skill_source",
            "Use a skills.sh repository or Skill detail URL such as https://skills.sh/owner/repository/skill-name.",
        ));
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
                skill_selector: None,
                subpath: None,
                provenance: SkillProvenanceSource {
                    source_type: "clawhub".to_owned(),
                    source_locator: normalized.to_owned(),
                    revision: None,
                    subpath: None,
                },
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
                skill_selector: None,
                subpath: None,
                provenance: SkillProvenanceSource {
                    source_type: "skillhub".to_owned(),
                    source_locator: normalized.to_owned(),
                    revision: None,
                    subpath: None,
                },
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
        "Use a supported Skill URL: a GitHub repository or tree URL, a skills.sh repository or Skill detail URL, a ClawHub Skill detail URL, or a SkillHub Skill detail URL.",
    )
}

fn invalid_skill_import(message: impl Into<String>) -> CliError {
    CliError::with_code("invalid_skill_package", message)
}
