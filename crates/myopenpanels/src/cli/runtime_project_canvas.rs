fn run_project_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        Some("read") => {
            let paths = parsed_current_paths(parsed)?;
            let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new())?;
            let payload = serde_json::json!({
                "project": bootstrap.project,
                "activePanel": {
                    "id": bootstrap.active_panel_id,
                    "kind": bootstrap.active_panel_kind,
                    "title": bootstrap.panel.title,
                },
                "panels": bootstrap.panels.iter().map(|snapshot| &snapshot.panel).collect::<Vec<_>>(),
            });
            write_result(parsed, stdout, &payload, &bootstrap.project.title)
        }
        Some("list") => {
            let fallback = parsed_paths(parsed)?;
            let paths = match parsed_current_paths(parsed) {
                Ok(paths) => paths,
                Err(error) if error.code() == Some("no_current_project") => fallback,
                Err(error) => return Err(error),
            };
            let storage = Storage::open(&paths)?;
            let projects = storage.list_projects()?;
            let current_id = read_active_project_id(&paths)?;
            let projects = projects
                .into_iter()
                .map(|session| {
                    let current = current_id.as_deref() == Some(session.id.as_str());
                    let mut value = serde_json::to_value(session)
                        .map_err(|error| CliError::new(error.to_string()))?;
                    value["current"] = serde_json::json!(current);
                    Ok(value)
                })
                .collect::<Result<Vec<_>, CliError>>()?;
            let payload = serde_json::json!({ "projects": projects });
            let count = payload["projects"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &payload, &format!("{count} project(s)"))
        }
        Some("create") => {
            let fallback = parsed_paths(parsed)?;
            let paths = match parsed_current_paths(parsed) {
                Ok(paths) => paths,
                Err(error) if error.code() == Some("no_current_project") => fallback,
                Err(error) => return Err(error),
            };
            let bootstrap = create_project(&paths, string_flag(parsed, "title"))?;
            let payload = serde_json::json!({
                "project": bootstrap.project,
                "activePanel": {
                    "id": bootstrap.active_panel_id,
                    "kind": bootstrap.active_panel_kind,
                    "title": bootstrap.panel.title,
                },
                "panels": bootstrap.panels.iter().map(|snapshot| &snapshot.panel).collect::<Vec<_>>(),
            });
            write_result(parsed, stdout, &payload, &bootstrap.project.title)
        }
        Some("activate") => {
            let fallback = parsed_paths(parsed)?;
            let paths = match parsed_current_paths(parsed) {
                Ok(paths) => paths,
                Err(error) if error.code() == Some("no_current_project") => fallback,
                Err(error) => return Err(error),
            };
            let project_id = required_flag(parsed, "project-id")?;
            let bootstrap = ensure_project_bootstrap(
                &paths,
                BootstrapRequest {
                    requested_panel_id: None,
                    requested_panel_kind: None,
                    requested_project_id: Some(project_id.to_owned()),
                },
            )?;
            let payload = serde_json::json!({
                "project": bootstrap.project,
                "activePanel": {
                    "id": bootstrap.active_panel_id,
                    "kind": bootstrap.active_panel_kind,
                    "title": bootstrap.panel.title,
                },
                "focusRevision": read_focus_revision(&paths)?,
            });
            write_result(parsed, stdout, &payload, project_id)
        }
        _ => Err(CliError::new(
            "Expected project subcommand: read, list, create, or activate.",
        )),
    }
}

fn run_panel_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        Some("list") => run_project_read_command(parsed, stdout, ProjectReadView::List),
        Some("activate") => {
            let paths = parsed_current_paths(parsed)?;
            let kind = parse_panel_kind(required_flag(parsed, "panel-kind")?)?;
            let bootstrap = activate_project_panel(&paths, kind)?;
            let payload = serde_json::json!({
                "project": bootstrap.project,
                "panel": bootstrap.panel,
                "focus": {
                    "focusRevision": read_focus_revision(&paths)?,
                    "projectId": bootstrap.project.id,
                    "panelId": bootstrap.panel.id,
                    "panelKind": bootstrap.panel.kind,
                }
            });
            write_result(parsed, stdout, &payload, "Panel activated.")
        }
        Some("read") => {
            let paths = parsed_current_paths(parsed)?;
            let kind = string_flag(parsed, "panel-kind")
                .map(parse_panel_kind)
                .transpose()?;
            if string_flag(parsed, "detail") == Some("full") {
                let payload = crate::panel::read_state(&paths, kind)?;
                write_result(parsed, stdout, &payload, "Panel state")
            } else {
                let payload = crate::panel::read_context(&paths, kind)?;
                write_result(parsed, stdout, &payload, "Panel summary")
            }
        }
        Some("selection") => {
            let paths = parsed_current_paths(parsed)?;
            let payload = crate::panel::read_selection(&paths)?;
            write_result(parsed, stdout, &payload, "Current panel selection")
        }
        _ => Err(CliError::new(
            "Expected panel subcommand: list, read, selection, or activate.",
        )),
    }
}

