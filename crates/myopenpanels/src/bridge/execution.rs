fn run_task_command(
    paths: &MyOpenPanelsPaths,
    command: &str,
    timeout_ms: u64,
    task: &Value,
    target_id: &str,
    lease_token: &str,
    agent_prompt: bool,
    attempt_id: Option<&str>,
    execution_generation: Option<i64>,
    task_broker_url: Option<&str>,
    execution_token: Option<&str>,
    shutdown: Option<&AtomicBool>,
) -> Result<Value, CliError> {
    let task_id = task.get("id").and_then(Value::as_str).unwrap_or("");
    let queue = task.get("queue").and_then(Value::as_str).unwrap_or("");
    let capability = task.get("capability").and_then(Value::as_str).unwrap_or("");
    let execution_workspace = paths
        .storage_dir
        .join("executions")
        .join(sanitize_path_part(task_id))
        .join(format!(
            "{}-{}",
            execution_generation.unwrap_or_default(),
            sanitize_path_part(attempt_id.unwrap_or("attempt"))
        ));
    if execution_workspace.exists() {
        fs::remove_dir_all(&execution_workspace).map_err(to_cli_error)?;
    }
    fs::create_dir_all(&execution_workspace).map_err(to_cli_error)?;
    let _workspace_cleanup = ExecutionWorkspaceCleanup(execution_workspace.clone());
    let execution_task = materialize_task_inputs(paths, task, &execution_workspace)?;
    let task_input = if agent_prompt {
        if is_document_conversion_task(&execution_task) {
            document_conversion_task_prompt(paths, &execution_task, &execution_workspace)?
        } else if is_document_generation_task(&execution_task) {
            document_generation_task_prompt(paths, &execution_task, &execution_workspace)?
        } else if is_writing_refinement_task(&execution_task) {
            writing_refinement_task_prompt(&execution_task, &execution_workspace)?
        } else if is_wiki_authoring_task(&execution_task) {
            wiki_authoring_task_prompt(paths, &execution_task, &execution_workspace)?
        } else {
            local_agent_task_prompt(paths, &execution_task)?
        }
    } else {
        serde_json::to_string_pretty(&execution_task).map_err(to_cli_error)?
    };
    let mut child = shell_command(command)
        .current_dir(&execution_workspace)
        .env_remove("MYOPENPANELS_STORAGE_DIR")
        .env("MYOPENPANELS_PROJECT_DIR", &execution_workspace)
        .env("MYOPENPANELS_CONTEXT_ID", &paths.context_id)
        .env("MYOPENPANELS_TASK_ID", task_id)
        .env("MYOPENPANELS_TASK_QUEUE", queue)
        .env("MYOPENPANELS_TASK_CAPABILITY", capability)
        .env("MYOPENPANELS_TARGET_ID", target_id)
        .env("MYOPENPANELS_TASK_LEASE_TOKEN", lease_token)
        .env(
            "MYOPENPANELS_TASK_ATTEMPT_ID",
            attempt_id.unwrap_or_default(),
        )
        .env(
            "MYOPENPANELS_TASK_EXECUTION_GENERATION",
            execution_generation.unwrap_or_default().to_string(),
        )
        .env(
            "MYOPENPANELS_EXECUTION_GENERATION",
            execution_generation.unwrap_or_default().to_string(),
        )
        .env("MYOPENPANELS_TASK_WORKSPACE", &execution_workspace)
        .env("MYOPENPANELS_EXECUTION_WORKSPACE", &execution_workspace)
        .env(
            "MYOPENPANELS_TASK_BROKER_URL",
            task_broker_url.unwrap_or_default(),
        )
        .env(
            "MYOPENPANELS_TASK_TOKEN",
            execution_token.unwrap_or_default(),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(to_cli_error)?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(task_input.as_bytes())
            .map_err(to_cli_error)?;
        stdin.write_all(b"\n").map_err(to_cli_error)?;
    }
    drop(child.stdin.take());

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_reader = std::thread::spawn(move || read_pipe(stdout));
    let stderr_reader = std::thread::spawn(move || read_pipe(stderr));
    let timeout = Duration::from_millis(timeout_ms.max(1));
    let started = Instant::now();
    let mut timed_out = false;
    let mut interrupted = false;
    let heartbeat_paths = paths.clone();
    let heartbeat_task_id = task_id.to_owned();
    let heartbeat_token = lease_token.to_owned();
    let heartbeat_target_id = target_id.to_owned();
    let heartbeat_running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let heartbeat_failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let heartbeat_flag = heartbeat_running.clone();
    let heartbeat_failed_flag = heartbeat_failed.clone();
    let heartbeat = std::thread::spawn(move || {
        let mut last_heartbeat = Instant::now();
        while heartbeat_flag.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(250));
            if heartbeat_flag.load(std::sync::atomic::Ordering::Relaxed)
                && last_heartbeat.elapsed() >= Duration::from_secs(30)
            {
                if tasks::heartbeat_task(&heartbeat_paths, &heartbeat_task_id, &heartbeat_token)
                    .is_err()
                {
                    heartbeat_failed_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    break;
                }
                let _ = tasks::heartbeat_target(&heartbeat_paths, &heartbeat_target_id);
                last_heartbeat = Instant::now();
            }
        }
    });
    let status = loop {
        if let Some(status) = child.try_wait().map_err(to_cli_error)? {
            break status;
        }
        if heartbeat_failed.load(std::sync::atomic::Ordering::Relaxed) {
            terminate_child_process(&mut child);
            break child.wait().map_err(to_cli_error)?;
        }
        if shutdown.is_some_and(|shutdown| shutdown.load(Ordering::Acquire)) {
            interrupted = true;
            terminate_child_process(&mut child);
            break child.wait().map_err(to_cli_error)?;
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            terminate_child_process(&mut child);
            break child.wait().map_err(to_cli_error)?;
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    heartbeat_running.store(false, std::sync::atomic::Ordering::Relaxed);
    let _ = heartbeat.join();
    let stdout = stdout_reader
        .join()
        .map_err(|_| CliError::new("Bridge stdout reader failed."))?
        .map_err(to_cli_error)?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| CliError::new("Bridge stderr reader failed."))?
        .map_err(to_cli_error)?;
    let stdout = truncate_output(&stdout, DEFAULT_OUTPUT_LIMIT_BYTES);
    let mut stderr = truncate_output(&stderr, DEFAULT_OUTPUT_LIMIT_BYTES);
    let mut command_result = Value::Null;
    let mut validation_error = None;
    if agent_prompt && status.success() && !timed_out && !interrupted {
        let validation = if is_document_conversion_task(&execution_task) {
            Some(validate_conversion_execution_result(
                paths,
                &execution_task,
                &execution_workspace,
            ))
        } else if is_document_generation_task(&execution_task) {
            Some(validate_generation_execution_result(
                paths,
                &execution_task,
                &execution_workspace,
            ))
        } else if is_writing_refinement_task(&execution_task) {
            Some(validate_refinement_execution_result(
                paths,
                &execution_task,
                &execution_workspace,
            ))
        } else if is_wiki_authoring_task(&execution_task) {
            Some(validate_wiki_execution_result(
                paths,
                &execution_task,
                &execution_workspace,
            ))
        } else {
            None
        };
        if let Some(validation) = validation {
            match validation {
                Ok(result) => command_result = result,
                Err(error) => {
                    if !stderr.trim().is_empty() {
                        stderr.push('\n');
                    }
                    stderr.push_str(error.message());
                    validation_error = Some(error.message().to_owned());
                }
            }
        }
    }
    let success = status.success() && !timed_out && !interrupted && validation_error.is_none();
    let payload = json!({
        "ran": true,
        "success": success,
        "timedOut": timed_out,
        "interrupted": interrupted,
        "statusCode": status.code(),
        "stdout": stdout,
        "stderr": stderr,
        "errorCode": validation_error.as_ref().map(|_| "invalid_output"),
        "error": validation_error,
        "commandResult": command_result,
        "task": task,
    });
    write_bridge_run(paths, task_id, &payload)?;
    Ok(payload)
}

