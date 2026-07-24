fn load_agent_skill_dirs(
    locale: &str,
) -> Result<Vec<(AgentSkill, &'static Dir<'static>)>, CliError> {
    let registry: BuiltinSkillRegistry =
        serde_json::from_str(BUILTIN_SKILL_REGISTRY).map_err(to_cli_error)?;
    let mut registered_ids = BTreeSet::new();
    let mut system_registrations = BTreeMap::new();
    let mut preset_registrations = BTreeMap::new();
    for registration in registry.system_skills {
        insert_builtin_registration(
            &mut registered_ids,
            &mut system_registrations,
            registration,
        )?;
    }
    for registration in registry.preset_skills {
        insert_builtin_registration(
            &mut registered_ids,
            &mut preset_registrations,
            registration,
        )?;
    }
    let mut seen = BTreeSet::new();
    let mut skills = Vec::new();
    load_registered_skill_dirs(
        &SYSTEM_SKILLS,
        &mut system_registrations,
        false,
        "",
        &mut seen,
        &mut skills,
    )?;
    load_registered_preset_skill_dirs(
        &PRESET_SKILLS,
        &mut preset_registrations,
        locale,
        &mut seen,
        &mut skills,
    )?;
    if let Some((package_dir, _)) = system_registrations
        .first_key_value()
        .or_else(|| preset_registrations.first_key_value())
    {
        return Err(CliError::new(format!(
            "Built-in Skill registry package is missing: {package_dir}"
        )));
    }
    skills.sort_by(|left, right| left.0.metadata.id.cmp(&right.0.metadata.id));
    Ok(skills)
}

fn builtin_skill_kind(skill_id: &str) -> Result<Option<&'static str>, CliError> {
    let registry: BuiltinSkillRegistry =
        serde_json::from_str(BUILTIN_SKILL_REGISTRY).map_err(to_cli_error)?;
    if registry
        .system_skills
        .iter()
        .any(|registration| registration.id == skill_id)
    {
        return Ok(Some("system"));
    }
    if registry
        .preset_skills
        .iter()
        .any(|registration| registration.id == skill_id)
    {
        return Ok(Some("preset"));
    }
    Ok(None)
}

fn insert_builtin_registration(
    registered_ids: &mut BTreeSet<String>,
    registrations: &mut BTreeMap<String, BuiltinSkillRegistration>,
    registration: BuiltinSkillRegistration,
) -> Result<(), CliError> {
    if registration.package_dir != registration.id
        || !registered_ids.insert(registration.id.clone())
        || registrations
            .insert(registration.package_dir.clone(), registration)
            .is_some()
    {
        return Err(CliError::new(
            "Built-in Skill registry contains a duplicate id, duplicate package directory, or package directory that differs from its id.",
        ));
    }
    Ok(())
}

fn load_registered_preset_skill_dirs(
    root: &'static Dir<'static>,
    registrations: &mut BTreeMap<String, BuiltinSkillRegistration>,
    locale: &str,
    seen: &mut BTreeSet<String>,
    skills: &mut Vec<(AgentSkill, &'static Dir<'static>)>,
) -> Result<(), CliError> {
    let package_dirs = registrations.keys().cloned().collect::<Vec<_>>();
    for package_dir in package_dirs {
        let localized_path = Path::new(locale).join(&package_dir);
        let directory = (!locale.is_empty())
            .then(|| root.get_dir(&localized_path))
            .flatten()
            .or_else(|| root.get_dir(&package_dir))
            .ok_or_else(|| {
                CliError::new(format!(
                    "Built-in Skill registry package is missing: {package_dir}"
                ))
            })?;
        load_registered_skill_dir(
            root,
            directory,
            registrations,
            true,
            locale,
            seen,
            skills,
        )?;
    }
    Ok(())
}

fn load_registered_skill_dirs(
    root: &'static Dir<'static>,
    registrations: &mut BTreeMap<String, BuiltinSkillRegistration>,
    is_preset: bool,
    locale: &str,
    seen: &mut BTreeSet<String>,
    skills: &mut Vec<(AgentSkill, &'static Dir<'static>)>,
) -> Result<(), CliError> {
    for dir in root.dirs() {
        load_registered_skill_dir(
            root,
            dir,
            registrations,
            is_preset,
            locale,
            seen,
            skills,
        )?;
    }
    Ok(())
}

fn load_registered_skill_dir(
    root: &'static Dir<'static>,
    dir: &'static Dir<'static>,
    registrations: &mut BTreeMap<String, BuiltinSkillRegistration>,
    is_preset: bool,
    locale: &str,
    seen: &mut BTreeSet<String>,
    skills: &mut Vec<(AgentSkill, &'static Dir<'static>)>,
) -> Result<(), CliError> {
    let skill_path = dir.path().join("SKILL.md");
    let file = root.get_file(&skill_path).ok_or_else(|| {
        CliError::new(format!(
            "MyOpenPanels agent skill is missing SKILL.md: {}",
            dir.path().display()
        ))
    })?;
    let source = std::str::from_utf8(file.contents()).map_err(to_cli_error)?;
    let package_dir = dir
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let registration = registrations.remove(package_dir).ok_or_else(|| {
        CliError::new(format!(
            "Built-in Skill package is not registered: {package_dir}"
        ))
    })?;
    if is_preset {
        reject_platform_contract_in_embedded_skill(dir)?;
    }
    let skill = registered_builtin_skill(
        parse_portable_skill(source, &skill_path.display().to_string())?,
        &registration,
        locale,
    )?;
    if !seen.insert(skill.metadata.id.clone()) {
        return Err(CliError::new(format!(
            "Duplicate MyOpenPanels agent skill id: {}",
            skill.metadata.id
        )));
    }
    skills.push((skill, dir));
    Ok(())
}

fn reject_platform_contract_in_embedded_skill(dir: &Dir<'_>) -> Result<(), CliError> {
    for file in dir.files() {
        if let Ok(source) = std::str::from_utf8(file.contents()) {
            if portable_skill_mentions_platform(source) {
                return Err(CliError::new(format!(
                    "Preset Skill contains a MyOpenPanels runtime contract: {}",
                    file.path().display()
                )));
            }
        }
    }
    for child in dir.dirs() {
        reject_platform_contract_in_embedded_skill(child)?;
    }
    Ok(())
}
