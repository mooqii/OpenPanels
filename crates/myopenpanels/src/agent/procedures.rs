#[derive(Debug, Clone)]
struct AgentProcedureRegistration {
    command_intents: Vec<String>,
    description: String,
    key: String,
    local_skill: LocalSkillPolicy,
    module_key: String,
    panel_kind: Option<String>,
    references: Vec<String>,
    selection_policy: String,
    system_skill_id: String,
}

#[derive(Debug, Clone)]
struct AgentProcedure {
    registration: AgentProcedureRegistration,
    skill_id: String,
}

struct AgentProcedureCatalog {
    procedures: Vec<AgentProcedure>,
}

fn load_agent_procedures() -> Result<AgentProcedureCatalog, CliError> {
    let registry: BuiltinSkillRegistry =
        serde_json::from_str(BUILTIN_SKILL_REGISTRY).map_err(to_cli_error)?;
    let capability_catalog = capability_catalog()?;
    let system_skills = registry
        .system_skills
        .into_iter()
        .map(|skill| (skill.id.clone(), skill))
        .collect::<BTreeMap<_, _>>();
    let handler_keys = crate::bridge::task_handler_keys()
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut used_handler_keys = BTreeSet::new();
    let mut procedures = Vec::new();

    for capability in &capability_catalog.capabilities {
        let skill = system_skills
            .get(&capability.platform_contract.system_skill_id)
            .ok_or_else(|| {
                CliError::with_code(
                    "capability_catalog_invalid",
                    format!(
                        "Capability {} references unknown system Skill {}.",
                        capability.key, capability.platform_contract.system_skill_id
                    ),
                )
            })?;
        validate_capability_registration(skill, capability)?;
        match &capability.invocation {
            CapabilityInvocation::Procedure {
                selection_policy,
                command_intents,
            } => {
                crate::cli::registry::descriptors_for_intents(command_intents)?;
                let registration = AgentProcedureRegistration {
                    command_intents: command_intents.clone(),
                    description: capability.description.clone(),
                    key: capability.key.clone(),
                    local_skill: capability.local_skill.clone(),
                    module_key: capability.module_key.clone(),
                    panel_kind: capability.panel_kind.clone(),
                    references: capability.platform_contract.references.clone(),
                    selection_policy: selection_policy.clone(),
                    system_skill_id: capability.platform_contract.system_skill_id.clone(),
                };
                procedures.push(AgentProcedure {
                    skill_id: registration.system_skill_id.clone(),
                    registration,
                });
            }
            CapabilityInvocation::Task { routes } => {
                for route in routes {
                    if !handler_keys.contains(&route.handler_key) {
                        return Err(CliError::with_code(
                            "task_handler_not_found",
                            format!(
                                "Task Capability {} references unknown Handler {}.",
                                capability.key, route.handler_key
                            ),
                        ));
                    }
                    used_handler_keys.insert(route.handler_key.clone());
                }
            }
            CapabilityInvocation::TaskScope { .. } => {}
        }
    }

    if used_handler_keys != handler_keys {
        let unused = handler_keys
            .difference(&used_handler_keys)
            .cloned()
            .collect::<Vec<_>>();
        return Err(CliError::with_code(
            "task_handler_unregistered",
            format!("Task Handlers are not referenced by the Capability Catalog: {unused:?}."),
        ));
    }

    procedures.sort_by(|left, right| left.registration.key.cmp(&right.registration.key));
    Ok(AgentProcedureCatalog { procedures })
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
        PanelKind::Typesetting => Some("references/publication-contract.md"),
        PanelKind::Publishing => Some("references/release-contract.md"),
    }
}

#[cfg(test)]
fn validate_agent_procedure(
    skill: &BuiltinSkillRegistration,
    procedure: &AgentProcedureRegistration,
) -> Result<(), CliError> {
    crate::cli::registry::descriptors_for_intents(&procedure.command_intents)?;
    validate_capability_contract(
        skill,
        &procedure.key,
        procedure.panel_kind.as_deref(),
        &procedure.references,
    )
}

fn validate_capability_registration(
    skill: &BuiltinSkillRegistration,
    capability: &CapabilityDefinition,
) -> Result<(), CliError> {
    validate_capability_contract(
        skill,
        &capability.key,
        capability.panel_kind.as_deref(),
        &capability.platform_contract.references,
    )
}