fn materialize_task_inputs(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
) -> Result<Value, CliError> {
    let mut materialized = task.clone();
    if task.get("type").and_then(Value::as_str) == Some("convert_document_to_markdown") {
        let document_id = task
            .pointer("/input/documentId")
            .and_then(Value::as_str)
            .or_else(|| task.get("documentId").and_then(Value::as_str));
        if let Some(document_id) = document_id {
            let original = crate::wiki::raw_document_original(paths, document_id)?;
            let inputs_dir = workspace.join("inputs").join("original");
            fs::create_dir_all(&inputs_dir).map_err(to_cli_error)?;
            let file_name = original
                .document
                .get("originalFileName")
                .and_then(Value::as_str)
                .unwrap_or("source.bin");
            let destination = inputs_dir.join(crate::paths::sanitize_path_part(file_name));
            fs::copy(&original.file_path, &destination).map_err(to_cli_error)?;
            materialized["executionInputs"]["originalDocument"] = json!({
                "documentId": document_id,
                "fileName": file_name,
                "filePath": destination,
                "mimeType": original.mime_type,
                "sizeBytes": original.size_bytes,
            });
        }
    }
    Ok(materialized)
}

struct ExecutionWorkspaceCleanup(PathBuf);

impl Drop for ExecutionWorkspaceCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
        if let Some(task_dir) = self.0.parent() {
            let _ = fs::remove_dir(task_dir);
        }
    }
}

fn is_wiki_authoring_task(task: &Value) -> bool {
    task.get("queue").and_then(Value::as_str) == Some("wiki")
        && matches!(
            task.get("type").and_then(Value::as_str),
            Some("ingest_markdown_into_wiki" | "maintain_wiki")
        )
}

fn is_document_conversion_task(task: &Value) -> bool {
    task.get("queue").and_then(Value::as_str) == Some("wiki")
        && task.get("type").and_then(Value::as_str) == Some("convert_document_to_markdown")
}

fn is_document_generation_task(task: &Value) -> bool {
    task.get("queue").and_then(Value::as_str) == Some("writing")
        && task.get("type").and_then(Value::as_str) == Some("generate_document")
}

fn is_writing_refinement_task(task: &Value) -> bool {
    task.get("queue").and_then(Value::as_str) == Some("writing")
        && task.get("type").and_then(Value::as_str) == Some("refine_writing_skill")
}

