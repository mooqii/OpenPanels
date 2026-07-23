fn run_writing_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let paths = parsed_paths(parsed)?;
    match parsed.intent() {
        "writing.request.read" => {
            let task_id = required_flag(parsed, "task-id")?;
            let result = crate::writing::read_request(&paths, task_id)?;
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Writing request {task_id}"),
            )
        }
        "writing.write" => {
            let task_id = required_flag(parsed, "task-id")?;
            let title = required_flag(parsed, "title")?;
            let document_format = string_flag(parsed, "document-format").unwrap_or("markdown");
            if !matches!(document_format, "markdown" | "text") {
                return Err(CliError::with_code(
                    "invalid_argument",
                    "Writing document format must be markdown or text.",
                ));
            }
            let result = operations::begin_writing(&paths, task_id, title, document_format)?;
            write_result(parsed, stdout, &result, "Started My Document write")
        }
        "writing.distillation.read" => {
            let task_id = required_flag(parsed, "task-id")?;
            let result = crate::writing::read_distillation(&paths, task_id)?;
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Writing Skill distillation {task_id}"),
            )
        }
        "writing.skill.install" => {
            let task_id = required_flag(parsed, "task-id")?;
            let skill_file = required_flag(parsed, "skill-file")?;
            let result = crate::writing::install_project_skill(&paths, task_id, skill_file)?;
            write_result(parsed, stdout, &result, "Installed shared Writing Skill")
        }
        _ => Err(CliError::new("Unknown writing command.")),
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ProjectReadView {
    List,
}

fn run_project_read_command(
    parsed: &Invocation,
    stdout: &mut impl Write,
    view: ProjectReadView,
) -> Result<(), CliError> {
    let paths = parsed_current_paths(parsed)?;
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new())?;

    match view {
        ProjectReadView::List => {
            let payload = serde_json::json!({
                "activePanelId": bootstrap.active_panel_id,
                "activePanelKind": bootstrap.active_panel_kind,
                "panels": bootstrap.panels.iter().map(|snapshot| &snapshot.panel).collect::<Vec<_>>(),
                "project": bootstrap.project,
            });
            let text = bootstrap
                .panels
                .iter()
                .map(|snapshot| {
                    if snapshot.panel.id == bootstrap.active_panel_id {
                        format!(
                            "* {}: {}",
                            snapshot.panel.kind.as_str(),
                            snapshot.panel.title
                        )
                    } else {
                        format!(
                            "  {}: {}",
                            snapshot.panel.kind.as_str(),
                            snapshot.panel.title
                        )
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            write_result(parsed, stdout, &payload, &text)
        }
    }
}

fn parsed_paths(parsed: &Invocation) -> Result<crate::paths::MyOpenPanelsPaths, CliError> {
    resolve_myopenpanels_paths(
        string_flag(parsed, "project-dir"),
        string_flag(parsed, "storage-dir"),
        string_flag(parsed, "context-id"),
    )
}

fn parsed_current_paths(parsed: &Invocation) -> Result<crate::paths::MyOpenPanelsPaths, CliError> {
    let paths = parsed_paths(parsed)?;
    if string_flag(parsed, "context-id").is_some() {
        return Ok(paths);
    }
    let Some(session) = resolve_current_studio_session(&paths)? else {
        return Err(CliError::with_recovery(
            "no_current_studio",
            "No running MyOpenPanels Studio is available for this storage directory.",
            true,
            format!(
                "Run `{} studio start --local-only --project-dir <dir> --format json`, then retry.",
                crate::cli_identity::agent_cli_shell_word()
            ),
        ));
    };
    let _ = session;
    Ok(paths)
}

fn parsed_bootstrap_paths(
    parsed: &Invocation,
) -> Result<crate::paths::MyOpenPanelsPaths, CliError> {
    if string_flag(parsed, "project-dir").is_some() || string_flag(parsed, "context-id").is_some() {
        return parsed_current_paths(parsed);
    }

    let paths = parsed_paths(parsed)?;
    let Some(session) = resolve_current_studio_session(&paths)? else {
        return Err(CliError::with_recovery(
            "no_current_studio",
            "No running user-visible MyOpenPanels Studio is available for Agent Bootstrap.",
            true,
            format!(
                "Run `{} studio start --local-only --project-dir <dir> --format json`, open the returned URL, then retry `{} agent bootstrap --format json`.",
                crate::cli_identity::agent_cli_shell_word(),
                crate::cli_identity::agent_cli_shell_word()
            ),
        ));
    };
    let _ = session;
    Ok(paths)
}

fn image_metadata_flag(parsed: &Invocation) -> Result<Option<Value>, CliError> {
    let metadata_json = string_flag(parsed, "metadata-json");
    let metadata_file = string_flag(parsed, "metadata-file");
    let metadata = match (metadata_json, metadata_file) {
        (Some(_), Some(_)) => {
            return Err(CliError::new(
                "Use either --metadata-json or --metadata-file, not both.",
            ));
        }
        (Some(value), None) => value.to_owned(),
        (None, Some(path)) => fs::read_to_string(path).map_err(|error| {
            CliError::new(format!("Failed to read --metadata-file {path}: {error}"))
        })?,
        (None, None) => return Ok(None),
    };
    let value = serde_json::from_str::<Value>(&metadata)
        .map_err(|error| CliError::new(format!("Invalid image metadata JSON: {error}")))?;
    if !value.is_object() {
        return Err(CliError::new(
            "Expected image metadata to be a JSON object.",
        ));
    }
    Ok(Some(value))
}
