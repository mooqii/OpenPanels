const MANAGED_CUSTOM_MODULES: [&str; 3] = ["wiki-update", "writing", "writing-refinement"];

pub fn install_device_skill(
    paths: &MyOpenPanelsPaths,
    location_path: &str,
    module_kind: &str,
) -> Result<Value, CliError> {
    validate_custom_module(module_kind)?;
    sync_builtin_agent_skills(paths)?;
    migrate_legacy_custom_agent_skills(paths)?;
    let (source_dir, discovered) = validated_device_skill(paths, location_path)?;
    let name_key = normalized_device_skill_name(&discovered.name);
    if let Some(listing) = find_installed_skill_by_identity(paths, &name_key)? {
        if managed_skill_kind(&listing)? != "custom" {
            return Err(CliError::with_code(
                "skill_read_only",
                "System and preset Skill associations cannot be changed.",
            ));
        }
        update_custom_skill_modules(paths, &listing, |modules| {
            if !modules.iter().any(|value| value == module_kind) {
                modules.push(module_kind.to_owned());
            }
        })?;
        return managed_skills(paths);
    }

    use sha2::Digest;
    let skill_id = format!(
        "device-{}",
        &format!("{:x}", sha2::Sha256::digest(name_key.as_bytes()))[..16]
    );
    let final_dir = paths.storage_dir.join("skills").join(crate::paths::sanitize_path_part(&skill_id));
    if final_dir.exists() {
        return Err(CliError::with_code("skill_name_conflict", format!("Skill target already exists: {skill_id}")));
    }
    let manifest = custom_manifest(&skill_id, &discovered.name, vec![module_kind.to_owned()]);
    atomic_copy_device_skill(&source_dir, &final_dir, &manifest)?;
    managed_skills(paths)
}

pub fn remove_skill_module(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
    module_kind: &str,
) -> Result<Value, CliError> {
    validate_custom_module(module_kind)?;
    let listing = managed_skill_listing(paths, skill_id)?;
    if managed_skill_kind(&listing)? != "custom" {
        return Err(CliError::with_code("skill_read_only", "System and preset Skill associations cannot be changed."));
    }
    let mut modules = custom_skill_modules(&listing)?;
    if !modules.iter().any(|value| value == module_kind) {
        return Err(CliError::with_code("skill_module_not_found", "Skill is not associated with this module."));
    }
    modules.retain(|value| value != module_kind);
    if modules.is_empty() {
        return delete_managed_skill(paths, skill_id);
    }
    write_custom_manifest(&listing, modules)?;
    clear_removed_skill_module_selections(paths, skill_id, module_kind)?;
    Ok(json!({ "deletedSkill": false, "skillId": skill_id, "moduleKind": module_kind }))
}

pub fn replace_skill_source(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
    location_path: &str,
) -> Result<Value, CliError> {
    let listing = managed_skill_listing(paths, skill_id)?;
    if managed_skill_kind(&listing)? != "custom" {
        return Err(CliError::with_code("skill_read_only", "System and preset Skills cannot be replaced."));
    }
    let (source_dir, discovered) = validated_device_skill(paths, location_path)?;
    let local_skill_path = PathBuf::from(&listing.local_dir).join("SKILL.md");
    let (local_identity, _) = parse_device_skill(&local_skill_path)?.ok_or_else(|| CliError::with_code("invalid_skill_package", "Installed Skill has invalid metadata."))?;
    if normalized_device_skill_name(&local_identity) != normalized_device_skill_name(&discovered.name) {
        return Err(CliError::with_code("skill_name_conflict", "Device Skill name does not match the installed Skill."));
    }
    let manifest: Value = serde_json::from_slice(&fs::read(PathBuf::from(&listing.local_dir).join("manifest.json")).map_err(to_cli_error)?).map_err(to_cli_error)?;
    atomic_replace_device_skill(&source_dir, Path::new(&listing.local_dir), &manifest)?;
    managed_skills(paths)
}

pub fn ignore_skill_mismatch(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
    location_path: &str,
    installed_hash: &str,
    device_hash: &str,
) -> Result<Value, CliError> {
    let listing = managed_skill_listing(paths, skill_id)?;
    if managed_skill_kind(&listing)? != "custom" {
        return Err(CliError::with_code(
            "skill_read_only",
            "System and preset Skill mismatches cannot be ignored.",
        ));
    }
    let (source_dir, discovered) = validated_device_skill(paths, location_path)?;
    let local_hash = skill_package_hash(Path::new(&listing.local_dir))?;
    let source_hash = skill_package_hash(&source_dir)?;
    if local_hash != installed_hash || source_hash != device_hash {
        return Err(CliError::with_code("skill_content_changed", "Skill content changed before the mismatch was ignored."));
    }
    let mut ignored = ignored_device_mismatches(paths)?;
    let name_key = normalized_device_skill_name(&discovered.name);
    let canonical_path = source_dir.to_string_lossy().to_string();
    ignored.retain(|item| !(item["nameKey"].as_str() == Some(&name_key) && item["path"].as_str() == Some(&canonical_path)));
    ignored.push(json!({
        "nameKey": name_key,
        "path": canonical_path,
        "installedHash": local_hash,
        "deviceHash": source_hash,
    }));
    crate::storage::Storage::open(paths)?.write_setting(
        "skill_management",
        "ignored_device_mismatches",
        &serde_json::to_string(&ignored).map_err(to_cli_error)?,
    )?;
    Ok(json!({ "ignored": true }))
}

