use crate::agent::{
    agent_context, capabilities, list_agent_guides, read_agent_guide, render_agent_guides_markdown,
};
use crate::canvas::{insert_image, insert_placeholder, InsertImageInput, InsertPlaceholderInput};
use crate::control::{ensure_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::resolve_openpanels_paths;
use crate::selection::{read_selection, read_selection_asset_to_file};
use crate::server::run_server;
use crate::studio::{
    start_studio, stop_studio, studio_status, wait_for_existing_studio, StudioServerStatus,
    StudioStartOptions,
};
use crate::types::{PanelKind, ProjectBootstrap};
use crate::update::{
    check_for_update, download_update, install_update, maybe_notify_update, UpdateCheckPayload,
    UpdateDownloadPayload, UpdateInstallPayload, DEFAULT_MANIFEST_URL,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{self, Write};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const HELP_TEXT: &str = concat!(
    "openpanels-local <command> [options]\n\n",
    "Commands:\n",
    "  studio start              Start or reuse the local studio\n",
    "  studio status             Show local studio status\n",
    "  studio open               Open the local studio in a browser\n",
    "  studio wait               Wait for the local studio to become ready\n",
    "  studio stop               Stop the local studio\n",
    "  agent context             Print compact agent context with capabilities\n",
    "  agent capabilities        Print the agent capability manifest\n",
    "  agent guides              List loadable agent guides\n",
    "  agent guide <id>          Print one full agent guide\n",
    "  agent-context             Print current project, panels, and agent instructions\n",
    "  panels                    List panels in the current project\n",
    "  active-panel              Read or switch the active project panel\n",
    "  panel-state               Read state for the active or requested panel\n",
    "  canvas-state              Read the current canvas state\n",
    "  selection                 Read the current canvas selection\n",
    "  read-selection-asset      Write the exported selection asset to a file\n",
    "  insert-placeholder        Insert a generation placeholder into a clear area\n",
    "  insert-image              Insert a local image into the canvas\n\n",
    "  update                    Install the latest GitHub Releases binary\n",
    "  update check              Check GitHub Releases for a newer binary\n\n",
    "Options:\n",
    "  --project <dir>           Project directory (default: cwd or OPENPANELS_PROJECT_DIR; data is global)\n",
    "  --host <host>             Studio bind host (default: 0.0.0.0; set 127.0.0.1 for local-only)\n",
    "  --local-only              Bind the studio to 127.0.0.1\n",
    "  --format json             Emit stable JSON output\n",
    "  --version                 Print the CLI version\n",
);

#[derive(Debug, Clone, Eq, PartialEq)]
struct ParsedArgs {
    flags: BTreeMap<String, FlagValue>,
    positionals: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum FlagValue {
    String(String),
    Bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Serialize)]
struct VersionPayload<'a> {
    version: &'a str,
}

#[derive(Debug, Serialize)]
struct ErrorPayload<'a> {
    ok: bool,
    error: &'a str,
}

pub fn run_cli(argv: &[String]) -> i32 {
    let stdout = io::stdout();
    let stderr = io::stderr();
    run_cli_with_io(argv, stdout.lock(), stderr.lock())
}

fn run_cli_with_io(argv: &[String], mut stdout: impl Write, mut stderr: impl Write) -> i32 {
    let parsed = parse_args(argv);
    match run_parsed_cli(&parsed, &mut stdout, &mut stderr) {
        Ok(()) => 0,
        Err(error) => {
            write_error(&parsed, &mut stdout, &mut stderr, &error);
            error.exit_code()
        }
    }
}

