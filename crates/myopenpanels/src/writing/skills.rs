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
    raw_ids: &[Value],
    generated_ids: &[Value],
) -> Result<(), CliError> {
    let raw_documents = wiki.state["rawDocuments"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for id in raw_ids.iter().filter_map(Value::as_str) {
        let ready = raw_documents.iter().any(|document| {
            document.get("id").and_then(Value::as_str) == Some(id)
                && document
                    .get("markdownRef")
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.is_empty())
        });
        if !ready {
            return Err(CliError::with_code(
                "writing_refinement_source_not_ready",
                format!("Raw document is not ready for refinement: {id}"),
            ));
        }
    }
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
                && normalized_name_key(&item.skill.title) == key
        })
    {
        return Err(CliError::with_code(
            "writing_skill_name_conflict",
            format!("A Writing Skill with this name already exists: {name}"),
        ));
    }
    let pending_conflict = Storage::open(paths)?
        .list_tasks(project_id)?
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
    let skill_action = crate::cli::registry::command_action(
        crate::cli::registry::CommandId::registered("agent.skill.read"),
        vec![
            "--skill-id".to_owned(),
            WRITING_SKILL_REFINER_ID.to_owned(),
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
        let skill = crate::agent::parse_skill(&source, skill_file)?;
        return crate::content::broker_prepare_skill(&crate::content::PrepareSkillRequest {
            skill_id: skill.metadata.id.clone(),
            source,
            manifest: json!({
                "schemaVersion": 1,
                "source": "custom",
                "taskId": task_id,
                "skillId": skill.metadata.id,
                "title": skill.metadata.title,
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
    let skill = crate::agent::parse_skill(&source, skill_file)?;
    validate_generated_project_skill(&skill, skill_id, name)?;
    validate_available_skill_name(paths, project_id, name, Some(skill_id))?;

    let skills_dir = crate::agent::custom_writing_skills_dir(paths);
    fs::create_dir_all(&skills_dir).map_err(to_cli_error)?;
    let final_dir = skills_dir.join(crate::paths::sanitize_path_part(skill_id));
    let manifest = json!({
        "schemaVersion": 1,
        "source": "custom",
        "originProjectId": project_id,
        "taskId": task_id,
        "skillId": skill_id,
        "title": name,
        "createdAt": now_iso(),
    });
    if final_dir.exists() {
        let existing_manifest = read_skill_manifest(&final_dir)?;
        let existing_source =
            fs::read_to_string(final_dir.join("SKILL.md")).map_err(to_cli_error)?;
        if existing_manifest["taskId"].as_str() == Some(task_id) && existing_source == source {
            let listing = crate::agent::project_agent_skill_listing(
                paths,
                project_id,
                skill.metadata.clone(),
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
    let listing =
        crate::agent::project_agent_skill_listing(paths, project_id, skill.metadata.clone());
    Ok(json!({ "skill": listing }))
}

fn validate_generated_project_skill(
    skill: &crate::agent::AgentSkill,
    expected_id: &str,
    expected_title: &str,
) -> Result<(), CliError> {
    let metadata = &skill.metadata;
    let valid = metadata.id == expected_id
        && metadata.title == expected_title
        && metadata.source == "custom"
        && metadata.applies_to == ["writing"]
        && metadata.task_types == ["generate_document"]
        && metadata.requires_commands.is_empty()
        && !metadata.description.trim().is_empty()
        && !skill.body.trim().is_empty();
    if !valid {
        return Err(CliError::with_code(
            "writing_skill_file_invalid",
            "Generated Writing Skill frontmatter or body does not match the refinement task.",
        ));
    }
    Ok(())
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
    let target = PathBuf::from(&listing.local_dir).join(&relative);
    if !target.is_file()
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
        let skill = crate::agent::parse_skill(content, "SKILL.md")?;
        validate_generated_project_skill(&skill, skill_id, &skill.metadata.title)?;
        let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
        validate_available_skill_name(
            paths,
            &bootstrap.project.id,
            &skill.metadata.title,
            Some(skill_id),
        )?;
        let mut manifest = read_skill_manifest(Path::new(&listing.local_dir))?;
        manifest["title"] = json!(skill.metadata.title);
        let manifest_content = format!(
            "{}\n",
            serde_json::to_string_pretty(&manifest).map_err(to_cli_error)?
        );
        if let Some(project_id) = crate::content::writing_skill_project_id(paths, skill_id)? {
            crate::content::commit_immediate_text(
                paths,
                &project_id,
                None,
                crate::content::ResourceKind::WritingSkill,
                skill_id,
                "manifest.json",
                manifest_content.as_bytes(),
                "application/json",
                false,
            )?;
            crate::content::commit_immediate_text(
                paths,
                &project_id,
                None,
                crate::content::ResourceKind::WritingSkill,
                skill_id,
                "SKILL.md",
                content.as_bytes(),
                "text/markdown",
                false,
            )?;
            return Ok(json!({ "path": relative_path, "content": content }));
        }
        fs::write(
            Path::new(&listing.local_dir).join("manifest.json"),
            manifest_content,
        )
        .map_err(to_cli_error)?;
    }
    fs::write(&target, content.as_bytes()).map_err(to_cli_error)?;
    Ok(json!({ "path": relative_path, "content": content }))
}

pub fn delete_custom_skill(paths: &MyOpenPanelsPaths, skill_id: &str) -> Result<Value, CliError> {
    let listing = crate::agent::writing_agent_skill(paths, skill_id)?;
    if listing.source != "custom" {
        return Err(CliError::with_code(
            "writing_skill_read_only",
            "Built-in Writing Skills cannot be deleted.",
        ));
    }
    crate::content::archive_resource(
        paths,
        None,
        crate::content::ResourceKind::WritingSkill,
        skill_id,
    )?;
    let _ = fs::remove_dir_all(&listing.local_dir);
    Ok(json!({ "deleted": true, "skillId": skill_id }))
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
