fn local_agent_task_prompt(paths: &MyOpenPanelsPaths, task: &Value) -> Result<String, CliError> {
    let task_json = serde_json::to_string_pretty(task).map_err(to_cli_error)?;
    let task_id = task.get("id").and_then(Value::as_str).unwrap_or("");
    let capability = task.get("capability").and_then(Value::as_str).unwrap_or("");
    let task_type = task.get("type").and_then(Value::as_str).unwrap_or("");
    let document_id = task
        .get("input")
        .and_then(|input| input.get("documentId"))
        .and_then(Value::as_str)
        .or_else(|| {
            task.get("source")
                .and_then(|source| source.get("documentId"))
                .and_then(Value::as_str)
        })
        .unwrap_or("");
    let wiki_space_id = task
        .get("source")
        .and_then(|source| source.get("wikiSpaceId"))
        .or_else(|| task.get("input").and_then(|input| input.get("wikiSpaceId")))
        .and_then(Value::as_str)
        .unwrap_or("wiki:default");
    let cli = std::env::var("MYOPENPANELS_CLI")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .map(|path| path.display().to_string())
        })
        .unwrap_or_else(|| "myopenpanels".to_owned());
    let wiki_skill = if task.get("queue").and_then(Value::as_str) == Some("wiki") {
        task.pointer("/source/agentSkillId")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                crate::wiki::wiki_context(paths).ok().map(|context| {
                    crate::wiki::selected_agent_skill_id(&context["state"]).to_owned()
                })
            })
            .unwrap_or_else(|| "karpathy-llm-wiki".to_owned())
    } else {
        String::new()
    };
    let wiki_steps = match task_type {
        "ingest_markdown_into_wiki" => format!(
            "Load the Wiki panel contract with `{cli} agent skill read --skill-id wiki-panel --task-id {task_id} --format json`, then read its authoring routing reference. Read the source with `{cli} wiki raw read --raw-document-id {document_id} --format json`. Load the selected authoring skill with `{cli} agent skill read --skill-id {wiki_skill} --task-id {task_id} --format json`, read its SKILL.md and routed references, then create useful Wiki pages with `{cli} wiki page create --space-id {wiki_space_id} --path <path.md> --content-file <file> --task-id {task_id} --format json`; use `wiki page update` with the same arguments when revising a page."
        ),
        "maintain_wiki" => format!(
            "Load the Wiki panel contract with `{cli} agent skill read --skill-id wiki-panel --task-id {task_id} --format json`, then load the selected authoring skill with `{cli} agent skill read --skill-id {wiki_skill} --task-id {task_id} --format json`. Follow that Skill while maintaining `{wiki_space_id}` in response to the changed paths and reasons in the Task JSON; use task-bound `wiki page create` and `wiki page update` commands."
        ),
        _ if task.get("queue").and_then(Value::as_str) == Some("wiki") => format!(
            "Load the task Skill and `{cli} agent catalog --domain wiki --format json`, then perform the requested Wiki writes using `--task-id {task_id}`."
        ),
        _ => format!(
            "Use `{cli} agent catalog --format json` to find the domain for task capability `{capability}`, load that complete domain catalog, then perform the requested panel writes."
        ),
    };
    Ok(format!(
        "You are the local MyOpenPanels agent target. Process exactly one already-claimed task, then stop.\n\nThis is an isolated protocol-v3 Task execution. Do not run Agent Bootstrap or start Studio; the Task Broker provides the captured context. Do not modify MyOpenPanels application source code. Use the agent-facing CLI commands only, always passing the supplied task id on writes. The bridge owns claim, heartbeat, complete, fail, and retry; do not call task lifecycle commands yourself. Exit nonzero if you cannot perform the requested panel writes reliably.\n\nCapability: {capability}\nTask id: {task_id}\n\n{wiki_steps}\n\nVerify the requested writes before exiting. Keep the final response brief.\n\nTask JSON:\n{task_json}\n\nContext: {}",
        paths.context_id,
    ))
}

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
