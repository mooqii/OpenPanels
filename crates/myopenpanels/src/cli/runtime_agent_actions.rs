fn run_agent_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        Some("bootstrap") => {
            let paths = parsed_bootstrap_paths(parsed)?;
            let payload = agent_bootstrap(&paths, VERSION, string_flag(parsed, "procedure"))?;
            write_result(
                parsed,
                stdout,
                &payload,
                &format!(
                    "MyOpenPanels agent protocol v{} bootstrap",
                    crate::agent::AGENT_GUIDANCE_PROTOCOL_VERSION
                ),
            )
        }
        Some("entry-skill") => match parsed.positionals.get(2).map(String::as_str) {
            Some("acknowledge") => {
                let paths = parsed_current_paths(parsed)?;
                let payload = crate::agent_control::acknowledge_entry_skill_update(
                    &paths,
                    required_flag(parsed, "event-id")?,
                    required_flag(parsed, "installed-version")?,
                )?;
                write_result(parsed, stdout, &payload, "Entry Skill update acknowledged")
            }
            _ => Err(CliError::new(
                "Expected agent entry-skill subcommand: acknowledge.",
            )),
        },
        Some("catalog") => {
            let payload = registry::catalog(string_flag(parsed, "domain")).ok_or_else(|| {
                CliError::with_code(
                    "catalog_domain_not_found",
                    format!(
                        "MyOpenPanels command catalog domain not found: {}",
                        string_flag(parsed, "domain").unwrap_or_default()
                    ),
                )
            })?;
            let text = render_agent_discovery_summary(&payload);
            write_result(parsed, stdout, &payload, &text)
        }
        Some("bridge") => {
            let paths = parsed_current_paths(parsed)?;
            if parsed.positionals.get(2).map(String::as_str) == Some("status") {
                let result = read_bridge_status(&paths)?;
                let text = result["status"].as_str().unwrap_or("idle");
                return write_result(parsed, stdout, &result, text);
            }
            let interval_ms = string_flag(parsed, "interval-ms")
                .map(|value| {
                    value
                        .parse::<u64>()
                        .map_err(|_| CliError::new("Expected --interval-ms to be a number."))
                })
                .transpose()?
                .unwrap_or(2000);
            let timeout_ms = string_flag(parsed, "timeout-ms")
                .map(|value| {
                    value
                        .parse::<u64>()
                        .map_err(|_| CliError::new("Expected --timeout-ms to be a number."))
                })
                .transpose()?
                .unwrap_or(600_000);
            let result = run_bridge(
                &paths,
                BridgeOptions {
                    agent_prompt: false,
                    capabilities: string_list_flag(parsed, "capability"),
                    command: string_flag(parsed, "command"),
                    host: None,
                    interval_ms,
                    manual_lifecycle: has_flag(parsed, "manual-lifecycle"),
                    name: string_flag(parsed, "name"),
                    once: has_flag(parsed, "once"),
                    priority: 0,
                    queue: string_flag(parsed, "queue"),
                    timeout_ms,
                },
            )?;
            let text = if result["ran"].as_bool().unwrap_or(false) {
                "Bridge command ran"
            } else {
                "No pending task"
            };
            write_result(parsed, stdout, &result, text)
        }
        Some("target") => {
            let paths = parsed_current_paths(parsed)?;
            match parsed.positionals.get(2).map(String::as_str) {
                Some("list") => {
                    let result = tasks::list_targets(&paths)?;
                    let count = result["targets"].as_array().map(Vec::len).unwrap_or(0);
                    write_result(parsed, stdout, &result, &format!("{count} target(s)"))
                }
                Some("register") => {
                    let priority = string_flag(parsed, "priority")
                        .map(|value| {
                            value.parse::<i64>().map_err(|_| {
                                CliError::new("Expected --priority to be an integer.")
                            })
                        })
                        .transpose()?
                        .unwrap_or(0);
                    let protocol_version = string_flag(parsed, "protocol-version")
                        .map(|value| value.parse::<i64>().map_err(|_| CliError::new("Expected --protocol-version to be an integer.")))
                        .transpose()?
                        .unwrap_or(crate::content::EXECUTION_PROTOCOL_VERSION);
                    let max_concurrency = string_flag(parsed, "max-concurrency")
                        .map(|value| value.parse::<i64>().map_err(|_| CliError::new("Expected --max-concurrency to be an integer.")))
                        .transpose()?
                        .unwrap_or(1);
                    let result = tasks::register_target(
                        &paths,
                        tasks::TargetRegistration {
                            name: required_flag(parsed, "name")?,
                            host: string_flag(parsed, "host"),
                            project_id: string_flag(parsed, "project-id"),
                            capabilities: string_list_flag(parsed, "capability"),
                            priority,
                            protocol_version,
                            max_concurrency,
                            model_gateway_connection_id: None,
                        },
                    )?;
                    write_result(
                        parsed,
                        stdout,
                        &result,
                        result["target"]["id"].as_str().unwrap_or("Registered target"),
                    )
                }
                Some("remove") => {
                    let target_id = required_flag(parsed, "target-id")?;
                    let result = tasks::remove_target(&paths, target_id)?;
                    write_result(parsed, stdout, &result, &format!("Removed {target_id}"))
                }
                _ => Err(CliError::new(
                    "Expected agent target subcommand: list, register, or remove.",
                )),
            }
        }
        Some("route") => {
            let paths = parsed_current_paths(parsed)?;
            match parsed.positionals.get(2).map(String::as_str) {
                Some("list") => {
                    let result = tasks::list_agent_routes(&paths)?;
                    let count = result["routes"].as_array().map(Vec::len).unwrap_or(0);
                    write_result(parsed, stdout, &result, &format!("{count} route(s)"))
                }
                Some("set") => {
                    let result = tasks::set_agent_route(
                        &paths,
                        required_flag(parsed, "capability")?,
                        &string_list_flag(parsed, "target-id"),
                    )?;
                    write_result(parsed, stdout, &result, "Agent route updated")
                }
                Some("remove") => {
                    let result = tasks::remove_agent_route(
                        &paths,
                        required_flag(parsed, "capability")?,
                    )?;
                    write_result(parsed, stdout, &result, "Agent route removed")
                }
                _ => Err(CliError::new("Expected agent route subcommand: list, set, or remove.")),
            }
        }
        Some("skill") if parsed.positionals.get(2).map(String::as_str) == Some("list") => {
            let skills = list_agent_skill_summaries(
                &parsed_current_paths(parsed)?,
                string_flag(parsed, "panel-kind"),
                string_flag(parsed, "task-type"),
            )?;
            let next_actions = discovery_read_actions(
                &skills,
                "agent.skill.read",
                "--skill-id",
                "Read this Skill when its loadWhen condition applies.",
            );
            let payload = serde_json::json!({
                "skills": skills,
                "actions": { "required": [], "suggested": next_actions },
            });
            let text = render_agent_discovery_summary(&payload);
            write_result(parsed, stdout, &payload, &text)
        }
        Some("skill") if parsed.positionals.get(2).map(String::as_str) == Some("read") => {
            let skill_id = required_flag(parsed, "skill-id")?;
            let paths = parsed_current_paths(parsed)?;
            let payload = read_agent_skill(&paths, skill_id, string_flag(parsed, "task-id"))?;
            let markdown = payload.markdown.clone();
            write_result(parsed, stdout, &payload, &markdown)
        }
        _ => Err(CliError::new(
            "Expected agent subcommand: bootstrap, catalog, entry-skill, bridge, skill, target, or route.",
        )),
    }
}

