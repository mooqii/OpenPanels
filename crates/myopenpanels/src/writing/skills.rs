fn normalize_skill_name(requested_name: &str) -> Result<String, CliError> {
    let name = requested_name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() {
        return Err(CliError::with_code(
            "writing_skill_name_required",
            "Writing Skill name cannot be empty.",
        ));
    }
    if name.chars().count() > 80 {
        return Err(CliError::with_code(
            "writing_skill_name_too_long",
            "Writing Skill name cannot exceed 80 characters.",
        ));
    }
    Ok(name)
}

fn validate_refinement_sources(
    wiki: &ProjectPanelSnapshot,
    generated_ids: &[Value],
) -> Result<(), CliError> {
    let generated_documents = wiki.state["generatedDocuments"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for id in generated_ids.iter().filter_map(Value::as_str) {
        let ready = generated_documents.iter().any(|document| {
            document.get("id").and_then(Value::as_str) == Some(id)
                && document
                    .get("contentRef")
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.is_empty())
                && matches!(
                    document
                        .pointer("/generation/status")
                        .and_then(Value::as_str),
                    None | Some("completed")
                )
        });
        if !ready {
            return Err(CliError::with_code(
                "writing_refinement_source_not_ready",
                format!("Generated document is not ready for refinement: {id}"),
            ));
        }
    }
    Ok(())
}

fn normalized_name_key(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn validate_available_skill_name(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    name: &str,
    allowed_skill_id: Option<&str>,
) -> Result<(), CliError> {
    let key = normalized_name_key(name);
    if crate::agent::list_agent_skills_for_project(paths, project_id)?
        .into_iter()
        .any(|item| {
            Some(item.skill.id.as_str()) != allowed_skill_id
                && normalized_name_key(&item.skill.name) == key
        })
    {
        return Err(CliError::with_code(
            "writing_skill_name_conflict",
            format!("A Writing Skill with this name already exists: {name}"),
        ));
    }
    let storage = Storage::open(paths)?;
    let mut tasks = Vec::new();
    for project in storage.list_projects()? {
        tasks.extend(storage.list_tasks(&project.id)?);
    }
    let pending_conflict = tasks
        .into_iter()
        .any(|task| {
            task.get("queue").and_then(Value::as_str) == Some("writing")
                && task.get("type").and_then(Value::as_str) == Some("refine_writing_skill")
                && matches!(
                    task.get("status").and_then(Value::as_str),
                    Some("queued" | "reserved" | "running" | "claimed" | "failed")
                )
                && task
                    .pointer("/input/name")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| normalized_name_key(candidate) == key)
                && task.pointer("/input/skillId").and_then(Value::as_str) != allowed_skill_id
        });
    if pending_conflict {
        return Err(CliError::with_code(
            "writing_skill_name_conflict",
            format!("A Writing Skill refinement with this name already exists: {name}"),
        ));
    }
    Ok(())
}

fn validate_writing_skills(
    paths: &MyOpenPanelsPaths,
    writing_skill_ids: &[String],
    required: bool,
    mode: &str,
) -> Result<Vec<crate::agent::AgentSkillListing>, CliError> {
    if required && writing_skill_ids.is_empty() {
        return Err(CliError::with_code(
            "writing_skill_required",
            "Select at least one Writing Skill.",
        ));
    }
    if required && mode == "revise" && writing_skill_ids.len() > 1 {
        return Err(CliError::with_code(
            "writing_revision_skill_limit",
            "Revision mode accepts exactly one Writing Skill.",
        ));
    }
    let mut seen = BTreeSet::new();
    let mut skills = Vec::with_capacity(writing_skill_ids.len());
    for skill_id in writing_skill_ids {
        if !seen.insert(skill_id.as_str()) {
            return Err(CliError::with_code(
                "duplicate_writing_skill",
                format!("Writing Skill was selected more than once: {skill_id}"),
            ));
        }
        skills.push(crate::agent::writing_agent_skill(paths, skill_id)?);
    }
    Ok(skills)
}

