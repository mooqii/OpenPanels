use super::*;

pub fn list_agent_targets(paths: &OpenPanelsPaths) -> Result<Value, CliError> {
    Ok(json!({ "targets": read_agent_targets(paths)? }))
}

pub fn register_agent_target(
    paths: &OpenPanelsPaths,
    host: &str,
    thread_id: &str,
    wake_url: Option<&str>,
) -> Result<Value, CliError> {
    let now = now_iso();
    let target = json!({
        "id": create_id("target"),
        "host": host,
        "threadId": thread_id,
        "projectDir": paths.project_dir,
        "contextId": paths.context_id,
        "wakeUrl": wake_url,
        "createdAt": now,
        "updatedAt": now,
    });
    let mut targets = read_agent_targets(paths)?;
    targets.retain(|item| {
        !(item.get("host").and_then(Value::as_str) == Some(host)
            && item.get("threadId").and_then(Value::as_str) == Some(thread_id))
    });
    targets.insert(0, target.clone());
    write_json(&agent_targets_path(paths), &Value::Array(targets.clone()))?;
    Ok(json!({ "target": target, "targets": targets }))
}

pub(super) fn save_process(
    paths: &OpenPanelsPaths,
    wiki: &WikiBootstrapValue,
    process: &Value,
) -> Result<(), CliError> {
    let Some(process_id) = process.get("id").and_then(Value::as_str) else {
        return Ok(());
    };
    let storage = Storage::open(paths)?;
    let path = wiki_panel_path(
        &storage.panel_dir(&wiki.session.id, &wiki.panel.id),
        &wiki_ref(&["processes", &format!("{process_id}.json")]),
    )?;
    write_json(&path, process)
}

pub(super) fn wake_queued_wiki_tasks(
    paths: &OpenPanelsPaths,
    wiki: &WikiBootstrapValue,
    task_ids: Option<&[String]>,
    allow_local_worker_from_agent_worker: bool,
) -> Result<(), CliError> {
    let targets = read_agent_targets(paths)?;
    let wakeups_dir = paths.context_dir.join("wakeups");
    let runs_dir = paths.context_dir.join("agent-runs");
    fs::create_dir_all(&wakeups_dir).map_err(to_cli_error)?;
    let queued_tasks = wiki
        .state
        .get("tasks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|task| {
            task.get("status").and_then(Value::as_str) == Some("queued")
                && task_ids.is_none_or(|ids| {
                    task.get("id")
                        .and_then(Value::as_str)
                        .is_some_and(|task_id| ids.iter().any(|id| id == task_id))
                })
        })
        .collect::<Vec<_>>();
    if queued_tasks.is_empty() {
        return Ok(());
    }

    for task in queued_tasks {
        let task_id = task.get("id").and_then(Value::as_str).unwrap_or("unknown");
        let wakeup_path = wakeups_dir.join(format!("{}.json", sanitize_path_part(task_id)));
        let run_path = runs_dir.join(format!("{}.json", sanitize_path_part(task_id)));
        let already_woken = wakeup_path.exists();
        let local_run_exists = run_path.exists();
        let message = wiki_wakeup_message(paths, wiki, &task);
        write_json(&wakeup_path, &message)?;

        let mut sent_to_target = false;
        for target in &targets {
            if let Some(wake_url) = target.get("wakeUrl").and_then(Value::as_str) {
                let payload = json!({
                    "projectDir": message["projectDir"],
                    "storageDir": message["storageDir"],
                    "contextId": message["contextId"],
                    "sessionId": message["sessionId"],
                    "wikiPanelId": message["wikiPanelId"],
                    "taskId": message["taskId"],
                    "taskType": message["taskType"],
                    "targetId": message["targetId"],
                    "documentId": message["documentId"],
                    "wikiSpaceId": message["wikiSpaceId"],
                    "wikiLanguage": message["wikiLanguage"],
                    "wikiLanguageLabel": message["wikiLanguageLabel"],
                    "createdAt": message["createdAt"],
                    "originalFilePath": message["originalFilePath"],
                    "target": target,
                });
                if ureq::post(wake_url)
                    .set("content-type", "application/json")
                    .send_json(payload)
                    .is_ok()
                {
                    sent_to_target = true;
                }
            }
        }
        if !(sent_to_target || (already_woken && local_run_exists)) {
            wake_local_agent_worker(paths, &message, allow_local_worker_from_agent_worker)?;
        }
    }
    Ok(())
}

