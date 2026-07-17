pub fn sync_builtin_agent_skills(paths: &MyOpenPanelsPaths) -> Result<(), CliError> {
    let skills_dir = paths.storage_dir.join("skills");
    fs::create_dir_all(&skills_dir).map_err(to_cli_error)?;
    for legacy_id in ["canvas-panel", "task-queue", "wiki-panel", "writing-panel"] {
        let legacy_dir = skills_dir.join(legacy_id);
        if legacy_dir.exists() {
            fs::remove_dir_all(legacy_dir).map_err(to_cli_error)?;
        }
    }
    for (skill, skill_dir) in load_agent_skill_dirs()? {
        let local_dir = skills_dir.join(&skill.metadata.id);
        if local_dir.exists() {
            fs::remove_dir_all(&local_dir).map_err(to_cli_error)?;
        }
        fs::create_dir_all(&local_dir).map_err(to_cli_error)?;
        extract_embedded_dir_contents(skill_dir, skill_dir.path(), &local_dir)?;
    }
    Ok(())
}

pub fn list_agent_skills(paths: &MyOpenPanelsPaths) -> Result<Vec<AgentSkillListing>, CliError> {
    sync_builtin_agent_skills(paths)?;
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    list_agent_skills_for_project(paths, &bootstrap.project.id)
}

pub(crate) fn list_agent_skills_for_project(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
) -> Result<Vec<AgentSkillListing>, CliError> {
    sync_builtin_agent_skills(paths)?;
    Ok(load_agent_skills(paths, project_id)?
        .into_iter()
        .map(|skill| agent_skill_listing(paths, project_id, skill.metadata))
        .collect())
}

pub fn list_writing_agent_skills(
    paths: &MyOpenPanelsPaths,
) -> Result<Vec<AgentSkillListing>, CliError> {
    Ok(list_agent_skills(paths)?
        .into_iter()
        .filter(|item| {
            metadata_matches(
                &item.skill.applies_to,
                &item.skill.task_types,
                Some("writing"),
                Some("generate_document"),
            )
        })
        .collect())
}

pub(crate) fn wiki_agent_skill_for_project(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    skill_id: &str,
) -> Result<AgentSkillListing, CliError> {
    load_agent_skills(paths, project_id)?
        .into_iter()
        .filter(|item| {
            metadata_matches(
                &item.metadata.applies_to,
                &item.metadata.task_types,
                Some("wiki"),
                Some("ingest_markdown_into_wiki"),
            ) && item
                .metadata
                .task_types
                .iter()
                .any(|task_type| task_type == "maintain_wiki")
        })
        .map(|skill| agent_skill_listing(paths, project_id, skill.metadata))
        .find(|item| item.skill.id == skill_id)
        .ok_or_else(|| {
            CliError::with_code(
                "wiki_skill_not_found",
                format!("Wiki generation Skill not found or incomplete: {skill_id}"),
            )
        })
}

pub fn writing_agent_skill(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
) -> Result<AgentSkillListing, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    writing_agent_skill_for_project(paths, &bootstrap.project.id, skill_id)
}

pub(crate) fn writing_agent_skill_for_project(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    skill_id: &str,
) -> Result<AgentSkillListing, CliError> {
    load_agent_skills(paths, project_id)?
        .into_iter()
        .filter(|item| {
            metadata_matches(
                &item.metadata.applies_to,
                &item.metadata.task_types,
                Some("writing"),
                Some("generate_document"),
            )
        })
        .map(|skill| agent_skill_listing(paths, project_id, skill.metadata))
        .find(|item| item.skill.id == skill_id)
        .ok_or_else(|| {
            CliError::with_code(
                "writing_skill_not_found",
                format!("Writing Skill not found: {skill_id}"),
            )
        })
}

pub fn list_agent_skill_summaries(
    paths: &MyOpenPanelsPaths,
    panel_kind: Option<&str>,
    task_type: Option<&str>,
) -> Result<Vec<Value>, CliError> {
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    Ok(load_agent_skills(paths, &bootstrap.project.id)?
        .into_iter()
        .filter(|skill| {
            metadata_matches(
                &skill.metadata.applies_to,
                &skill.metadata.task_types,
                panel_kind,
                task_type,
            )
        })
        .map(|skill| {
            let metadata = skill.metadata;
            json!({
                "id": metadata.id,
                "name": metadata.name,
                "description": metadata.description,
                "appliesTo": metadata.applies_to,
                "taskTypes": metadata.task_types,
                "loadWhen": metadata.load_when,
            })
        })
        .collect())
}

