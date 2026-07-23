pub(crate) fn validate_portable_writing_skill(
    source: &str,
    file_name: &str,
    expected_id: &str,
) -> Result<AgentSkill, CliError> {
    let skill = parse_portable_skill(source, file_name)?;
    if skill.metadata.id != expected_id || portable_skill_mentions_platform(source) {
        return Err(CliError::with_code(
            "writing_skill_file_invalid",
            "Writing Skill must be portable and match the requested Skill id.",
        ));
    }
    Ok(skill)
}

pub(crate) fn portable_skill_mentions_platform(source: &str) -> bool {
    let normalized = source.to_ascii_lowercase();
    [
        "myopenpanels",
        "my open panels",
        "--task-id",
        "agent bootstrap",
        "agent skill read",
        "writing skill install",
        "operation complete",
        "task.claim",
        "task.heartbeat",
        "task.complete",
        "task.fail",
        "bridge-managed",
    ]
    .iter()
    .any(|forbidden| normalized.contains(forbidden))
}

pub(crate) fn custom_agent_skill_from_source(
    source: &str,
    file_name: &str,
    manifest: &Value,
) -> Result<AgentSkill, CliError> {
    let skill_id = manifest
        .get("skillId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let name = manifest
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if manifest.get("source").and_then(Value::as_str) != Some("custom")
        || skill_id.is_empty()
        || name.is_empty()
    {
        return Err(CliError::with_code(
            "invalid_custom_skill",
            format!("Custom Skill manifest is invalid: {file_name}"),
        ));
    }
    let mut skill = external_custom_skill_from_source(source, file_name, skill_id)?;
    let module_kinds = manifest
        .pointer("/binding/moduleKinds")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            CliError::with_code(
                "invalid_custom_skill",
                format!("Custom Skill has no module binding: {skill_id}"),
            )
        })?;
    let mut applies_to = Vec::new();
    let mut task_types = Vec::new();
    for module_kind in module_kinds {
        let module_kind = module_kind.as_str().ok_or_else(|| {
            CliError::with_code(
                "invalid_custom_skill",
                format!("Custom Skill has an invalid module binding: {skill_id}"),
            )
        })?;
        let (panel_kind, tasks): (&str, &[&str]) = match module_kind {
            "wiki-update" => ("wiki", &["ingest_markdown_into_wiki", "maintain_wiki"]),
            "writing" => ("writing", &["write_my_document"]),
            "writing-distillation" | "writing-refinement" => {
                ("writing", &["distill_writing_skill"])
            }
            "release" => ("publishing", &["release_xiaohongshu"]),
            "publication-cover" => ("typesetting", &[crate::publication::COVER_TASK_TYPE]),
            "publication-title" => ("typesetting", &[crate::publication::TITLE_TASK_TYPE]),
            "publication-layout" => ("typesetting", &[crate::publication::LAYOUT_TASK_TYPE]),
            _ => {
                return Err(CliError::with_code(
                    "invalid_custom_skill",
                    format!("Custom Skill has an invalid module binding: {module_kind}"),
                ));
            }
        };
        if !applies_to.iter().any(|value| value == panel_kind) {
            applies_to.push(panel_kind.to_owned());
        }
        for task in tasks {
            if !task_types.iter().any(|value| value == task) {
                task_types.push((*task).to_owned());
            }
        }
    }
    if skill.metadata.id == skill_id {
        skill.metadata.applies_to = applies_to;
        skill.metadata.load_when = vec![format!("The current task selected {name}.")];
        skill.metadata.source = "custom".to_owned();
        skill.metadata.task_types = task_types;
        skill.metadata.name = name.to_owned();
        skill.metadata.tokens = "short".to_owned();
        return Ok(skill);
    }
    Err(CliError::with_code(
        "invalid_custom_skill",
        format!("Custom Skill package is invalid: {skill_id}"),
    ))
}

fn external_custom_skill_from_source(
    source: &str,
    file_name: &str,
    skill_id: &str,
) -> Result<AgentSkill, CliError> {
    let (frontmatter, body) = split_skill(source, file_name)?;
    let name = scalar(&frontmatter, "name")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CliError::new(format!("Custom Skill requires name: {file_name}")))?;
    let description = scalar(&frontmatter, "description")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CliError::new(format!("Custom Skill requires description: {file_name}")))?;
    if body.trim().is_empty() {
        return Err(CliError::new(format!(
            "Custom Skill body cannot be empty: {file_name}"
        )));
    }
    Ok(AgentSkill {
        metadata: AgentSkillMetadata {
            applies_to: Vec::new(),
            description,
            id: skill_id.to_owned(),
            load_when: Vec::new(),
            requires_commands: Vec::new(),
            source: "custom".to_owned(),
            task_types: Vec::new(),
            name,
            tokens: "short".to_owned(),
        },
        body,
    })
}