fn wiki_wakeup_message(paths: &OpenPanelsPaths, wiki: &WikiBootstrapValue, task: &Value) -> Value {
    json!({
        "projectDir": paths.project_dir,
        "storageDir": paths.storage_dir,
        "contextId": paths.context_id,
        "sessionId": wiki.session.id,
        "wikiPanelId": wiki.panel.id,
        "taskId": task.get("id").cloned().unwrap_or(Value::Null),
        "taskType": task.get("type").cloned().unwrap_or(Value::Null),
        "targetId": task.get("targetId").cloned().unwrap_or(Value::Null),
        "documentId": task.get("documentId").cloned().unwrap_or(Value::Null),
        "wikiSpaceId": task.get("wikiSpaceId").cloned().unwrap_or(Value::Null),
        "wikiLanguage": wiki.state.get("wikiLanguage").cloned().unwrap_or(Value::Null),
        "wikiLanguageLabel": wiki_language_label(wiki.state.get("wikiLanguage").and_then(Value::as_str)),
        "createdAt": now_iso(),
        "originalFilePath": original_file_path_for_task(paths, wiki, task),
    })
}

fn wake_local_agent_worker(
    paths: &OpenPanelsPaths,
    message: &Value,
    allow_local_worker_from_agent_worker: bool,
) -> Result<(), CliError> {
    let Some(worker) =
        resolve_local_agent_worker(paths, message, allow_local_worker_from_agent_worker)
    else {
        return Ok(());
    };
    let task_id = message
        .get("taskId")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let runs_dir = paths.context_dir.join("agent-runs");
    fs::create_dir_all(&runs_dir).map_err(to_cli_error)?;
    let run_path = runs_dir.join(format!("{}.json", sanitize_path_part(task_id)));
    let created = json!({
        "host": worker.host,
        "status": "spawning",
        "message": message,
        "createdAt": now_iso(),
    });
    let mut run_file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&run_path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
        Err(error) => return Err(to_cli_error(error)),
    };
    write!(
        run_file,
        "{}\n",
        serde_json::to_string_pretty(&created).map_err(to_cli_error)?
    )
    .map_err(to_cli_error)?;

    let log_path = runs_dir.join(format!("{}.log", sanitize_path_part(task_id)));
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(to_cli_error)?;
    let log_for_stderr = log.try_clone().map_err(to_cli_error)?;
    let mut command = Command::new(&worker.executable);
    trace::record(TraceEventInput {
        audience: None,
        category: Some("agent".to_owned()),
        detail: Some(json!({
            "executable": worker.executable.clone(),
            "args": worker.args.clone(),
            "stdin": worker.stdin.clone(),
            "task": message,
        })),
        direction: Some("spawn".to_owned()),
        release_summary: Some("Started local agent worker".to_owned()),
        run_id: Some(task_id.to_owned()),
        source: Some(worker.host.clone()),
        summary: Some(format!("Spawning {} for task {task_id}", worker.host)),
        task_id: Some(task_id.to_owned()),
    });
    command
        .args(&worker.args)
        .env("OPENPANELS_AGENT_WORKER", "1")
        .env("OPENPANELS_PROJECT_DIR", &paths.project_dir)
        .env("OPENPANELS_STORAGE_DIR", &paths.storage_dir)
        .env("OPENPANELS_TRACE_RUN_ID", task_id)
        .env("OPENPANELS_TRACE_AUDIENCE", "development")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = &worker.cwd {
        command.current_dir(cwd);
    }
    match command.spawn() {
        Ok(mut child) => {
            if let Some(stdin) = &worker.stdin {
                if let Some(mut child_stdin) = child.stdin.take() {
                    let _ = child_stdin.write_all(stdin.as_bytes());
                }
            }
            if let Some(stdout) = child.stdout.take() {
                trace_agent_pipe(
                    stdout,
                    log,
                    "stdout",
                    task_id.to_owned(),
                    worker.host.clone(),
                );
            }
            if let Some(stderr) = child.stderr.take() {
                trace_agent_pipe(
                    stderr,
                    log_for_stderr,
                    "stderr",
                    task_id.to_owned(),
                    worker.host.clone(),
                );
            }
            let host = worker.host;
            let message = message.clone();
            thread::spawn(move || {
                let status = child.wait();
                let success = status.as_ref().is_ok_and(|status| status.success());
                let payload = match &status {
                    Ok(status) => json!({
                        "host": host,
                        "status": if status.success() { "finished" } else { "exited" },
                        "code": status.code(),
                        "signal": Value::Null,
                        "logPath": log_path,
                        "message": message,
                        "updatedAt": now_iso(),
                    }),
                    Err(error) => json!({
                        "host": host,
                        "status": "spawn_failed",
                        "logPath": log_path,
                        "message": message,
                        "error": error.to_string(),
                        "updatedAt": now_iso(),
                    }),
                };
                trace::record(TraceEventInput {
                    audience: None,
                    category: Some(if success {
                        "agent".to_owned()
                    } else {
                        "error".to_owned()
                    }),
                    detail: Some(payload.clone()),
                    direction: Some("exit".to_owned()),
                    release_summary: Some(if success {
                        "Agent worker finished".to_owned()
                    } else {
                        "Agent worker exited with an error".to_owned()
                    }),
                    run_id: Some(
                        message
                            .get("taskId")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                            .to_owned(),
                    ),
                    source: Some(host.clone()),
                    summary: Some(format!(
                        "{} exited for task {}",
                        host,
                        message
                            .get("taskId")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                    )),
                    task_id: message
                        .get("taskId")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                });
                let _ = write_json(&run_path, &payload);
            });
        }
        Err(error) => {
            let payload = json!({
                "host": worker.host,
                "status": "spawn_failed",
                "message": message,
                "error": error.to_string(),
                "updatedAt": now_iso(),
            });
            trace::record(TraceEventInput {
                audience: None,
                category: Some("error".to_owned()),
                detail: Some(payload.clone()),
                direction: Some("spawn".to_owned()),
                release_summary: Some("Failed to start local agent worker".to_owned()),
                run_id: Some(task_id.to_owned()),
                source: Some(worker.host.clone()),
                summary: Some(format!("Failed to spawn {}: {}", worker.host, error)),
                task_id: Some(task_id.to_owned()),
            });
            write_json(&run_path, &payload)?;
        }
    }
    Ok(())
}

