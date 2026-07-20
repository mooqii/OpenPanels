const TASK_HANDOFF_VERSION: u32 = 1;
const TASK_HANDOFF_CONTROL_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskHandoffControl {
    schema_version: u32,
    handoff_id: String,
    target_id: String,
    project_id: String,
    project_dir: String,
    scope: Value,
    task_id: String,
    attempt_id: String,
    execution_generation: i64,
    handler_key: String,
    execution_bundle_hash: String,
    lease_token: String,
    task_broker_url: String,
    execution_token: String,
    workspace_path: String,
    task: Value,
}

pub fn start_task_handoff(
    paths: &MyOpenPanelsPaths,
    scope: &TaskExecutionScope,
) -> Result<Value, CliError> {
    let initial = read_task_scope(paths, scope)?;
    if initial.get("scopeState").and_then(Value::as_str) != Some("ready") {
        return Ok(terminal_handoff_payload(&initial, None));
    }
    let project_id = initial
        .pointer("/scope/projectId")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::new("Task Handoff scope Project is missing."))?
        .to_owned();
    let capabilities = initial
        .get("requiredCapabilities")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let handoff_id = crate::ids::random_id("task-handoff");
    let target_name = format!("manual-task-handoff:{handoff_id}");
    let task_broker_url = task_handoff_broker_url(paths)?;
    let registration = register_target(
        paths,
        TargetRegistration {
            name: &target_name,
            host: Some("agent-message"),
            project_id: Some(&project_id),
            capabilities,
            priority: 0,
            protocol_version: crate::content::EXECUTION_PROTOCOL_VERSION,
            max_concurrency: 1,
            model_gateway_connection_id: None,
        },
    )?;
    let target_id = registration
        .pointer("/target/id")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::new("Task Handoff target id is missing."))?
        .to_owned();
    match advance_task_handoff(
        paths,
        scope,
        &handoff_id,
        &target_id,
        Some(&task_broker_url),
    ) {
        Ok(payload) => Ok(payload),
        Err(error) => {
            let _ = remove_target(paths, &target_id);
            let _ = remove_handoff_directory(paths, &handoff_id);
            Err(error)
        }
    }
}

pub fn heartbeat_task_handoff(
    paths: &MyOpenPanelsPaths,
    handoff_id: &str,
) -> Result<Value, CliError> {
    let control = read_handoff_control(paths, handoff_id)?;
    let task = heartbeat_task(paths, &control.task_id, &control.lease_token)?;
    heartbeat_target(paths, &control.target_id)?;
    Ok(json!({
        "taskHandoffVersion": TASK_HANDOFF_VERSION,
        "handoffId": handoff_id,
        "task": task.get("task"),
    }))
}

pub fn complete_task_handoff(
    paths: &MyOpenPanelsPaths,
    handoff_id: &str,
) -> Result<Value, CliError> {
    let control = read_handoff_control(paths, handoff_id)?;
    let workspace = std::path::PathBuf::from(&control.workspace_path);
    let previous = crate::bridge::finalize_execution_unit(
        paths,
        crate::bridge::FinalizeExecutionUnitRequest {
            task: &control.task,
            workspace: &workspace,
            handler_key: &control.handler_key,
            execution_bundle_hash: &control.execution_bundle_hash,
            attempt_id: &control.attempt_id,
            execution_generation: control.execution_generation,
            lease_token: &control.lease_token,
            execution_token: &control.execution_token,
        },
    )?;
    cleanup_handoff_workspace(&control);
    let scope = task_handoff_scope_from_value(&control.scope)?;
    let mut payload = advance_task_handoff(
        paths,
        &scope,
        &control.handoff_id,
        &control.target_id,
        Some(&control.task_broker_url),
    )?;
    payload["previousExecution"] = previous;
    Ok(payload)
}

pub fn fail_task_handoff(
    paths: &MyOpenPanelsPaths,
    handoff_id: &str,
    message: &str,
    failure_class: TaskFailureClass,
) -> Result<Value, CliError> {
    let control = read_handoff_control(paths, handoff_id)?;
    let lifecycle = fail_task_with_class(
        paths,
        &control.task_id,
        &control.lease_token,
        message,
        None,
        failure_class,
    )?;
    cleanup_handoff_workspace(&control);
    let scope = task_handoff_scope_from_value(&control.scope)?;
    let mut payload = advance_task_handoff(
        paths,
        &scope,
        &control.handoff_id,
        &control.target_id,
        Some(&control.task_broker_url),
    )?;
    payload["previousExecution"] = json!({
        "taskId": control.task_id,
        "status": "failed",
        "lifecycle": lifecycle,
    });
    Ok(payload)
}

