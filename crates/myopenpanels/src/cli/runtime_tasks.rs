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
        Some("claim-next") => {
            let paths = parsed_current_paths(parsed)?;
            let target_id = required_flag(parsed, "target-id")?;
            let wait_ms = string_flag(parsed, "wait-ms")
                .map(|value| {
                    value
                        .parse::<u64>()
                        .map_err(|_| CliError::new("Expected --wait-ms to be a number."))
                })
                .transpose()?;
            let result = tasks::claim_next(
                &paths,
                target_id,
                string_flag(parsed, "capability"),
                wait_ms,
            )?;
            let text = result["task"]["id"]
                .as_str()
                .unwrap_or("No matching task");
            write_result(parsed, stdout, &result, text)
        }
        Some("claim") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let target_id = required_flag(parsed, "target-id")?;
            let result = tasks::claim_task(&paths, task_id, target_id)?;
            write_result(parsed, stdout, &result, &format!("Claimed {task_id}"))
        }
        Some("heartbeat") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let lease_token = required_flag(parsed, "lease-token")?;
            let result = tasks::heartbeat_task(&paths, task_id, lease_token)?;
            write_result(parsed, stdout, &result, &format!("Heartbeat {task_id}"))
        }
        Some("complete") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let lease_token = required_flag(parsed, "lease-token")?;
            let result_value = string_flag(parsed, "result-file")
                .map(|path| {
                    let raw = fs::read_to_string(path)
                        .map_err(|error| CliError::new(error.to_string()))?;
                    serde_json::from_str::<Value>(&raw)
                        .map_err(|error| CliError::new(error.to_string()))
                })
                .transpose()?;
            let result = tasks::complete_task(&paths, task_id, lease_token, result_value)?;
            write_result(parsed, stdout, &result, &format!("Completed {task_id}"))
        }
        Some("fail") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let lease_token = required_flag(parsed, "lease-token")?;
            let message = required_flag(parsed, "message")?;
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
            let result = tasks::fail_task_with_class(
                &paths,
                task_id,
                lease_token,
                message,
                string_flag(parsed, "retry-after"),
                failure_class,
            )?;
            write_result(parsed, stdout, &result, &format!("Failed {task_id}"))
        }
        Some("release") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let lease_token = required_flag(parsed, "lease-token")?;
            let result = tasks::release_task(&paths, task_id, lease_token)?;
            write_result(parsed, stdout, &result, &format!("Released {task_id}"))
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
        Some("events") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = tasks::list_task_events(&paths, task_id)?;
            let count = result["events"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} event(s)"))
        }
        Some("attempts") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = tasks::list_task_attempts(&paths, task_id)?;
            let count = result["attempts"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} attempt(s)"))
        }
        _ => Err(CliError::new(
            "Expected task subcommand: list, next, read, claim-next, claim, heartbeat, complete, fail, release, retry, cancel, archive, events, or attempts.",
        )),
    }
}

fn run_workflows_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let paths = parsed_current_paths(parsed)?;
    match parsed.positionals.get(1).map(String::as_str) {
        Some("list") => {
            let result = tasks::list_workflows(&paths)?;
            let count = result["workflows"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} workflow(s)"))
        }
        Some("read") => {
            let workflow_id = required_flag(parsed, "workflow-id")?;
            let result = tasks::read_workflow(&paths, workflow_id)?;
            write_result(parsed, stdout, &result, workflow_id)
        }
        _ => Err(CliError::new("Expected workflow subcommand: list or read.")),
    }
}

fn task_list_filter(parsed: &Invocation) -> tasks::TaskListFilter<'_> {
    tasks::TaskListFilter {
        pending: has_flag(parsed, "pending"),
        queue: string_flag(parsed, "queue"),
        status: string_flag(parsed, "status"),
    }
}
