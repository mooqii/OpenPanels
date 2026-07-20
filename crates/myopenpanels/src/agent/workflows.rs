#[derive(Debug, Clone)]
struct AgentWorkflow {
    registration: AgentWorkflowRegistration,
    skill_id: String,
}

fn load_agent_workflows() -> Result<Vec<AgentWorkflow>, CliError> {
    let registry: BuiltinSkillRegistry =
        serde_json::from_str(BUILTIN_SKILL_REGISTRY).map_err(to_cli_error)?;
    if registry.schema_version != 2 {
        return Err(CliError::new(format!(
            "Unsupported built-in Skill registry schema: {}",
            registry.schema_version
        )));
    }

    let mut keys = BTreeSet::new();
    let mut workflows = Vec::new();
    for skill in registry.system_skills {
        for workflow in &skill.workflows {
            validate_agent_workflow(&skill, workflow)?;
            if !keys.insert(workflow.key.clone()) {
                return Err(CliError::with_code(
                    "duplicate_agent_workflow",
                    format!("Duplicate Agent Workflow key: {}", workflow.key),
                ));
            }
            workflows.push(AgentWorkflow {
                registration: workflow.clone(),
                skill_id: skill.id.clone(),
            });
        }
    }
    workflows.sort_by(|left, right| left.registration.key.cmp(&right.registration.key));
    Ok(workflows)
}

fn validate_agent_workflow(
    skill: &BuiltinSkillRegistration,
    workflow: &AgentWorkflowRegistration,
) -> Result<(), CliError> {
    if workflow.key.trim().is_empty()
        || workflow.description.trim().is_empty()
        || !matches!(
            workflow.execution_mode.as_str(),
            "bootstrap" | "handoff-only"
        )
        || !matches!(
            workflow.selection_policy.as_str(),
            "none" | "summary" | "optional-detail" | "active-detail" | "explicit-detail"
        )
    {
        return Err(CliError::with_code(
            "agent_workflow_invalid",
            format!("Agent Workflow registration is invalid: {}", workflow.key),
        ));
    }
    let panel_kind = workflow
        .panel_kind
        .as_deref()
        .map(|kind| {
            PanelKind::parse(kind).ok_or_else(|| {
                CliError::with_code(
                    "agent_workflow_invalid",
                    format!(
                        "Agent Workflow {} has an invalid panel kind: {kind}",
                        workflow.key
                    ),
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
                "agent_workflow_invalid",
                format!(
                    "Agent Workflow {} targets {panel_kind}, but Skill {} does not.",
                    workflow.key, skill.id
                ),
            ));
        }
    }
    let reference = Path::new(&workflow.reference);
    if workflow.reference.trim().is_empty()
        || reference.is_absolute()
        || reference
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(CliError::with_code(
            "agent_workflow_invalid",
            format!("Agent Workflow reference is invalid: {}", workflow.key),
        ));
    }
    let embedded_reference = Path::new(&skill.package_dir).join(reference);
    if SYSTEM_SKILLS.get_file(&embedded_reference).is_none() {
        return Err(CliError::with_code(
            "agent_workflow_reference_not_found",
            format!(
                "Agent Workflow {} reference is missing: {}",
                workflow.key,
                embedded_reference.display()
            ),
        ));
    }
    crate::cli::registry::descriptors_for_intents(&workflow.command_intents)?;
    Ok(())
}