pub fn stop_task_handoff(
    paths: &MyOpenPanelsPaths,
    handoff_id: &str,
) -> Result<Value, CliError> {
    let control = read_handoff_control(paths, handoff_id)?;
    let release = release_task(paths, &control.task_id, &control.lease_token);
    cleanup_handoff_workspace(&control);
    let removed = remove_target(paths, &control.target_id);
    remove_handoff_directory(paths, handoff_id)?;
    release?;
    if let Err(error) = removed {
        if error.code() != Some("target_not_found") {
            return Err(error);
        }
    }
    Ok(json!({
        "taskHandoffVersion": TASK_HANDOFF_VERSION,
        "handoffId": handoff_id,
        "scope": control.scope,
        "scopeState": "stopped",
        "stopped": true,
    }))
}

pub fn execute_task_handoff_command(
    paths: &MyOpenPanelsPaths,
    handoff_id: &str,
    command: &[String],
) -> Result<Value, CliError> {
    let control = read_handoff_control(paths, handoff_id)?;
    let intent = handoff_command_intent(command).ok_or_else(|| {
        CliError::with_code(
            "task_handoff_command_not_allowed",
            "The Task Handoff command is not recognized or allowed.",
        )
    })?;
    let handler = crate::bridge::task_handler_by_key(&control.handler_key).ok_or_else(|| {
        CliError::with_code(
            "task_handler_not_found",
            format!("Task Handler is not registered: {}", control.handler_key),
        )
    })?;
    if !handler.allowed_agent_command_intents.contains(&intent) {
        return Err(CliError::with_code(
            "task_handoff_command_not_allowed",
            format!(
                "Command intent {intent} is not allowed by Task Handler {}.",
                handler.key
            ),
        ));
    }
    validate_handoff_command_args(&control, command)?;
    heartbeat_task(paths, &control.task_id, &control.lease_token)?;
    heartbeat_target(paths, &control.target_id)?;
    let executable = std::env::var("MYOPENPANELS_CLI")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .ok_or_else(|| CliError::new("Unable to resolve the MyOpenPanels executable."))?;
    let executable = if executable.is_absolute() {
        executable
    } else {
        std::env::current_dir().map_err(to_cli_error)?.join(executable)
    };
    let output = std::process::Command::new(executable)
        .args(command)
        .current_dir(&control.workspace_path)
        .env_remove("MYOPENPANELS_STORAGE_DIR")
        .env("MYOPENPANELS_PROJECT_DIR", &control.workspace_path)
        .env("MYOPENPANELS_CONTEXT_ID", &paths.context_id)
        .env("MYOPENPANELS_TASK_ID", &control.task_id)
        .env(
            "MYOPENPANELS_TASK_QUEUE",
            control.task.get("queue").and_then(Value::as_str).unwrap_or(""),
        )
        .env(
            "MYOPENPANELS_TASK_CAPABILITY",
            control
                .task
                .get("capability")
                .and_then(Value::as_str)
                .unwrap_or(""),
        )
        .env("MYOPENPANELS_TARGET_ID", &control.target_id)
        .env("MYOPENPANELS_TASK_LEASE_TOKEN", &control.lease_token)
        .env("MYOPENPANELS_TASK_ATTEMPT_ID", &control.attempt_id)
        .env(
            "MYOPENPANELS_TASK_EXECUTION_GENERATION",
            control.execution_generation.to_string(),
        )
        .env(
            "MYOPENPANELS_EXECUTION_GENERATION",
            control.execution_generation.to_string(),
        )
        .env("MYOPENPANELS_TASK_WORKSPACE", &control.workspace_path)
        .env("MYOPENPANELS_EXECUTION_WORKSPACE", &control.workspace_path)
        .env("MYOPENPANELS_TASK_BROKER_URL", &control.task_broker_url)
        .env("MYOPENPANELS_TASK_TOKEN", &control.execution_token)
        .output()
        .map_err(|error| CliError::with_code("task_handoff_command_failed", error.to_string()))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if !output.status.success() {
        let parsed = serde_json::from_str::<Value>(&stderr).ok();
        let code = parsed
            .as_ref()
            .and_then(|value| value.pointer("/error/subtype"))
            .and_then(Value::as_str)
            .unwrap_or("task_handoff_command_failed");
        let message = parsed
            .as_ref()
            .and_then(|value| value.pointer("/error/message"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .or_else(|| (!stderr.is_empty()).then_some(stderr.as_str()))
            .unwrap_or("The Task Handoff work command failed.");
        return Err(CliError::with_code(code, message));
    }
    let result = serde_json::from_str::<Value>(&stdout).unwrap_or_else(|_| json!(stdout));
    Ok(json!({
        "taskHandoffVersion": TASK_HANDOFF_VERSION,
        "handoffId": handoff_id,
        "commandIntent": intent,
        "result": result,
    }))
}

fn advance_task_handoff(
    paths: &MyOpenPanelsPaths,
    scope: &TaskExecutionScope,
    handoff_id: &str,
    target_id: &str,
    task_broker_url: Option<&str>,
) -> Result<Value, CliError> {
    let claimed =
        claim_task_scope_with_broker_url(paths, scope, target_id, task_broker_url)?;
    let Some(task) = claimed.get("task").filter(|task| !task.is_null()) else {
        let removed = remove_target(paths, target_id);
        let _ = remove_handoff_directory(paths, handoff_id);
        if let Err(error) = removed {
            if error.code() != Some("target_not_found") {
                return Err(error);
            }
        }
        return Ok(terminal_handoff_payload(&claimed, Some(handoff_id)));
    };
    let task_id = task
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::new("Claimed Task id is missing."))?
        .to_owned();
    let lease_token = required_claim_string(&claimed, "leaseToken")?;
    let prepared = (|| -> Result<Value, CliError> {
        let handoff_directory = handoff_directory(paths, handoff_id);
        std::fs::create_dir_all(&handoff_directory).map_err(to_cli_error)?;
        set_private_directory_permissions(&handoff_directory)?;
        let workspace = handoff_directory.join("workspace");
        if workspace.exists() {
            std::fs::remove_dir_all(&workspace).map_err(to_cli_error)?;
        }
        std::fs::create_dir_all(&workspace).map_err(to_cli_error)?;
        let prepared = crate::bridge::prepare_execution_bundle(paths, task, &workspace)?;
        let project_id = claimed
            .pointer("/scope/projectId")
            .and_then(Value::as_str)
            .ok_or_else(|| CliError::new("Claimed Task scope Project is missing."))?
            .to_owned();
        let control = TaskHandoffControl {
            schema_version: TASK_HANDOFF_CONTROL_SCHEMA_VERSION,
            handoff_id: handoff_id.to_owned(),
            target_id: target_id.to_owned(),
            project_id,
            project_dir: paths.project_dir.display().to_string(),
            scope: claimed.get("scope").cloned().unwrap_or_else(|| json!({})),
            task_id: task_id.clone(),
            attempt_id: required_claim_string(&claimed, "attemptId")?,
            execution_generation: claimed
                .get("executionGeneration")
                .and_then(Value::as_i64)
                .ok_or_else(|| CliError::new("Claimed Task execution generation is missing."))?,
            handler_key: prepared.bundle.handler_key.clone(),
            execution_bundle_hash: prepared.bundle.content_hash.clone(),
            lease_token: lease_token.clone(),
            task_broker_url: required_claim_string(&claimed, "taskBrokerUrl")?,
            execution_token: required_claim_string(&claimed, "executionToken")?,
            workspace_path: workspace.display().to_string(),
            task: prepared.task,
        };
        write_handoff_control(paths, &control)?;
        let prompt = crate::bridge::render_task_handoff_prompt(&prepared.bundle, handoff_id);
        let mut payload = scope_summary(&claimed);
        payload["taskHandoffVersion"] = json!(TASK_HANDOFF_VERSION);
        payload["handoff"] = json!({
            "id": handoff_id,
            "targetId": target_id,
            "state": "running",
        });
        payload["executionBundle"] =
            serde_json::to_value(&prepared.bundle).map_err(to_cli_error)?;
        payload["delivery"] = json!({
            "mode": "agent-message",
            "workingDirectory": workspace,
            "prompt": prompt,
            "actions": handoff_delivery_actions(handoff_id),
        });
        Ok(payload)
    })();
    match prepared {
        Ok(payload) => Ok(payload),
        Err(error) => {
            let _ = release_task(paths, &task_id, &lease_token);
            let _ = remove_target(paths, target_id);
            let _ = remove_handoff_directory(paths, handoff_id);
            Err(error)
        }
    }
}

fn task_handoff_broker_url(paths: &MyOpenPanelsPaths) -> Result<String, CliError> {
    if let Some(url) = crate::content::task_broker_url_for_claim() {
        return Ok(url);
    }
    let session = crate::studio::resolve_current_studio_session(paths)?.ok_or_else(|| {
        CliError::with_code(
            "broker_unavailable",
            "Task Handoff requires a running Studio Task Broker.",
        )
    })?;
    Ok(session
        .local_server_url
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(session.server_url))
}

fn terminal_handoff_payload(scope: &Value, handoff_id: Option<&str>) -> Value {
    let mut payload = scope_summary(scope);
    payload["taskHandoffVersion"] = json!(TASK_HANDOFF_VERSION);
    payload["handoff"] = handoff_id.map_or(Value::Null, |id| {
        json!({ "id": id, "state": payload.get("scopeState") })
    });
    payload["executionBundle"] = Value::Null;
    payload["delivery"] = json!({
        "mode": "agent-message",
        "prompt": Value::Null,
        "actions": {},
    });
    payload
}

fn scope_summary(payload: &Value) -> Value {
    json!({
        "scope": payload.get("scope"),
        "scopeState": payload.get("scopeState"),
        "counts": payload.get("counts"),
        "blockers": payload.get("blockers"),
        "requiredCapabilities": payload.get("requiredCapabilities"),
    })
}

fn handoff_delivery_actions(handoff_id: &str) -> Value {
    let action = |intent: &str, command: &str| {
        json!({
            "intent": intent,
            "argv": [
                "task", "handoff", command,
                "--handoff-id", handoff_id,
                "--format", "json"
            ],
        })
    };
    json!({
        "heartbeat": action("task.handoff.heartbeat", "heartbeat"),
        "complete": action("task.handoff.complete", "complete"),
        "fail": {
            "intent": "task.handoff.fail",
            "argvTemplate": [
                "task", "handoff", "fail",
                "--handoff-id", handoff_id,
                "--message", "<failure-message>",
                "--failure-class", "retryable_channel",
                "--format", "json"
            ]
        },
        "stop": action("task.handoff.stop", "stop"),
    })
}

fn handoff_command_intent(command: &[String]) -> Option<&'static str> {
    match command {
        [domain, resource, action, ..] if domain == "wiki" && resource == "raw" && action == "update" => Some("wiki.raw.update"),
        [domain, resource, action, ..] if domain == "wiki" && resource == "page" && action == "read" => Some("wiki.page.read"),
        [domain, resource, action, ..] if domain == "wiki" && resource == "page" && action == "create" => Some("wiki.page.create"),
        [domain, resource, action, ..] if domain == "wiki" && resource == "page" && action == "update" => Some("wiki.page.update"),
        [domain, action, ..] if domain == "writing" && action == "generate" => Some("writing.generate"),
        [domain, resource, action, ..] if domain == "writing" && resource == "skill" && action == "install" => Some("writing.skill.install"),
        [domain, action, ..] if domain == "publishing" && action == "checkpoint" => Some("publishing.checkpoint"),
        [domain, action, ..] if domain == "operation" && action == "complete" => Some("operation.complete"),
        _ => None,
    }
}