fn trace_agent_pipe<R: Read + Send + 'static>(
    mut reader: R,
    mut log: fs::File,
    stream: &'static str,
    task_id: String,
    host: String,
) {
    thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        loop {
            let read = match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => read,
                Err(_) => break,
            };
            let chunk = &buffer[..read];
            let _ = log.write_all(chunk);
            let text = String::from_utf8_lossy(chunk).to_string();
            trace::record(TraceEventInput {
                audience: None,
                category: Some("agent".to_owned()),
                detail: Some(json!({ "stream": stream, "text": text })),
                direction: Some(stream.to_owned()),
                release_summary: None,
                run_id: Some(task_id.clone()),
                source: Some(host.clone()),
                summary: Some(format!("{host} {stream}: {text}")),
                task_id: Some(task_id.clone()),
            });
        }
    });
}

struct LocalAgentWorker {
    args: Vec<String>,
    cwd: Option<PathBuf>,
    executable: String,
    host: String,
    stdin: Option<String>,
}

fn resolve_local_agent_worker(
    paths: &OpenPanelsPaths,
    message: &Value,
    allow_local_worker_from_agent_worker: bool,
) -> Option<LocalAgentWorker> {
    if !should_wake_local_agent_worker(allow_local_worker_from_agent_worker) {
        return None;
    }
    let requested = env::var("OPENPANELS_LOCAL_AGENT_HOST").ok();
    let codex = find_executable("codex", env::var("OPENPANELS_CODEX_EXECUTABLE").ok());
    let hermes = find_executable("hermes", env::var("OPENPANELS_HERMES_EXECUTABLE").ok());
    let preference = requested.or_else(|| {
        if has_codex_agent_environment() {
            Some("codex".to_owned())
        } else if has_hermes_agent_environment() {
            Some("hermes".to_owned())
        } else {
            None
        }
    });

    match preference.as_deref() {
        Some("hermes") => hermes.map(|executable| hermes_worker(paths, message, executable)),
        Some("codex") => codex.map(|executable| codex_worker(paths, message, executable)),
        _ => codex
            .map(|executable| codex_worker(paths, message, executable))
            .or_else(|| hermes.map(|executable| hermes_worker(paths, message, executable))),
    }
}