fn agent_workflow_bootstrap(
    paths: &MyOpenPanelsPaths,
    cli_version: &str,
    visible: &ProjectBootstrap,
    workflow_key: &str,
) -> Result<Value, CliError> {
    sync_builtin_agent_skills(paths)?;
    let workflow_key = workflow_key.trim();
    let workflows = load_agent_workflows()?;
    let workflow = workflows
        .iter()
        .find(|workflow| workflow.registration.key == workflow_key)
        .ok_or_else(|| {
            CliError::with_recovery(
                "agent_workflow_not_found",
                format!("Agent Workflow not found: {workflow_key}"),
                true,
                "Run the generic Agent Bootstrap and select a supported Workflow from current discovery context.",
            )
            .with_recovery_action(generic_bootstrap_recovery_action(paths))
        })?;
    if workflow.registration.execution_mode == "handoff-only" {
        return Err(CliError::with_recovery(
            "agent_workflow_not_bootstrappable",
            format!(
                "Agent Workflow {} is executed only from its exact Task handoff.",
                workflow.registration.key
            ),
            false,
            "Execute the unchanged task scope or claimed Task handoff command. Do not replace it with Agent Bootstrap.",
        ));
    }

    let requested_panel_kind = workflow
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
    let panel_skill = required_agent_skill(&skills, &workflow.skill_id)?;
    let (_, skill_path) = agent_skill_local_paths(paths, &visible.project.id, &panel_skill.metadata);
    let skill_dir = skill_path
        .parent()
        .ok_or_else(|| CliError::new("Agent Workflow Skill path has no parent directory."))?;
    let reference_path = skill_dir.join(&workflow.registration.reference);
    if !reference_path.is_file() {
        return Err(CliError::with_code(
            "agent_workflow_reference_not_found",
            format!(
                "Agent Workflow reference is unavailable: {}",
                reference_path.display()
            ),
        ));
    }

    let (selection, mut blockers) = workflow_selection(paths, visible, target, workflow)?;
    let storage = crate::storage::Storage::open(paths)?;
    let operations = storage.list_agent_operations(Some(&paths.context_id), Some("active"))?;
    let commands = crate::cli::registry::descriptors_for_intents(
        &workflow.registration.command_intents,
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
    let mut required_actions = vec![
        json!({
            "id": format!("skill.{}.body", workflow.skill_id),
            "intent": "agent-host.file.read",
            "executor": "agent-host",
            "kind": "read-file",
            "path": skill_path.display().to_string(),
        }),
        json!({
            "id": format!("workflow.{}.reference", workflow.registration.key),
            "intent": "agent-host.file.read",
            "executor": "agent-host",
            "kind": "read-file",
            "path": reference_path.display().to_string(),
        }),
    ];
    let suggested_actions = workflow_state_actions(paths, visible, workflow, &operations);
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
        "id": workflow.skill_id,
        "role": "panel",
        "localPath": skill_path.display().to_string(),
        "referencePaths": [reference_path.display().to_string()],
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
        "workflowCatalogVersion": AGENT_WORKFLOW_CATALOG_VERSION,
        "commandCatalogVersion": crate::cli::registry::COMMAND_CATALOG_VERSION,
        "cliVersion": cli_version,
        "bootstrapBudget": {
            "maxBytes": MAX_BOOTSTRAP_ENVELOPE_BYTES,
            "unit": "utf8",
        },
        "agentWorkflow": {
            "key": workflow.registration.key,
            "description": workflow.registration.description,
            "executionMode": workflow.registration.execution_mode,
            "panelKind": workflow.registration.panel_kind,
            "selectionPolicy": workflow.registration.selection_policy,
            "skillId": workflow.skill_id,
            "referencePath": reference_path.display().to_string(),
            "commandIntents": workflow.registration.command_intents,
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

fn workflow_selection(
    paths: &MyOpenPanelsPaths,
    visible: &ProjectBootstrap,
    target: Option<&ProjectBootstrap>,
    workflow: &AgentWorkflow,
) -> Result<(Value, Vec<Value>), CliError> {
    let policy = workflow.registration.selection_policy.as_str();
    if policy == "none" {
        let blockers = if target.is_none() && workflow.registration.panel_kind.is_some() {
            vec![json!({
                "code": "target_panel_required",
                "message": format!(
                    "Workflow {} requires an available {} panel.",
                    workflow.registration.key,
                    workflow.registration.panel_kind.as_deref().unwrap_or("target")
                ),
                "expectedPanelKind": workflow.registration.panel_kind,
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
                    "Workflow {} requires an available {} panel.",
                    workflow.registration.key,
                    workflow.registration.panel_kind.as_deref().unwrap_or("target")
                ),
                "expectedPanelKind": workflow.registration.panel_kind,
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
                    "Workflow {} requires the {} panel to be active.",
                    workflow.registration.key,
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
                "Workflow {} requires an explicit user selection.",
                workflow.registration.key
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

fn workflow_state_actions(
    paths: &MyOpenPanelsPaths,
    bootstrap: &ProjectBootstrap,
    workflow: &AgentWorkflow,
    operations: &[Value],
) -> Vec<Value> {
    let mut actions = Vec::new();
    if workflow.registration.key == "task.queue.inspect" {
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
    if workflow
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
