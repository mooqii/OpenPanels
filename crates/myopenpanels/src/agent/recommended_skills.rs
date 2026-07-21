static RECOMMENDED_SKILL_CATALOG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../agent-resources/recommended-skills.json"
));

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecommendedSkillCatalog {
    schema_version: u32,
    skills: Vec<RecommendedSkillRegistration>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecommendedSkillRegistration {
    id: String,
    name: String,
    description: String,
    source_url: String,
    module_kinds: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
enum RecommendedSkillInstallStatus {
    NotInstalled,
    Installed,
    BindingsMissing,
    Conflict,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecommendedSkillListing {
    id: String,
    name: String,
    description: String,
    source_url: String,
    source_type: String,
    source_locator: String,
    module_kinds: Vec<String>,
    install_status: RecommendedSkillInstallStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    installed_skill_id: Option<String>,
    installed_module_kinds: Vec<String>,
    missing_module_kinds: Vec<String>,
    can_check_updates: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    conflict_reason: Option<&'static str>,
}

pub fn recommended_skills(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    sync_builtin_agent_skills(paths)?;
    migrate_legacy_custom_agent_skills(paths)?;
    migrate_skill_provenance(paths)?;
    let catalog = load_recommended_skill_catalog(RECOMMENDED_SKILL_CATALOG)?;
    let mut installed_by_name = BTreeMap::new();
    for listing in list_agent_skills(paths)? {
        installed_by_name.insert(
            normalized_device_skill_name(&listing.skill.name),
            listing,
        );
    }
    let skills = catalog
        .skills
        .into_iter()
        .map(|registration| {
            let name_key = normalized_device_skill_name(&registration.name);
            recommended_skill_listing(
                registration,
                installed_by_name.get(&name_key),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "schemaVersion": catalog.schema_version,
        "skills": skills,
    }))
}

pub fn install_recommended_skill(
    paths: &MyOpenPanelsPaths,
    catalog_id: &str,
) -> Result<Value, CliError> {
    sync_builtin_agent_skills(paths)?;
    migrate_legacy_custom_agent_skills(paths)?;
    migrate_skill_provenance(paths)?;
    let catalog = load_recommended_skill_catalog(RECOMMENDED_SKILL_CATALOG)?;
    let registration = catalog
        .skills
        .into_iter()
        .find(|skill| skill.id == catalog_id)
        .ok_or_else(|| {
            CliError::with_code(
                "recommended_skill_not_found",
                format!("Recommended Skill not found: {catalog_id}"),
            )
        })?;
    install_recommended_skill_registration(paths, registration)
}

fn install_recommended_skill_registration(
    paths: &MyOpenPanelsPaths,
    registration: RecommendedSkillRegistration,
) -> Result<Value, CliError> {
    let catalog_source = parse_remote_skill_source(&registration.source_url)?.provenance;
    let name_key = normalized_device_skill_name(&registration.name);

    if let Some(listing) = find_installed_skill_by_identity(paths, &name_key)? {
        if managed_skill_kind(&listing)? != "custom" {
            return Err(recommended_skill_conflict(
                "A built-in Skill already uses this recommended Skill name.",
            ));
        }
        let manifest = read_skill_manifest(&listing)?;
        let provenance = skill_provenance_from_manifest(&manifest).ok_or_else(|| {
            recommended_skill_conflict(
                "The installed Skill does not have a reusable installation source.",
            )
        })?;
        if !recommended_source_matches(&provenance, &catalog_source) {
            return Err(recommended_skill_conflict(
                "The installed Skill with this name comes from a different source.",
            ));
        }
        let current_modules = custom_skill_modules(&listing)?;
        let missing_modules = missing_recommended_modules(
            &registration.module_kinds,
            &current_modules,
        );
        let operation = if missing_modules.is_empty() {
            "unchanged"
        } else {
            update_custom_skill_modules(paths, &listing, |modules| {
                for module_kind in &registration.module_kinds {
                    if !modules.contains(module_kind) {
                        modules.push(module_kind.clone());
                    }
                }
            })?;
            "associated"
        };
        let installed = managed_skill_listing(paths, &listing.skill.id)?;
        return Ok(json!({
            "operation": operation,
            "skill": managed_skill_from_listing(
                &installed,
                "custom",
                managed_skill_module_kinds(&installed),
            ),
        }));
    }

    let (source, package) = prepare_remote_skill(&registration.source_url)?;
    validate_recommended_skill_name(&package.name, &registration.name)?;
    let origin = source.provenance.source_locator.clone();
    let installed = install_skill_package(
        paths,
        &package.root,
        package.name,
        package.description,
        &registration.module_kinds,
        false,
        &origin,
        Some(source.provenance),
    )?;
    if installed["status"] != "installed" {
        return Err(recommended_skill_conflict(
            "A Skill with this name was installed before the recommendation could be applied.",
        ));
    }
    Ok(json!({
        "operation": "installed",
        "skill": installed["skill"].clone(),
    }))
}

fn validate_recommended_skill_name(actual: &str, expected: &str) -> Result<(), CliError> {
    if normalized_device_skill_name(actual) == normalized_device_skill_name(expected) {
        return Ok(());
    }
    Err(CliError::with_code(
        "recommended_skill_name_mismatch",
        format!(
            "The source Skill name '{actual}' does not match the recommended catalog name '{expected}'."
        ),
    ))
}

fn recommended_skill_listing(
    registration: RecommendedSkillRegistration,
    installed: Option<&AgentSkillListing>,
) -> Result<RecommendedSkillListing, CliError> {
    let source = parse_remote_skill_source(&registration.source_url)?.provenance;
    let mut install_status = RecommendedSkillInstallStatus::NotInstalled;
    let mut installed_skill_id = None;
    let mut installed_module_kinds = Vec::new();
    let mut missing_module_kinds = Vec::new();
    let mut can_check_updates = false;
    let mut conflict_reason = None;

    if let Some(listing) = installed {
        installed_skill_id = Some(listing.skill.id.clone());
        installed_module_kinds = managed_skill_module_kinds(listing);
        let kind = managed_skill_kind(listing)?;
        if kind != "custom" {
            install_status = RecommendedSkillInstallStatus::Conflict;
            conflict_reason = Some("reservedName");
        } else {
            let manifest = read_skill_manifest(listing)?;
            match skill_provenance_from_manifest(&manifest) {
                Some(provenance) if recommended_source_matches(&provenance, &source) => {
                    can_check_updates = true;
                    missing_module_kinds = missing_recommended_modules(
                        &registration.module_kinds,
                        &installed_module_kinds,
                    );
                    install_status = if missing_module_kinds.is_empty() {
                        RecommendedSkillInstallStatus::Installed
                    } else {
                        RecommendedSkillInstallStatus::BindingsMissing
                    };
                }
                Some(_) => {
                    install_status = RecommendedSkillInstallStatus::Conflict;
                    conflict_reason = Some("differentSource");
                }
                None => {
                    install_status = RecommendedSkillInstallStatus::Conflict;
                    conflict_reason = Some("unmanagedSource");
                }
            }
        }
    }

    Ok(RecommendedSkillListing {
        id: registration.id,
        name: registration.name,
        description: registration.description,
        source_url: registration.source_url,
        source_type: source.source_type,
        source_locator: source.source_locator,
        module_kinds: registration.module_kinds,
        install_status,
        installed_skill_id,
        installed_module_kinds,
        missing_module_kinds,
        can_check_updates,
        conflict_reason,
    })
}

fn load_recommended_skill_catalog(source: &str) -> Result<RecommendedSkillCatalog, CliError> {
    let mut catalog: RecommendedSkillCatalog = serde_json::from_str(source).map_err(|error| {
        CliError::with_code(
            "invalid_recommended_skill_catalog",
            format!("Recommended Skill catalog is invalid: {error}"),
        )
    })?;
    if catalog.schema_version != 1 {
        return Err(CliError::with_code(
            "invalid_recommended_skill_catalog",
            format!(
                "Unsupported recommended Skill catalog schema: {}",
                catalog.schema_version
            ),
        ));
    }
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for skill in &mut catalog.skills {
        skill.id = skill.id.trim().to_owned();
        skill.name = skill.name.trim().to_owned();
        skill.description = skill.description.trim().to_owned();
        skill.source_url = skill.source_url.trim().to_owned();
        if skill.id.is_empty()
            || skill
                .id
                .chars()
                .any(|ch| !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-'))
            || !ids.insert(skill.id.clone())
        {
            return Err(invalid_recommended_catalog_entry(
                "Every recommended Skill must have a unique lowercase id containing only letters, numbers, and hyphens.",
            ));
        }
        let name_key = normalized_device_skill_name(&skill.name);
        if skill.name.is_empty()
            || skill.name.len() > MAX_SKILL_NAME_BYTES
            || !names.insert(name_key)
        {
            return Err(invalid_recommended_catalog_entry(
                "Recommended Skill names must be present, unique, and no longer than 64 bytes.",
            ));
        }
        if skill.description.is_empty() || skill.description.len() > MAX_SKILL_DESCRIPTION_BYTES {
            return Err(invalid_recommended_catalog_entry(
                "Recommended Skill descriptions must be present and no longer than 1024 bytes.",
            ));
        }
        parse_remote_skill_source(&skill.source_url).map_err(|error| {
            invalid_recommended_catalog_entry(format!(
                "Recommended Skill '{}' has an invalid source: {}",
                skill.id,
                error.message()
            ))
        })?;
        if skill.module_kinds.is_empty() {
            return Err(invalid_recommended_catalog_entry(format!(
                "Recommended Skill '{}' must declare at least one module.",
                skill.id
            )));
        }
        let mut modules = BTreeSet::new();
        let mut normalized_modules = Vec::new();
        for module_kind in &skill.module_kinds {
            let module_kind = match module_kind.as_str() {
                "publishing-xiaohongshu" => "publishing",
                value => value,
            };
            validate_custom_module(module_kind).map_err(|_| {
                invalid_recommended_catalog_entry(format!(
                    "Recommended Skill '{}' has an unsupported module: {module_kind}",
                    skill.id
                ))
            })?;
            if modules.insert(module_kind.to_owned()) {
                normalized_modules.push(module_kind.to_owned());
            }
        }
        skill.module_kinds = normalized_modules;
    }
    Ok(catalog)
}

fn missing_recommended_modules(
    recommended: &[String],
    installed: &[String],
) -> Vec<String> {
    recommended
        .iter()
        .filter(|module_kind| !installed.contains(module_kind))
        .cloned()
        .collect()
}

fn recommended_source_matches(
    installed: &SkillProvenance,
    recommended: &SkillProvenanceSource,
) -> bool {
    installed.source_type == recommended.source_type
        && installed.source_locator == recommended.source_locator
        && installed.revision == recommended.revision
        && installed.subpath == recommended.subpath
}

fn invalid_recommended_catalog_entry(message: impl Into<String>) -> CliError {
    CliError::with_code("invalid_recommended_skill_catalog", message)
}

fn recommended_skill_conflict(message: impl Into<String>) -> CliError {
    CliError::with_code("recommended_skill_conflict", message)
}
