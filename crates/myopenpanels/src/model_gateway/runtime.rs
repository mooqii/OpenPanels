pub fn selected_worker_spec(
    paths: &MyOpenPanelsPaths,
) -> Result<Option<GatewayWorkerSpec>, CliError> {
    Ok(worker_specs(paths)?.into_iter().next())
}

fn normalize_settings(
    mut settings: ModelGatewaySettings,
) -> Result<ModelGatewaySettings, CliError> {
    settings.mode = settings.mode.trim().to_owned();
    if !matches!(settings.mode.as_str(), "localCli" | "byok") {
        return Err(CliError::with_code(
            "invalid_model_gateway_settings",
            "Model gateway mode must be localCli or byok.",
        ));
    }
    settings.local_cli.provider_id = clean_owned(settings.local_cli.provider_id);
    settings.local_cli.model = clean_owned(settings.local_cli.model);
    settings.local_cli.reasoning = clean_owned(settings.local_cli.reasoning);
    settings.byok.provider_id = clean_owned(settings.byok.provider_id);
    settings.byok.base_url = clean_owned(settings.byok.base_url);
    settings.byok.model = clean_owned(settings.byok.model);
    settings
        .local_cli
        .executable_paths
        .retain(|id, value| definition(id).is_some() && !value.trim().is_empty());
    for value in settings.local_cli.executable_paths.values_mut() {
        *value = value.trim().to_owned();
    }
    if let Some(provider_id) = settings.local_cli.provider_id.as_deref() {
        if definition(provider_id).is_none() {
            return Err(CliError::with_code(
                "unsupported_model_provider",
                format!("Unsupported Local CLI provider: {provider_id}"),
            ));
        }
    }
    let mut seen = HashSet::new();
    settings
        .local_cli
        .enabled_provider_ids
        .retain(|provider_id| {
            definition(provider_id).is_some() && seen.insert(provider_id.clone())
        });
    seen.clear();
    settings.local_cli.provider_order.retain(|provider_id| {
        definition(provider_id).is_some()
            && settings
                .local_cli
                .enabled_provider_ids
                .contains(provider_id)
            && seen.insert(provider_id.clone())
    });
    if let Some(provider_id) = settings.local_cli.provider_id.clone() {
        settings
            .local_cli
            .enabled_provider_ids
            .retain(|candidate| candidate != &provider_id);
        settings
            .local_cli
            .enabled_provider_ids
            .insert(0, provider_id.clone());
        settings
            .local_cli
            .provider_order
            .retain(|candidate| candidate != &provider_id);
        settings.local_cli.provider_order.insert(0, provider_id);
    }
    if settings.local_cli.enabled_provider_ids.is_empty() {
        if let Some(provider_id) = settings.local_cli.provider_id.clone() {
            settings.local_cli.enabled_provider_ids.push(provider_id);
        }
    }
    for provider_id in &settings.local_cli.enabled_provider_ids {
        if !settings.local_cli.provider_order.contains(provider_id) {
            settings.local_cli.provider_order.push(provider_id.clone());
        }
    }
    settings.local_cli.enabled_provider_ids = settings.local_cli.provider_order.clone();
    settings.local_cli.provider_id = settings.local_cli.provider_order.first().cloned();
    if let Some(model) = settings.local_cli.model.as_deref() {
        validate_cli_value("model", model)?;
    }
    if let Some(reasoning) = settings.local_cli.reasoning.as_deref() {
        validate_cli_value("reasoning", reasoning)?;
    }
    Ok(settings)
}

fn clean_owned(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}

fn clean_optional(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn validate_cli_value(name: &str, value: &str) -> Result<(), CliError> {
    if value.len() > 160
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | ':' | '/'))
    {
        return Err(CliError::with_code(
            "invalid_model_gateway_settings",
            format!("Invalid {name} value."),
        ));
    }
    Ok(())
}

fn default_provider_from_env() -> Option<String> {
    match std::env::var("MYOPENPANELS_AGENT_PROVIDER").ok().as_deref() {
        Some("none") => None,
        Some("hermes") => Some("hermes".to_owned()),
        _ => Some("codex".to_owned()),
    }
}

fn definition(id: &str) -> Option<LocalCliDefinition> {
    LOCAL_CLI_DEFINITIONS
        .iter()
        .copied()
        .find(|item| item.id == id)
}

