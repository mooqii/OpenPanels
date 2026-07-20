#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: String,
    pub module_kinds: Vec<String>,
    pub can_edit: bool,
    pub can_delete: bool,
    pub local_dir: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedSkillModule {
    pub kind: String,
    pub skills: Vec<ManagedSkill>,
}

pub fn list_writing_refinement_agent_skills(
    paths: &MyOpenPanelsPaths,
) -> Result<Vec<AgentSkillListing>, CliError> {
    Ok(list_agent_skills(paths)?
        .into_iter()
        .filter(|item| {
            metadata_matches(
                &item.skill.applies_to,
                &item.skill.task_types,
                Some("writing"),
                Some("refine_writing_skill"),
            )
        })
        .collect())
}

pub fn writing_refinement_agent_skill(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
) -> Result<AgentSkillListing, CliError> {
    list_writing_refinement_agent_skills(paths)?
        .into_iter()
        .find(|item| item.skill.id == skill_id)
        .ok_or_else(|| {
            CliError::with_code(
                "writing_refinement_skill_not_found",
                format!("Writing refinement Skill not found: {skill_id}"),
            )
        })
}

pub fn managed_skills(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    sync_builtin_agent_skills(paths)?;
    migrate_legacy_custom_agent_skills(paths)?;
    let listings = list_agent_skills(paths)?;
    let mut system_skills = Vec::new();
    let mut modules = BTreeMap::<String, Vec<ManagedSkill>>::new();
    for listing in listings {
        let kind = managed_skill_kind(&listing)?;
        let module_kinds = managed_skill_module_kinds(&listing);
        let managed = managed_skill_from_listing(&listing, kind, module_kinds.clone());
        if kind == "system" {
            system_skills.push(managed);
            continue;
        }
        for module_kind in module_kinds {
            if module_kind != "any" {
                modules
                    .entry(module_kind)
                    .or_default()
                    .push(managed.clone());
            }
        }
    }
    sort_managed_skills(&mut system_skills);
    let modules = modules
        .into_iter()
        .map(|(kind, mut skills)| {
            sort_managed_skills(&mut skills);
            ManagedSkillModule { kind, skills }
        })
        .collect::<Vec<_>>();
    Ok(json!({ "systemSkills": system_skills, "modules": modules }))
}

pub fn read_managed_skill_files(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
) -> Result<Value, CliError> {
    let listing = managed_skill_listing(paths, skill_id)?;
    let kind = managed_skill_kind(&listing)?;
    let root = PathBuf::from(&listing.local_dir);
    let mut files = Vec::new();
    collect_managed_skill_files(&root, &root, &mut files)?;
    files.sort_by(|left, right| left["path"].as_str().cmp(&right["path"].as_str()));
    Ok(json!({
        "skill": managed_skill_from_listing(
            &listing,
            kind,
            managed_skill_module_kinds(&listing),
        ),
        "files": files,
    }))
}

pub fn write_managed_skill_file(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
    relative_path: &str,
    content: &str,
) -> Result<Value, CliError> {
    let listing = managed_skill_listing(paths, skill_id)?;
    if managed_skill_kind(&listing)? != "custom" {
        return Err(CliError::with_code(
            "skill_read_only",
            "System and preset Skills cannot be edited.",
        ));
    }
    let relative = Path::new(relative_path);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(CliError::with_code(
            "skill_file_invalid",
            "Skill file path is invalid.",
        ));
    }
    let root = fs::canonicalize(&listing.local_dir).map_err(to_cli_error)?;
    let target = root.join(relative);
    let metadata = fs::symlink_metadata(&target).map_err(|_| {
        CliError::with_code("skill_file_not_found", "Skill file does not exist.")
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(CliError::with_code(
            "skill_file_invalid",
            "Skill file must be an existing regular file.",
        ));
    }
    let canonical = fs::canonicalize(&target).map_err(to_cli_error)?;
    if !canonical.starts_with(&root) || fs::read_to_string(&canonical).is_err() {
        return Err(CliError::with_code(
            "skill_file_invalid",
            "Skill file is not an editable UTF-8 text file.",
        ));
    }
    if relative_path == "SKILL.md" {
        let manifest: Value = serde_json::from_slice(
            &fs::read(root.join("manifest.json")).map_err(to_cli_error)?,
        )
        .map_err(to_cli_error)?;
        let expected_name = manifest.get("name").and_then(Value::as_str).unwrap_or_default();
        let parsed = external_custom_skill_from_source(content, relative_path, skill_id).map_err(|_| {
            CliError::with_code(
                "skill_file_invalid",
                "SKILL.md must remain a valid portable Skill with the same id.",
            )
        })?;
        if normalized_device_skill_name(&parsed.metadata.name)
            != normalized_device_skill_name(expected_name)
        {
            return Err(CliError::with_code(
                "skill_file_invalid",
                "SKILL.md name cannot be changed.",
            ));
        }
    }
    let temporary = canonical.with_file_name(format!(
        ".skill-file-{}",
        crate::ids::random_base64url_96()
    ));
    fs::write(&temporary, content).map_err(to_cli_error)?;
    fs::rename(&temporary, &canonical).map_err(to_cli_error)?;
    read_managed_skill_files(paths, skill_id)
}

