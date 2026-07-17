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

fn claude_smoke_invocation(
    _cwd: &Path,
    model: Option<&str>,
    reasoning: Option<&str>,
) -> LocalCliInvocation {
    stdin_smoke_invocation(claude_args(model, reasoning))
}

fn opencode_smoke_invocation(
    _cwd: &Path,
    model: Option<&str>,
    reasoning: Option<&str>,
) -> LocalCliInvocation {
    stdin_smoke_invocation(opencode_args(model, reasoning))
}

fn gemini_smoke_invocation(
    _cwd: &Path,
    model: Option<&str>,
    _reasoning: Option<&str>,
) -> LocalCliInvocation {
    stdin_smoke_invocation(gemini_args(model))
}

fn copilot_smoke_invocation(
    _cwd: &Path,
    model: Option<&str>,
    _reasoning: Option<&str>,
) -> LocalCliInvocation {
    stdin_smoke_invocation(copilot_args(model))
}

fn cursor_smoke_invocation(
    _cwd: &Path,
    model: Option<&str>,
    _reasoning: Option<&str>,
) -> LocalCliInvocation {
    stdin_smoke_invocation(cursor_args(model))
}

fn qwen_smoke_invocation(
    _cwd: &Path,
    model: Option<&str>,
    _reasoning: Option<&str>,
) -> LocalCliInvocation {
    stdin_smoke_invocation(qwen_args(model))
}

fn kimi_smoke_invocation(
    _cwd: &Path,
    model: Option<&str>,
    _reasoning: Option<&str>,
) -> LocalCliInvocation {
    stdin_smoke_invocation(kimi_args(model))
}

fn kilo_smoke_invocation(
    _cwd: &Path,
    model: Option<&str>,
    _reasoning: Option<&str>,
) -> LocalCliInvocation {
    stdin_smoke_invocation(kilo_args(model))
}

