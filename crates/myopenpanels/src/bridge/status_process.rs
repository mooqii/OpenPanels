fn read_pipe(mut pipe: Option<impl Read>) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    if let Some(pipe) = pipe.as_mut() {
        pipe.read_to_end(&mut bytes)?;
    }
    Ok(bytes)
}

fn recover_lifecycle_after_error(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    error: &CliError,
) -> Result<Value, CliError> {
    let task = tasks::inspect_task(paths, task_id)?;
    if matches!(error.code(), Some("invalid_lease" | "lease_expired"))
        && task_is_terminal(&task["task"])
    {
        return Ok(task);
    }
    Ok(task)
}

fn task_is_terminal(task: &Value) -> bool {
    matches!(
        task.get("status").and_then(Value::as_str),
        Some("succeeded" | "failed" | "cancelled")
    )
}

fn truncate_output(bytes: &[u8], limit: usize) -> String {
    if bytes.len() <= limit {
        return String::from_utf8_lossy(bytes).to_string();
    }
    let mut text = String::from_utf8_lossy(&bytes[..limit]).to_string();
    text.push_str("\n[myopenpanels: output truncated]");
    text
}

fn write_bridge_run(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    payload: &Value,
) -> Result<(), CliError> {
    let runs_dir = paths.context_dir.join("bridge-runs");
    fs::create_dir_all(&runs_dir).map_err(to_cli_error)?;
    let timestamp = crate::control::now_iso();
    let file_name = format!(
        "{}-{}.json",
        sanitize_path_part(task_id),
        sanitize_path_part(&timestamp)
    );
    let content = serde_json::to_string_pretty(&json!({
        "createdAt": timestamp,
        "run": payload,
    }))
    .map_err(to_cli_error)?;
    fs::write(runs_dir.join(file_name), content).map_err(to_cli_error)
}

fn write_bridge_status(
    paths: &MyOpenPanelsPaths,
    status: &str,
    current_task: Option<&Value>,
    last_task: Option<&Value>,
    last_error: Option<&str>,
) -> Result<(), CliError> {
    static WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = crate::control::now_iso();
    let mut existing = read_bridge_status(paths).unwrap_or_else(|_| json!({}));
    if !existing.is_object() {
        existing = json!({});
    }
    let object = existing
        .as_object_mut()
        .ok_or_else(|| CliError::new("Bridge status is invalid."))?;
    object.insert("status".to_owned(), json!(status));
    object.insert("heartbeatAt".to_owned(), json!(now));
    object.insert("updatedAt".to_owned(), json!(now));
    object.insert(
        "currentTask".to_owned(),
        current_task.cloned().unwrap_or(Value::Null),
    );
    if let Some(last_task) = last_task {
        object.insert("lastTask".to_owned(), last_task.clone());
    } else {
        object.entry("lastTask").or_insert_with(|| Value::Null);
    }
    if let Some(last_error) = last_error {
        object.insert("lastError".to_owned(), json!(last_error));
    } else if status != "error" {
        object.insert("lastError".to_owned(), Value::Null);
    } else {
        object.entry("lastError").or_insert_with(|| Value::Null);
    }
    let path = bridge_status_path(paths);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(
        path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&existing).map_err(to_cli_error)?
        ),
    )
    .map_err(to_cli_error)
}

fn bridge_status_path(paths: &MyOpenPanelsPaths) -> PathBuf {
    paths.context_dir.join("agent-bridge-status.json")
}

#[cfg(unix)]
fn shell_command(command: &str) -> Command {
    use std::os::unix::process::CommandExt;

    let mut child = Command::new("sh");
    child.arg("-c").arg(command);
    child.process_group(0);
    child
}

#[cfg(unix)]
fn terminate_child_process(child: &mut std::process::Child) {
    let process_group = format!("-{}", child.id());
    let _ = Command::new("kill")
        .arg("-TERM")
        .arg(&process_group)
        .status();
    std::thread::sleep(Duration::from_millis(250));
    if matches!(child.try_wait(), Ok(None)) {
        let _ = Command::new("kill")
            .arg("-KILL")
            .arg(&process_group)
            .status();
    }
}

#[cfg(windows)]
fn terminate_child_process(child: &mut std::process::Child) {
    let _ = child.kill();
}

#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    let mut child = Command::new("cmd");
    child.arg("/C").arg(command);
    child
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