pub fn read_request(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    if crate::content::broker_execution_available() {
        return crate::content::broker_task_context(&crate::content::TaskContextRequest {
            task_id: task_id.to_owned(),
            context_kind: "writing_request".to_owned(),
        });
    }
    crate::content::require_broker_for_task_execution()?;
    let mut payload = crate::tasks::inspect_task(paths, task_id)?;
    if payload["task"]["queue"].as_str() != Some("writing") {
        return Err(CliError::with_code(
            "task_kind_mismatch",
            format!("Task is not a writing request: {task_id}"),
        ));
    }
    let skill_id = payload["task"]["input"]["writingSkillId"]
        .as_str()
        .ok_or_else(|| {
            CliError::with_code(
                "writing_skill_missing",
                format!("Writing task has no captured Writing Skill: {task_id}"),
            )
        })?;
    let skill_action = crate::cli::registry::command_action(
        crate::cli::registry::CommandId::registered("agent.skill.read"),
        vec![
            "--skill-id".to_owned(),
            skill_id.to_owned(),
            "--task-id".to_owned(),
            task_id.to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ],
    )
    .ok_or_else(|| CliError::new("Agent Skill read capability is unavailable."))?;
    payload["writingSkill"] = payload["task"]["input"]["writingSkill"].clone();
    payload["actions"] = json!({ "required": [skill_action], "suggested": [] });
    Ok(payload)
}

pub fn read_refinement(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    if crate::content::broker_execution_available() {
        return crate::content::broker_task_context(&crate::content::TaskContextRequest {
            task_id: task_id.to_owned(),
            context_kind: "writing_refinement".to_owned(),
        });
    }
    crate::content::require_broker_for_task_execution()?;
    let mut payload = crate::tasks::inspect_task(paths, task_id)?;
    if payload["task"]["queue"].as_str() != Some("writing")
        || payload["task"]["type"].as_str() != Some("refine_writing_skill")
    {
        return Err(CliError::with_code(
            "task_kind_mismatch",
            format!("Task is not a Writing Skill refinement: {task_id}"),
        ));
    }
    let refiner_skill_id = payload["task"]["input"]["refinerSkillId"]
        .as_str()
        .unwrap_or(DEFAULT_WRITING_REFINEMENT_SKILL_ID);
    let skill_action = crate::cli::registry::command_action(
        crate::cli::registry::CommandId::registered("agent.skill.read"),
        vec![
            "--skill-id".to_owned(),
            refiner_skill_id.to_owned(),
            "--task-id".to_owned(),
            task_id.to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ],
    )
    .ok_or_else(|| CliError::new("Agent Skill read capability is unavailable."))?;
    payload["actions"] = json!({ "required": [skill_action], "suggested": [] });
    Ok(payload)
}

pub fn install_project_skill(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    skill_file: &str,
) -> Result<Value, CliError> {
    if crate::content::broker_execution_available() {
        let source = fs::read_to_string(skill_file).map_err(|error| {
            CliError::with_code(
                "writing_skill_file_invalid",
                format!("Could not read Writing Skill file: {error}"),
            )
        })?;
        let skill = crate::agent::parse_portable_skill(&source, skill_file)?;
        return crate::content::broker_prepare_skill(&crate::content::PrepareSkillRequest {
            skill_id: skill.metadata.id.clone(),
            source,
            manifest: json!({
                "schemaVersion": 2,
                "source": "custom",
                "taskId": task_id,
                "skillId": skill.metadata.id,
                "binding": {
                    "appliesTo": ["writing"],
                    "taskTypes": ["generate_document"],
                },
                "createdAt": now_iso(),
            }),
        });
    }
    crate::content::require_broker_for_task_execution()?;
    crate::tasks::verify_task_write_access(paths, task_id)?;
    let payload = read_refinement(paths, task_id)?;
    let task = &payload["task"];
    if !matches!(
        task.get("status").and_then(Value::as_str),
        Some("reserved" | "running" | "claimed")
    ) {
        return Err(CliError::with_code(
            "task_not_claimed",
            "Claim the refinement task before installing its Writing Skill.",
        ));
    }
    let project_id = task["projectId"].as_str().unwrap_or_default();
    let skill_id = task["input"]["skillId"].as_str().unwrap_or_default();
    let name = task["input"]["name"].as_str().unwrap_or_default();
    if project_id.is_empty() || skill_id.is_empty() || name.is_empty() {
        return Err(CliError::with_code(
            "writing_refinement_invalid",
            "The refinement task is missing its project, Skill id, or name.",
        ));
    }
    let source = fs::read_to_string(skill_file).map_err(|error| {
        CliError::with_code(
            "writing_skill_file_invalid",
            format!("Could not read Writing Skill file: {error}"),
        )
    })?;
    crate::agent::validate_portable_writing_skill(&source, skill_file, skill_id)?;
    validate_available_skill_name(paths, project_id, name, Some(skill_id))?;

    let skills_dir = crate::agent::custom_writing_skills_dir(paths);
    fs::create_dir_all(&skills_dir).map_err(to_cli_error)?;
    let final_dir = skills_dir.join(crate::paths::sanitize_path_part(skill_id));
    let manifest = json!({
        "schemaVersion": 2,
        "source": "custom",
        "originProjectId": project_id,
        "taskId": task_id,
        "skillId": skill_id,
        "name": name,
        "binding": {
            "appliesTo": ["writing"],
            "taskTypes": ["generate_document"],
        },
        "createdAt": now_iso(),
    });
    if final_dir.exists() {
        let existing_manifest = read_skill_manifest(&final_dir)?;
        let existing_source =
            fs::read_to_string(final_dir.join("SKILL.md")).map_err(to_cli_error)?;
        if existing_manifest["taskId"].as_str() == Some(task_id) && existing_source == source {
            let stored = crate::agent::custom_writing_skill_from_source(
                &source,
                skill_file,
                &manifest,
            )?;
            let listing = crate::agent::project_agent_skill_listing(
                paths,
                project_id,
                stored.metadata,
            );
            return Ok(json!({ "skill": listing }));
        }
        return Err(CliError::with_code(
            "writing_skill_conflict",
            format!("Writing Skill target already exists: {skill_id}"),
        ));
    }
    let staging_dir = skills_dir.join(format!(
        ".{skill_id}.tmp-{}",
        crate::ids::random_base64url_96()
    ));
    fs::create_dir_all(&staging_dir).map_err(to_cli_error)?;
    let install_result = (|| -> Result<(), CliError> {
        fs::write(staging_dir.join("SKILL.md"), source.as_bytes()).map_err(to_cli_error)?;
        fs::write(
            staging_dir.join("manifest.json"),
            format!(
                "{}\n",
                serde_json::to_string_pretty(&manifest).map_err(to_cli_error)?
            ),
        )
        .map_err(to_cli_error)?;
        fs::rename(&staging_dir, &final_dir).map_err(to_cli_error)
    })();
    if install_result.is_err() {
        let _ = fs::remove_dir_all(&staging_dir);
    }
    install_result?;
    let stored =
        crate::agent::custom_writing_skill_from_source(&source, skill_file, &manifest)?;
    let listing = crate::agent::project_agent_skill_listing(paths, project_id, stored.metadata);
    Ok(json!({ "skill": listing }))
}