fn discovery_read_actions(
    items: &[Value],
    intent: &str,
    id_flag: &str,
    load_when: &str,
) -> Vec<Value> {
    items
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .map(|id| {
            let mut action = registry::command_action(
                registry::CommandId::registered(intent),
                vec![
                    id_flag.to_owned(),
                    id.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            )
            .unwrap_or_else(|| panic!("missing Command Registry action for {intent}"));
            action["condition"] = serde_json::json!({
                "type": "agent-judgment",
                "description": load_when,
            });
            action
        })
        .collect()
}

fn with_task_actions(mut payload: Value, list_mode: bool) -> Value {
    let mut actions = Vec::new();
    if list_mode {
        let tasks = payload
            .get("tasks")
            .and_then(Value::as_array)
            .cloned()
            .or_else(|| {
                payload
                    .get("task")
                    .filter(|task| !task.is_null())
                    .map(|task| vec![task.clone()])
            })
            .unwrap_or_default();
        for task in tasks {
            let Some(task_id) = task.get("id").and_then(Value::as_str) else {
                continue;
            };
            let mut action = registry::command_action(
                registry::CommandId::registered("task.read"),
                vec![
                    "--task-id".to_owned(),
                    task_id.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            )
            .expect("registered Task read action");
            action["condition"] = serde_json::json!({
                "type": "agent-judgment",
                "description": "The user request should inspect or continue this Task."
            });
            actions.push(action);
        }
    } else if let Some(task) = payload.get("task") {
        if let Some(domain) = task
            .get("capability")
            .and_then(Value::as_str)
            .and_then(registry::catalog_domain_for_intent)
        {
            actions.push(catalog_domain_action(
                domain,
                "The Task requires a command from this domain.",
            ));
        }
        let status = task.get("status").and_then(Value::as_str);
        let lifecycle = match status {
            Some("queued") => &["task.claim", "task.cancel"][..],
            Some("failed") => &["task.claim", "task.retry", "task.cancel"][..],
            Some("reserved" | "running" | "claimed" | "converting" | "indexing") => &[
                "task.heartbeat",
                "task.complete",
                "task.fail",
                "task.release",
            ][..],
            _ => &[][..],
        };
        if !lifecycle.is_empty() {
            actions.push(catalog_domain_action(
                "task",
                "The Task lifecycle requires a task command.",
            ));
        }
    }
    payload["actions"] = serde_json::json!({ "required": [], "suggested": actions });
    payload
}

fn with_operation_actions(mut payload: Value, list_mode: bool) -> Value {
    let mut actions = Vec::new();
    if list_mode {
        for operation in payload
            .get("operations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(operation_id) = operation.get("id").and_then(Value::as_str) else {
                continue;
            };
            let mut action = registry::command_action(
                registry::CommandId::registered("operation.read"),
                vec![
                    "--operation-id".to_owned(),
                    operation_id.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            )
            .expect("registered Operation read action");
            action["condition"] = serde_json::json!({
                "type": "agent-judgment",
                "description": "The user request should inspect or continue this Operation."
            });
            actions.push(action);
        }
    } else {
        if let Some(skill_id) = payload.get("skillId").and_then(Value::as_str) {
            let mut action = registry::command_action(
                registry::CommandId::registered("agent.skill.read"),
                vec![
                    "--skill-id".to_owned(),
                    skill_id.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            )
            .expect("registered Skill read action");
            action["condition"] = serde_json::json!({
                "type": "resource-field",
                "field": "skillId",
                "operator": "present"
            });
            actions.push(action);
        }
        if matches!(
            payload.get("status").and_then(Value::as_str),
            Some("active" | "failed")
        ) {
            actions.push(catalog_domain_action(
                "operation",
                "The Operation requires a lifecycle command.",
            ));
        }
    }
    payload["actions"] = serde_json::json!({ "required": [], "suggested": actions });
    payload
}

fn catalog_domain_action(domain: &str, load_when: &str) -> Value {
    let mut action = registry::command_action(
        registry::CommandId::registered("agent.catalog"),
        vec![
            "--domain".to_owned(),
            domain.to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ],
    )
    .expect("registered catalog action");
    action["condition"] = serde_json::json!({
        "type": "agent-judgment",
        "description": load_when,
    });
    action
}