pub fn delete_managed_skill(paths: &MyOpenPanelsPaths, skill_id: &str) -> Result<Value, CliError> {
    let listing = managed_skill_listing(paths, skill_id)?;
    if managed_skill_kind(&listing)? != "custom" {
        return Err(CliError::with_code(
            "skill_read_only",
            "System and preset Skills cannot be deleted.",
        ));
    }
    crate::content::archive_resource(
        paths,
        None,
        crate::content::ResourceKind::WritingSkill,
        skill_id,
    )?;
    fs::remove_dir_all(&listing.local_dir).map_err(to_cli_error)?;
    clear_deleted_writing_skill_selections(paths, skill_id)?;
    Ok(json!({ "deleted": true, "skillId": skill_id }))
}

fn managed_skill_listing(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
) -> Result<AgentSkillListing, CliError> {
    list_agent_skills(paths)?
        .into_iter()
        .find(|listing| listing.skill.id == skill_id)
        .ok_or_else(|| {
            CliError::with_code("skill_not_found", format!("Skill not found: {skill_id}"))
        })
}

fn managed_skill_kind(listing: &AgentSkillListing) -> Result<&'static str, CliError> {
    if listing.source == "custom" {
        return Ok("custom");
    }
    builtin_skill_kind(&listing.skill.id)?.ok_or_else(|| {
        CliError::with_code(
            "invalid_skill_package",
            format!("Skill has no management classification: {}", listing.skill.id),
        )
    })
}

fn managed_skill_from_listing(
    listing: &AgentSkillListing,
    kind: &str,
    module_kinds: Vec<String>,
) -> ManagedSkill {
    ManagedSkill {
        id: listing.skill.id.clone(),
        name: listing.skill.name.clone(),
        description: listing.skill.description.clone(),
        kind: kind.to_owned(),
        module_kinds,
        can_edit: kind == "custom",
        can_delete: kind == "custom",
        local_dir: listing.local_dir.clone(),
    }
}

fn managed_skill_module_kinds(listing: &AgentSkillListing) -> Vec<String> {
    let mut module_kinds = Vec::new();
    let has_panel = |kind: &str| listing.skill.applies_to.iter().any(|value| value == kind);
    let has_task = |kind: &str| listing.skill.task_types.iter().any(|value| value == kind);
    if has_panel("wiki")
        && (has_task("ingest_markdown_into_wiki") || has_task("maintain_wiki"))
    {
        module_kinds.push("wiki-update".to_owned());
    }
    if has_panel("writing") && has_task("generate_document") {
        module_kinds.push("writing".to_owned());
    }
    if has_panel("writing") && has_task("refine_writing_skill") {
        module_kinds.push("writing-refinement".to_owned());
    }
    if has_panel("publishing") && has_task("publish_xiaohongshu_note") {
        module_kinds.push("publishing-xiaohongshu".to_owned());
    }
    for applies_to in &listing.skill.applies_to {
        if !matches!(
            applies_to.as_str(),
            "wiki" | "writing" | "publishing" | "any"
        )
            && !module_kinds.contains(applies_to)
        {
            module_kinds.push(applies_to.clone());
        }
    }
    module_kinds
}