fn codex_worker(paths: &OpenPanelsPaths, message: &Value, executable: String) -> LocalAgentWorker {
    let mut args = vec![
        "exec".to_owned(),
        "--cd".to_owned(),
        paths.project_dir.display().to_string(),
        "--add-dir".to_owned(),
        paths.storage_dir.display().to_string(),
        "--dangerously-bypass-approvals-and-sandbox".to_owned(),
        "-".to_owned(),
    ];
    if let Some(original_file_path) = message.get("originalFilePath").and_then(Value::as_str) {
        if is_image_file(original_file_path) {
            args.splice(1..1, ["--image".to_owned(), original_file_path.to_owned()]);
        }
    }
    LocalAgentWorker {
        args,
        cwd: None,
        executable,
        host: "codex-cli".to_owned(),
        stdin: Some(local_agent_worker_prompt(paths, message, "codex-cli")),
    }
}

fn hermes_worker(paths: &OpenPanelsPaths, message: &Value, executable: String) -> LocalAgentWorker {
    LocalAgentWorker {
        args: vec![
            "--yolo".to_owned(),
            "--accept-hooks".to_owned(),
            "--pass-session-id".to_owned(),
            "--oneshot".to_owned(),
            local_agent_worker_prompt(paths, message, "hermes"),
        ],
        cwd: Some(paths.project_dir.clone()),
        executable,
        host: "hermes".to_owned(),
        stdin: None,
    }
}

fn should_wake_local_agent_worker(allow_local_worker_from_agent_worker: bool) -> bool {
    if env::var("OPENPANELS_DISABLE_LOCAL_AGENT").ok().as_deref() == Some("1") {
        return false;
    }
    if cfg!(test) || env::var("NODE_ENV").ok().as_deref() == Some("test") {
        return false;
    }
    if env::var("OPENPANELS_AGENT_WORKER").ok().as_deref() == Some("1")
        && !allow_local_worker_from_agent_worker
    {
        return false;
    }
    env::var("OPENPANELS_ENABLE_LOCAL_AGENT").ok().as_deref() == Some("1")
        || has_codex_agent_environment()
        || has_hermes_agent_environment()
}

fn has_codex_agent_environment() -> bool {
    env::var("CODEX_THREAD_ID").is_ok()
        || env::var("CODEX_SHELL").is_ok()
        || env::var("CODEX_INTERNAL_ORIGINATOR_OVERRIDE").is_ok()
}

fn has_hermes_agent_environment() -> bool {
    env::var("HERMES_THREAD_ID").is_ok()
        || env::var("HERMES_CONVERSATION_ID").is_ok()
        || env::var("HERMES_SESSION_ID").is_ok()
        || env::var("HERMES_PROFILE").is_ok()
        || env::var("HERMES_HOME").is_ok()
}

fn find_executable(name: &str, override_path: Option<String>) -> Option<String> {
    if let Some(path) = override_path {
        if file_looks_available(&path) {
            return Some(path);
        }
    }
    for directory in env::var_os("PATH")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default()
    {
        let candidate = directory.join(name);
        if file_looks_available(candidate.to_string_lossy().as_ref()) {
            return Some(candidate.display().to_string());
        }
    }
    env::var_os("HOME").and_then(|home| {
        let candidate = PathBuf::from(home).join(".local").join("bin").join(name);
        file_looks_available(candidate.to_string_lossy().as_ref())
            .then(|| candidate.display().to_string())
    })
}

fn file_looks_available(path: &str) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

