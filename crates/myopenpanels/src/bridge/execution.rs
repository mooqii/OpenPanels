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
    let (execution_task, execution_bundle) = if agent_prompt {
        let prepared = prepare_execution_bundle(paths, task, &execution_workspace)?;
        (prepared.task, Some(prepared.bundle))
    } else {
        (
            materialize_task_inputs(paths, task, &execution_workspace)?,
            None,
        )
    };
    let task_input = execution_bundle.as_ref().map_or_else(
        || serde_json::to_string_pretty(&execution_task).map_err(to_cli_error),
        |bundle| Ok(bundle.instructions.clone()),
    )?;
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
    let mut validation_error_code = None;
    let mut runtime_finalization = Value::Null;
    let mut runtime_finalized = false;
    if agent_prompt && status.success() && !timed_out && !interrupted {
        let validation = execution_bundle.as_ref().map(|bundle| {
            finalize_execution_unit(
                paths,
                FinalizeExecutionUnitRequest {
                    task: &execution_task,
                    workspace: &execution_workspace,
                    handler_key: &bundle.handler_key,
                    execution_bundle_hash: &bundle.content_hash,
                    attempt_id: attempt_id.unwrap_or_default(),
                    execution_generation: execution_generation.unwrap_or_default(),
                    lease_token,
                    execution_token: execution_token.unwrap_or_default(),
                },
            )
        });
        if let Some(validation) = validation {
            match validation {
                Ok(finalization) => {
                    runtime_finalized = true;
                    command_result = finalization
                        .get("result")
                        .cloned()
                        .unwrap_or_else(|| finalization.clone());
                    if finalization.get("status").and_then(Value::as_str) != Some("succeeded") {
                        validation_error_code = finalization
                            .pointer("/error/code")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                            .or_else(|| Some("runtime_finalization_failed".to_owned()));
                        validation_error = Some(
                            finalization
                                .pointer("/error/message")
                                .and_then(Value::as_str)
                                .unwrap_or("Runtime Finalizer did not complete the Task.")
                                .to_owned(),
                        );
                    }
                    runtime_finalization = finalization;
                }
                Err(error) => {
                    runtime_finalized = true;
                    validation_error_code = Some(
                        error
                            .code()
                            .unwrap_or("runtime_finalization_failed")
                            .to_owned(),
                    );
                    if !stderr.trim().is_empty() {
                        stderr.push('\n');
                    }
                    stderr.push_str(error.message());
                    validation_error = Some(error.message().to_owned());
                    runtime_finalization = json!({
                        "taskId": task_id,
                        "status": "fenced",
                        "error": {
                            "code": error.code(),
                            "message": error.message(),
                        },
                    });
                }
            }
        }
    }
    let success = status.success() && !timed_out && !interrupted && validation_error.is_none();
    let runtime_lifecycle = runtime_finalization
        .get("lifecycle")
        .cloned()
        .unwrap_or(Value::Null);
    let payload = json!({
        "ran": true,
        "success": success,
        "timedOut": timed_out,
        "interrupted": interrupted,
        "statusCode": status.code(),
        "stdout": stdout,
        "stderr": stderr,
        "errorCode": validation_error_code,
        "error": validation_error,
        "commandResult": command_result,
        "runtimeFinalized": runtime_finalized,
        "runtimeFinalization": runtime_finalization,
        "lifecycle": runtime_lifecycle,
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
    if task
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(crate::publishing::is_publishing_task_type)
    {
        materialize_publishing_inputs(paths, task, workspace, &mut materialized)?;
    }
    if task.get("type").and_then(Value::as_str) == Some(crate::typesetting::COVER_TASK_TYPE) {
        materialize_typesetting_cover_inputs(paths, task, workspace, &mut materialized)?;
    }
    Ok(materialized)
}

fn materialize_typesetting_cover_inputs(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
    materialized: &mut Value,
) -> Result<(), CliError> {
    let storage = crate::storage::Storage::open(paths)?;
    let inputs_dir = workspace.join("inputs");
    let skill_dir = inputs_dir.join("skill");
    fs::create_dir_all(&skill_dir).map_err(to_cli_error)?;
    let title_path = inputs_dir.join("title.txt");
    let body_path = inputs_dir.join("body.txt");
    fs::write(
        &title_path,
        task.pointer("/input/snapshot/title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .as_bytes(),
    )
    .map_err(to_cli_error)?;
    fs::write(
        &body_path,
        task.pointer("/input/snapshot/bodyText")
            .and_then(Value::as_str)
            .unwrap_or("")
            .as_bytes(),
    )
    .map_err(to_cli_error)?;

    let mut skill_files = Vec::new();
    for file in task
        .pointer("/input/coverSkillSnapshot/files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let relative = file.get("path").and_then(Value::as_str).ok_or_else(|| {
            CliError::with_code("invalid_target", "Cover Skill file path is missing.")
        })?;
        let asset_ref = file
            .get("assetRef")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                CliError::with_code("invalid_target", "Cover Skill asset reference is missing.")
            })?;
        let bytes = storage.read_asset(asset_ref)?;
        let actual_hash = format!("sha256:{:x}", Sha256::digest(&bytes));
        if file.get("contentHash").and_then(Value::as_str) != Some(actual_hash.as_str()) {
            return Err(CliError::with_code(
                "cover_skill_snapshot_corrupt",
                format!("Cover Skill snapshot failed integrity validation: {relative}"),
            ));
        }
        let mut destination = skill_dir.clone();
        for part in Path::new(relative).components() {
            let std::path::Component::Normal(part) = part else {
                return Err(CliError::with_code(
                    "invalid_target",
                    "Cover Skill path is unsafe.",
                ));
            };
            destination.push(sanitize_path_part(&part.to_string_lossy()));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(to_cli_error)?;
        }
        fs::write(&destination, &bytes).map_err(to_cli_error)?;
        skill_files.push(json!({
            "path": relative,
            "filePath": destination,
            "contentHash": actual_hash,
        }));
    }
    let mut manifest_hash = Sha256::new();
    for file in &skill_files {
        manifest_hash.update(file.get("path").and_then(Value::as_str).unwrap_or("").as_bytes());
        manifest_hash.update(
            file.get("contentHash")
                .and_then(Value::as_str)
                .unwrap_or("")
                .as_bytes(),
        );
    }
    let actual_manifest_hash = format!("sha256:{:x}", manifest_hash.finalize());
    if task
        .pointer("/input/coverSkillSnapshot/contentHash")
        .and_then(Value::as_str)
        != Some(actual_manifest_hash.as_str())
    {
        return Err(CliError::with_code(
            "cover_skill_snapshot_corrupt",
            "Cover Skill snapshot manifest failed integrity validation.",
        ));
    }
    let skill_file_path = skill_dir.join("SKILL.md");
    if !skill_file_path.is_file() {
        return Err(CliError::with_code(
            "cover_skill_snapshot_corrupt",
            "Cover Skill snapshot has no SKILL.md.",
        ));
    }
    materialized["executionInputs"]["typesettingCover"] = json!({
        "titleFilePath": title_path,
        "bodyFilePath": body_path,
        "skillFilePath": skill_file_path,
        "skillDirectory": skill_dir,
        "skillFiles": skill_files,
    });
    Ok(())
}

