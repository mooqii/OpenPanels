fn studio_host(parsed: &Invocation) -> String {
    if has_flag(parsed, "local-only") {
        return "127.0.0.1".to_owned();
    }
    string_flag(parsed, "host")
        .map(str::to_owned)
        .or_else(|| std::env::var("MYOPENPANELS_STUDIO_HOST").ok())
        .unwrap_or_else(|| "0.0.0.0".to_owned())
}

fn studio_port(parsed: &Invocation) -> Result<Option<u16>, CliError> {
    string_flag(parsed, "port")
        .map(|port| {
            port.parse::<u16>()
                .map_err(|_| CliError::new("Expected --port to be a number between 0 and 65535."))
        })
        .transpose()
}

fn studio_session_payload(
    session: &crate::studio::StudioSession,
    fields: &[(&str, Value)],
) -> Result<Value, CliError> {
    let mut value =
        serde_json::to_value(session).map_err(|error| CliError::new(error.to_string()))?;
    let Some(object) = value.as_object_mut() else {
        return Err(CliError::new("Failed to serialize studio session."));
    };
    for (key, field_value) in fields {
        object.insert((*key).to_owned(), field_value.clone());
    }
    Ok(value)
}

fn bootstrap_studio(paths: &crate::paths::MyOpenPanelsPaths) -> Result<ProjectBootstrap, CliError> {
    ensure_project_bootstrap(paths, BootstrapRequest::new())
}

fn studio_launch_payload(
    paths: &crate::paths::MyOpenPanelsPaths,
    result: &StudioStartResult,
    bootstrap: &ProjectBootstrap,
    extra: Option<(&str, bool)>,
    open_required: bool,
) -> Result<Value, CliError> {
    let system_browser_url = result
        .session
        .system_browser_url
        .as_deref()
        .unwrap_or(&result.session.server_url);
    let mut payload = serde_json::json!({
        "ok": true,
        "reusedExisting": result.reused_existing,
        "serverVersion": result.server_version,
        "lifecycle": result.lifecycle,
        "previousVersion": result.previous_version,
        "browserRefreshRequired": result.browser_refresh_required,
        "projectReady": true,
        "serverUrl": result.session.server_url,
        "embeddedBrowserUrl": system_browser_url,
        "systemBrowserUrl": system_browser_url,
        "recommendedOpenTarget": "in_app_browser",
        "context": {
            "id": "studio",
            "projectDir": paths.project_dir,
            "storageDir": result.session.storage_dir,
        },
        "project": {
            "id": bootstrap.project.id,
            "title": bootstrap.project.title,
        },
        "activePanel": {
            "id": bootstrap.active_panel_id,
            "kind": bootstrap.active_panel_kind,
            "title": bootstrap.panel.title,
        },
    });
    if let Some((key, value)) = extra {
        payload[key] = serde_json::json!(value);
    }
    if open_required {
        let fallback_action = registry::command_action(
            registry::CommandId::registered("studio.open-system-browser"),
            vec![
                "--local-only".to_owned(),
                "--project-dir".to_owned(),
                paths.project_dir.display().to_string(),
                "--format".to_owned(),
                "json".to_owned(),
            ],
        )
        .expect("registered Studio browser fallback action");
        payload["actions"] = serde_json::json!({
            "required": [
                {
                    "id": "studio.open.in-app",
                    "intent": "studio.open",
                    "executor": "agent-host",
                    "kind": "open-url",
                    "url": system_browser_url,
                },
                {
                    "id": "studio.open.system",
                    "intent": "studio.open-system-browser",
                    "executor": fallback_action["executor"],
                    "argv": fallback_action["argv"],
                    "condition": {
                        "actionId": "studio.open.in-app",
                        "outcomes": ["failed", "unavailable"]
                    }
                }
            ],
            "suggested": []
        });
    }
    Ok(payload)
}

fn studio_system_browser_payload(
    paths: &crate::paths::MyOpenPanelsPaths,
    result: &StudioStartResult,
    bootstrap: &ProjectBootstrap,
    opener: impl FnOnce(&str) -> Result<(), CliError>,
) -> Result<Value, CliError> {
    let system_url = result
        .session
        .system_browser_url
        .as_deref()
        .unwrap_or(&result.session.server_url);
    opener(system_url)?;
    let mut payload = studio_launch_payload(paths, result, bootstrap, None, false)?;
    payload["opened"] = serde_json::json!(true);
    payload["openTarget"] = serde_json::json!("system_browser");
    Ok(payload)
}