fn metadata_matches(
    applies_to: &[String],
    task_types: &[String],
    panel_kind: Option<&str>,
    task_type: Option<&str>,
) -> bool {
    panel_kind.is_none_or(|kind| {
        applies_to
            .iter()
            .any(|candidate| candidate == kind || candidate == "any")
    }) && task_type.is_none_or(|kind| task_types.iter().any(|candidate| candidate == kind))
}

pub fn read_agent_skill(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
    task_id: Option<&str>,
) -> Result<AgentSkillReadPayload, CliError> {
    if let Some(task_id) = task_id.filter(|_| crate::content::broker_execution_available()) {
        let payload = crate::content::broker_read_skill(&crate::content::SkillReadRequest {
            task_id: task_id.to_owned(),
            skill_id: skill_id.to_owned(),
        })?;
        return serde_json::from_value(payload).map_err(to_cli_error);
    }
    if task_id.is_some() {
        crate::content::require_broker_for_task_execution()?;
    }
    sync_builtin_agent_skills(paths)?;
    let bootstrap = read_project_bootstrap(paths, BootstrapRequest::new())?;
    let task = task_id.and_then(|id| find_wiki_task(&bootstrap, id));
    let captured_snapshot = task
        .as_ref()
        .and_then(|task| {
            task.get("writingSkillSnapshot")
                .or_else(|| task.get("refinerSkillSnapshot"))
        })
        .filter(|snapshot| snapshot.get("id").and_then(Value::as_str) == Some(skill_id));
    let captured_skill = captured_snapshot
        .and_then(|snapshot| snapshot.get("markdown").and_then(Value::as_str));
    let skill = if let Some(markdown) = captured_skill {
        let mut skill = parse_skill(markdown, "captured-writing-skill.md").or_else(|_| {
            external_custom_skill_from_source(markdown, "captured-writing-skill.md", skill_id)
        })?;
        if skill.metadata.source == "portable" {
            skill.metadata.applies_to = vec!["writing".to_owned()];
            skill.metadata.load_when = vec!["The current Writing Task selected this Skill.".to_owned()];
            skill.metadata.source = task
                .as_ref()
                .and_then(|value| value.pointer("/writingSkill/source"))
                .and_then(Value::as_str)
                .unwrap_or("custom")
                .to_owned();
            skill.metadata.task_types = vec!["generate_document".to_owned()];
            skill.metadata.name = task
                .as_ref()
                .and_then(|value| {
                    value
                        .pointer("/writingSkill/name")
                        .or_else(|| value.pointer("/writingSkill/title"))
                })
                .and_then(Value::as_str)
                .unwrap_or(skill_id)
                .to_owned();
            skill.metadata.tokens = "short".to_owned();
        }
        if task
            .as_ref()
            .and_then(|value| value.get("refinerSkillSnapshot"))
            .is_some()
        {
            skill.metadata.id = skill_id.to_owned();
            skill.metadata.applies_to = vec!["writing".to_owned()];
            skill.metadata.task_types = vec!["refine_writing_skill".to_owned()];
            skill.metadata.name = captured_snapshot
                .and_then(|value| value.get("name"))
                .and_then(Value::as_str)
                .unwrap_or(skill_id)
                .to_owned();
            skill.metadata.source = captured_snapshot
                .and_then(|value| value.get("source"))
                .and_then(Value::as_str)
                .unwrap_or("custom")
                .to_owned();
        }
        skill
    } else {
        load_agent_skills(paths, &bootstrap.project.id)?
            .into_iter()
            .find(|skill| skill.metadata.id == skill_id)
            .ok_or_else(|| {
                CliError::new(format!("MyOpenPanels agent skill not found: {skill_id}"))
            })?
    };
    let selection = (task_id.is_none() && bootstrap.active_panel_kind == PanelKind::Canvas)
        .then(|| read_selection(paths, None, false).ok())
        .flatten();
    let wiki_selection = if task_id.is_some() {
        None
    } else {
        match bootstrap.active_panel_kind {
            PanelKind::Wiki => read_agent_selection(paths).ok(),
            PanelKind::Writing => crate::writing::panel_selection(paths, &bootstrap).ok(),
            _ => None,
        }
    };
    let (local_dir, local_path) = if let (Some(task_id), Some(markdown)) = (task_id, captured_skill)
    {
        let local_dir = paths
            .storage_dir
            .join("task-snapshots")
            .join(crate::paths::sanitize_path_part(task_id));
        fs::create_dir_all(&local_dir).map_err(to_cli_error)?;
        let local_path = local_dir.join("SKILL.md");
        if !local_path.is_file() {
            fs::write(&local_path, markdown.as_bytes()).map_err(to_cli_error)?;
        }
        (local_dir, local_path)
    } else {
        agent_skill_local_paths(paths, &bootstrap.project.id, &skill.metadata)
    };
    let markdown = render_agent_skill(
        &skill,
        &bootstrap,
        selection.as_ref(),
        wiki_selection.as_ref(),
        task_id,
        &local_dir,
        &local_path,
    )?;
    let required_action = json!({
        "id": format!("skill.{}.body", skill.metadata.id),
        "intent": "agent-host.file.read",
        "executor": "agent-host",
        "kind": "read-file",
        "path": local_path.display().to_string(),
    });
    Ok(AgentSkillReadPayload {
        actions: json!({
            "required": [required_action],
            "suggested": catalog_actions(&skill.metadata.requires_commands),
        }),
        skill: skill.metadata,
        local_dir: local_dir.display().to_string(),
        local_path: local_path.display().to_string(),
        markdown,
    })
}