fn scan_local_cli(
    definition: LocalCliDefinition,
    configured_path: Option<String>,
    cwd: &Path,
) -> LocalCliInfo {
    let resolution = resolve_executable(definition.bin, configured_path.as_deref());
    let Some(path) = resolution.path.clone() else {
        let mut unavailable = unavailable_cli(definition.id, definition.name, definition.bin);
        unavailable.configured_path = configured_path;
        unavailable.diagnostic = resolution.diagnostic;
        return unavailable;
    };
    let version_output = run_process(
        &path,
        &owned_args(definition.version_args),
        None,
        Some(cwd),
        Duration::from_secs(5),
    );
    let Ok(version_output) = version_output else {
        let mut unavailable = unavailable_cli(definition.id, definition.name, definition.bin);
        unavailable.path = Some(path);
        unavailable.configured_path = configured_path;
        unavailable.diagnostic =
            Some("The executable was found but could not be started.".to_owned());
        return unavailable;
    };
    let version = first_non_empty(&version_output.stdout, &version_output.stderr)
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned);
    let (auth_status, auth_message) = probe_auth(definition, &path, cwd);
    let (models, models_source) = probe_models(definition, &path, cwd);
    LocalCliInfo {
        id: definition.id.to_owned(),
        name: definition.name.to_owned(),
        bin: definition.bin.to_owned(),
        available: true,
        path: Some(path),
        configured_path,
        version,
        auth_status,
        auth_message,
        diagnostic: resolution.diagnostic,
        models,
        models_source,
        reasoning_options: model_options(definition.reasoning_options),
    }
}

fn unavailable_cli(id: &str, name: &str, bin: &str) -> LocalCliInfo {
    let definition = definition(id);
    LocalCliInfo {
        id: id.to_owned(),
        name: name.to_owned(),
        bin: bin.to_owned(),
        available: false,
        path: None,
        configured_path: None,
        version: None,
        auth_status: "unknown".to_owned(),
        auth_message: None,
        diagnostic: None,
        models: definition
            .map(|item| model_options(item.fallback_models))
            .unwrap_or_else(|| model_options(DEFAULT_MODELS)),
        models_source: "fallback".to_owned(),
        reasoning_options: definition
            .map(|item| model_options(item.reasoning_options))
            .unwrap_or_default(),
    }
}

fn probe_auth(
    definition: LocalCliDefinition,
    executable: &str,
    cwd: &Path,
) -> (String, Option<String>) {
    if definition.auth_args.is_empty() {
        return ("unknown".to_owned(), None);
    }
    let args = owned_args(definition.auth_args);
    match run_process(executable, &args, None, Some(cwd), Duration::from_secs(8)) {
        Ok(output) if output.success => ("ok".to_owned(), None),
        Ok(output) => (
            "missing".to_owned(),
            Some(
                first_non_empty(&output.stderr, &output.stdout)
                    .chars()
                    .take(500)
                    .collect(),
            ),
        ),
        Err(error) => ("unknown".to_owned(), Some(error.message().to_owned())),
    }
}

fn probe_models(
    definition: LocalCliDefinition,
    executable: &str,
    cwd: &Path,
) -> (Vec<ModelOption>, String) {
    let probed = (definition.probe_models)(executable, cwd);
    match probed.filter(|models| models.len() > 1) {
        Some(models) => (models, "live".to_owned()),
        None => (
            model_options(definition.fallback_models),
            "fallback".to_owned(),
        ),
    }
}

fn probe_codex_models(executable: &str, cwd: &Path) -> Option<Vec<ModelOption>> {
    run_process(
        executable,
        &owned_args(&["debug", "models"]),
        None,
        Some(cwd),
        Duration::from_secs(10),
    )
    .ok()
    .filter(|output| output.success)
    .and_then(|output| parse_codex_models(&output.stdout))
}

fn probe_hermes_models(executable: &str, cwd: &Path) -> Option<Vec<ModelOption>> {
    probe_hermes_acp_models(executable, cwd).ok()
}