fn find_installed_skill_by_identity(paths: &MyOpenPanelsPaths, name_key: &str) -> Result<Option<AgentSkillListing>, CliError> {
    for listing in list_agent_skills(paths)? {
        if let Some((name, _)) = parse_device_skill(&PathBuf::from(&listing.local_dir).join("SKILL.md"))? {
            if normalized_device_skill_name(&name) == name_key {
                return Ok(Some(listing));
            }
        }
    }
    Ok(None)
}

fn validate_custom_module(module_kind: &str) -> Result<(), CliError> {
    if MANAGED_CUSTOM_MODULES.contains(&module_kind) {
        Ok(())
    } else {
        Err(CliError::with_code("invalid_skill_module", format!("Unsupported Skill module: {module_kind}")))
    }
}

fn custom_skill_modules(listing: &AgentSkillListing) -> Result<Vec<String>, CliError> {
    let manifest: Value = serde_json::from_slice(&fs::read(PathBuf::from(&listing.local_dir).join("manifest.json")).map_err(to_cli_error)?).map_err(to_cli_error)?;
    if let Some(modules) = manifest.pointer("/binding/moduleKinds").and_then(Value::as_array) {
        return Ok(modules.iter().filter_map(Value::as_str).map(str::to_owned).collect());
    }
    Ok(managed_skill_module_kinds(listing))
}

fn update_custom_skill_modules(
    _paths: &MyOpenPanelsPaths,
    listing: &AgentSkillListing,
    update: impl FnOnce(&mut Vec<String>),
) -> Result<(), CliError> {
    let mut modules = custom_skill_modules(listing)?;
    update(&mut modules);
    write_custom_manifest(listing, modules)
}

fn write_custom_manifest(listing: &AgentSkillListing, modules: Vec<String>) -> Result<(), CliError> {
    let path = PathBuf::from(&listing.local_dir).join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&path).map_err(to_cli_error)?).map_err(to_cli_error)?;
    manifest["schemaVersion"] = json!(3);
    manifest["binding"] = json!({ "moduleKinds": modules });
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(&manifest).map_err(to_cli_error)?)).map_err(to_cli_error)
}

fn custom_manifest(skill_id: &str, name: &str, modules: Vec<String>) -> Value {
    json!({
        "schemaVersion": 3,
        "source": "custom",
        "skillId": skill_id,
        "name": name,
        "binding": { "moduleKinds": modules },
        "createdAt": crate::control::now_iso(),
        "origin": "device",
    })
}

fn atomic_copy_device_skill(source: &Path, target: &Path, manifest: &Value) -> Result<(), CliError> {
    let parent = target.parent().ok_or_else(|| CliError::new("Invalid Skill target path."))?;
    fs::create_dir_all(parent).map_err(to_cli_error)?;
    let staging = parent.join(format!(".skill-import-{}", crate::ids::random_base64url_96()));
    let result = (|| {
        let mut files = Vec::new();
        collect_skill_package_files(source, source, 0, &mut files)?;
        for (relative, bytes) in files {
            let destination = staging.join(&relative);
            if let Some(parent) = destination.parent() { fs::create_dir_all(parent).map_err(to_cli_error)?; }
            fs::write(destination, bytes).map_err(to_cli_error)?;
        }
        fs::write(staging.join("manifest.json"), format!("{}\n", serde_json::to_string_pretty(manifest).map_err(to_cli_error)?)).map_err(to_cli_error)?;
        fs::rename(&staging, target).map_err(to_cli_error)
    })();
    if result.is_err() { let _ = fs::remove_dir_all(&staging); }
    result
}

fn atomic_replace_device_skill(source: &Path, target: &Path, manifest: &Value) -> Result<(), CliError> {
    let parent = target.parent().ok_or_else(|| CliError::new("Invalid Skill target path."))?;
    let replacement = parent.join(format!(".skill-replace-{}", crate::ids::random_base64url_96()));
    atomic_copy_device_skill(source, &replacement, manifest)?;
    let backup = parent.join(format!(".skill-backup-{}", crate::ids::random_base64url_96()));
    fs::rename(target, &backup).map_err(to_cli_error)?;
    if let Err(error) = fs::rename(&replacement, target) {
        let _ = fs::rename(&backup, target);
        return Err(to_cli_error(error));
    }
    fs::remove_dir_all(backup).map_err(to_cli_error)
}

fn clear_removed_skill_module_selections(paths: &MyOpenPanelsPaths, skill_id: &str, module_kind: &str) -> Result<(), CliError> {
    match module_kind {
        "wiki-update" => clear_wiki_skill_selections(paths, skill_id)?,
        "writing" => clear_writing_skill_module_selections(paths, skill_id, true, false)?,
        "writing-refinement" => {
            clear_writing_skill_module_selections(paths, skill_id, false, true)?;
        }
        _ => {}
    }
    Ok(())
}