fn catalog_actions(intents: &[String]) -> Vec<Value> {
    intents
        .iter()
        .filter_map(|intent| crate::cli::registry::catalog_domain_for_intent(intent))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(catalog_action)
        .collect()
}

fn catalog_action(domain: &str) -> Value {
    let mut action = command_action(
        "agent.catalog",
        vec![
            "--domain".to_owned(),
            domain.to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ],
    );
    action["condition"] = json!({
        "type": "agent-judgment",
        "description": format!("The user request needs a command from the {domain} domain.")
    });
    action
}

fn recommended_catalog_domains(active_panel_kind: PanelKind) -> Vec<&'static str> {
    let mut scopes = ["panel", "task", "operation"]
        .into_iter()
        .collect::<BTreeSet<_>>();
    let panel_scope = active_panel_kind.as_str();
    if crate::cli::registry::catalog(Some(panel_scope)).is_some() {
        scopes.insert(panel_scope);
    }
    scopes.into_iter().collect()
}

pub fn render_agent_skills_markdown(skills: &[AgentSkillListing]) -> String {
    format!(
        "# MyOpenPanels Agent Skills\n\n{}\n",
        render_skill_table(skills)
    )
}

#[allow(clippy::too_many_arguments)]
fn render_agent_skill(
    skill: &AgentSkill,
    bootstrap: &ProjectBootstrap,
    selection: Option<&crate::selection::SelectionPayload>,
    wiki_selection: Option<&Value>,
    task_id: Option<&str>,
    local_dir: &Path,
    local_path: &Path,
) -> Result<String, CliError> {
    let task = task_id.and_then(|id| find_wiki_task(bootstrap, id));
    if task_id.is_some() && task.is_none() {
        return Err(CliError::new(format!(
            "Project task not found: {}",
            task_id.unwrap_or_default()
        )));
    }
    Ok(format!(
        "# Skill: {}\n\nName: {}\nSource: {}\nLocal dir: {}\nLocal path: {}\nApplies to: {}\n\n## How To Load This Skill\n\nRead `SKILL.md` directly from the local path above. Treat this CLI output as the task-specific loader and command context, not as the skill body. Resolve referenced files relative to the local dir above.\n\n## Current Context\n\n{}\n\n## Required Commands\n\n{}\n\nUse the structured `agent catalog --domain <domain>` actions returned by the CLI for argument definitions. Do not infer command syntax from this loader.\n",
        skill.metadata.id,
        skill.metadata.name,
        skill.metadata.source,
        local_dir.display(),
        local_path.display(),
        if skill.metadata.applies_to.is_empty() { "any".to_owned() } else { skill.metadata.applies_to.join(", ") },
        render_current_context(bootstrap, selection, wiki_selection, task.as_ref()),
        skill.metadata.requires_commands.iter().map(|intent| format!("- `{intent}`")).collect::<Vec<_>>().join("\n"),
    ))
}