fn parse_codex_models(stdout: &str) -> Option<Vec<ModelOption>> {
    let parsed = serde_json::from_str::<Value>(stdout).ok()?;
    let entries = parsed.get("models")?.as_array()?;
    let mut result = model_options(DEFAULT_MODELS);
    let mut seen = HashSet::from(["default".to_owned()]);
    for entry in entries {
        if entry.get("visibility").and_then(Value::as_str) == Some("hidden") {
            continue;
        }
        let Some(id) = entry
            .get("slug")
            .or_else(|| entry.get("id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if !seen.insert(id.to_owned()) {
            continue;
        }
        let label = entry
            .get("display_name")
            .or_else(|| entry.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(id);
        result.push(ModelOption {
            id: id.to_owned(),
            label: label.to_owned(),
        });
    }
    Some(result)
}

fn probe_hermes_acp_models(executable: &str, cwd: &Path) -> Result<Vec<ModelOption>, CliError> {
    let mut command = Command::new(executable);
    command
        .args(["acp", "--accept-hooks"])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group(&mut command);
    let mut child = command.spawn().map_err(to_cli_error)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CliError::new("Hermes ACP stdout is unavailable."))?;
    let stderr = child.stderr.take();
    let (sender, receiver) = mpsc::channel::<String>();
    let stdout_reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let stderr_reader = thread::spawn(move || read_pipe(stderr));
    let stdin = child
        .stdin
        .as_mut()
        .ok_or_else(|| CliError::new("Hermes ACP stdin is unavailable."))?;
    write_json_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": 1,
                "clientCapabilities": { "terminal": false },
                "clientInfo": { "name": "myopenpanels-detect", "version": env!("CARGO_PKG_VERSION") }
            }
        }),
    )?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let result = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break Err(CliError::new("Hermes ACP model detection timed out."));
        }
        let line = match receiver.recv_timeout(remaining) {
            Ok(line) => line,
            Err(_) => break Err(CliError::new("Hermes ACP model detection timed out.")),
        };
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if message.get("id").and_then(Value::as_i64) == Some(1) {
            if let Some(error) = message.get("error") {
                break Err(CliError::new(format!(
                    "Hermes ACP initialize failed: {error}"
                )));
            }
            if let Some(stdin) = child.stdin.as_mut() {
                write_json_line(
                    stdin,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": 2,
                        "method": "session/new",
                        "params": { "cwd": cwd.display().to_string(), "mcpServers": [] }
                    }),
                )?;
            }
            continue;
        }
        if message.get("id").and_then(Value::as_i64) == Some(2) {
            if let Some(error) = message.get("error") {
                break Err(CliError::new(format!("Hermes ACP session failed: {error}")));
            }
            break Ok(normalize_acp_models(
                message.pointer("/result/models"),
                message.pointer("/result/configOptions"),
            ));
        }
    };
    terminate_process(&mut child);
    drop(child.stdin.take());
    let _ = stdout_reader.join();
    let _ = stderr_reader.join();
    result
}

fn normalize_acp_models(
    models: Option<&Value>,
    config_options: Option<&Value>,
) -> Vec<ModelOption> {
    let mut result = model_options(DEFAULT_MODELS);
    let mut seen = HashSet::from(["default".to_owned()]);
    if let Some(options) = config_options.and_then(Value::as_array) {
        for option in options {
            let id = option.get("id").and_then(Value::as_str).unwrap_or("");
            let category = option.get("category").and_then(Value::as_str).unwrap_or("");
            if normalize_token(id) != "model" && normalize_token(category) != "model" {
                continue;
            }
            let current = option.get("currentValue").and_then(Value::as_str);
            for value in option
                .get("options")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let id = value
                    .get("value")
                    .or_else(|| value.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                push_acp_model(
                    &mut result,
                    &mut seen,
                    id,
                    value.get("name").and_then(Value::as_str),
                    current == Some(id),
                );
            }
        }
    }
    if result.len() == 1 {
        let current = models
            .and_then(|value| value.get("currentModelId"))
            .and_then(Value::as_str);
        for model in models
            .and_then(|value| value.get("availableModels"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let id = model.get("modelId").and_then(Value::as_str).unwrap_or("");
            push_acp_model(
                &mut result,
                &mut seen,
                id,
                model.get("name").and_then(Value::as_str),
                current == Some(id),
            );
        }
    }
    result
}

fn push_acp_model(
    result: &mut Vec<ModelOption>,
    seen: &mut HashSet<String>,
    id: &str,
    name: Option<&str>,
    current: bool,
) {
    let id = id.trim();
    if id.is_empty() || !seen.insert(id.to_owned()) {
        return;
    }
    let label = name
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != id)
        .map(|name| format!("{name} ({id})"))
        .unwrap_or_else(|| id.to_owned());
    result.push(ModelOption {
        id: id.to_owned(),
        label: if current {
            format!("{label} (current)")
        } else {
            label
        },
    });
}

