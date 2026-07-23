fn run_tasks_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        Some("list") => {
            let paths = parsed_current_paths(parsed)?;
            let result = with_task_actions(
                tasks::list_tasks(&paths, task_list_filter(parsed))?,
                true,
            );
            let count = result["tasks"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} task(s)"))
        }
        Some("next") => {
            let paths = parsed_current_paths(parsed)?;
            let result = with_task_actions(
                tasks::next_task(&paths, task_list_filter(parsed))?,
                true,
            );
            let text = result["task"]["id"].as_str().unwrap_or("No pending task");
            write_result(parsed, stdout, &result, text)
        }
        Some("read") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = with_task_actions(tasks::inspect_task(&paths, task_id)?, false);
            write_result(parsed, stdout, &result, task_id)
        }
        Some("handoff-start") => {
            let paths = parsed_current_paths(parsed)?;
            let scope = task_execution_scope(parsed)?;
            let result = tasks::start_task_handoff(&paths, &scope)?;
            let text = result["scopeState"].as_str().unwrap_or("unknown");
            write_result(parsed, stdout, &result, text)
        }
        Some("handoff-exec") => {
            let paths = parsed_current_paths(parsed)?;
            let handoff_id = required_flag(parsed, "handoff-id")?;
            let command = serde_json::from_str::<Vec<String>>(required_flag(
                parsed,
                "command-json",
            )?)
            .map_err(|error| CliError::with_code("invalid_argument", error.to_string()))?;
            let result = tasks::execute_task_handoff_command(
                &paths,
                handoff_id,
                &command,
            )?;
            write_result(parsed, stdout, &result, result["commandIntent"].as_str().unwrap_or("Task Handoff command ran"))
        }
        Some("handoff-heartbeat") => {
            let paths = parsed_current_paths(parsed)?;
            let handoff_id = required_flag(parsed, "handoff-id")?;
            let result = tasks::heartbeat_task_handoff(&paths, handoff_id)?;
            write_result(parsed, stdout, &result, &format!("Heartbeat {handoff_id}"))
        }
        Some("handoff-complete") => {
            let paths = parsed_current_paths(parsed)?;
            let handoff_id = required_flag(parsed, "handoff-id")?;
            let result = tasks::complete_task_handoff(&paths, handoff_id)?;
            let text = result["scopeState"].as_str().unwrap_or("unknown");
            write_result(parsed, stdout, &result, text)
        }
        Some("handoff-fail") => {
            let paths = parsed_current_paths(parsed)?;
            let handoff_id = required_flag(parsed, "handoff-id")?;
            let failure_class = string_flag(parsed, "failure-class")
                .map(|value| {
                    tasks::TaskFailureClass::parse(value).ok_or_else(|| {
                        CliError::with_code(
                            "invalid_argument",
                            "--failure-class must be retryable_channel, retryable_output, or terminal_task.",
                        )
                        .with_param("--failure-class")
                    })
                })
                .transpose()?
                .unwrap_or(tasks::TaskFailureClass::RetryableChannel);
            let result = tasks::fail_task_handoff(
                &paths,
                handoff_id,
                required_flag(parsed, "message")?,
                failure_class,
            )?;
            let text = result["scopeState"].as_str().unwrap_or("unknown");
            write_result(parsed, stdout, &result, text)
        }
        Some("handoff-stop") => {
            let paths = parsed_current_paths(parsed)?;
            let handoff_id = required_flag(parsed, "handoff-id")?;
            let result = tasks::stop_task_handoff(&paths, handoff_id)?;
            write_result(parsed, stdout, &result, &format!("Stopped {handoff_id}"))
        }
        Some("retry") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = tasks::retry_task(&paths, task_id)?;
            write_result(parsed, stdout, &result, &format!("Retried {task_id}"))
        }
        Some("cancel") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = tasks::cancel_task(&paths, task_id)?;
            write_result(parsed, stdout, &result, &format!("Cancelled {task_id}"))
        }
        Some("archive") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = tasks::archive_task(&paths, task_id)?;
            write_result(parsed, stdout, &result, &format!("Archived {task_id}"))
        }
        _ => Err(CliError::new(
            "Expected task subcommand: list, next, read, handoff, retry, cancel, or archive.",
        )),
    }
}

fn task_execution_scope(parsed: &Invocation) -> Result<tasks::TaskExecutionScope, CliError> {
    let scope = required_flag(parsed, "scope")?;
    let project_id = string_flag(parsed, "project-id");
    let task_id = string_flag(parsed, "task-id");
    let mutation_key = string_flag(parsed, "mutation-key");
    let invalid = || {
        CliError::with_code(
            "invalid_task_scope",
            "Task scope selectors must match the selected scope kind.",
        )
    };
    match scope {
        "project-drain" if task_id.is_none() && mutation_key.is_none() => {
            Ok(tasks::TaskExecutionScope::ProjectDrain {
                project_id: project_id.ok_or_else(invalid)?.to_owned(),
            })
        }
        "exact-task" if project_id.is_none() && mutation_key.is_none() => {
            Ok(tasks::TaskExecutionScope::ExactTask {
                task_id: task_id.ok_or_else(invalid)?.to_owned(),
            })
        }
        "wiki-mutation-drain" if task_id.is_none() => {
            Ok(tasks::TaskExecutionScope::WikiMutationDrain {
                project_id: project_id.ok_or_else(invalid)?.to_owned(),
                mutation_key: mutation_key.ok_or_else(invalid)?.to_owned(),
            })
        }
        _ => Err(invalid()),
    }
}

fn task_list_filter(parsed: &Invocation) -> tasks::TaskListFilter<'_> {
    tasks::TaskListFilter {
        pending: has_flag(parsed, "pending"),
        queue: string_flag(parsed, "queue"),
        status: string_flag(parsed, "status"),
    }
}