fn validate_handoff_command_args(
    control: &TaskHandoffControl,
    command: &[String],
) -> Result<(), CliError> {
    for forbidden in [
        "--storage-dir",
        "--context-id",
        "--lease-token",
        "--handoff-id",
    ] {
        if command.iter().any(|argument| {
            argument == forbidden
                || argument
                    .strip_prefix(forbidden)
                    .is_some_and(|suffix| suffix.starts_with('='))
        }) {
            return Err(CliError::with_code(
                "task_handoff_command_not_allowed",
                format!("Task Handoff work commands cannot override {forbidden}."),
            ));
        }
    }
    for value in handoff_option_values(command, "--task-id") {
        if value != control.task_id {
            return Err(CliError::with_code(
                "execution_fenced",
                "The Task Handoff command targets a different Task.",
            ));
        }
    }
    for value in handoff_option_values(command, "--project-dir") {
        if value != control.project_dir {
            return Err(CliError::with_code(
                "task_handoff_command_not_allowed",
                "The Task Handoff command targets a different Project directory.",
            ));
        }
    }
    for option in ["--content-file", "--artifact-file", "--skill-file"] {
        for value in handoff_option_values(command, option) {
            validate_handoff_workspace_path(control, value)?;
        }
    }
    Ok(())
}

fn handoff_option_values<'a>(command: &'a [String], option: &str) -> Vec<&'a str> {
    let inline_prefix = format!("{option}=");
    command
        .iter()
        .enumerate()
        .filter_map(|(index, argument)| {
            if argument == option {
                command.get(index + 1).map(String::as_str)
            } else {
                argument.strip_prefix(&inline_prefix)
            }
        })
        .collect()
}