fn materialize_publishing_inputs(
    paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
    materialized: &mut Value,
) -> Result<(), CliError> {
    let storage = crate::storage::Storage::open(paths)?;
    let inputs_dir = workspace.join("inputs");
    let media_dir = inputs_dir.join("media");
    let skill_dir = inputs_dir.join("skill");
    fs::create_dir_all(&media_dir).map_err(to_cli_error)?;
    fs::create_dir_all(&skill_dir).map_err(to_cli_error)?;

    let title = task
        .pointer("/input/snapshot/title")
        .and_then(Value::as_str)
        .unwrap_or("");
    let body = task
        .pointer("/input/snapshot/bodyText")
        .and_then(Value::as_str)
        .unwrap_or("");
    let title_path = inputs_dir.join("title.txt");
    let body_path = inputs_dir.join("body.txt");
    fs::write(&title_path, title.as_bytes()).map_err(to_cli_error)?;
    fs::write(&body_path, body.as_bytes()).map_err(to_cli_error)?;

    let mut media_files = Vec::new();
    for (index, media) in task
        .pointer("/input/snapshot/media")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let asset_ref = media
            .get("assetRef")
            .and_then(Value::as_str)
            .ok_or_else(|| CliError::with_code("invalid_target", "Publishing media is missing its asset reference."))?;
        let bytes = storage.read_asset(asset_ref)?;
        let actual_hash = format!("sha256:{:x}", Sha256::digest(&bytes));
        if media
            .get("contentHash")
            .and_then(Value::as_str)
            .is_some_and(|expected| expected != actual_hash)
        {
            return Err(CliError::with_code(
                "publishing_snapshot_corrupt",
                format!("Publishing media failed integrity validation: {asset_ref}"),
            ));
        }
        let name = media
            .get("fileName")
            .and_then(Value::as_str)
            .unwrap_or("image");
        let destination = media_dir.join(format!(
            "{:03}-{}",
            index + 1,
            sanitize_path_part(name)
        ));
        fs::write(&destination, &bytes).map_err(to_cli_error)?;
        media_files.push(json!({
            "index": index + 1,
            "isPrimary": index == 0,
            "filePath": destination,
            "fileName": name,
            "mimeType": media.get("mimeType").cloned().unwrap_or_else(|| json!("image/*")),
            "contentHash": actual_hash,
        }));
    }

    let mut skill_files = Vec::new();
    for file in task
        .pointer("/input/publishingSkillSnapshot/files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let relative = file
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| CliError::with_code("invalid_target", "Publishing Skill file path is missing."))?;
        let asset_ref = file
            .get("assetRef")
            .and_then(Value::as_str)
            .ok_or_else(|| CliError::with_code("invalid_target", "Publishing Skill asset reference is missing."))?;
        let bytes = storage.read_asset(asset_ref)?;
        let actual_hash = format!("sha256:{:x}", Sha256::digest(&bytes));
        if file.get("contentHash").and_then(Value::as_str) != Some(actual_hash.as_str()) {
            return Err(CliError::with_code(
                "publishing_snapshot_corrupt",
                format!("Publishing Skill snapshot file failed integrity validation: {relative}"),
            ));
        }
        let mut destination = skill_dir.clone();
        for part in Path::new(relative).components() {
            let std::path::Component::Normal(part) = part else {
                return Err(CliError::with_code("invalid_target", "Publishing Skill path is unsafe."));
            };
            destination.push(sanitize_path_part(&part.to_string_lossy()));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(to_cli_error)?;
        }
        fs::write(&destination, &bytes).map_err(to_cli_error)?;
        skill_files.push(json!({
            "path": relative,
            "filePath": destination,
            "contentHash": actual_hash,
        }));
    }
    let mut skill_manifest_hash = Sha256::new();
    for file in &skill_files {
        skill_manifest_hash.update(file.get("path").and_then(Value::as_str).unwrap_or("").as_bytes());
        skill_manifest_hash.update(
            file.get("contentHash")
                .and_then(Value::as_str)
                .unwrap_or("")
                .as_bytes(),
        );
    }
    let actual_skill_hash = format!("sha256:{:x}", skill_manifest_hash.finalize());
    if task
        .pointer("/input/publishingSkillSnapshot/contentHash")
        .and_then(Value::as_str)
        != Some(actual_skill_hash.as_str())
    {
        return Err(CliError::with_code(
            "publishing_snapshot_corrupt",
            "Publishing Skill snapshot manifest failed integrity validation.",
        ));
    }

    materialized["executionInputs"]["publishing"] = json!({
        "titleFilePath": title_path,
        "bodyFilePath": body_path,
        "media": media_files,
        "skillDirectory": skill_dir,
        "skillFiles": skill_files,
    });
    Ok(())
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