fn run_canvas_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    let action = parsed.positionals.get(2).map(String::as_str);
    match (subcommand, action) {
        (Some("image"), Some("generate")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = operations::begin_canvas(
                &paths,
                number_flag(parsed, "display-width")?,
                number_flag(parsed, "display-height")?,
                has_flag(parsed, "use-selection"),
                string_flag(parsed, "text"),
            )?;
            write_result(
                parsed,
                stdout,
                &result,
                result["operation"]["id"]
                    .as_str()
                    .unwrap_or("Started canvas generation"),
            )
        }
        (Some("selection"), Some("export")) => {
            let paths = parsed_current_paths(parsed)?;
            let bootstrap = require_active_panel(&paths, PanelKind::Canvas, None)?;
            let output = required_flag(parsed, "output-file")?;
            let result = read_selection_asset_for_panel(
                &paths,
                &bootstrap.project.id,
                &bootstrap.panel.id,
                output,
            )?;
            let text = format!("Wrote {}", result.output_path);
            write_result(parsed, stdout, &result, &text)
        }
        (Some("image"), Some("create")) => {
            let paths = parsed_current_paths(parsed)?;
            let image_path = required_flag(parsed, "image-file")?;
            let result = insert_image(
                &paths,
                InsertImageInput {
                    anchor_shape_id: string_flag(parsed, "anchor-shape-id"),
                    display_height: number_flag(parsed, "display-height")?,
                    display_width: number_flag(parsed, "display-width")?,
                    file_name: string_flag(parsed, "file-name"),
                    image_path,
                    metadata: image_metadata_flag(parsed)?,
                    placement: string_flag(parsed, "placement").or(Some("auto")),
                    replace_shape_id: string_flag(parsed, "replace-shape-id"),
                },
            )?;
            let text = format!("Inserted image shape {}", result.shape_id);
            write_result(parsed, stdout, &result, &text)
        }
        _ => Err(CliError::new(
            "Expected canvas subcommand: selection export, image create, or image generate.",
        )),
    }
}