fn sort_managed_skills(skills: &mut [ManagedSkill]) {
    skills.sort_by(|left, right| {
        let rank = |kind: &str| match kind {
            "preset" => 0,
            "custom" => 1,
            _ => 0,
        };
        rank(&left.kind)
            .cmp(&rank(&right.kind))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn collect_managed_skill_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<Value>,
) -> Result<(), CliError> {
    for entry in fs::read_dir(directory).map_err(to_cli_error)? {
        let entry = entry.map_err(to_cli_error)?;
        let file_type = entry.file_type().map_err(to_cli_error)?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_managed_skill_files(root, &entry.path(), files)?;
            continue;
        }
        if !file_type.is_file() || entry.file_name() == "manifest.json" {
            continue;
        }
        let path = entry.path();
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let relative = path.strip_prefix(root).map_err(to_cli_error)?;
        files.push(json!({
            "path": relative.to_string_lossy().replace('\\', "/"),
            "content": content,
        }));
    }
    Ok(())
}

fn clear_deleted_writing_skill_selections(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
) -> Result<(), CliError> {
    clear_writing_skill_module_selections(paths, skill_id, true, true)?;
    clear_wiki_skill_selections(paths, skill_id)?;
    clear_publishing_skill_selections(paths, skill_id)
}

pub fn list_xiaohongshu_publishing_skills(
    paths: &MyOpenPanelsPaths,
) -> Result<Vec<AgentSkillListing>, CliError> {
    sync_builtin_agent_skills(paths)?;
    Ok(list_agent_skills(paths)?
        .into_iter()
        .filter(|item| {
            metadata_matches(
                &item.skill.applies_to,
                &item.skill.task_types,
                Some("publishing"),
                Some("publish_xiaohongshu_note"),
            )
        })
        .collect())
}

pub fn xiaohongshu_publishing_skill(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
) -> Result<AgentSkillListing, CliError> {
    list_xiaohongshu_publishing_skills(paths)?
        .into_iter()
        .find(|item| item.skill.id == skill_id)
        .ok_or_else(|| {
            CliError::with_code(
                "publishing_skill_not_found",
                format!("Xiaohongshu Publishing Skill not found: {skill_id}"),
            )
        })
}

fn clear_wiki_skill_selections(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
) -> Result<(), CliError> {
    let storage = crate::storage::Storage::open(paths)?;
    for project in storage.list_projects()? {
        for panel_id in &project.panel_ids {
            let Some(panel) = storage.read_panel(&project.id, panel_id)? else {
                continue;
            };
            if panel.kind != PanelKind::Wiki {
                continue;
            }
            let Some(mut state) = storage.read_panel_state(&project.id, panel_id)? else {
                continue;
            };
            if state.get("wikiAgentSkillId").and_then(Value::as_str) == Some(skill_id) {
                state["wikiAgentSkillId"] = json!("karpathy-llm-wiki");
                storage.write_panel_state(&project.id, panel_id, &state)?;
            }
        }
    }
    Ok(())
}

fn clear_writing_skill_module_selections(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
    clear_writing: bool,
    clear_refinement: bool,
) -> Result<(), CliError> {
    let storage = crate::storage::Storage::open(paths)?;
    for project in storage.list_projects()? {
        for panel_id in &project.panel_ids {
            let Some(panel) = storage.read_panel(&project.id, panel_id)? else {
                continue;
            };
            if panel.kind != PanelKind::Writing {
                continue;
            }
            let Some(mut state) = storage.read_panel_state(&project.id, panel_id)? else {
                continue;
            };
            let mut changed = false;
            if clear_writing {
                if let Some(selected) = state
                    .get_mut("selectedCreateWritingSkillIds")
                    .and_then(Value::as_array_mut)
                {
                    let previous_len = selected.len();
                    selected.retain(|value| value.as_str() != Some(skill_id));
                    changed |= selected.len() != previous_len;
                    if selected.is_empty() {
                        selected.push(json!("writing-default"));
                        changed = true;
                    }
                }
                if state
                    .get("selectedRevisionWritingSkillId")
                    .and_then(Value::as_str)
                    == Some(skill_id)
                {
                    state["selectedRevisionWritingSkillId"] = json!("writing-default");
                    changed = true;
                }
            }
            if clear_refinement && state
                .get("selectedRefinementSkillId")
                .and_then(Value::as_str)
                == Some(skill_id)
            {
                state["selectedRefinementSkillId"] =
                    json!(crate::writing::WRITING_SKILL_REFINER_ID);
                changed = true;
            }
            if changed {
                storage.write_panel_state(&project.id, panel_id, &state)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn migrate_legacy_custom_agent_skills(
    paths: &MyOpenPanelsPaths,
) -> Result<(), CliError> {
    let skills_dir = paths.storage_dir.join("skills");
    fs::create_dir_all(&skills_dir).map_err(to_cli_error)?;
    let legacy_dir = paths.storage_dir.join("writing-skills");
    if legacy_dir.is_dir() {
        for entry in fs::read_dir(&legacy_dir).map_err(to_cli_error)? {
            let entry = entry.map_err(to_cli_error)?;
            if !entry.file_type().map_err(to_cli_error)?.is_dir() {
                continue;
            }
            migrate_custom_skill_dir(&entry.path(), &skills_dir)?;
        }
    }
    for (skill_id, source, manifest, local_dir) in
        crate::content::active_writing_skill_sources(paths)?
    {
        let source_dir = PathBuf::from(local_dir);
        let target_dir = skills_dir.join(crate::paths::sanitize_path_part(&skill_id));
        if target_dir.is_dir()
            && fs::read(target_dir.join("manifest.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .and_then(|value| value.get("source").and_then(Value::as_str).map(str::to_owned))
                .as_deref()
                == Some("custom")
        {
            continue;
        }
        install_migrated_custom_skill(&source_dir, &target_dir, &source, &manifest)?;
    }
    Ok(())
}

fn migrate_custom_skill_dir(source_dir: &Path, skills_dir: &Path) -> Result<(), CliError> {
    let skill_path = source_dir.join("SKILL.md");
    let manifest_path = source_dir.join("manifest.json");
    if !skill_path.is_file() || !manifest_path.is_file() {
        return Ok(());
    }
    let source = fs::read_to_string(&skill_path).map_err(to_cli_error)?;
    let manifest: Value =
        serde_json::from_slice(&fs::read(&manifest_path).map_err(to_cli_error)?)
            .map_err(to_cli_error)?;
    let skill_id = manifest
        .get("skillId")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            source_dir
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("default")
        });
    let target_dir = skills_dir.join(crate::paths::sanitize_path_part(skill_id));
    install_migrated_custom_skill(source_dir, &target_dir, &source, &manifest)?;
    if source_dir != target_dir {
        fs::remove_dir_all(source_dir).map_err(to_cli_error)?;
    }
    Ok(())
}

fn install_migrated_custom_skill(
    source_dir: &Path,
    target_dir: &Path,
    source: &str,
    manifest: &Value,
) -> Result<(), CliError> {
    if target_dir.is_dir() {
        let existing_source = fs::read_to_string(target_dir.join("SKILL.md")).unwrap_or_default();
        let existing_manifest = fs::read(target_dir.join("manifest.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        if existing_source == source && existing_manifest.as_ref() == Some(manifest) {
            return Ok(());
        }
        return Err(CliError::with_code(
            "writing_skill_conflict",
            format!("Custom Skill migration target already exists: {}", target_dir.display()),
        ));
    }
    let parent = target_dir.parent().ok_or_else(|| CliError::new("Invalid Skill path"))?;
    let staging = parent.join(format!(
        ".skill-migration-{}",
        crate::ids::random_base64url_96()
    ));
    copy_skill_package(source_dir, &staging)?;
    if !staging.join("SKILL.md").is_file() {
        fs::write(staging.join("SKILL.md"), source.as_bytes()).map_err(to_cli_error)?;
    }
    if !staging.join("manifest.json").is_file() {
        fs::write(
            staging.join("manifest.json"),
            format!("{}\n", serde_json::to_string_pretty(manifest).map_err(to_cli_error)?),
        )
        .map_err(to_cli_error)?;
    }
    if let Err(error) = fs::rename(&staging, target_dir) {
        let _ = fs::remove_dir_all(&staging);
        return Err(to_cli_error(error));
    }
    Ok(())
}

fn copy_skill_package(source: &Path, destination: &Path) -> Result<(), CliError> {
    fs::create_dir_all(destination).map_err(to_cli_error)?;
    for entry in fs::read_dir(source).map_err(to_cli_error)? {
        let entry = entry.map_err(to_cli_error)?;
        let file_type = entry.file_type().map_err(to_cli_error)?;
        if file_type.is_symlink() {
            continue;
        }
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_skill_package(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), target).map_err(to_cli_error)?;
        }
    }
    Ok(())
}