fn read_skill_manifest(skill_dir: &Path) -> Result<Value, CliError> {
    let source = fs::read_to_string(skill_dir.join("manifest.json")).map_err(to_cli_error)?;
    serde_json::from_str(&source).map_err(to_cli_error)
}

pub fn read_skill_files(paths: &MyOpenPanelsPaths, skill_id: &str) -> Result<Value, CliError> {
    let listing = crate::agent::writing_agent_skill(paths, skill_id)?;
    let root = PathBuf::from(&listing.local_dir);
    let mut files = Vec::new();
    collect_skill_files(&root, &root, &mut files)?;
    files.sort_by(|left, right| left["path"].as_str().cmp(&right["path"].as_str()));
    Ok(json!({ "skill": listing, "files": files }))
}

fn collect_skill_files(
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
            collect_skill_files(root, &entry.path(), files)?;
            continue;
        }
        if !file_type.is_file() || entry.file_name() == "manifest.json" {
            continue;
        }
        let path = entry.path();
        let relative = path.strip_prefix(root).map_err(to_cli_error)?;
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        files.push(json!({
            "path": relative.to_string_lossy().replace('\\', "/"),
            "content": content,
        }));
    }
    Ok(())
}

pub fn write_custom_skill_file(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
    relative_path: &str,
    content: &str,
) -> Result<Value, CliError> {
    let listing = crate::agent::writing_agent_skill(paths, skill_id)?;
    if listing.source != "custom" {
        return Err(CliError::with_code(
            "writing_skill_read_only",
            "Built-in Writing Skills cannot be edited.",
        ));
    }
    let relative = safe_skill_relative_path(relative_path)?;
    let root = PathBuf::from(&listing.local_dir);
    let target = root.join(&relative);
    let target_metadata = fs::symlink_metadata(&target).ok();
    let stays_inside_root = fs::canonicalize(&target)
        .ok()
        .zip(fs::canonicalize(&root).ok())
        .is_some_and(|(target, root)| target.starts_with(root));
    if target_metadata
        .as_ref()
        .is_none_or(|metadata| !metadata.is_file() || metadata.file_type().is_symlink())
        || !stays_inside_root
        || target
            .file_name()
            .is_some_and(|name| name == "manifest.json")
    {
        return Err(CliError::with_code(
            "writing_skill_file_not_found",
            format!("Writing Skill file not found: {relative_path}"),
        ));
    }
    if relative == Path::new("SKILL.md") {
        let parsed = crate::agent::parse_skill(content, "SKILL.md")?;
        if parsed.metadata.id != skill_id {
            return Err(CliError::with_code(
                "writing_skill_file_invalid",
                "Writing Skill name does not match its registered id.",
            ));
        }
        let was_legacy = parsed.metadata.source == "custom";
        let portable_content = if was_legacy {
            crate::agent::render_portable_skill(&parsed)
        } else {
            content.to_owned()
        };
        crate::agent::validate_portable_writing_skill(
            &portable_content,
            "SKILL.md",
            skill_id,
        )?;
        let name = if was_legacy {
            parsed.metadata.name.as_str()
        } else {
            listing.skill.name.as_str()
        };
        let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
        validate_available_skill_name(
            paths,
            &bootstrap.project.id,
            name,
            Some(skill_id),
        )?;
        let mut manifest = read_skill_manifest(Path::new(&listing.local_dir))?;
        manifest["schemaVersion"] = json!(2);
        manifest["name"] = json!(name);
        if let Some(object) = manifest.as_object_mut() {
            object.remove("title");
        }
        manifest["binding"] = json!({
            "appliesTo": ["writing"],
            "taskTypes": ["generate_document"],
        });
        let manifest_content = format!(
            "{}\n",
            serde_json::to_string_pretty(&manifest).map_err(to_cli_error)?
        );
        fs::write(
            Path::new(&listing.local_dir).join("manifest.json"),
            manifest_content,
        )
        .map_err(to_cli_error)?;
        fs::write(&target, portable_content.as_bytes()).map_err(to_cli_error)?;
        return Ok(json!({ "path": relative_path, "content": portable_content }));
    }
    fs::write(&target, content.as_bytes()).map_err(to_cli_error)?;
    Ok(json!({ "path": relative_path, "content": content }))
}

