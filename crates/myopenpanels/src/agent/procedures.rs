#[derive(Debug, Clone)]
struct AgentProcedure {
    registration: AgentProcedureRegistration,
    skill_id: String,
}

struct AgentProcedureCatalog {
    procedures: Vec<AgentProcedure>,
    task_handoff_keys: BTreeSet<String>,
}

fn load_agent_procedures() -> Result<AgentProcedureCatalog, CliError> {
    let registry: BuiltinSkillRegistry =
        serde_json::from_str(BUILTIN_SKILL_REGISTRY).map_err(to_cli_error)?;
    if registry.schema_version != 4 {
        return Err(CliError::new(format!(
            "Unsupported built-in Skill registry schema: {}",
            registry.schema_version
        )));
    }

    let mut keys = BTreeSet::new();
    let mut procedures = Vec::new();
    let mut task_handoff_keys = BTreeSet::new();
    for skill in registry.system_skills {
        for procedure in &skill.procedures {
            validate_agent_procedure(&skill, procedure)?;
            if !keys.insert(procedure.key.clone()) {
                return Err(CliError::with_code(
                    "duplicate_agent_procedure",
                    format!("Duplicate Agent Procedure key: {}", procedure.key),
                ));
            }
            procedures.push(AgentProcedure {
                registration: procedure.clone(),
                skill_id: skill.id.clone(),
            });
        }
        for handoff in &skill.task_handoffs {
            validate_task_handoff(&skill, handoff)?;
            if !keys.insert(handoff.key.clone()) {
                return Err(CliError::with_code(
                    "duplicate_agent_route",
                    format!("Duplicate Agent route key: {}", handoff.key),
                ));
            }
            task_handoff_keys.insert(handoff.key.clone());
        }
    }
    procedures.sort_by(|left, right| left.registration.key.cmp(&right.registration.key));
    Ok(AgentProcedureCatalog {
        procedures,
        task_handoff_keys,
    })
}