fn run_studio_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        Some("start") => {
            let paths = parsed_paths(parsed)?;
            sync_builtin_agent_skills(&paths)?;
            let result = start_studio(
                &paths,
                StudioStartOptions {
                    host: studio_host(parsed),
                    static_dir: string_flag(parsed, "static-dir").map(std::path::PathBuf::from),
                },
            )?;
            let bootstrap = match bootstrap_studio(&paths) {
                Ok(bootstrap) => bootstrap,
                Err(error) => {
                    if !result.reused_existing {
                        let _ = stop_studio_session(&paths);
                    }
                    return Err(error);
                }
            };
            let payload = studio_launch_payload(&paths, &result, &bootstrap, None, true)?;
            let text = payload["embeddedBrowserUrl"].as_str().unwrap_or("");
            write_result(parsed, stdout, &payload, text)
        }
        Some("status") => {
            let paths = parsed_paths(parsed)?;
            let status = studio_status(&paths)?;
            let text = studio_status_text(status.server);
            write_result(parsed, stdout, &status, &text)
        }
        Some("open-system-browser") => {
            let paths = parsed_paths(parsed)?;
            sync_builtin_agent_skills(&paths)?;
            let result = start_studio(
                &paths,
                StudioStartOptions {
                    host: studio_host(parsed),
                    static_dir: string_flag(parsed, "static-dir").map(std::path::PathBuf::from),
                },
            )?;
            let bootstrap = match bootstrap_studio(&paths) {
                Ok(bootstrap) => bootstrap,
                Err(error) => {
                    if !result.reused_existing {
                        let _ = stop_studio_session(&paths);
                    }
                    return Err(error);
                }
            };
            let payload = studio_system_browser_payload(&paths, &result, &bootstrap, open_browser)?;
            let system_url = payload["systemBrowserUrl"].as_str().unwrap_or("");
            write_result(parsed, stdout, &payload, &format!("Opened {system_url}"))
        }
        Some("serve") => {
            let paths = parsed_paths(parsed)?;
            let service_paths = resolve_studio_service_paths(&paths)?;
            let transition_lock = acquire_studio_transition_lock(&paths)?;
            crate::context_cleanup::cleanup_context_storage(&paths);
            sync_builtin_agent_skills(&paths)?;
            if let Some(session) = reuse_existing_studio(&paths)? {
                let bootstrap = bootstrap_studio(&paths)?;
                let server_version = crate::studio::studio_version(&session)?
                    .unwrap_or_else(|| "unknown".to_owned());
                let result = StudioStartResult {
                    session,
                    reused_existing: true,
                    server_version,
                    lifecycle: crate::studio::StudioLifecycle::Reused,
                    previous_version: None,
                    browser_refresh_required: false,
                };
                let payload = studio_launch_payload(
                    &paths,
                    &result,
                    &bootstrap,
                    Some(("foreground", false)),
                    true,
                )?;
                let text = payload["embeddedBrowserUrl"].as_str().unwrap_or("");
                return write_result(parsed, stdout, &payload, text);
            }
            let host = studio_host(parsed);
            let port = studio_port(parsed)?.unwrap_or(find_open_port(&host)?);
            let local_server_url = format!("http://127.0.0.1:{port}");
            let session = StudioSession {
                system_browser_url: Some(local_server_url.clone()),
                host: Some(host.clone()),
                lan_server_urls: Some(Vec::new()),
                local_server_url: Some(local_server_url.clone()),
                log_path: paths.studio_dir.join("studio.log").display().to_string(),
                pid: std::process::id(),
                port,
                server_url: local_server_url.clone(),
                started_at: crate::control::now_iso(),
                storage_dir: paths.storage_dir.display().to_string(),
            };
            let bootstrap = ensure_project_bootstrap(&service_paths, BootstrapRequest::new())?;
            write_studio_session(&paths, &session)?;
            let result = StudioStartResult {
                session,
                reused_existing: false,
                server_version: VERSION.to_owned(),
                lifecycle: crate::studio::StudioLifecycle::Started,
                previous_version: None,
                browser_refresh_required: false,
            };
            let payload = studio_launch_payload(
                &paths,
                &result,
                &bootstrap,
                Some(("foreground", true)),
                true,
            )?;
            let text = payload["embeddedBrowserUrl"].as_str().unwrap_or("");
            write_result(parsed, stdout, &payload, text)?;
            stdout
                .flush()
                .map_err(|error| CliError::new(error.to_string()))?;
            drop(transition_lock);
            let static_dir = string_flag(parsed, "static-dir").map(std::path::PathBuf::from);
            let exit_code = run_server(&host, port, service_paths, static_dir)?;
            std::process::exit(exit_code);
        }
        Some("wait") => {
            let paths = parsed_paths(parsed)?;
            let timeout = string_flag(parsed, "timeout")
                .unwrap_or("10")
                .parse::<u64>()
                .map_err(|_| CliError::new("Expected --timeout to be a number."))?;
            let session =
                wait_for_existing_studio(&paths, std::time::Duration::from_secs(timeout))?;
            let payload = studio_session_payload(&session, &[("ok", serde_json::json!(true))])?;
            write_result(parsed, stdout, &payload, &session.server_url)
        }
        Some("stop") => {
            let paths = parsed_paths(parsed)?;
            let result = stop_studio_session(&paths)?;
            let payload = serde_json::json!({
                "ok": true,
                "contextDir": paths.context_dir,
                "contextId": paths.context_id,
                "contextIdSource": paths.context_id_source,
                "projectDir": paths.project_dir,
                "storageDir": paths.storage_dir,
                "stopped": result.stopped,
            });
            let text = if result.stopped {
                "Stopped MyOpenPanels"
            } else {
                "No MyOpenPanels Studio is running"
            };
            write_result(parsed, stdout, &payload, text)
        }
        _ => Err(CliError::new(
            "Expected studio subcommand: start, status, open-system-browser, serve, wait, or stop.",
        )),
    }
}

fn run_operation_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let paths = parsed_current_paths(parsed)?;
    match parsed.positionals.get(1).map(String::as_str) {
        Some("list") => {
            let payload = with_operation_actions(
                operations::list(&paths, string_flag(parsed, "status"))?,
                true,
            );
            write_result(parsed, stdout, &payload, "Operations")
        }
        Some("read") => {
            let id = required_flag(parsed, "operation-id")?;
            let payload = with_operation_actions(operations::inspect(&paths, id)?, false);
            write_result(
                parsed,
                stdout,
                &payload,
                payload["status"].as_str().unwrap_or("unknown"),
            )
        }
        Some("complete") => {
            let payload = operations::complete(
                &paths,
                required_flag(parsed, "operation-id")?,
                required_flag(parsed, "artifact-file")?,
                image_metadata_flag(parsed)?,
            )?;
            write_result(parsed, stdout, &payload, "Operation completed")
        }
        Some("fail") => {
            let payload = operations::finish_any(
                &paths,
                required_flag(parsed, "operation-id")?,
                "failed",
                Some(required_flag(parsed, "message")?),
            )?;
            write_result(parsed, stdout, &payload, "Operation failed")
        }
        Some("cancel") => {
            let payload = operations::finish_any(
                &paths,
                required_flag(parsed, "operation-id")?,
                "cancelled",
                None,
            )?;
            write_result(parsed, stdout, &payload, "Operation cancelled")
        }
        _ => Err(CliError::new(
            "Expected operation subcommand: list, read, complete, fail, or cancel.",
        )),
    }
}