pub fn delete_custom_skill(paths: &MyOpenPanelsPaths, skill_id: &str) -> Result<Value, CliError> {
    crate::agent::delete_managed_skill(paths, skill_id).map_err(|error| {
        if error.code() == Some("skill_read_only") {
            CliError::with_code("writing_skill_read_only", error.message())
        } else {
            error
        }
    })
}

fn safe_skill_relative_path(value: &str) -> Result<PathBuf, CliError> {
    let path = Path::new(value);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(CliError::with_code(
            "writing_skill_file_invalid",
            "Writing Skill file path is invalid.",
        ));
    }
    Ok(path.to_path_buf())
}

fn installed_project_skill_for_task(
    paths: &MyOpenPanelsPaths,
    task: &Value,
) -> Result<bool, CliError> {
    let skill_id = task["input"]["skillId"].as_str().unwrap_or_default();
    if skill_id.is_empty() {
        return Ok(false);
    }
    let skill_dir = crate::agent::custom_writing_skills_dir(paths)
        .join(crate::paths::sanitize_path_part(skill_id));
    if !skill_dir.join("SKILL.md").is_file() || !skill_dir.join("manifest.json").is_file() {
        return Ok(false);
    }
    Ok(read_skill_manifest(&skill_dir)?["taskId"].as_str()
        == task.get("id").and_then(Value::as_str))
}

fn read_writing_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let payload = crate::tasks::inspect_task(paths, task_id)?;
    match payload["task"]["type"].as_str() {
        Some("generate_document") => read_request(paths, task_id),
        Some("refine_writing_skill") => read_refinement(paths, task_id),
        _ => Err(CliError::with_code(
            "task_kind_mismatch",
            format!("Unsupported writing task: {task_id}"),
        )),
    }
}

pub fn panel_context(bootstrap: &ProjectBootstrap) -> Value {
    let state = &bootstrap.state;
    let writing_tasks = bootstrap
        .tasks
        .iter()
        .filter(|task| task.get("queue").and_then(Value::as_str) == Some("writing"))
        .count();
    json!({
        "panelKind": "writing",
        "draftLength": state.get("draft").and_then(Value::as_str).map(str::len).unwrap_or(0),
        "mode": state.get("mode").cloned().unwrap_or_else(|| json!("create")),
        "selectedWritingSkillCount": if state.get("mode").and_then(Value::as_str) == Some("revise") {
            usize::from(state.get("selectedRevisionWritingSkillId").is_some_and(Value::is_string))
        } else {
            state.get("selectedCreateWritingSkillIds").and_then(Value::as_array).map(Vec::len).unwrap_or(0)
        },
        "writingTaskCount": writing_tasks,
    })
}