fn normalize_token(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !matches!(ch, ' ' | '_' | '-'))
        .flat_map(char::to_lowercase)
        .collect()
}

fn write_json_line(writer: &mut impl Write, value: &Value) -> Result<(), CliError> {
    serde_json::to_writer(&mut *writer, value).map_err(to_cli_error)?;
    writer.write_all(b"\n").map_err(to_cli_error)?;
    writer.flush().map_err(to_cli_error)
}

fn model_options(entries: &[(&str, &str)]) -> Vec<ModelOption> {
    entries
        .iter()
        .map(|(id, label)| ModelOption {
            id: (*id).to_owned(),
            label: (*label).to_owned(),
        })
        .collect()
}

fn owned_args(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

struct ExecutableResolution {
    path: Option<String>,
    diagnostic: Option<String>,
}

fn resolve_executable(bin: &str, configured_path: Option<&str>) -> ExecutableResolution {
    let configured_path = configured_path
        .map(str::trim)
        .filter(|path| !path.is_empty());
    if let Some(configured) = configured_path {
        let path = PathBuf::from(configured);
        if is_invocable_file(&path) {
            return ExecutableResolution {
                path: Some(path.display().to_string()),
                diagnostic: None,
            };
        }
    }
    let detected = executable_search_dirs()
        .into_iter()
        .flat_map(|directory| executable_candidates(&directory, bin))
        .find(|path| is_invocable_file(path))
        .map(|path| path.display().to_string());
    ExecutableResolution {
        path: detected,
        diagnostic: configured_path.map(|path| {
            format!("Configured executable is not usable: {path}. PATH detection was used instead.")
        }),
    }
}

fn executable_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join(".cargo/bin"));
        dirs.push(home.join("bin"));
    }
    dirs.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ]);
    let mut seen = HashSet::new();
    dirs.into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

fn executable_candidates(directory: &Path, bin: &str) -> Vec<PathBuf> {
    let base = directory.join(bin);
    if cfg!(windows) {
        vec![
            base.clone(),
            directory.join(format!("{bin}.exe")),
            directory.join(format!("{bin}.cmd")),
            directory.join(format!("{bin}.bat")),
        ]
    } else {
        vec![base]
    }
}

fn is_invocable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn codex_smoke_invocation(
    cwd: &Path,
    model: Option<&str>,
    reasoning: Option<&str>,
) -> LocalCliInvocation {
    let mut args = vec![
        "exec".to_owned(),
        "--json".to_owned(),
        "--ephemeral".to_owned(),
        "--ignore-rules".to_owned(),
        "--skip-git-repo-check".to_owned(),
        "--sandbox".to_owned(),
        "workspace-write".to_owned(),
        "-C".to_owned(),
        cwd.display().to_string(),
    ];
    push_codex_model_args(&mut args, model, reasoning);
    LocalCliInvocation {
        args,
        input: Some(SMOKE_PROMPT),
    }
}

fn hermes_smoke_invocation(
    _cwd: &Path,
    model: Option<&str>,
    _reasoning: Option<&str>,
) -> LocalCliInvocation {
    let mut args = vec!["--ignore-rules".to_owned()];
    if let Some(model) = model.filter(|value| *value != "default") {
        args.push("--model".to_owned());
        args.push(model.to_owned());
    }
    args.extend(["--oneshot".to_owned(), SMOKE_PROMPT.to_owned()]);
    LocalCliInvocation { args, input: None }
}

fn codex_task_command(executable: &str, model: Option<&str>, reasoning: Option<&str>) -> String {
    let mut args = vec![
        shell_quote(executable),
        "exec".to_owned(),
        "--json".to_owned(),
        "--ephemeral".to_owned(),
        "--ignore-rules".to_owned(),
        "--skip-git-repo-check".to_owned(),
        "--sandbox".to_owned(),
        "workspace-write".to_owned(),
        "-c".to_owned(),
        "sandbox_workspace_write.network_access=true".to_owned(),
        "-C".to_owned(),
        "\"$MYOPENPANELS_EXECUTION_WORKSPACE\"".to_owned(),
    ];
    push_codex_model_args(&mut args, clean_optional(model), clean_optional(reasoning));
    args.join(" ")
}