fn stdin_smoke_invocation(args: Vec<String>) -> LocalCliInvocation {
    LocalCliInvocation {
        args,
        input: Some(SMOKE_PROMPT),
    }
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

fn claude_task_command(executable: &str, model: Option<&str>, reasoning: Option<&str>) -> String {
    stdin_task_command(
        executable,
        claude_args(clean_optional(model), clean_optional(reasoning)),
    )
}

fn opencode_task_command(executable: &str, model: Option<&str>, reasoning: Option<&str>) -> String {
    stdin_task_command(
        executable,
        opencode_args(clean_optional(model), clean_optional(reasoning)),
    )
}

fn gemini_task_command(executable: &str, model: Option<&str>, _reasoning: Option<&str>) -> String {
    stdin_task_command(executable, gemini_args(clean_optional(model)))
}

fn copilot_task_command(executable: &str, model: Option<&str>, _reasoning: Option<&str>) -> String {
    stdin_task_command(executable, copilot_args(clean_optional(model)))
}

fn cursor_task_command(executable: &str, model: Option<&str>, _reasoning: Option<&str>) -> String {
    stdin_task_command(executable, cursor_args(clean_optional(model)))
}

fn qwen_task_command(executable: &str, model: Option<&str>, _reasoning: Option<&str>) -> String {
    stdin_task_command(executable, qwen_args(clean_optional(model)))
}

fn kimi_task_command(executable: &str, model: Option<&str>, _reasoning: Option<&str>) -> String {
    stdin_task_command(executable, kimi_args(clean_optional(model)))
}

fn kilo_task_command(executable: &str, model: Option<&str>, _reasoning: Option<&str>) -> String {
    stdin_task_command(executable, kilo_args(clean_optional(model)))
}

fn claude_args(model: Option<&str>, reasoning: Option<&str>) -> Vec<String> {
    let mut args = owned_args(&[
        "-p",
        "--output-format",
        "json",
        "--dangerously-skip-permissions",
    ]);
    push_model_flag(&mut args, "--model", model);
    push_model_flag(&mut args, "--effort", reasoning);
    args
}

fn opencode_args(model: Option<&str>, reasoning: Option<&str>) -> Vec<String> {
    let mut args = owned_args(&["run", "--format", "json"]);
    push_model_flag(&mut args, "--model", model);
    push_model_flag(&mut args, "--variant", reasoning);
    args
}

fn gemini_args(model: Option<&str>) -> Vec<String> {
    let mut args = owned_args(&["--approval-mode=yolo", "--output-format", "json"]);
    push_model_flag(&mut args, "--model", model);
    args
}

fn copilot_args(model: Option<&str>) -> Vec<String> {
    let mut args = owned_args(&[
        "--allow-all",
        "--no-ask-user",
        "--output-format",
        "json",
    ]);
    push_model_flag(&mut args, "--model", model);
    args
}

fn cursor_args(model: Option<&str>) -> Vec<String> {
    let mut args = owned_args(&[
        "--print",
        "--output-format",
        "json",
        "--force",
        "--trust",
    ]);
    push_model_flag(&mut args, "--model", model);
    args
}

fn qwen_args(model: Option<&str>) -> Vec<String> {
    let mut args = owned_args(&["--approval-mode=yolo", "--output-format", "json"]);
    push_model_flag(&mut args, "--model", model);
    args
}

fn kimi_args(model: Option<&str>) -> Vec<String> {
    let mut args = owned_args(&[
        "--print",
        "--output-format",
        "stream-json",
        "--final-message-only",
    ]);
    push_model_flag(&mut args, "--model", model);
    args
}

fn kilo_args(model: Option<&str>) -> Vec<String> {
    let mut args = owned_args(&["run", "--format", "json", "--auto"]);
    push_model_flag(&mut args, "--model", model);
    args
}

fn push_model_flag(args: &mut Vec<String>, flag: &str, model: Option<&str>) {
    if let Some(model) = model.filter(|value| *value != "default") {
        args.push(flag.to_owned());
        args.push(model.to_owned());
    }
}

fn stdin_task_command(executable: &str, args: Vec<String>) -> String {
    std::iter::once(shell_quote(executable))
        .chain(args.iter().map(|arg| shell_quote(arg)))
        .collect::<Vec<_>>()
        .join(" ")
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
    if matches!(provider_id, "claude" | "gemini" | "cursor-agent" | "qwen") {
        if let Ok(value) = serde_json::from_str::<Value>(stdout.trim()) {
            for pointer in ["/result", "/response", "/message/content"] {
                if let Some(text) = value.pointer(pointer).and_then(Value::as_str) {
                    return text.trim().to_owned();
                }
            }
        }
        return stdout.trim().to_owned();
    }
    if provider_id == "kimi" {
        let messages = stdout
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|event| event.get("role").and_then(Value::as_str) == Some("assistant"))
            .filter_map(|event| {
                event
                    .get("content")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect::<Vec<_>>();
        if !messages.is_empty() {
            return messages.join("").trim().to_owned();
        }
        return stdout.trim().to_owned();
    }
    if matches!(provider_id, "opencode" | "copilot" | "kilo") {
        let messages = stdout
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|event| {
                ["/part/text", "/data/content", "/message/content", "/result"]
                    .into_iter()
                    .find_map(|pointer| {
                        event
                            .pointer(pointer)
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
            })
            .collect::<Vec<_>>();
        if !messages.is_empty() {
            return messages.join("").trim().to_owned();
        }
        return stdout.trim().to_owned();
    }
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

fn output_semantically_succeeded(stdout: &str) -> bool {
    let mut parsed_any = false;
    for line in stdout.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        parsed_any = true;
        if event.get("is_error").and_then(Value::as_bool) == Some(true)
            || event.get("type").and_then(Value::as_str) == Some("error")
            || event
                .get("subtype")
                .and_then(Value::as_str)
                .is_some_and(|kind| matches!(kind, "error" | "failure" | "failed"))
            || event
                .get("error")
                .is_some_and(|error| match error {
                    Value::Null => false,
                    Value::String(message) => !message.is_empty(),
                    _ => true,
                })
        {
            return false;
        }
    }
    !parsed_any || !stdout.trim().is_empty()
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