fn validate_capability_contract(
    skill: &BuiltinSkillRegistration,
    key: &str,
    panel_kind: Option<&str>,
    references: &[String],
) -> Result<(), CliError> {
    if let Some(panel_kind) = panel_kind {
        if !skill
            .applies_to
            .iter()
            .any(|candidate| candidate == panel_kind || candidate == "any")
        {
            return Err(CliError::with_code(
                "capability_catalog_invalid",
                format!(
                    "Capability {key} targets {panel_kind}, but Skill {} does not.",
                    skill.id
                ),
            ));
        }
    }
    if references.is_empty() {
        return Err(CliError::with_code(
            "capability_catalog_invalid",
            format!("Capability {key} has no platform references."),
        ));
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
                "capability_catalog_invalid",
                format!("Capability {key} has an invalid or duplicate platform reference."),
            ));
        }
        let embedded_reference = Path::new(&skill.package_dir).join(reference_path);
        if SYSTEM_SKILLS.get_file(&embedded_reference).is_none() {
            return Err(CliError::with_code(
                "capability_reference_not_found",
                format!(
                    "Capability {key} reference is missing: {}",
                    embedded_reference.display(),
                ),
            ));
        }
    }
    Ok(())
}

fn agent_procedure_bootstrap(
    paths: &MyOpenPanelsPaths,
    visible: &ProjectBootstrap,
    procedure_key: &str,
) -> Result<Value, CliError> {
    sync_builtin_agent_skills(paths)?;
    let procedure_key = procedure_key.trim();
    let catalog = load_agent_procedures()?;
    let procedure = catalog
        .procedures
        .iter()
        .find(|procedure| procedure.registration.key == procedure_key);
    let Some(procedure) = procedure else {
        let mut fallback = agent_bootstrap(paths, None)?;
        fallback["procedureFallback"] = json!({
            "requestedKey": procedure_key,
            "reason": "agent_procedure_not_found",
        });
        return Ok(fallback);
    };

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
    let references = procedure
        .registration
        .references
        .iter()
        .map(|reference| {
            Ok(json!({
                "path": reference,
                "body": embedded_system_skill_text(&procedure.skill_id, reference)?,
            }))
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    let primary_reference = procedure
        .registration
        .references
        .last()
        .expect("validated Agent Procedure references");

    let selection_target = target.or_else(|| {
        procedure
            .registration
            .panel_kind
            .is_none()
            .then_some(visible)
    });
    let (selection, mut blockers) =
        procedure_selection(paths, visible, selection_target, procedure)?;
    let storage = crate::storage::Storage::open(paths)?;
    let operations = storage.list_direct_operations(Some(&paths.context_id), Some("active"))?;
    let commands = crate::cli::registry::descriptors_for_intents(
        &procedure.registration.command_intents,
    )?;
    let focus_revision = crate::control::read_focus_revision(paths)?;
    let mut context_truncated = false;
    let context_target = target.or_else(|| {
        (procedure.registration.panel_kind.is_none()
            && procedure.registration.selection_policy != "none")
            .then_some(visible)
    });
    let panel_context = context_target
        .map(|target| {
            bounded_json(
                crate::panel::context_for_bootstrap(target),
                0,
                &mut context_truncated,
            )
        })
        .unwrap_or(Value::Null);
    let required_actions = Vec::<Value>::new();
    let suggested_actions = procedure_state_actions(paths, visible, procedure, &operations);
    let available_panel_kinds = visible
        .panels
        .iter()
        .map(|snapshot| snapshot.panel.kind.as_str())
        .collect::<Vec<_>>();
    let target_value = match (requested_panel_kind, target) {
        (Some(_), Some(target)) => json!({
            "kind": "panel",
            "moduleKey": procedure.registration.module_key,
            "projectId": target.project.id,
            "panelId": target.panel.id,
            "panelKind": target.panel.kind,
            "revision": target.revision,
            "resourceVersions": selected_resource_versions(&selection),
        }),
        (Some(panel_kind), None) => json!({
            "kind": "panel",
            "moduleKey": procedure.registration.module_key,
            "projectId": visible.project.id,
            "panelId": null,
            "panelKind": panel_kind,
            "revision": null,
            "resourceVersions": [],
        }),
        (None, _) => json!({
            "kind": "module",
            "moduleKey": procedure.registration.module_key,
            "projectId": visible.project.id,
            "panelId": null,
            "panelKind": null,
            "revision": null,
            "resourceVersions": selected_resource_versions(&selection),
            "selectionSource": if procedure.registration.selection_policy == "none" {
                Value::Null
            } else {
                json!({
                    "panelId": visible.panel.id,
                    "panelKind": visible.panel.kind,
                    "revision": visible.revision,
                })
            },
        }),
    };
    let mut skill_entries = vec![json!({
        "id": procedure.skill_id,
        "role": "system",
        "name": panel_skill.metadata.name,
        "body": panel_skill.body,
        "references": references,
    })];
    if let Some(target) = target {
        let selected_skill_ids = match procedure.registration.local_skill.mode.as_str() {
            "none" => Vec::new(),
            "fixed" => procedure
                .registration
                .local_skill
                .skill_id
                .as_deref()
                .into_iter()
                .collect(),
            _ => selected_portable_skill_ids(target),
        };
        if procedure.registration.local_skill.mode == "required" && selected_skill_ids.is_empty() {
            blockers.push(json!({
                "code": "local_skill_required",
                "message": format!(
                    "Procedure {} requires a selected Local Skill.",
                    procedure.registration.key
                ),
            }));
        }
        for selected_skill_id in selected_skill_ids {
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
            skill_entries.push(json!({
                "id": selected_skill.metadata.id,
                "role": "selected-portable",
                "name": selected_skill.metadata.name,
                "body": selected_skill.body,
            }));
        }
    }

    let execution_contract = procedure_execution_contract(&commands);
    let mut payload = json!({
        "bootstrapBudget": {
            "maxBytes": MAX_BOOTSTRAP_ENVELOPE_BYTES,
            "unit": "utf8",
        },
        "agentProcedure": {
            "key": procedure.registration.key,
            "moduleKey": procedure.registration.module_key,
            "description": procedure.registration.description,
            "panelKind": procedure.registration.panel_kind,
            "selectionPolicy": procedure.registration.selection_policy,
            "localSkill": procedure.registration.local_skill,
            "skillId": procedure.skill_id,
            "referencePath": primary_reference,
            "referencePaths": procedure.registration.references,
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
        "skills": skill_entries,
        "commands": {
            "items": commands,
        },
        "executionContract": execution_contract,
        "actions": {
            "required": required_actions,
            "suggested": suggested_actions,
        },
    });
    if procedure.registration.module_key == "task" {
        payload["tasks"] = compact_task_queue_summary(visible);
    }
    if procedure
        .registration
        .command_intents
        .iter()
        .any(|intent| intent.starts_with("operation."))
    {
        payload["operations"] = compact_operation_summary(&operations);
    }
    Ok(payload)
}

fn selected_resource_versions(selection: &Value) -> Vec<Value> {
    selection
        .pointer("/value/selectedMyDocuments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|document| {
            let resource_id = document.get("id").and_then(Value::as_str)?;
            Some(json!({
                "resourceKind": "my-document",
                "resourceId": resource_id,
                "contentVersion": document.get("contentVersion").cloned().unwrap_or(Value::Null),
                "updatedAt": document.get("updatedAt").cloned().unwrap_or(Value::Null),
            }))
        })
        .collect()
}

fn procedure_execution_contract(commands: &[Value]) -> Value {
    let artifact_inputs = commands
        .iter()
        .flat_map(|command| {
            let intent = command
                .get("intent")
                .and_then(Value::as_str)
                .unwrap_or_default();
            command
                .get("args")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|argument| {
                    matches!(
                        argument.get("type").and_then(Value::as_str),
                        Some("path" | "path-array")
                    )
                })
                .map(move |argument| {
                    json!({
                        "commandIntent": intent,
                        "name": argument.get("name").cloned().unwrap_or(Value::Null),
                        "flag": argument.get("flag").cloned().unwrap_or(Value::Null),
                        "required": argument.get("required").cloned().unwrap_or(json!(false)),
                        "type": argument.get("type").cloned().unwrap_or(Value::Null),
                    })
                })
        })
        .collect::<Vec<_>>();
    let intents = commands
        .iter()
        .filter_map(|command| command.get("intent").and_then(Value::as_str))
        .collect::<BTreeSet<_>>();
    let operation_completion = intents.contains("operation.complete");
    json!({
        "artifactInputs": artifact_inputs,
        "completion": if operation_completion {
            json!({
                "kind": "operation",
                "successIntent": "operation.complete",
                "failureIntent": intents.contains("operation.fail").then_some("operation.fail"),
                "cancellationIntent": intents.contains("operation.cancel").then_some("operation.cancel"),
            })
        } else {
            json!({
                "kind": "command-response",
                "successField": "ok",
            })
        },
        "recovery": {
            "authority": "cli-error-actions",
            "reuseBootstrap": false,
        },
    })
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
            Some("distill") => target.state["selectedDistillationSkillId"]
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
            let mut action = project_command_action(
                paths,
                "task.read",
                vec![
                    "--task-id".to_owned(),
                    task_id.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            );
            action["condition"] = json!({
                "type": "resource-status",
                "resource": "next-task",
                "statuses": ["queued", "failed"],
            });
            actions.push(action);
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
            let mut action = project_command_action(
                paths,
                "operation.read",
                vec![
                    "--operation-id".to_owned(),
                    operation_id.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            );
            action["condition"] = json!({
                "type": "resource-status",
                "resource": "operation",
                "resourceId": operation_id,
                "statuses": ["active"],
            });
            Some(action)
        }));
    }
    actions
}