fn run_parsed_cli(
    parsed: &ParsedArgs,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<(), CliError> {
    let command = parsed.positionals.first().map(String::as_str);

    if has_flag(parsed, "version") || command == Some("version") {
        write_result(
            parsed,
            stdout,
            &VersionPayload { version: VERSION },
            VERSION,
        )?;
        return Ok(());
    }

    if command.is_none() || command == Some("help") || has_flag(parsed, "help") {
        write_text(stdout, HELP_TEXT)?;
        return Ok(());
    }

    if command == Some("update") {
        return run_update_command(parsed, stdout);
    }

    if command == Some("__serve-studio") {
        let paths = parsed_paths(parsed)?;
        let port = string_flag(parsed, "port")
            .ok_or_else(|| CliError::new("Missing --port for internal studio server."))?
            .parse::<u16>()
            .map_err(|_| CliError::new("Expected --port to be a number."))?;
        let host = string_flag(parsed, "host").unwrap_or("127.0.0.1");
        let static_dir = string_flag(parsed, "static-dir").map(std::path::PathBuf::from);
        let exit_code = run_server(host, port, paths, static_dir)?;
        std::process::exit(exit_code);
    }

    if command == Some("studio") {
        return run_studio_command(parsed, stdout);
    }

    if command == Some("agent") {
        return run_agent_command(parsed, stdout);
    }

    if command == Some("agent-context") {
        let paths = parsed_paths(parsed)?;
        let (payload, markdown) = agent_context(&paths, VERSION, None)?;
        write_result(parsed, stdout, &payload, &markdown)?;
        return Ok(());
    }

    if command == Some("wiki") {
        return run_wiki_command(parsed, stdout);
    }

    if matches!(
        command,
        Some("panels" | "active-panel" | "panel-state" | "canvas-state")
    ) {
        return run_project_read_command(parsed, stdout, command.unwrap());
    }

    if command == Some("selection") {
        let paths = resolve_openpanels_paths(
            string_flag(parsed, "project"),
            string_flag(parsed, "storage-dir"),
            string_flag(parsed, "context-id"),
        )?;
        let result = read_selection(
            &paths,
            string_flag(parsed, "session-id"),
            has_flag(parsed, "include-image-base64"),
        )?;
        let text = format!(
            "Selection contains {} shape(s)",
            selected_shape_count(&result.selection)
        );
        write_result(parsed, stdout, &result, &text)?;
        return Ok(());
    }

    if command == Some("read-selection-asset") {
        let paths = parsed_paths(parsed)?;
        let output = required_flag(parsed, "output")?;
        let result =
            read_selection_asset_to_file(&paths, string_flag(parsed, "session-id"), output)?;
        let text = format!("Wrote {}", result.output_path);
        write_result(parsed, stdout, &result, &text)?;
        return Ok(());
    }

    if command == Some("insert-image") {
        let paths = parsed_paths(parsed)?;
        let image_path = required_flag(parsed, "image")?;
        let result = insert_image(
            &paths,
            InsertImageInput {
                anchor_shape_id: string_flag(parsed, "anchor-shape-id"),
                display_height: number_flag(parsed, "display-height")?,
                display_width: number_flag(parsed, "display-width")?,
                file_name: string_flag(parsed, "file-name"),
                image_path,
                placement: string_flag(parsed, "placement"),
                replace_shape_id: string_flag(parsed, "replace-shape-id"),
            },
        )?;
        let text = format!("Inserted image shape {}", result.shape_id);
        write_result(parsed, stdout, &result, &text)?;
        return Ok(());
    }

    if command == Some("insert-placeholder") {
        let paths = parsed_paths(parsed)?;
        let result = insert_placeholder(
            &paths,
            InsertPlaceholderInput {
                anchor_shape_id: string_flag(parsed, "anchor-shape-id"),
                display_height: number_flag(parsed, "display-height")?,
                display_width: number_flag(parsed, "display-width")?,
                text: string_flag(parsed, "text"),
            },
        )?;
        let text = format!("Inserted placeholder shape {}", result.shape_id);
        write_result(parsed, stdout, &result, &text)?;
        return Ok(());
    }

    if should_check_for_updates(parsed, command) {
        maybe_notify_update(VERSION, stderr);
    }

    Err(CliError::new(format!(
        "Unknown command: {}",
        command.unwrap_or_default()
    )))
}

fn run_studio_command(parsed: &ParsedArgs, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        Some("start") => {
            let paths = parsed_paths(parsed)?;
            let session = start_studio(
                &paths,
                StudioStartOptions {
                    host: studio_host(parsed),
                    open_browser: !has_flag(parsed, "no-open"),
                    static_dir: string_flag(parsed, "static-dir").map(std::path::PathBuf::from),
                },
            )?;
            let payload = studio_session_payload(&session, &[("ok", serde_json::json!(true))])?;
            write_result(parsed, stdout, &payload, &session.server_url)
        }
        Some("status") => {
            let paths = parsed_paths(parsed)?;
            let status = studio_status(&paths)?;
            let text = studio_status_text(status.server);
            write_result(parsed, stdout, &status, &text)
        }
        Some("open") => {
            let paths = parsed_paths(parsed)?;
            let session = start_studio(
                &paths,
                StudioStartOptions {
                    host: studio_host(parsed),
                    open_browser: true,
                    static_dir: string_flag(parsed, "static-dir").map(std::path::PathBuf::from),
                },
            )?;
            let payload = studio_session_payload(
                &session,
                &[
                    ("ok", serde_json::json!(true)),
                    ("opened", serde_json::json!(true)),
                ],
            )?;
            write_result(
                parsed,
                stdout,
                &payload,
                &format!("Opened {}", session.server_url),
            )
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
            stop_studio(&paths)?;
            let payload = serde_json::json!({
                "ok": true,
                "contextDir": paths.context_dir,
                "contextId": paths.context_id,
                "contextIdSource": paths.context_id_source,
                "projectDir": paths.project_dir,
                "storageDir": paths.storage_dir,
                "stopped": true,
            });
            write_result(parsed, stdout, &payload, "Stopped MyOpenPanels")
        }
        _ => Err(CliError::new(
            "Expected studio subcommand: start, status, open, wait, or stop.",
        )),
    }
}