fn procedure_command_intents_for_panel(panel_kind: PanelKind) -> Result<Vec<String>, CliError> {
    let panel_kind = panel_kind.as_str();
    Ok(load_agent_procedures()?
        .procedures
        .into_iter()
        .filter(|procedure| procedure.registration.panel_kind.as_deref() == Some(panel_kind))
        .flat_map(|procedure| procedure.registration.command_intents)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

fn panel_contract_reference(panel_kind: PanelKind) -> Option<&'static str> {
    match panel_kind {
        PanelKind::Canvas => Some("references/canvas-contract.md"),
        PanelKind::Wiki => Some("references/wiki-contract.md"),
        PanelKind::Writing => Some("references/writing-contract.md"),
        _ => None,
    }
}

fn validate_agent_procedure(
    skill: &BuiltinSkillRegistration,
    procedure: &AgentProcedureRegistration,
) -> Result<(), CliError> {
    if procedure.key.trim().is_empty()
        || procedure.description.trim().is_empty()
        || !matches!(
            procedure.selection_policy.as_str(),
            "none" | "summary" | "optional-detail" | "active-detail" | "explicit-detail"
        )
    {
        return Err(CliError::with_code(
            "agent_procedure_invalid",
            format!("Agent Procedure registration is invalid: {}", procedure.key),
        ));
    }
    validate_agent_route(
        skill,
        &procedure.key,
        &procedure.description,
        procedure.panel_kind.as_deref(),
        &procedure.references,
        &procedure.command_intents,
        "agent_procedure_invalid",
        "Agent Procedure",
    )
}

fn validate_task_handoff(
    skill: &BuiltinSkillRegistration,
    handoff: &TaskHandoffRegistration,
) -> Result<(), CliError> {
    validate_agent_route(
        skill,
        &handoff.key,
        &handoff.description,
        handoff.panel_kind.as_deref(),
        std::slice::from_ref(&handoff.reference),
        &handoff.command_intents,
        "task_handoff_invalid",
        "Task Handoff",
    )
}

#[allow(clippy::too_many_arguments)]
fn validate_agent_route(
    skill: &BuiltinSkillRegistration,
    key: &str,
    description: &str,
    panel_kind: Option<&str>,
    references: &[String],
    command_intents: &[String],
    invalid_code: &str,
    label: &str,
) -> Result<(), CliError> {
    if key.trim().is_empty()
        || description.trim().is_empty()
        || references.is_empty()
        || command_intents.is_empty()
    {
        return Err(CliError::with_code(
            invalid_code,
            format!("{label} registration is invalid: {key}"),
        ));
    }
    let panel_kind = panel_kind
        .map(|kind| {
            PanelKind::parse(kind).ok_or_else(|| {
                CliError::with_code(
                    invalid_code,
                    format!("{label} {key} has an invalid panel kind: {kind}"),
                )
            })
        })
        .transpose()?;
    if let Some(panel_kind) = panel_kind {
        let panel_kind = panel_kind.as_str();
        if !skill
            .applies_to
            .iter()
            .any(|candidate| candidate == panel_kind || candidate == "any")
        {
            return Err(CliError::with_code(
                invalid_code,
                format!(
                    "{label} {key} targets {panel_kind}, but Skill {} does not.",
                    skill.id
                ),
            ));
        }
    }
    let mut seen_references = BTreeSet::new();
    for reference in references {
        let reference_path = Path::new(reference);
        if reference.trim().is_empty()
            || reference_path.is_absolute()
            || reference_path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
            || !seen_references.insert(reference)
        {
            return Err(CliError::with_code(
                invalid_code,
                format!("{label} reference is invalid or duplicated: {key}"),
            ));
        }
        let embedded_reference = Path::new(&skill.package_dir).join(reference_path);
        if SYSTEM_SKILLS.get_file(&embedded_reference).is_none() {
            return Err(CliError::with_code(
                if label == "Agent Procedure" {
                    "agent_procedure_reference_not_found"
                } else {
                    "task_handoff_reference_not_found"
                },
                format!(
                    "{label} {key} reference is missing: {}",
                    embedded_reference.display(),
                ),
            ));
        }
    }
    crate::cli::registry::descriptors_for_intents(command_intents)?;
    Ok(())
}

fn agent_procedure_bootstrap(
    paths: &MyOpenPanelsPaths,
    cli_version: &str,
    visible: &ProjectBootstrap,
    procedure_key: &str,
) -> Result<Value, CliError> {
    sync_builtin_agent_skills(paths)?;
    let procedure_key = procedure_key.trim();
    let catalog = load_agent_procedures()?;
    let procedure = catalog
        .procedures
        .iter()
        .find(|procedure| procedure.registration.key == procedure_key)
        .ok_or_else(|| {
            if catalog.task_handoff_keys.contains(procedure_key) {
                return CliError::with_recovery(
                    "task_handoff_required",
                    format!(
                        "{} is a Task Handoff and cannot be used as an Agent Procedure.",
                        procedure_key
                    ),
                    false,
                    "Execute the unchanged Task Handoff or claimed Task command. Do not replace it with Agent Bootstrap.",
                );
            }
            CliError::with_recovery(
                "agent_procedure_not_found",
                format!("Agent Procedure not found: {procedure_key}"),
                true,
                "Run the generic Agent Bootstrap and select a supported Procedure from current discovery context.",
            )
            .with_recovery_action(generic_bootstrap_recovery_action(paths))
        })?;

    let requested_panel_kind = procedure
        .registration
        .panel_kind
        .as_deref()
        .and_then(PanelKind::parse);
    let target_owned = requested_panel_kind
        .map(|panel_kind| {
            read_project_bootstrap(
                paths,
                BootstrapRequest {
                    requested_panel_id: None,
                    requested_panel_kind: Some(panel_kind),
                    requested_project_id: Some(visible.project.id.clone()),
                },
            )
        })
        .transpose()?;
    let target = target_owned
        .as_ref()
        .filter(|target| requested_panel_kind == Some(target.active_panel_kind));

    let skills = load_agent_skills(paths, &visible.project.id)?;
    let panel_skill = required_agent_skill(&skills, &procedure.skill_id)?;
    let (_, skill_path) = agent_skill_local_paths(paths, &visible.project.id, &panel_skill.metadata);
    let skill_dir = skill_path
        .parent()
        .ok_or_else(|| CliError::new("Agent Procedure Skill path has no parent directory."))?;
    let reference_paths = procedure
        .registration
        .references
        .iter()
        .map(|reference| skill_dir.join(reference))
        .collect::<Vec<_>>();
    for reference_path in &reference_paths {
        if !reference_path.is_file() {
            return Err(CliError::with_code(
                "agent_procedure_reference_not_found",
                format!(
                    "Agent Procedure reference is unavailable: {}",
                    reference_path.display()
                ),
            ));
        }
    }
    let primary_reference_path = reference_paths
        .last()
        .expect("validated Agent Procedure references");

    let (selection, mut blockers) = procedure_selection(paths, visible, target, procedure)?;
    let storage = crate::storage::Storage::open(paths)?;
    let operations = storage.list_agent_operations(Some(&paths.context_id), Some("active"))?;
    let commands = crate::cli::registry::descriptors_for_intents(
        &procedure.registration.command_intents,
    )?;
    let focus_revision = crate::control::read_focus_revision(paths)?;
    let mut context_truncated = false;
    let panel_context = target
        .filter(|_| requested_panel_kind.is_some())
        .map(|target| {
            bounded_json(
                crate::panel::context_for_bootstrap(target),
                0,
                &mut context_truncated,
            )
        })
        .unwrap_or(Value::Null);
    let mut required_actions = vec![json!({
        "id": format!("skill.{}.body", procedure.skill_id),
        "intent": "agent-host.file.read",
        "executor": "agent-host",
        "kind": "read-file",
        "path": skill_path.display().to_string(),
    })];
    required_actions.extend(reference_paths.iter().enumerate().map(|(index, path)| {
        json!({
            "id": format!("reference.{index}"),
            "intent": "agent-host.file.read",
            "executor": "agent-host",
            "kind": "read-file",
            "path": path.display().to_string(),
        })
    }));
    let suggested_actions = procedure_state_actions(paths, visible, procedure, &operations);
    let available_panel_kinds = visible
        .panels
        .iter()
        .map(|snapshot| snapshot.panel.kind.as_str())
        .collect::<Vec<_>>();
    let target_value = match (requested_panel_kind, target) {
        (Some(_), Some(target)) => json!({
            "projectId": target.project.id,
            "panelId": target.panel.id,
            "panelKind": target.panel.kind,
            "revision": target.revision,
        }),
        (Some(panel_kind), None) => json!({
            "projectId": visible.project.id,
            "panelId": null,
            "panelKind": panel_kind,
            "revision": null,
        }),
        (None, _) => json!({
            "projectId": visible.project.id,
            "panelId": null,
            "panelKind": null,
            "revision": null,
        }),
    };
    let mut skill_entries = vec![json!({
        "id": procedure.skill_id,
        "role": "panel",
        "localPath": skill_path.display().to_string(),
        "referencePaths": reference_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>(),
    })];
    if let Some(target) = target {
        for selected_skill_id in selected_portable_skill_ids(target) {
            let Some(selected_skill) = skills
                .iter()
                .find(|skill| skill.metadata.id == selected_skill_id)
            else {
                blockers.push(json!({
                    "code": "selected_skill_not_found",
                    "message": format!(
                        "The selected portable Skill is unavailable: {selected_skill_id}"
                    ),
                    "skillId": selected_skill_id,
                }));
                continue;
            };
            let (_, selected_skill_path) =
                agent_skill_local_paths(paths, &visible.project.id, &selected_skill.metadata);
            skill_entries.push(json!({
                "id": selected_skill.metadata.id,
                "role": "selected-portable",
                "localPath": selected_skill_path.display().to_string(),
            }));
            required_actions.push(json!({
                "id": format!("skill.{}.body", selected_skill.metadata.id),
                "intent": "agent-host.file.read",
                "executor": "agent-host",
                "kind": "read-file",
                "path": selected_skill_path.display().to_string(),
            }));
        }
    }

    Ok(json!({
        "protocolVersion": AGENT_GUIDANCE_PROTOCOL_VERSION,
        "procedureCatalogVersion": AGENT_PROCEDURE_CATALOG_VERSION,
        "commandCatalogVersion": crate::cli::registry::COMMAND_CATALOG_VERSION,
        "cliVersion": cli_version,
        "bootstrapBudget": {
            "maxBytes": MAX_BOOTSTRAP_ENVELOPE_BYTES,
            "unit": "utf8",
        },
        "agentProcedure": {
            "key": procedure.registration.key,
            "description": procedure.registration.description,
            "panelKind": procedure.registration.panel_kind,
            "selectionPolicy": procedure.registration.selection_policy,
            "skillId": procedure.skill_id,
            "referencePath": primary_reference_path.display().to_string(),
            "referencePaths": reference_paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>(),
            "commandIntents": procedure.registration.command_intents,
        },
        "readiness": if blockers.is_empty() { "ready" } else { "blocked" },
        "blockers": blockers,
        "focus": {
            "focusRevision": focus_revision,
            "projectId": visible.project.id,
            "panelId": visible.panel.id,
            "panelKind": visible.panel.kind,
            "availablePanelKinds": available_panel_kinds,
        },
        "target": target_value,
        "panel": {
            "context": panel_context,
            "contextTruncated": context_truncated,
            "selection": selection,
        },
        "tasks": compact_task_summary(visible),
        "operations": compact_operation_summary(&operations),
        "skills": skill_entries,
        "commands": {
            "catalogVersion": crate::cli::registry::COMMAND_CATALOG_VERSION,
            "items": commands,
        },
        "actions": {
            "required": required_actions,
            "suggested": suggested_actions,
        },
    }))
}

fn procedure_selection(
    paths: &MyOpenPanelsPaths,
    visible: &ProjectBootstrap,
    target: Option<&ProjectBootstrap>,
    procedure: &AgentProcedure,
) -> Result<(Value, Vec<Value>), CliError> {
    let policy = procedure.registration.selection_policy.as_str();
    if policy == "none" {
        let blockers = if target.is_none() && procedure.registration.panel_kind.is_some() {
            vec![json!({
                "code": "target_panel_required",
                "message": format!(
                    "Procedure {} requires an available {} panel.",
                    procedure.registration.key,
                    procedure.registration.panel_kind.as_deref().unwrap_or("target")
                ),
                "expectedPanelKind": procedure.registration.panel_kind,
            })]
        } else {
            Vec::new()
        };
        return Ok((
            json!({
                "policy": policy,
                "available": false,
                "isTargetActive": target.is_some_and(|target| target.panel.id == visible.panel.id),
            }),
            blockers,
        ));
    }

    let Some(target) = target else {
        return Ok((
            json!({
                "policy": policy,
                "available": false,
                "isTargetActive": false,
                "supported": false,
                "isExplicit": false,
                "summary": { "itemCount": 0 },
                "value": null,
            }),
            vec![json!({
                "code": "target_panel_required",
                "message": format!(
                    "Procedure {} requires an available {} panel.",
                    procedure.registration.key,
                    procedure.registration.panel_kind.as_deref().unwrap_or("target")
                ),
                "expectedPanelKind": procedure.registration.panel_kind,
            })],
        ));
    };

    let is_target_active = target.panel.id == visible.panel.id;
    let mut blockers = Vec::new();
    if !is_target_active {
        if matches!(policy, "active-detail" | "explicit-detail") {
            blockers.push(json!({
                "code": "active_panel_required",
                "message": format!(
                    "Procedure {} requires the {} panel to be active.",
                    procedure.registration.key,
                    target.panel.kind.as_str()
                ),
                "expectedPanelKind": target.panel.kind,
                "actualPanelKind": visible.panel.kind,
            }));
        }
        return Ok((
            json!({
                "policy": policy,
                "available": false,
                "isTargetActive": false,
                "supported": false,
                "isExplicit": false,
                "summary": { "itemCount": 0 },
                "value": null,
            }),
            blockers,
        ));
    }

    let payload = crate::panel::read_selection(paths)?;
    let is_explicit = payload.is_explicit;
    if policy == "explicit-detail" && !is_explicit {
        blockers.push(json!({
            "code": "explicit_selection_required",
            "message": format!(
                "Procedure {} requires an explicit user selection.",
                procedure.registration.key
            ),
            "panelKind": target.panel.kind,
        }));
    }
    let value = if policy == "summary" || policy == "explicit-detail" && !is_explicit {
        Value::Null
    } else {
        payload.value
    };
    Ok((
        json!({
            "policy": policy,
            "available": true,
            "isTargetActive": true,
            "supported": payload.supported,
            "selectionKind": payload.selection_kind,
            "isExplicit": is_explicit,
            "updatedAt": payload.updated_at,
            "summary": payload.summary,
            "value": value,
        }),
        blockers,
    ))
}

fn selected_portable_skill_ids(target: &ProjectBootstrap) -> Vec<&str> {
    match target.active_panel_kind {
        PanelKind::Wiki => vec![selected_agent_skill_id(&target.state)],
        PanelKind::Writing => match target.state["mode"].as_str() {
            Some("revise") => target.state["selectedRevisionWritingSkillId"]
                .as_str()
                .into_iter()
                .collect(),
            Some("refine") => target.state["selectedRefinementSkillId"]
                .as_str()
                .into_iter()
                .collect(),
            _ => target.state["selectedCreateWritingSkillIds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect(),
        },
        _ => Vec::new(),
    }
}

fn procedure_state_actions(
    paths: &MyOpenPanelsPaths,
    bootstrap: &ProjectBootstrap,
    procedure: &AgentProcedure,
    operations: &[Value],
) -> Vec<Value> {
    let mut actions = Vec::new();
    if procedure.registration.key == "task.queue.inspect" {
        if let Some(task_id) = next_task_id(bootstrap) {
            actions.push(project_command_action(
                paths,
                "task.read",
                vec![
                    "--task-id".to_owned(),
                    task_id.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            ));
        }
    }
    if procedure
        .registration
        .command_intents
        .iter()
        .any(|intent| intent.starts_with("operation."))
    {
        actions.extend(operations.iter().take(3).filter_map(|operation| {
            let operation_id = operation.get("id").and_then(Value::as_str)?;
            Some(project_command_action(
                paths,
                "operation.read",
                vec![
                    "--operation-id".to_owned(),
                    operation_id.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            ))
        }));
    }
    actions
}

fn generic_bootstrap_recovery_action(
    paths: &MyOpenPanelsPaths,
) -> crate::error::CliRecoveryAction {
    crate::error::CliRecoveryAction::cli_intent(
        "agent.bootstrap.read",
        [
            "agent".to_owned(),
            "bootstrap".to_owned(),
            "--project-dir".to_owned(),
            paths.project_dir.display().to_string(),
            "--format".to_owned(),
            "json".to_owned(),
        ],
    )
}