fn write_result(
    parsed: &Invocation,
    stdout: &mut impl Write,
    payload: &impl Serialize,
    text: &str,
) -> Result<(), CliError> {
    match output_format(parsed) {
        OutputFormat::Json => {
            let mut data =
                serde_json::to_value(payload).map_err(|error| CliError::new(error.to_string()))?;
            let actions = take_response_actions(&mut data)?;
            let envelope = SuccessPayload {
                ok: true,
                schema_version: CLI_ENVELOPE_SCHEMA_VERSION,
                intent: parsed.intent(),
                data: &data,
                actions: &actions,
                meta: ResponseMeta {
                    cli_version: VERSION,
                },
            };
            let json = serde_json::to_string(&envelope)
                .map_err(|error| CliError::new(error.to_string()))?;
            let envelope_bytes = json.len() + 1;
            if parsed.intent() == "agent.bootstrap.read"
                && envelope_bytes > crate::agent::MAX_BOOTSTRAP_ENVELOPE_BYTES
            {
                return Err(CliError::with_recovery(
                    "bootstrap_budget_exceeded",
                    format!(
                        "Agent Bootstrap is {envelope_bytes} bytes; the maximum is {} bytes.",
                        crate::agent::MAX_BOOTSTRAP_ENVELOPE_BYTES
                    ),
                    false,
                    "Report this Bootstrap size regression; use scoped Agent discovery commands instead of requesting a full payload.",
                ));
            }
            write_text(stdout, &format!("{json}\n"))
        }
        OutputFormat::Text => write_text(stdout, &format!("{text}\n")),
    }
}

fn write_error(
    parsed: &Invocation,
    _stdout: &mut impl Write,
    stderr: &mut impl Write,
    error: &CliError,
) {
    match output_format(parsed) {
        OutputFormat::Json => {
            let payload = ErrorPayload {
                ok: false,
                schema_version: CLI_ENVELOPE_SCHEMA_VERSION,
                intent: parsed.intent(),
                error: ErrorDetail {
                    category: error.category(),
                    subtype: error.subtype(),
                    message: error.message(),
                    retryable: error.retryable(),
                    param: error.param(),
                    hint: error
                        .recovery()
                        .unwrap_or("Review the command help and retry with valid arguments."),
                },
                actions: ResponseActions {
                    required: Vec::new(),
                    suggested: error
                        .recovery_actions()
                        .iter()
                        .map(|action| serde_json::to_value(action).unwrap_or(Value::Null))
                        .collect(),
                },
                meta: ResponseMeta {
                    cli_version: VERSION,
                },
            };
            let _ = serde_json::to_writer(&mut *stderr, &payload);
            let _ = stderr.write_all(b"\n");
        }
        OutputFormat::Text => {
            let _ = write_text(stderr, &format!("Error: {}\n", error.message()));
        }
    }
}

fn take_response_actions(data: &mut Value) -> Result<ResponseActions, CliError> {
    let Some(actions) = data
        .as_object_mut()
        .and_then(|object| object.remove("actions"))
    else {
        return Ok(ResponseActions::default());
    };
    let object = actions.as_object().ok_or_else(|| {
        CliError::with_code("invalid_output", "Response actions must be a JSON object.")
    })?;
    let read = |name: &str| -> Result<Vec<Value>, CliError> {
        object
            .get(name)
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]))
            .as_array()
            .cloned()
            .ok_or_else(|| {
                CliError::with_code(
                    "invalid_output",
                    format!("Response actions.{name} must be an array."),
                )
            })
    };
    Ok(ResponseActions {
        required: read("required")?,
        suggested: read("suggested")?,
    })
}

fn concise_parse_error_message(message: &str) -> String {
    let message = message
        .trim()
        .strip_prefix("error: ")
        .unwrap_or(message.trim());
    message
        .split("\n\nUsage:")
        .next()
        .unwrap_or(message)
        .split("\n\nFor more information")
        .next()
        .unwrap_or(message)
        .trim()
        .to_owned()
}

fn parse_error_param(message: &str) -> Option<String> {
    message
        .lines()
        .find_map(|line| {
            let value = line.trim();
            value
                .starts_with("--")
                .then(|| value.split_whitespace().next().unwrap_or(value).to_owned())
        })
        .or_else(|| {
            message
                .split('\'')
                .find(|part| part.starts_with("--") && *part != "--help")
                .map(str::to_owned)
        })
}

fn parse_error_args(argv: &[String]) -> Invocation {
    let json = argv.windows(2).any(|parts| parts == ["--format", "json"])
        || argv.iter().any(|arg| arg == "--format=json");
    let mut flags = BTreeMap::new();
    if json {
        flags.insert("format".to_owned(), FlagValue::String("json".to_owned()));
    }
    Invocation {
        command_id: registry::CommandId::ParseError,
        flags,
        positionals: Vec::new(),
    }
}

fn write_text(stream: &mut impl Write, text: &str) -> Result<(), CliError> {
    stream
        .write_all(text.as_bytes())
        .map_err(|error| CliError::new(error.to_string()))
}