fn run_agent_command(parsed: &ParsedArgs, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        None | Some("context") => {
            let paths = parsed_paths(parsed)?;
            let (payload, markdown) = agent_context(&paths, VERSION, None)?;
            write_result(parsed, stdout, &payload, &markdown)
        }
        Some("capabilities") => {
            let capabilities = capabilities();
            let payload = serde_json::json!({ "capabilities": capabilities });
            write_result(
                parsed,
                stdout,
                &payload,
                &render_capabilities_summary(&capabilities),
            )
        }
        Some("guides") => {
            let guides = list_agent_guides()?;
            let payload = serde_json::json!({ "guides": guides });
            write_result(
                parsed,
                stdout,
                &payload,
                &render_agent_guides_markdown(&guides),
            )
        }
        Some("guide") => {
            let guide_id = parsed
                .positionals
                .get(2)
                .ok_or_else(|| CliError::new("Missing guide id."))?;
            let paths = parsed_paths(parsed)?;
            let payload = read_agent_guide(&paths, guide_id, string_flag(parsed, "task-id"))?;
            let markdown = payload.markdown.clone();
            write_result(parsed, stdout, &payload, &markdown)
        }
        _ => Err(CliError::new(
            "Expected agent subcommand: context, capabilities, guides, or guide.",
        )),
    }
}

fn run_wiki_command(parsed: &ParsedArgs, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        None | Some("context") => {
            let paths = parsed_paths(parsed)?;
            let (payload, markdown) = agent_context(&paths, VERSION, Some(PanelKind::Wiki))?;
            write_result(parsed, stdout, &payload, &markdown)
        }
        _ => Err(CliError::new("Unknown wiki command.")),
    }
}