fn hermes_task_command(executable: &str, model: Option<&str>, _reasoning: Option<&str>) -> String {
    let mut args = vec![shell_quote(executable), "--ignore-rules".to_owned()];
    if let Some(model) = clean_optional(model).filter(|value| *value != "default") {
        args.push("--model".to_owned());
        args.push(shell_quote(model));
    }
    args.extend(["--oneshot".to_owned(), "\"$(cat)\"".to_owned()]);
    args.join(" ")
}

fn push_codex_model_args(args: &mut Vec<String>, model: Option<&str>, reasoning: Option<&str>) {
    if let Some(model) = model.filter(|value| *value != "default") {
        args.push("--model".to_owned());
        args.push(model.to_owned());
    }
    if let Some(reasoning) = reasoning.filter(|value| *value != "default") {
        args.push("-c".to_owned());
        args.push(format!("model_reasoning_effort=\"{reasoning}\""));
    }
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

struct ProcessOutput {
    success: bool,
    status_code: Option<i32>,
    timed_out: bool,
    stdout: String,
    stderr: String,
}

fn run_process(
    executable: &str,
    args: &[String],
    input: Option<&str>,
    cwd: Option<&Path>,
    timeout: Duration,
) -> Result<ProcessOutput, CliError> {
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    configure_process_group(&mut command);
    let mut child = command.spawn().map_err(to_cli_error)?;
    if let (Some(input), Some(stdin)) = (input, child.stdin.as_mut()) {
        stdin.write_all(input.as_bytes()).map_err(to_cli_error)?;
        stdin.write_all(b"\n").map_err(to_cli_error)?;
    }
    drop(child.stdin.take());
    let stdout_reader = {
        let stdout = child.stdout.take();
        thread::spawn(move || read_pipe(stdout))
    };
    let stderr_reader = {
        let stderr = child.stderr.take();
        thread::spawn(move || read_pipe(stderr))
    };
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(to_cli_error)? {
            break status;
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            terminate_process(&mut child);
            break child.wait().map_err(to_cli_error)?;
        }
        thread::sleep(Duration::from_millis(25));
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| CliError::new("Process stdout reader failed."))?
        .map_err(to_cli_error)?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| CliError::new("Process stderr reader failed."))?
        .map_err(to_cli_error)?;
    Ok(ProcessOutput {
        success: status.success() && !timed_out,
        status_code: status.code(),
        timed_out,
        stdout: String::from_utf8_lossy(&stdout[..stdout.len().min(PROCESS_OUTPUT_LIMIT)])
            .to_string(),
        stderr: String::from_utf8_lossy(&stderr[..stderr.len().min(PROCESS_OUTPUT_LIMIT)])
            .to_string(),
    })
}

fn read_pipe(mut pipe: Option<impl Read>) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    if let Some(pipe) = pipe.as_mut() {
        pipe.read_to_end(&mut bytes)?;
    }
    Ok(bytes)
}

fn assistant_sample(provider_id: &str, stdout: &str) -> String {
    if provider_id != "codex" {
        return stdout.trim().to_owned();
    }
    let mut messages = Vec::new();
    for line in stdout.lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if event.get("type").and_then(Value::as_str) == Some("item.completed")
            && event.pointer("/item/type").and_then(Value::as_str) == Some("agent_message")
        {
            if let Some(text) = event.pointer("/item/text").and_then(Value::as_str) {
                messages.push(text.to_owned());
            }
        }
    }
    messages
        .last()
        .cloned()
        .unwrap_or_else(|| stdout.trim().to_owned())
}

fn first_non_empty<'a>(primary: &'a str, fallback: &'a str) -> &'a str {
    if primary.trim().is_empty() {
        fallback
    } else {
        primary
    }
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_process(child: &mut Child) {
    let group = format!("-{}", child.id());
    let _ = Command::new("kill").args(["-TERM", &group]).status();
    thread::sleep(Duration::from_millis(150));
    if matches!(child.try_wait(), Ok(None)) {
        let _ = Command::new("kill").args(["-KILL", &group]).status();
    }
}

#[cfg(not(unix))]
fn terminate_process(child: &mut Child) {
    let _ = child.kill();
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
