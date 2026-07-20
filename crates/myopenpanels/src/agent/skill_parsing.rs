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

pub(crate) fn render_portable_skill(skill: &AgentSkill) -> String {
    format!(
        "---\nname: {}\ndescription: {}\n---\n\n{}\n",
        skill.metadata.id,
        skill.metadata.description.trim(),
        skill.body.trim()
    )
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

pub(crate) fn custom_writing_skill_from_source(
    source: &str,
    file_name: &str,
    manifest: &Value,
) -> Result<AgentSkill, CliError> {
    let schema_version = manifest
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let skill_id = manifest
        .get("skillId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let name = manifest
        .get(if schema_version == 1 { "title" } else { "name" })
        .and_then(Value::as_str)
        .unwrap_or_default();
    if manifest.get("source").and_then(Value::as_str) != Some("custom")
        || skill_id.is_empty()
        || name.is_empty()
    {
        return Err(CliError::with_code(
            "invalid_custom_skill",
            format!("Custom Writing Skill manifest is invalid: {file_name}"),
        ));
    }
    if schema_version == 1 {
        let skill = parse_skill(source, file_name)?;
        if skill.metadata.id == skill_id
            && skill.metadata.name == name
            && skill.metadata.source == "custom"
            && skill.metadata.applies_to == ["writing"]
            && skill.metadata.task_types == ["generate_document"]
            && skill.metadata.requires_commands.is_empty()
        {
            return Ok(skill);
        }
    } else if schema_version == 2 || schema_version == 3 {
        let mut skill = if schema_version == 3 {
            external_custom_skill_from_source(source, file_name, skill_id)?
        } else {
            parse_portable_skill(source, file_name)?
        };
        let binding = manifest.get("binding").unwrap_or(&Value::Null);
        if schema_version == 3 {
            let module_kinds = binding
                .get("moduleKinds")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let mut applies_to = Vec::new();
            let mut task_types = Vec::new();
            for module_kind in module_kinds.iter().filter_map(Value::as_str) {
                let (panel_kind, tasks): (&str, &[&str]) = match module_kind {
                    "wiki-update" => {
                        ("wiki", &["ingest_markdown_into_wiki", "maintain_wiki"])
                    }
                    "writing" => ("writing", &["generate_document"]),
                    "writing-refinement" => ("writing", &["refine_writing_skill"]),
                    "publishing-xiaohongshu" => {
                        ("publishing", &["publish_xiaohongshu_note"])
                    }
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
            if module_kinds.is_empty() || skill.metadata.id != skill_id {
                return Err(CliError::with_code(
                    "invalid_custom_skill",
                    format!("Custom Skill binding is invalid: {skill_id}"),
                ));
            }
            skill.metadata.applies_to = applies_to;
            skill.metadata.load_when = vec![format!("The current task selected {name}.")];
            skill.metadata.source = "custom".to_owned();
            skill.metadata.task_types = task_types;
            skill.metadata.name = name.to_owned();
            skill.metadata.tokens = "short".to_owned();
            return Ok(skill);
        }
        let applies_to = binding
            .get("appliesTo")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let task_types = binding
            .get("taskTypes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if skill.metadata.id == skill_id
            && applies_to == [Value::String("writing".to_owned())]
            && task_types == [Value::String("generate_document".to_owned())]
        {
            skill.metadata.applies_to = vec!["writing".to_owned()];
            skill.metadata.load_when = vec![format!("The writing request selected {name}.")];
            skill.metadata.source = "custom".to_owned();
            skill.metadata.task_types = vec!["generate_document".to_owned()];
            skill.metadata.name = name.to_owned();
            skill.metadata.tokens = "short".to_owned();
            return Ok(skill);
        }
    }
    Err(CliError::with_code(
        "invalid_custom_skill",
        format!("Custom Writing Skill package is invalid: {skill_id}"),
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