fn load_agent_skills(
    paths: &MyOpenPanelsPaths,
    _project_id: &str,
) -> Result<Vec<AgentSkill>, CliError> {
    let builtin = load_agent_skill_dirs()?
        .into_iter()
        .map(|(skill, _dir)| skill)
        .collect::<Vec<_>>();
    let custom = load_custom_agent_skills(paths)?;
    merge_agent_skill_providers([builtin, custom])
}

fn merge_agent_skill_providers(
    providers: impl IntoIterator<Item = Vec<AgentSkill>>,
) -> Result<Vec<AgentSkill>, CliError> {
    let mut seen = BTreeSet::new();
    let mut skills = Vec::new();
    for provider in providers {
        for skill in provider {
            if !seen.insert(skill.metadata.id.clone()) {
                return Err(CliError::new(format!(
                    "Duplicate MyOpenPanels agent skill id: {}",
                    skill.metadata.id
                )));
            }
            skills.push(skill);
        }
    }
    Ok(skills)
}

fn load_custom_agent_skills(paths: &MyOpenPanelsPaths) -> Result<Vec<AgentSkill>, CliError> {
    migrate_legacy_custom_agent_skills(paths)?;
    let skills_dir = paths.storage_dir.join("skills");
    let mut skills = Vec::new();
    if skills_dir.exists() {
        for entry in fs::read_dir(&skills_dir).map_err(to_cli_error)? {
            let entry = entry.map_err(to_cli_error)?;
            if !entry.file_type().map_err(to_cli_error)?.is_dir() {
                continue;
            }
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let skill_path = entry.path().join("SKILL.md");
            let manifest_path = entry.path().join("manifest.json");
            if !skill_path.is_file() || !manifest_path.is_file() {
                continue;
            }
            let manifest: Value =
                serde_json::from_slice(&fs::read(&manifest_path).map_err(to_cli_error)?)
                    .map_err(to_cli_error)?;
            let source = fs::read_to_string(&skill_path).map_err(to_cli_error)?;
            let skill = custom_writing_skill_from_source(
                &source,
                &skill_path.display().to_string(),
                &manifest,
            )?;
            skills.push(skill);
        }
    }
    skills.sort_by(|left, right| {
        left.metadata
            .name
            .to_lowercase()
            .cmp(&right.metadata.name.to_lowercase())
            .then_with(|| left.metadata.id.cmp(&right.metadata.id))
    });
    Ok(skills)
}

fn extract_embedded_dir_contents(
    dir: &Dir<'_>,
    root: &Path,
    destination: &Path,
) -> Result<(), CliError> {
    for file in dir.files() {
        let relative_path = file.path().strip_prefix(root).map_err(to_cli_error)?;
        let target_path = destination.join(relative_path);
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(to_cli_error)?;
        }
        fs::write(target_path, file.contents()).map_err(to_cli_error)?;
    }
    for child_dir in dir.dirs() {
        extract_embedded_dir_contents(child_dir, root, destination)?;
    }
    Ok(())
}

pub(crate) fn parse_skill(source: &str, file_name: &str) -> Result<AgentSkill, CliError> {
    let (frontmatter, body) = split_skill(source, file_name)?;
    if frontmatter.contains_key("name") {
        return portable_skill_from_parts(frontmatter, body, file_name);
    }
    legacy_skill_from_parts(frontmatter, body, file_name)
}

pub(crate) fn parse_portable_skill(
    source: &str,
    file_name: &str,
) -> Result<AgentSkill, CliError> {
    let (frontmatter, body) = split_skill(source, file_name)?;
    let keys = frontmatter.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if keys != BTreeSet::from(["description", "name"]) {
        return Err(CliError::new(format!(
            "Standard Skill frontmatter must contain exactly name and description: {file_name}"
        )));
    }
    portable_skill_from_parts(frontmatter, body, file_name)
}

fn split_skill(
    source: &str,
    file_name: &str,
) -> Result<(BTreeMap<String, Vec<String>>, String), CliError> {
    let normalized_source;
    let source = if source.contains("\r\n") {
        normalized_source = source.replace("\r\n", "\n");
        normalized_source.as_str()
    } else {
        source
    };
    let rest = source
        .strip_prefix("---\n")
        .ok_or_else(|| CliError::new(format!("Agent skill is missing frontmatter: {file_name}")))?;
    let (frontmatter, body) = rest
        .split_once("\n---")
        .ok_or_else(|| CliError::new(format!("Agent skill is missing frontmatter: {file_name}")))?;
    Ok((
        parse_frontmatter(frontmatter),
        body.trim_start_matches('\n').to_owned(),
    ))
}