fn validate_handoff_workspace_path(
    control: &TaskHandoffControl,
    value: &str,
) -> Result<(), CliError> {
    let workspace = std::path::PathBuf::from(&control.workspace_path);
    let candidate = std::path::PathBuf::from(value);
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        workspace.join(candidate)
    };
    let normalized = candidate.canonicalize().map_err(|_| {
        CliError::with_code(
            "task_handoff_command_not_allowed",
            format!("Task Handoff input file does not exist: {value}"),
        )
    })?;
    let root = workspace.canonicalize().map_err(to_cli_error)?;
    if !normalized.starts_with(&root) {
        return Err(CliError::with_code(
            "task_handoff_command_not_allowed",
            "Task Handoff work files must remain inside the execution workspace.",
        ));
    }
    Ok(())
}

fn required_claim_string(payload: &Value, key: &str) -> Result<String, CliError> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| CliError::new(format!("Claimed Task {key} is missing.")))
}

fn task_handoff_scope_from_value(value: &Value) -> Result<TaskExecutionScope, CliError> {
    match value.get("kind").and_then(Value::as_str) {
        Some("project-drain") => Ok(TaskExecutionScope::ProjectDrain {
            project_id: required_scope_string(value, "projectId")?,
        }),
        Some("exact-task") => Ok(TaskExecutionScope::ExactTask {
            task_id: required_scope_string(value, "taskId")?,
        }),
        Some("wiki-mutation-drain") => Ok(TaskExecutionScope::WikiMutationDrain {
            project_id: required_scope_string(value, "projectId")?,
            mutation_key: required_scope_string(value, "mutationKey")?,
        }),
        _ => Err(CliError::with_code(
            "invalid_task_scope",
            "Task Handoff control has an invalid scope.",
        )),
    }
}