fn run_project_read_command(
    parsed: &ParsedArgs,
    stdout: &mut impl Write,
    command: &str,
) -> Result<(), CliError> {
    let paths = parsed_paths(parsed)?;
    let mut request = BootstrapRequest::new();
    request.requested_session_id = string_flag(parsed, "session-id").map(str::to_owned);
    request.requested_panel_id = string_flag(parsed, "panel-id").map(str::to_owned);
    request.requested_panel_kind = if command == "canvas-state" {
        Some(PanelKind::Canvas)
    } else {
        string_flag(parsed, "kind")
            .map(parse_panel_kind)
            .transpose()?
    };
    let bootstrap = ensure_project_bootstrap(&paths, request)?;

    match command {
        "panels" => {
            let payload = serde_json::json!({
                "activePanelId": bootstrap.active_panel_id,
                "activePanelKind": bootstrap.active_panel_kind,
                "panels": bootstrap.panels.iter().map(|snapshot| &snapshot.panel).collect::<Vec<_>>(),
                "project": bootstrap.session,
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
        "active-panel" => {
            let payload = serde_json::json!({
                "activePanelId": bootstrap.active_panel_id,
                "activePanelKind": bootstrap.active_panel_kind,
                "panel": bootstrap.panel,
                "project": bootstrap.session,
            });
            let text = format!(
                "{}: {}",
                bootstrap.active_panel_kind.as_str(),
                bootstrap.panel.title
            );
            write_result(parsed, stdout, &payload, &text)
        }
        "panel-state" => {
            let payload = serde_json::json!({
                "activePanelId": bootstrap.active_panel_id,
                "activePanelKind": bootstrap.active_panel_kind,
                "panel": bootstrap.panel,
                "project": bootstrap.session,
                "state": bootstrap.state,
            });
            let text = format!("{} state ready", bootstrap.active_panel_kind.as_str());
            write_result(parsed, stdout, &payload, &text)
        }
        "canvas-state" => {
            let text = format!("Canvas ready at {}", bootstrap.storage_dir);
            write_result(
                parsed,
                stdout,
                &CanvasBootstrapPayload::from(bootstrap),
                &text,
            )
        }
        _ => Err(CliError::new(format!("Unknown command: {command}"))),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CanvasBootstrapPayload {
    context_dir: String,
    context_id: String,
    context_id_source: String,
    panel: crate::types::Panel,
    panel_dir: String,
    session: crate::types::Session,
    sessions: Vec<crate::types::Session>,
    state: Value,
    storage_dir: String,
}

impl From<ProjectBootstrap> for CanvasBootstrapPayload {
    fn from(bootstrap: ProjectBootstrap) -> Self {
        Self {
            context_dir: bootstrap.context_dir,
            context_id: bootstrap.context_id,
            context_id_source: bootstrap.context_id_source,
            panel: bootstrap.panel,
            panel_dir: bootstrap.panel_dir,
            session: bootstrap.session,
            sessions: bootstrap.sessions,
            state: bootstrap.state,
            storage_dir: bootstrap.storage_dir,
        }
    }
}

fn parsed_paths(parsed: &ParsedArgs) -> Result<crate::paths::OpenPanelsPaths, CliError> {
    resolve_openpanels_paths(
        string_flag(parsed, "project"),
        string_flag(parsed, "storage-dir"),
        string_flag(parsed, "context-id"),
    )
}

fn studio_host(parsed: &ParsedArgs) -> String {
    if has_flag(parsed, "local-only") {
        return "127.0.0.1".to_owned();
    }
    string_flag(parsed, "host")
        .map(str::to_owned)
        .or_else(|| std::env::var("OPENPANELS_STUDIO_HOST").ok())
        .unwrap_or_else(|| "0.0.0.0".to_owned())
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

fn run_update_command(parsed: &ParsedArgs, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    if subcommand == Some("help") || has_flag(parsed, "help") {
        write_text(stdout, &update_help_text())?;
        return Ok(());
    }

    if has_flag(parsed, "check") || subcommand == Some("check") {
        let payload = check_for_update(VERSION, false)?;
        let text = update_check_text(&payload);
        write_result(parsed, stdout, &payload, &text)?;
        return Ok(());
    }

    if subcommand == Some("download") {
        let payload = download_update(VERSION)?;
        let text = update_download_text(&payload);
        write_result(parsed, stdout, &payload, &text)?;
        return Ok(());
    }

    if subcommand.is_none() || subcommand == Some("install") {
        let payload = install_update(VERSION)?;
        let text = update_install_text(&payload);
        write_result(parsed, stdout, &payload, &text)?;
        return Ok(());
    }

    Err(CliError::new(format!(
        "Unknown update command: {}",
        subcommand.unwrap_or_default()
    )))
}

fn parse_args(argv: &[String]) -> ParsedArgs {
    let mut flags = BTreeMap::new();
    let mut positionals = Vec::new();
    let mut index = 0;
    while index < argv.len() {
        let arg = &argv[index];
        if !arg.starts_with("--") {
            positionals.push(arg.clone());
            index += 1;
            continue;
        }

        let raw = &arg[2..];
        if let Some((name, value)) = raw.split_once('=') {
            flags.insert(name.to_owned(), FlagValue::String(value.to_owned()));
            index += 1;
            continue;
        }

        if let Some(next) = argv.get(index + 1) {
            if !next.starts_with("--") {
                flags.insert(raw.to_owned(), FlagValue::String(next.clone()));
                index += 2;
                continue;
            }
        }

        flags.insert(raw.to_owned(), FlagValue::Bool);
        index += 1;
    }

    ParsedArgs { flags, positionals }
}

fn has_flag(parsed: &ParsedArgs, name: &str) -> bool {
    parsed.flags.contains_key(name)
}

fn string_flag<'a>(parsed: &'a ParsedArgs, name: &str) -> Option<&'a str> {
    match parsed.flags.get(name) {
        Some(FlagValue::String(value)) => Some(value),
        _ => None,
    }
}

fn required_flag<'a>(parsed: &'a ParsedArgs, name: &str) -> Result<&'a str, CliError> {
    string_flag(parsed, name)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CliError::new(format!("Missing required --{name} <value>.")))
}

fn number_flag(parsed: &ParsedArgs, name: &str) -> Result<Option<f64>, CliError> {
    string_flag(parsed, name)
        .map(|value| {
            value
                .parse::<f64>()
                .map_err(|_| CliError::new(format!("Expected --{name} to be a number.")))
        })
        .transpose()
}

fn parse_panel_kind(value: &str) -> Result<PanelKind, CliError> {
    PanelKind::parse(value).ok_or_else(|| {
        CliError::new("Expected --kind to be one of: wiki, canvas, image, diff, preview, files.")
    })
}

fn output_format(parsed: &ParsedArgs) -> OutputFormat {
    match string_flag(parsed, "format") {
        Some("json") => OutputFormat::Json,
        _ => OutputFormat::Text,
    }
}

fn selected_shape_count(selection: &Value) -> usize {
    selection
        .get("selectedShapes")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

fn studio_status_text(status: StudioServerStatus) -> String {
    match status {
        StudioServerStatus::Running => "MyOpenPanels studio running".to_owned(),
        StudioServerStatus::Missing => "MyOpenPanels studio missing".to_owned(),
        StudioServerStatus::Stale => "MyOpenPanels studio stale".to_owned(),
        StudioServerStatus::Unavailable => "MyOpenPanels studio unavailable".to_owned(),
    }
}

fn render_capabilities_summary(capabilities: &[Value]) -> String {
    let rows = capabilities
        .iter()
        .map(|capability| {
            format!(
                "| `{}` | `{}` |",
                capability["intent"].as_str().unwrap_or(""),
                capability["command"].as_str().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("# OpenPanels Agent Capabilities\n\n| Intent | Command |\n| --- | --- |\n{rows}\n")
}

fn should_check_for_updates(parsed: &ParsedArgs, command: Option<&str>) -> bool {
    if output_format(parsed) == OutputFormat::Json {
        return false;
    }

    matches!(
        command,
        Some(
            "studio"
                | "agent"
                | "agent-context"
                | "panels"
                | "active-panel"
                | "panel-state"
                | "canvas-state"
                | "selection"
                | "read-selection-asset"
                | "insert-placeholder"
                | "insert-image"
                | "wiki"
        )
    )
}

fn update_check_text(payload: &UpdateCheckPayload) -> String {
    let latest = payload.latest_version.as_deref().unwrap_or("unknown");
    if payload.update_available {
        if payload.asset_available {
            format!(
                "Update available: openpanels-local {} -> {latest}. Run `openpanels-local update` to install.",
                payload.current_version
            )
        } else {
            format!(
                "Update available: openpanels-local {} -> {latest}, but no asset exists for {}.",
                payload.current_version, payload.target
            )
        }
    } else {
        format!("openpanels-local is up to date ({latest}).")
    }
}

fn update_install_text(payload: &UpdateInstallPayload) -> String {
    let latest = payload.latest_version.as_deref().unwrap_or("unknown");
    if payload.updated {
        format!(
            "Updated openpanels-local {} -> {latest}.",
            payload.current_version
        )
    } else {
        format!("openpanels-local is already up to date ({latest}).")
    }
}

fn update_download_text(payload: &UpdateDownloadPayload) -> String {
    let latest = payload.latest_version.as_deref().unwrap_or("unknown");
    if payload.downloaded {
        format!("Downloaded openpanels-local {latest}.")
    } else if payload.update_available {
        format!(
            "Update available: openpanels-local {} -> {latest}, but it was not downloaded.",
            payload.current_version
        )
    } else {
        format!("openpanels-local is already up to date ({latest}).")
    }
}

fn update_help_text() -> String {
    format!(concat!(
        "openpanels-local update [check|install] [options]\n\n",
        "Commands:\n",
        "  update                    Download, verify, and install the latest GitHub Releases binary\n",
        "  update install            Same as `update`\n",
        "  update download           Download and cache the latest binary without installing it\n",
        "  update check              Check whether a newer GitHub Releases binary exists\n\n",
        "Options:\n",
        "  --check                   Same as `update check`\n",
        "  --format json             Emit stable JSON output\n\n",
        "Environment:\n",
        "  OPENPANELS_UPDATE_MANIFEST_URL  Override the release manifest URL\n",
        "  OPENPANELS_UPDATE_CACHE_DIR     Override the 24-hour update check cache directory\n",
        "  OPENPANELS_DISABLE_UPDATE_CHECK Disable opportunistic 24-hour update checks\n\n",
        "Default manifest:\n",
        "{}\n",
    ), DEFAULT_MANIFEST_URL)
}

fn write_result(
    parsed: &ParsedArgs,
    stdout: &mut impl Write,
    payload: &impl Serialize,
    text: &str,
) -> Result<(), CliError> {
    match output_format(parsed) {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(payload)
                .map_err(|error| CliError::new(error.to_string()))?;
            write_text(stdout, &format!("{json}\n"))
        }
        OutputFormat::Text => write_text(stdout, &format!("{text}\n")),
    }
}

fn write_error(
    parsed: &ParsedArgs,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    error: &CliError,
) {
    match output_format(parsed) {
        OutputFormat::Json => {
            let payload = ErrorPayload {
                ok: false,
                error: error.message(),
            };
            let _ = serde_json::to_writer_pretty(&mut *stdout, &payload);
            let _ = stdout.write_all(b"\n");
        }
        OutputFormat::Text => {
            let _ = write_text(stderr, &format!("Error: {}\n", error.message()));
        }
    }
}

fn write_text(stream: &mut impl Write, text: &str) -> Result<(), CliError> {
    stream
        .write_all(text.as_bytes())
        .map_err(|error| CliError::new(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use rusqlite::{params, Connection};
    use serde_json::{json, Value};
    use std::fs;
    use std::path::Path;

    fn run(args: &[&str]) -> (i32, String, String) {
        let argv = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_cli_with_io(&argv, &mut stdout, &mut stderr);
        (
            code,
            String::from_utf8(stdout).expect("stdout should be utf8"),
            String::from_utf8(stderr).expect("stderr should be utf8"),
        )
    }

    #[test]
    fn project_read_commands_bootstrap_project() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");

        let (code, stdout, stderr) = run(&[
            "panels",
            "--project",
            project_dir.to_str().unwrap(),
            "--storage-dir",
            storage_dir.to_str().unwrap(),
            "--context-id",
            "ctx",
            "--format",
            "json",
        ]);

        assert_eq!(code, 0, "{stderr}");
        let payload = serde_json::from_str::<Value>(&stdout).expect("json");
        assert_eq!(payload["activePanelKind"], "wiki");
        assert_eq!(
            payload["panels"]
                .as_array()
                .expect("panels")
                .iter()
                .map(|panel| panel["kind"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["wiki", "canvas"]
        );
        assert_eq!(stderr, "");

        let (code, stdout, stderr) = run(&[
            "active-panel",
            "--project",
            project_dir.to_str().unwrap(),
            "--storage-dir",
            storage_dir.to_str().unwrap(),
            "--context-id",
            "ctx",
            "--kind",
            "canvas",
            "--format",
            "json",
        ]);
        assert_eq!(code, 0, "{stderr}");
        assert_eq!(
            serde_json::from_str::<Value>(&stdout).expect("json")["activePanelKind"],
            "canvas"
        );
    }

    #[test]
    fn agent_commands_emit_context_guides_and_capabilities() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");

        let (code, stdout, stderr) = run(&[
            "agent-context",
            "--project",
            project_dir.to_str().unwrap(),
            "--storage-dir",
            storage_dir.to_str().unwrap(),
            "--context-id",
            "ctx",
            "--format",
            "json",
        ]);
        assert_eq!(code, 0, "{stderr}");
        let payload = serde_json::from_str::<Value>(&stdout).expect("json");
        assert_eq!(payload["protocolVersion"], 1);
        assert_eq!(payload["cliVersion"], VERSION);
        assert_eq!(payload["activePanel"]["kind"], "wiki");
        assert_eq!(payload["state"]["wiki"]["language"], Value::Null);
        assert!(payload["capabilities"]
            .as_array()
            .expect("capabilities")
            .iter()
            .any(|capability| capability["intent"] == "canvas.placeholder.create"));
        assert!(payload["availableGuides"]
            .as_array()
            .expect("available guides")
            .iter()
            .any(|guide| guide["id"] == "canvas.image-generation" && guide["source"] == "builtin"));

        let (code, stdout, stderr) = run(&[
            "agent",
            "context",
            "--project",
            project_dir.to_str().unwrap(),
            "--storage-dir",
            storage_dir.to_str().unwrap(),
            "--context-id",
            "ctx",
        ]);
        assert_eq!(code, 0, "{stderr}");
        assert!(stdout.contains("# OpenPanels Agent Context"));
        assert!(stdout.contains("## Capabilities"));

        let (code, stdout, stderr) = run(&[
            "agent",
            "guides",
            "--project",
            project_dir.to_str().unwrap(),
            "--storage-dir",
            storage_dir.to_str().unwrap(),
            "--context-id",
            "ctx",
        ]);
        assert_eq!(code, 0, "{stderr}");
        assert!(stdout.contains("wiki.index-document"));

        let (code, stdout, stderr) = run(&[
            "agent",
            "guide",
            "canvas.image-generation",
            "--project",
            project_dir.to_str().unwrap(),
            "--storage-dir",
            storage_dir.to_str().unwrap(),
            "--context-id",
            "ctx",
        ]);
        assert_eq!(code, 0, "{stderr}");
        assert!(stdout.contains("# Guide: canvas.image-generation"));
        assert!(stdout.contains("## Instructions"));

        let (code, stdout, stderr) = run(&[
            "wiki",
            "context",
            "--project",
            project_dir.to_str().unwrap(),
            "--storage-dir",
            storage_dir.to_str().unwrap(),
            "--context-id",
            "ctx",
            "--format",
            "json",
        ]);
        assert_eq!(code, 0, "{stderr}");
        assert_eq!(
            serde_json::from_str::<Value>(&stdout).expect("json")["activePanel"]["kind"],
            "wiki"
        );
    }

    #[test]
    fn canvas_write_commands_insert_and_replace_shapes() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        let image_path = project_dir.join("image.png");
        fs::create_dir_all(&project_dir).expect("project dir");
        fs::write(&image_path, tiny_png()).expect("image");

        let (code, stdout, stderr) = run(&[
            "insert-image",
            "--project",
            project_dir.to_str().unwrap(),
            "--storage-dir",
            storage_dir.to_str().unwrap(),
            "--context-id",
            "ctx",
            "--image",
            image_path.to_str().unwrap(),
            "--display-width",
            "512",
            "--display-height",
            "512",
            "--format",
            "json",
        ]);
        assert_eq!(code, 0, "{stderr}");
        let inserted = serde_json::from_str::<Value>(&stdout).expect("json");
        assert_eq!(
            inserted["bounds"],
            json!({ "x": 160.0, "y": 160.0, "width": 512.0, "height": 512.0 })
        );

        let (code, stdout, stderr) = run(&[
            "insert-placeholder",
            "--project",
            project_dir.to_str().unwrap(),
            "--storage-dir",
            storage_dir.to_str().unwrap(),
            "--context-id",
            "ctx",
            "--anchor-shape-id",
            inserted["shapeId"].as_str().unwrap(),
            "--display-width",
            "512",
            "--display-height",
            "512",
            "--format",
            "json",
        ]);
        assert_eq!(code, 0, "{stderr}");
        let placeholder = serde_json::from_str::<Value>(&stdout).expect("json");
        assert_eq!(
            placeholder["bounds"],
            json!({ "x": 752.0, "y": 160.0, "width": 512.0, "height": 512.0 })
        );

        let (code, stdout, stderr) = run(&[
            "insert-image",
            "--project",
            project_dir.to_str().unwrap(),
            "--storage-dir",
            storage_dir.to_str().unwrap(),
            "--context-id",
            "ctx",
            "--image",
            image_path.to_str().unwrap(),
            "--replace-shape-id",
            placeholder["shapeId"].as_str().unwrap(),
            "--format",
            "json",
        ]);
        assert_eq!(code, 0, "{stderr}");
        let replaced = serde_json::from_str::<Value>(&stdout).expect("json");
        assert_eq!(replaced["replacedShapeId"], placeholder["shapeId"]);
        assert_eq!(replaced["bounds"], placeholder["bounds"]);

        let (code, stdout, stderr) = run(&[
            "panel-state",
            "--project",
            project_dir.to_str().unwrap(),
            "--storage-dir",
            storage_dir.to_str().unwrap(),
            "--context-id",
            "ctx",
            "--kind",
            "canvas",
            "--format",
            "json",
        ]);
        assert_eq!(code, 0, "{stderr}");
        let state = serde_json::from_str::<Value>(&stdout).expect("json")["state"].clone();
        assert_eq!(state["selectedShapeIds"], json!([replaced["shapeId"]]));
        assert!(state["store"][placeholder["shapeId"].as_str().unwrap()].is_null());
    }

    #[test]
    fn studio_status_reports_missing_session() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");

        let (code, stdout, stderr) = run(&[
            "studio",
            "status",
            "--project",
            project_dir.to_str().unwrap(),
            "--storage-dir",
            storage_dir.to_str().unwrap(),
            "--context-id",
            "ctx",
            "--format",
            "json",
        ]);

        assert_eq!(code, 0, "{stderr}");
        let payload = serde_json::from_str::<Value>(&stdout).expect("json");
        assert_eq!(payload["ok"], true);
        assert_eq!(payload["server"], "missing");
        assert_eq!(payload["contextId"], "ctx");
        assert_eq!(stderr, "");
    }

    #[test]
    fn selection_reads_sqlite_panel_selection() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        let context_dir = storage_dir.join("contexts").join("ctx");
        fs::create_dir_all(&context_dir).expect("context dir");
        fs::create_dir_all(&project_dir).expect("project dir");
        seed_selection_database(
            &storage_dir,
            "session:1",
            "panel:canvas",
            serde_json::json!({
                "sessionId": "session:1",
                "panelId": "panel:canvas",
                "selectedShapeIds": ["shape:1"],
                "selectedShapes": [{
                    "id": "shape:1",
                    "type": "geo",
                    "parentId": "page:main",
                    "props": {},
                    "bounds": { "x": 1, "y": 2, "width": 3, "height": 4 }
                }],
                "assetRef": null,
                "updatedAt": "2026-07-08T00:00:00.000Z"
            }),
            None,
        );
        fs::write(
            context_dir.join("active-session.json"),
            r#"{"sessionId":"session:1"}"#,
        )
        .expect("active session");

        let (code, stdout, stderr) = run(&[
            "selection",
            "--project",
            project_dir.to_str().unwrap(),
            "--storage-dir",
            storage_dir.to_str().unwrap(),
            "--context-id",
            "ctx",
            "--format",
            "json",
        ]);

        assert_eq!(code, 0, "{stderr}");
        let payload = serde_json::from_str::<Value>(&stdout).expect("json");
        assert_eq!(
            payload["selection"]["selectedShapeIds"],
            serde_json::json!(["shape:1"])
        );
        assert_eq!(payload["contextId"], "ctx");
        assert_eq!(stderr, "");
    }

    #[test]
    fn version_prints_text() {
        let (code, stdout, stderr) = run(&["version"]);

        assert_eq!(code, 0);
        assert_eq!(stdout, format!("{VERSION}\n"));
        assert_eq!(stderr, "");
    }

    #[test]
    fn version_prints_json() {
        let (code, stdout, stderr) = run(&["--version", "--format", "json"]);

        assert_eq!(code, 0);
        assert_eq!(stdout, format!("{{\n  \"version\": \"{VERSION}\"\n}}\n"));
        assert_eq!(stderr, "");
    }

    #[test]
    fn help_prints_current_command_map() {
        let (code, stdout, stderr) = run(&[]);

        assert_eq!(code, 0);
        assert!(stdout.contains("openpanels-local <command> [options]"));
        assert!(stdout.contains("studio start"));
        assert!(stdout.contains("insert-image"));
        assert!(stdout.contains("update check"));
        assert_eq!(stderr, "");
    }

    #[test]
    fn update_help_prints_manifest_controls() {
        let (code, stdout, stderr) = run(&["update", "help"]);

        assert_eq!(code, 0);
        assert!(stdout.contains("openpanels-local update"));
        assert!(stdout.contains("OPENPANELS_UPDATE_MANIFEST_URL"));
        assert_eq!(stderr, "");
    }

    #[test]
    fn unknown_command_prints_text_error() {
        let (code, stdout, stderr) = run(&["nope"]);

        assert_eq!(code, 1);
        assert_eq!(stdout, "");
        assert_eq!(stderr, "Error: Unknown command: nope\n");
    }

    #[test]
    fn unknown_command_prints_json_error() {
        let (code, stdout, stderr) = run(&["nope", "--format=json"]);

        assert_eq!(code, 1);
        assert_eq!(
            stdout,
            "{\n  \"ok\": false,\n  \"error\": \"Unknown command: nope\"\n}\n"
        );
        assert_eq!(stderr, "");
    }

    fn seed_selection_database(
        storage_dir: &Path,
        session_id: &str,
        panel_id: &str,
        selection: Value,
        state: Option<Value>,
    ) {
        fs::create_dir_all(storage_dir).expect("storage dir");
        let connection = Connection::open(storage_dir.join("main.sqlite3")).expect("db");
        connection
            .execute_batch(
                r#"
                CREATE TABLE sessions (
                  id TEXT PRIMARY KEY NOT NULL,
                  title TEXT NOT NULL,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL,
                  panel_ids_json TEXT NOT NULL DEFAULT '[]',
                  session_json TEXT NOT NULL
                );
                CREATE TABLE panels (
                  id TEXT NOT NULL,
                  session_id TEXT NOT NULL,
                  kind TEXT NOT NULL,
                  title TEXT NOT NULL,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL,
                  state_ref TEXT,
                  panel_json TEXT NOT NULL,
                  PRIMARY KEY (session_id, id)
                );
                CREATE TABLE panel_states (
                  session_id TEXT NOT NULL,
                  panel_id TEXT NOT NULL,
                  schema_version INTEGER,
                  state_json TEXT NOT NULL,
                  updated_at TEXT NOT NULL,
                  PRIMARY KEY (session_id, panel_id)
                );
                CREATE TABLE panel_selections (
                  session_id TEXT NOT NULL,
                  panel_id TEXT NOT NULL,
                  asset_ref TEXT,
                  selected_shape_ids_json TEXT NOT NULL DEFAULT '[]',
                  selection_json TEXT NOT NULL,
                  updated_at TEXT NOT NULL,
                  PRIMARY KEY (session_id, panel_id)
                );
                "#,
            )
            .expect("schema");
        connection
            .execute(
                "INSERT INTO sessions (id, title, created_at, updated_at, panel_ids_json, session_json) VALUES (?, 'Project 1', '2026-07-08T00:00:00.000Z', '2026-07-08T00:00:00.000Z', ?, ?)",
                params![
                    session_id,
                    serde_json::json!([panel_id]).to_string(),
                    serde_json::json!({
                        "id": session_id,
                        "title": "Project 1",
                        "panelIds": [panel_id],
                        "createdAt": "2026-07-08T00:00:00.000Z",
                        "updatedAt": "2026-07-08T00:00:00.000Z"
                    }).to_string()
                ],
            )
            .expect("session");
        connection
            .execute(
                "INSERT INTO panels (id, session_id, kind, title, created_at, updated_at, state_ref, panel_json) VALUES (?, ?, 'canvas', 'Design canvas', '2026-07-08T00:00:00.000Z', '2026-07-08T00:00:00.000Z', NULL, ?)",
                params![
                    panel_id,
                    session_id,
                    serde_json::json!({
                        "id": panel_id,
                        "sessionId": session_id,
                        "kind": "canvas",
                        "title": "Design canvas",
                        "createdAt": "2026-07-08T00:00:00.000Z",
                        "updatedAt": "2026-07-08T00:00:00.000Z"
                    }).to_string()
                ],
            )
            .expect("panel");
        if let Some(state) = state {
            connection
                .execute(
                    "INSERT INTO panel_states (session_id, panel_id, schema_version, state_json, updated_at) VALUES (?, ?, 1, ?, '2026-07-08T00:00:00.000Z')",
                    params![session_id, panel_id, state.to_string()],
                )
                .expect("state");
        }
        connection
            .execute(
                "INSERT INTO panel_selections (session_id, panel_id, asset_ref, selected_shape_ids_json, selection_json, updated_at) VALUES (?, ?, NULL, ?, ?, '2026-07-08T00:00:00.000Z')",
                params![
                    session_id,
                    panel_id,
                    selection["selectedShapeIds"].to_string(),
                    selection.to_string()
                ],
            )
            .expect("selection");
    }

    fn tiny_png() -> Vec<u8> {
        base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==")
            .expect("tiny png")
    }
}