fn portable_skill_from_parts(
    frontmatter: BTreeMap<String, Vec<String>>,
    body: String,
    file_name: &str,
) -> Result<AgentSkill, CliError> {
    let name = scalar(&frontmatter, "name")
        .ok_or_else(|| CliError::new(format!("Portable Skill requires name: {file_name}")))?;
    let description = scalar(&frontmatter, "description").unwrap_or_default();
    let valid_name = name.len() <= 64
        && name
            .chars()
            .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-')
        && name
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        && name
            .chars()
            .last()
            .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit());
    if !valid_name || description.trim().is_empty() || body.trim().is_empty() {
        return Err(CliError::new(format!(
            "Portable Skill requires a valid name, description, and body: {file_name}"
        )));
    }
    Ok(AgentSkill {
        metadata: AgentSkillMetadata {
            applies_to: Vec::new(),
            description,
            id: name.clone(),
            load_when: Vec::new(),
            requires_commands: Vec::new(),
            source: "portable".to_owned(),
            task_types: Vec::new(),
            name,
            tokens: "medium".to_owned(),
        },
        body,
    })
}

fn legacy_skill_from_parts(
    frontmatter: BTreeMap<String, Vec<String>>,
    body: String,
    file_name: &str,
) -> Result<AgentSkill, CliError> {
    let id = scalar(&frontmatter, "id")
        .ok_or_else(|| CliError::new(format!("Legacy Agent Skill requires id and title: {file_name}")))?;
    let title = scalar(&frontmatter, "title")
        .ok_or_else(|| CliError::new(format!("Legacy Agent Skill requires id and title: {file_name}")))?;
    Ok(AgentSkill {
        metadata: AgentSkillMetadata {
            applies_to: list(&frontmatter, "appliesTo"),
            description: scalar(&frontmatter, "description").unwrap_or_default(),
            id,
            load_when: list(&frontmatter, "loadWhen"),
            requires_commands: list(&frontmatter, "requiresCommands"),
            source: scalar(&frontmatter, "source").unwrap_or_else(|| "builtin".to_owned()),
            task_types: list(&frontmatter, "taskTypes"),
            name: title,
            tokens: scalar(&frontmatter, "tokens").unwrap_or_else(|| "medium".to_owned()),
        },
        body,
    })
}

fn registered_builtin_skill(
    mut skill: AgentSkill,
    registration: &BuiltinSkillRegistration,
) -> Result<AgentSkill, CliError> {
    if skill.metadata.id != registration.id {
        return Err(CliError::new(format!(
            "Portable Skill name {} does not match registered id {}",
            skill.metadata.id, registration.id
        )));
    }
    skill.metadata.applies_to = registration.applies_to.clone();
    skill.metadata.load_when = registration.load_when.clone();
    skill.metadata.name = registration.name.clone();
    skill.metadata.requires_commands = registration.requires_commands.clone();
    skill.metadata.source = registration.source.clone();
    skill.metadata.task_types = registration.task_types.clone();
    skill.metadata.tokens = registration.tokens.clone();
    Ok(skill)
}

fn parse_frontmatter(source: &str) -> BTreeMap<String, Vec<String>> {
    let mut result = BTreeMap::new();
    let mut current_key: Option<String> = None;
    for line in source.lines() {
        if let Some(value) = line.trim_start().strip_prefix("- ") {
            if let Some(key) = &current_key {
                result
                    .entry(key.clone())
                    .or_insert_with(Vec::new)
                    .push(value.trim().to_owned());
            }
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let value = value.trim();
            current_key = Some(key.to_owned());
            result.insert(
                key.to_owned(),
                if value.is_empty() {
                    Vec::new()
                } else {
                    vec![value.to_owned()]
                },
            );
        }
    }
    result
}

fn scalar(frontmatter: &BTreeMap<String, Vec<String>>, key: &str) -> Option<String> {
    frontmatter
        .get(key)
        .and_then(|values| values.first())
        .cloned()
}

fn list(frontmatter: &BTreeMap<String, Vec<String>>, key: &str) -> Vec<String> {
    frontmatter.get(key).cloned().unwrap_or_default()
}