fn local_agent_worker_prompt(paths: &OpenPanelsPaths, message: &Value, agent_host: &str) -> String {
    let cli = preferred_local_cli_command();
    let task_type = message
        .get("taskType")
        .and_then(Value::as_str)
        .unwrap_or("");
    let document_id = message
        .get("documentId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let original_path = message
        .get("originalFilePath")
        .and_then(Value::as_str)
        .unwrap_or("");
    let wiki_space_id = message
        .get("wikiSpaceId")
        .and_then(Value::as_str)
        .unwrap_or("wiki:default");
    let wiki_language = message
        .get("wikiLanguageLabel")
        .and_then(Value::as_str)
        .unwrap_or("the wiki panel language selected by the user");
    let task_id = message.get("taskId").and_then(Value::as_str).unwrap_or("");
    let common_flags = format!(
        "--project {} --storage-dir {} --context-id {} --format json",
        shell_quote(paths.project_dir.display().to_string().as_str()),
        shell_quote(paths.storage_dir.display().to_string().as_str()),
        shell_quote(paths.context_id.as_str())
    );

    format!(
        r#"You are a MyOpenPanels wiki agent worker. Process exactly one queued wiki task, then stop.

Do not modify application source code. Only read project files as needed and write MyOpenPanels wiki data through the CLI/API.

Task:
{}

Use this CLI command prefix:
{} {}

Required workflow:
1. Claim the task:
   {} {} wiki tasks claim --task-id {} --agent-host {} --thread-id {}
2. Process according to taskType.
   - Use {} for any newly generated structured wiki pages, index entries, summaries, and log text. Do not rewrite already-generated wiki content solely to translate it.
3. On success, call:
   {} {} wiki tasks complete --task-id {}
4. If the task cannot be completed reliably, call:
   {} {} wiki tasks fail --task-id {} --message <short reason>

For taskType={}:
- convert_document_to_markdown:
  - Source document id: {}
  - Original file path: {}
  - Convert the original file into clean Markdown. Preserve titles, headings, lists, tables, quoted text, and useful image/file placeholders.
  - Write the Markdown to a temporary .md file, then run:
    {} {} wiki markdown write --document-id {} --file <temporary-md-file> --task-id {}
  - Complete the conversion task. Completing it will enqueue the follow-up structured wiki ingest task automatically.
- ingest_markdown_into_wiki:
  - Read source Markdown:
    {} {} wiki markdown read --document-id {}
  - Update the target wiki space {}: create/update a source page under sources/, update relevant topic/category pages when useful, update index.md, and append log.md.
  - Use page writes with --task-id {} so your own wiki edits do not enqueue redundant rebuild tasks:
    {} {} wiki pages write --wiki-space-id {} --path <page-path> --file <md-file> --task-id {}
  - Complete the task.
- rebuild_wiki_index:
  - Read current pages for wiki space {}, rebuild index.md and append log.md so deleted/edited sources are reflected.
  - Use wiki pages write with --task-id {}.
  - Complete the task.

Keep the final response brief."#,
        serde_json::to_string_pretty(message).unwrap_or_else(|_| "{}".to_owned()),
        cli,
        common_flags,
        cli,
        common_flags,
        shell_quote(task_id),
        shell_quote(agent_host),
        shell_quote(local_agent_thread_id(agent_host).as_str()),
        wiki_language,
        cli,
        common_flags,
        shell_quote(task_id),
        cli,
        common_flags,
        shell_quote(task_id),
        task_type,
        document_id,
        if original_path.is_empty() {
            "(read it from raw document metadata)"
        } else {
            original_path
        },
        cli,
        common_flags,
        shell_quote(document_id),
        shell_quote(task_id),
        cli,
        common_flags,
        shell_quote(document_id),
        wiki_space_id,
        shell_quote(task_id),
        cli,
        common_flags,
        shell_quote(wiki_space_id),
        shell_quote(task_id),
        wiki_space_id,
        shell_quote(task_id),
    )
}

fn preferred_local_cli_command() -> String {
    if let Ok(cli) = env::var("OPENPANELS_LOCAL_CLI") {
        return shell_quote(&cli);
    }
    env::current_exe()
        .ok()
        .map(|path| shell_quote(path.display().to_string().as_str()))
        .unwrap_or_else(|| "openpanels-local".to_owned())
}

fn local_agent_thread_id(agent_host: &str) -> String {
    if agent_host == "hermes" {
        return env::var("HERMES_THREAD_ID")
            .or_else(|_| env::var("HERMES_CONVERSATION_ID"))
            .or_else(|_| env::var("HERMES_SESSION_ID"))
            .unwrap_or_else(|_| "hermes-oneshot".to_owned());
    }
    env::var("CODEX_THREAD_ID").unwrap_or_else(|_| "codex-exec".to_owned())
}

fn original_file_path_for_task(
    paths: &OpenPanelsPaths,
    wiki: &WikiBootstrapValue,
    task: &Value,
) -> Value {
    let Some(document_id) = task.get("documentId").and_then(Value::as_str) else {
        return Value::Null;
    };
    let Ok(document) = find_document(&wiki.state, document_id) else {
        return Value::Null;
    };
    let Some(original_ref) = document.get("originalRef").and_then(Value::as_str) else {
        return Value::Null;
    };
    let Ok(storage) = Storage::open(paths) else {
        return Value::Null;
    };
    match wiki_panel_path(
        &storage.panel_dir(&wiki.session.id, &wiki.panel.id),
        original_ref,
    ) {
        Ok(path) => json!(path),
        Err(_) => Value::Null,
    }
}

fn wiki_language_label(language: Option<&str>) -> &'static str {
    match language {
        Some("zh-CN") => "Simplified Chinese",
        Some("en") => "English",
        _ => "the wiki panel language selected by the user",
    }
}

fn is_image_file(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp" | "gif")
    )
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':'))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