fn required_scope_string(value: &Value, key: &str) -> Result<String, CliError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| CliError::with_code("invalid_task_scope", format!("Scope {key} is missing.")))
}

fn handoff_directory(paths: &MyOpenPanelsPaths, handoff_id: &str) -> std::path::PathBuf {
    paths
        .context_dir
        .join("task-handoffs")
        .join(crate::paths::sanitize_path_part(handoff_id))
}

fn control_path(paths: &MyOpenPanelsPaths, handoff_id: &str) -> Result<std::path::PathBuf, CliError> {
    if crate::paths::sanitize_path_part(handoff_id) != handoff_id {
        return Err(CliError::with_code(
            "task_handoff_not_found",
            "Task Handoff id is invalid.",
        ));
    }
    Ok(handoff_directory(paths, handoff_id).join("control.json"))
}

fn read_handoff_control(
    paths: &MyOpenPanelsPaths,
    handoff_id: &str,
) -> Result<TaskHandoffControl, CliError> {
    let path = control_path(paths, handoff_id)?;
    let raw = std::fs::read_to_string(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CliError::with_code(
                "task_handoff_not_found",
                format!("Task Handoff not found: {handoff_id}"),
            )
        } else {
            to_cli_error(error)
        }
    })?;
    let control: TaskHandoffControl = serde_json::from_str(&raw).map_err(to_cli_error)?;
    if control.schema_version != TASK_HANDOFF_CONTROL_SCHEMA_VERSION
        || control.handoff_id != handoff_id
    {
        return Err(CliError::with_code(
            "task_handoff_invalid",
            "Task Handoff control is invalid or unsupported.",
        ));
    }
    Ok(control)
}

fn write_handoff_control(
    paths: &MyOpenPanelsPaths,
    control: &TaskHandoffControl,
) -> Result<(), CliError> {
    let directory = handoff_directory(paths, &control.handoff_id);
    std::fs::create_dir_all(&directory).map_err(to_cli_error)?;
    set_private_directory_permissions(&directory)?;
    let path = directory.join("control.json");
    let temporary = directory.join("control.json.tmp");
    let bytes = serde_json::to_vec(control).map_err(to_cli_error)?;
    write_private_file(&temporary, &bytes)?;
    std::fs::rename(temporary, path).map_err(to_cli_error)
}

fn write_private_file(path: &std::path::Path, bytes: &[u8]) -> Result<(), CliError> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(to_cli_error)?;
    std::io::Write::write_all(&mut file, bytes).map_err(to_cli_error)
}

fn set_private_directory_permissions(path: &std::path::Path) -> Result<(), CliError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(to_cli_error)?;
    }
    Ok(())
}

fn cleanup_handoff_workspace(control: &TaskHandoffControl) {
    let _ = std::fs::remove_dir_all(&control.workspace_path);
}

fn remove_handoff_directory(
    paths: &MyOpenPanelsPaths,
    handoff_id: &str,
) -> Result<(), CliError> {
    let directory = handoff_directory(paths, handoff_id);
    match std::fs::remove_dir_all(directory) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(to_cli_error(error)),
    }
}