fn render_skill_table(skills: &[AgentSkillListing]) -> String {
    if skills.is_empty() {
        return "- none".to_owned();
    }
    let rows = skills
        .iter()
        .map(|item| {
            format!(
                "| `{}` | {} | {} | {} | {} |",
                item.skill.id,
                item.source,
                item.skill.applies_to.join(", "),
                item.skill.task_types.join(", "),
                item.local_path
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("| ID | Source | Applies To | Task Types | Local Path |\n| --- | --- | --- | --- | --- |\n{rows}")
}

fn agent_skill_listing(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    skill: AgentSkillMetadata,
) -> AgentSkillListing {
    let (local_dir, local_path) = agent_skill_local_paths(paths, project_id, &skill);
    AgentSkillListing {
        source: skill.source.clone(),
        skill,
        local_dir: local_dir.display().to_string(),
        local_path: local_path.display().to_string(),
    }
}

pub(crate) fn project_agent_skill_listing(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    skill: AgentSkillMetadata,
) -> AgentSkillListing {
    agent_skill_listing(paths, project_id, skill)
}

fn agent_skill_local_paths(
    paths: &MyOpenPanelsPaths,
    _project_id: &str,
    skill: &AgentSkillMetadata,
) -> (PathBuf, PathBuf) {
    let local_dir = paths.storage_dir.join("skills").join(&skill.id);
    let local_path = local_dir.join("SKILL.md");
    (local_dir, local_path)
}

pub(crate) fn custom_writing_skills_dir(paths: &MyOpenPanelsPaths) -> PathBuf {
    paths.storage_dir.join("skills")
}

fn wiki_summary(bootstrap: &ProjectBootstrap, selection: Option<&Value>) -> Value {
    let state = bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
        .map(|snapshot| &snapshot.state)
        .unwrap_or(&bootstrap.state);
    let tasks = state
        .get("tasks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|task| {
            task.get("status")
                .and_then(Value::as_str)
                .is_some_and(|status| ["queued", "claimed", "running", "failed"].contains(&status))
        })
        .collect::<Vec<_>>();
    let next_task = tasks
        .iter()
        .find(|task| task.get("status").and_then(Value::as_str) == Some("queued"))
        .or_else(|| {
            tasks
                .iter()
                .find(|task| task.get("status").and_then(Value::as_str) == Some("failed"))
        })
        .or_else(|| tasks.first())
        .cloned();
    let active_space_id = state
        .get("activeWikiSpaceId")
        .and_then(Value::as_str)
        .unwrap_or("wiki:default");
    let active_space = state
        .get("wikiSpaces")
        .and_then(Value::as_array)
        .and_then(|spaces| {
            spaces
                .iter()
                .find(|space| space.get("id").and_then(Value::as_str) == Some(active_space_id))
                .or_else(|| spaces.first())
        });
    let selected_documents = selection
        .and_then(|value| value.get("selectedRawDocuments"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|document| {
            json!({
                "documentId": document.get("id").cloned().unwrap_or(Value::Null),
                "title": document.get("title").cloned().unwrap_or(Value::Null),
                "mimeType": document.get("mimeType").cloned().unwrap_or(Value::Null),
                "markdownVersion": document.get("markdownVersion").cloned().unwrap_or(Value::Null),
                "originalFilePath": document.get("originalFilePath").cloned().unwrap_or(Value::Null),
                "markdownFilePath": document.get("markdownFilePath").cloned().unwrap_or(Value::Null),
                "originalAccess": document.get("originalAccess").cloned().unwrap_or(Value::Null),
                "markdownAccess": document.get("markdownAccess").cloned().unwrap_or(Value::Null),
            })
        })
        .collect::<Vec<_>>();
    let selected_generated_documents = selection
        .and_then(|value| value.get("selectedGeneratedDocuments"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|document| {
            json!({
                "documentId": document.get("id").cloned().unwrap_or(Value::Null),
                "title": document.get("title").cloned().unwrap_or(Value::Null),
                "format": document.get("format").cloned().unwrap_or(Value::Null),
                "contentVersion": document.get("contentVersion").cloned().unwrap_or(Value::Null),
                "contentFilePath": document.get("contentFilePath").cloned().unwrap_or(Value::Null),
                "contentAccess": document.get("contentAccess").cloned().unwrap_or(Value::Null),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "agentSkillId": selected_agent_skill_id(state),
        "nextTaskAgentSkillId": next_task.as_ref().and_then(|task| task.get("agentSkillId")).and_then(Value::as_str).unwrap_or_else(|| selected_agent_skill_id(state)),
        "available": state.get("wikiSpaces").and_then(Value::as_array).is_some_and(|spaces| !spaces.is_empty()),
        "selected": selection.and_then(|value| value.get("selection")).and_then(|value| value.get("isWikiSelected")).or_else(|| selection.and_then(|value| value.get("isWikiSelected"))).and_then(Value::as_bool).unwrap_or(false),
        "wikiSpaceId": selection.and_then(|value| value.get("wiki")).and_then(|value| value.get("wikiSpaceId")).cloned().unwrap_or_else(|| json!(active_space_id)),
        "wikiTitle": selection.and_then(|value| value.get("wiki")).and_then(|value| value.get("title")).cloned().or_else(|| active_space.and_then(|space| space.get("title")).cloned()).unwrap_or_else(|| json!("Wiki")),
        "pageCount": selection.and_then(|value| value.get("wiki")).and_then(|value| value.get("pageCount")).cloned().unwrap_or_else(|| json!(active_space.and_then(|space| space.get("pageIndex")).and_then(Value::as_array).map(Vec::len).unwrap_or(0))),
        "querySkillId": WIKI_PANEL_SKILL_ID,
        "localAccess": selection.and_then(|value| value.get("wiki")).and_then(|value| value.get("localAccess")).cloned().unwrap_or(Value::Null),
        "selectedRawDocumentCount": selected_documents.len(),
        "selectedRawDocuments": selected_documents,
        "selectedGeneratedDocumentCount": selected_generated_documents.len(),
        "selectedGeneratedDocuments": selected_generated_documents,
        "nextTask": next_task,
        "pendingTaskCount": tasks.len(),
    })
}

fn canvas_summary(selection: Option<&crate::selection::SelectionPayload>) -> Value {
    let is_explicit_selection = selection
        .and_then(|selection| selection.selection.get("isExplicitSelection"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let selected_shapes = selection
        .and_then(|selection| selection.selection.get("selectedShapes"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let selected_ids = selection
        .and_then(|selection| selection.selection.get("selectedShapeIds"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    json!({
        "fallback": selection.and_then(|selection| selection.selection.get("fallback")).and_then(Value::as_str),
        "hasSelectedImageAsset": is_explicit_selection && selection.and_then(|selection| selection.selection.get("assetRef")).and_then(Value::as_str).is_some(),
        "hasSelection": is_explicit_selection && (selected_shapes > 0 || selected_ids > 0),
        "isExplicitSelection": is_explicit_selection,
        "selectedShapeCount": if is_explicit_selection { if selected_shapes > 0 { selected_shapes } else { selected_ids } } else { 0 },
    })
}

fn next_project_task(bootstrap: &ProjectBootstrap) -> Option<&Value> {
    bootstrap
        .tasks
        .iter()
        .filter(|task| task.get("ready").and_then(Value::as_bool).unwrap_or(false))
        .find(|task| task.get("status").and_then(Value::as_str) == Some("queued"))
        .or_else(|| {
            bootstrap
                .tasks
                .iter()
                .filter(|task| task.get("ready").and_then(Value::as_bool).unwrap_or(false))
                .find(|task| task.get("status").and_then(Value::as_str) == Some("failed"))
        })
}

fn next_project_task_for_queue<'a>(
    bootstrap: &'a ProjectBootstrap,
    queue: &str,
) -> Option<&'a Value> {
    bootstrap
        .tasks
        .iter()
        .filter(|task| task.get("ready").and_then(Value::as_bool).unwrap_or(false))
        .filter(|task| task.get("queue").and_then(Value::as_str) == Some(queue))
        .find(|task| task.get("status").and_then(Value::as_str) == Some("queued"))
        .or_else(|| {
            bootstrap
                .tasks
                .iter()
                .filter(|task| task.get("ready").and_then(Value::as_bool).unwrap_or(false))
                .filter(|task| task.get("queue").and_then(Value::as_str) == Some(queue))
                .find(|task| task.get("status").and_then(Value::as_str) == Some("failed"))
        })
}

fn render_current_context(
    bootstrap: &ProjectBootstrap,
    selection: Option<&crate::selection::SelectionPayload>,
    wiki_selection: Option<&Value>,
    task: Option<&Value>,
) -> String {
    let wiki = wiki_summary(bootstrap, wiki_selection);
    let selected_shape_count = canvas_summary(selection)["selectedShapeCount"]
        .as_u64()
        .unwrap_or(0);
    let mut lines = vec![
        format!(
            "- project: {} ({})",
            bootstrap.project.title, bootstrap.project.id
        ),
        format!(
            "- active panel: {} ({})",
            bootstrap.active_panel_kind.as_str(),
            bootstrap.panel.title
        ),
        format!(
            "- wiki agent skill: {}",
            task.and_then(|value| value.get("agentSkillId"))
                .and_then(Value::as_str)
                .unwrap_or_else(|| wiki["agentSkillId"].as_str().unwrap_or("karpathy-llm-wiki"))
        ),
        format!(
            "- wiki selected as context: {}",
            wiki["selected"].as_bool().unwrap_or(false)
        ),
        format!(
            "- selected raw document count: {}",
            wiki["selectedRawDocumentCount"].as_u64().unwrap_or(0)
        ),
        format!("- canvas selected shape count: {selected_shape_count}"),
    ];
    if let Some(access) = wiki.get("localAccess").filter(|value| value.is_object()) {
        lines.push(format!(
            "- wiki local access: status={}, root={}, manifest={}",
            access.get("status").and_then(Value::as_str).unwrap_or("unavailable"),
            access.get("rootPath").and_then(Value::as_str).unwrap_or("none"),
            access
                .get("manifestFilePath")
                .and_then(Value::as_str)
                .unwrap_or("none")
        ));
        if let Some(argv) = access
            .pointer("/materializeAction/argv")
            .and_then(Value::as_array)
        {
            let command = argv
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            lines.push(format!("- wiki materialize command: `{command}`"));
        }
    }
    for document in wiki["selectedRawDocuments"]
        .as_array()
        .into_iter()
        .flatten()
    {
        lines.push(format!(
            "- selected raw document: {} ({}); markdown status={}; markdown path={}; original path={}",
            document.get("title").and_then(Value::as_str).unwrap_or("untitled"),
            document.get("documentId").and_then(Value::as_str).unwrap_or("unknown"),
            document.pointer("/markdownAccess/status").and_then(Value::as_str).unwrap_or("unavailable"),
            document.get("markdownFilePath").and_then(Value::as_str).unwrap_or("none"),
            document.get("originalFilePath").and_then(Value::as_str).unwrap_or("none")
        ));
    }
    for document in wiki["selectedGeneratedDocuments"]
        .as_array()
        .into_iter()
        .flatten()
    {
        lines.push(format!(
            "- selected generated document: {} ({}); content status={}; content path={}",
            document.get("title").and_then(Value::as_str).unwrap_or("untitled"),
            document.get("documentId").and_then(Value::as_str).unwrap_or("unknown"),
            document.pointer("/contentAccess/status").and_then(Value::as_str).unwrap_or("unavailable"),
            document.get("contentFilePath").and_then(Value::as_str).unwrap_or("none")
        ));
    }
    if let Some(task) = task {
        lines.push(format!("- task id: {}", task["id"].as_str().unwrap_or("")));
        lines.push(format!(
            "- task type: {}",
            task["type"].as_str().unwrap_or("")
        ));
        lines.push(format!(
            "- task status: {}",
            task["status"].as_str().unwrap_or("")
        ));
        lines.push(format!(
            "- document id: {}",
            task["documentId"].as_str().unwrap_or("none")
        ));
        lines.push(format!(
            "- wiki space id: {}",
            task["wikiSpaceId"].as_str().unwrap_or("none")
        ));
        if let Some(skill_id) = task.get("writingSkillId").and_then(Value::as_str) {
            lines.push(format!("- writing skill: {skill_id}"));
        }
    }
    lines.join("\n")
}

fn find_wiki_task(bootstrap: &ProjectBootstrap, task_id: &str) -> Option<Value> {
    bootstrap
        .tasks
        .iter()
        .find(|task| task.get("id").and_then(Value::as_str) == Some(task_id))
        .map(|task| {
            let mut normalized = task.as_object().cloned().unwrap_or_default();
            for field in ["input", "source"] {
                if let Some(values) = normalized
                    .remove(field)
                    .and_then(|value| value.as_object().cloned())
                {
                    normalized.extend(values);
                }
            }
            Value::Object(normalized)
        })
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
