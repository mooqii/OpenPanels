#[derive(Debug, Clone)]
struct SkillProvenanceSource {
    source_type: String,
    source_locator: String,
    revision: Option<String>,
    subpath: Option<String>,
}

impl SkillProvenanceSource {
    fn with_content_hash(self, content_hash: &str) -> SkillProvenance {
        SkillProvenance {
            source_type: self.source_type,
            source_locator: self.source_locator,
            revision: self.revision,
            subpath: self.subpath,
            installed_content_hash: content_hash.to_owned(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillProvenance {
    pub source_type: String,
    pub source_locator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,
    pub installed_content_hash: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSkillProvenance {
    pub source_type: String,
    pub source_locator: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillUpdateState {
    pub skill_id: String,
    pub status: SkillUpdateStatus,
    pub local_modified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_locator: Option<String>,
    pub checked_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkillUpdateStatus {
    Unmanaged,
    UpToDate,
    UpdateAvailable,
    SourceUnavailable,
}

pub fn skill_update_ids(paths: &MyOpenPanelsPaths) -> Result<Vec<String>, CliError> {
    sync_builtin_agent_skills(paths)?;
    sync_task_created_agent_skills(paths)?;
    let mut ids = list_agent_skills(paths)?
        .into_iter()
        .filter(|listing| listing.source == "custom")
        .map(|listing| listing.skill.id)
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    Ok(ids)
}

pub fn check_skill_update(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
) -> Result<SkillUpdateState, CliError> {
    let listing = managed_skill_listing(paths, skill_id)?;
    if managed_skill_kind(&listing)? != "custom" {
        return Ok(unmanaged_skill_update_state(skill_id));
    }
    let manifest = read_skill_manifest(&listing)?;
    let Some(provenance) = skill_provenance_from_manifest(&manifest) else {
        return Ok(unmanaged_skill_update_state(skill_id));
    };
    let local_hash = match skill_package_hash(Path::new(&listing.local_dir)) {
        Ok(hash) => hash,
        Err(error) => {
            return Ok(unavailable_skill_update_state(
                skill_id,
                &provenance,
                false,
                error.message(),
            ));
        }
    };
    let local_modified = local_hash != provenance.installed_content_hash;
    let prepared = match prepare_provenance_source(paths, &provenance) {
        Ok(prepared) => prepared,
        Err(error) => {
            return Ok(unavailable_skill_update_state(
                skill_id,
                &provenance,
                local_modified,
                error.message(),
            ));
        }
    };
    if normalized_device_skill_name(&prepared.name)
        != normalized_device_skill_name(&listing.skill.name)
    {
        return Ok(unavailable_skill_update_state(
            skill_id,
            &provenance,
            local_modified,
            "The source Skill name no longer matches the installed Skill.",
        ));
    }
    let source_hash = match skill_package_hash(&prepared.root) {
        Ok(hash) => hash,
        Err(error) => {
            return Ok(unavailable_skill_update_state(
                skill_id,
                &provenance,
                local_modified,
                error.message(),
            ));
        }
    };
    let (status, local_modified) = compare_skill_hashes(
        &provenance.installed_content_hash,
        &local_hash,
        &source_hash,
    );
    Ok(SkillUpdateState {
        skill_id: skill_id.to_owned(),
        status,
        local_modified,
        source_type: Some(provenance.source_type),
        source_locator: Some(provenance.source_locator),
        checked_at: crate::control::now_iso(),
        message: None,
    })
}

fn compare_skill_hashes(
    installed_hash: &str,
    local_hash: &str,
    source_hash: &str,
) -> (SkillUpdateStatus, bool) {
    (
        if source_hash == installed_hash {
            SkillUpdateStatus::UpToDate
        } else {
            SkillUpdateStatus::UpdateAvailable
        },
        local_hash != installed_hash,
    )
}

pub fn update_managed_skill(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
    force: bool,
) -> Result<Value, CliError> {
    let listing = managed_skill_listing(paths, skill_id)?;
    if managed_skill_kind(&listing)? != "custom" {
        return Err(CliError::with_code(
            "skill_read_only",
            "System and preset Skills cannot be updated independently.",
        ));
    }
    let mut manifest = read_skill_manifest(&listing)?;
    let provenance = skill_provenance_from_manifest(&manifest).ok_or_else(|| {
        CliError::with_code(
            "skill_update_unavailable",
            "This Skill does not have a reusable installation source.",
        )
    })?;
    let local_hash = skill_package_hash(Path::new(&listing.local_dir))?;
    if local_hash != provenance.installed_content_hash && !force {
        return Err(CliError::with_code(
            "skill_local_modifications",
            "This Skill has local changes. Confirm that they can be discarded before updating.",
        ));
    }
    let prepared = prepare_provenance_source(paths, &provenance)?;
    if normalized_device_skill_name(&prepared.name)
        != normalized_device_skill_name(&listing.skill.name)
    {
        return Err(CliError::with_code(
            "skill_name_conflict",
            "The source Skill name no longer matches the installed Skill.",
        ));
    }
    let source_hash = skill_package_hash(&prepared.root)?;
    let next_provenance = SkillProvenance {
        installed_content_hash: source_hash,
        ..provenance
    };
    manifest["origin"] = json!(next_provenance.source_locator);
    manifest["provenance"] = serde_json::to_value(&next_provenance).map_err(to_cli_error)?;
    manifest["updatedAt"] = json!(crate::control::now_iso());
    atomic_replace_device_skill(
        &prepared.root,
        Path::new(&listing.local_dir),
        &manifest,
    )?;
    clear_ignored_device_mismatches(paths, &listing.skill.name)?;
    let updated = managed_skill_listing(paths, skill_id)?;
    Ok(json!({
        "skill": managed_skill_from_listing(
            &updated,
            "custom",
            managed_skill_module_kinds(&updated),
        ),
        "updateState": SkillUpdateState {
            skill_id: skill_id.to_owned(),
            status: SkillUpdateStatus::UpToDate,
            local_modified: false,
            source_type: Some(next_provenance.source_type),
            source_locator: Some(next_provenance.source_locator),
            checked_at: crate::control::now_iso(),
            message: None,
        },
    }))
}

fn managed_skill_provenance(listing: &AgentSkillListing) -> Option<ManagedSkillProvenance> {
    let manifest = read_skill_manifest(listing).ok()?;
    let provenance = skill_provenance_from_manifest(&manifest)?;
    Some(ManagedSkillProvenance {
        source_type: provenance.source_type,
        source_locator: provenance.source_locator,
        revision: provenance.revision,
        subpath: provenance.subpath,
    })
}

fn device_skill_provenance(source_dir: &Path) -> Result<SkillProvenance, CliError> {
    let canonical = fs::canonicalize(source_dir).map_err(to_cli_error)?;
    Ok(SkillProvenanceSource {
        source_type: "device".to_owned(),
        source_locator: canonical.to_string_lossy().to_string(),
        revision: None,
        subpath: None,
    }
    .with_content_hash(&skill_package_hash(&canonical)?))
}

fn manifest_with_device_provenance(
    mut manifest: Value,
    source_dir: &Path,
) -> Result<Value, CliError> {
    let provenance = device_skill_provenance(source_dir)?;
    manifest["origin"] = json!(provenance.source_locator);
    manifest["provenance"] = serde_json::to_value(provenance).map_err(to_cli_error)?;
    Ok(manifest)
}

fn attach_device_provenance_if_content_matches(
    listing: &AgentSkillListing,
    source_dir: &Path,
) -> Result<(), CliError> {
    let manifest = read_skill_manifest(listing)?;
    if skill_provenance_from_manifest(&manifest).is_some()
        || skill_package_hash(Path::new(&listing.local_dir))? != skill_package_hash(source_dir)?
    {
        return Ok(());
    }
    let manifest = manifest_with_device_provenance(manifest, source_dir)?;
    write_skill_manifest(listing, &manifest)
}

fn skill_provenance_from_manifest(manifest: &Value) -> Option<SkillProvenance> {
    let provenance = serde_json::from_value::<SkillProvenance>(manifest.get("provenance")?.clone())
        .ok()?;
    if !matches!(
        provenance.source_type.as_str(),
        "github" | "skills-sh" | "clawhub" | "skillhub" | "device"
    ) || provenance.source_locator.trim().is_empty()
        || provenance.installed_content_hash.len() != 64
        || !provenance
            .installed_content_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    Some(provenance)
}

fn prepare_provenance_source(
    paths: &MyOpenPanelsPaths,
    provenance: &SkillProvenance,
) -> Result<PreparedSkillPackage, CliError> {
    if provenance.source_type == "device" {
        let (root, skill) = validated_device_skill(paths, &provenance.source_locator).map_err(
            |error| CliError::with_code("skill_source_unavailable", error.message()),
        )?;
        return Ok(PreparedSkillPackage {
            _temporary: None,
            root,
            name: skill.name,
            description: skill.description,
        });
    }
    let prepared = prepare_remote_archive(&provenance.source_locator)?;
    if prepared.source.provenance.source_type != provenance.source_type {
        return Err(CliError::with_code(
            "unsupported_skill_source",
            "The stored Skill source type does not match its URL.",
        ));
    }
    let root = if let Some(subpath) = provenance.subpath.as_deref() {
        resolve_import_subpath(&prepared.archive_root, Some(subpath))?
    } else {
        find_imported_skill(
            &prepared.search_root,
            prepared.source.skill_selector.as_deref(),
        )?
    };
    let (name, description) = validate_imported_skill(&root)?;
    Ok(PreparedSkillPackage {
        _temporary: Some(prepared._temporary),
        root,
        name,
        description,
    })
}

fn read_skill_manifest(listing: &AgentSkillListing) -> Result<Value, CliError> {
    serde_json::from_slice(
        &fs::read(PathBuf::from(&listing.local_dir).join("manifest.json"))
            .map_err(to_cli_error)?,
    )
    .map_err(to_cli_error)
}

fn write_skill_manifest(listing: &AgentSkillListing, manifest: &Value) -> Result<(), CliError> {
    let path = PathBuf::from(&listing.local_dir).join("manifest.json");
    let temporary = path.with_file_name(format!(
        ".manifest-{}",
        crate::ids::random_base64url_96()
    ));
    fs::write(
        &temporary,
        format!(
            "{}\n",
            serde_json::to_string_pretty(manifest).map_err(to_cli_error)?
        ),
    )
    .map_err(to_cli_error)?;
    fs::rename(temporary, path).map_err(to_cli_error)
}

fn unmanaged_skill_update_state(skill_id: &str) -> SkillUpdateState {
    SkillUpdateState {
        skill_id: skill_id.to_owned(),
        status: SkillUpdateStatus::Unmanaged,
        local_modified: false,
        source_type: None,
        source_locator: None,
        checked_at: crate::control::now_iso(),
        message: None,
    }
}

fn unavailable_skill_update_state(
    skill_id: &str,
    provenance: &SkillProvenance,
    local_modified: bool,
    message: &str,
) -> SkillUpdateState {
    SkillUpdateState {
        skill_id: skill_id.to_owned(),
        status: SkillUpdateStatus::SourceUnavailable,
        local_modified,
        source_type: Some(provenance.source_type.clone()),
        source_locator: Some(provenance.source_locator.clone()),
        checked_at: crate::control::now_iso(),
        message: Some(message.to_owned()),
    }
}

fn clear_ignored_device_mismatches(
    paths: &MyOpenPanelsPaths,
    skill_name: &str,
) -> Result<(), CliError> {
    let storage = crate::storage::Storage::open(paths)?;
    let mut ignored = ignored_device_mismatches(paths)?;
    let name_key = normalized_device_skill_name(skill_name);
    let before = ignored.len();
    ignored.retain(|item| item["nameKey"].as_str() != Some(&name_key));
    if ignored.len() != before {
        storage.write_setting(
            "skill_management",
            "ignored_device_mismatches",
            &serde_json::to_string(&ignored).map_err(to_cli_error)?,
        )?;
    }
    Ok(())
}
