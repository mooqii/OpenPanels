use crate::agent::{
    agent_bootstrap, list_agent_guide_summaries, list_agent_skill_summaries, read_agent_guide,
    read_agent_skill, sync_builtin_agent_skills,
};
use crate::bridge::{read_bridge_status, run_bridge, BridgeOptions};
use crate::canvas::{insert_image, InsertImageInput};
use crate::control::{
    create_project, ensure_project_bootstrap, read_active_session_id, read_focus_revision,
    read_project_bootstrap, require_active_panel, BootstrapRequest,
};
use crate::error::CliError;
use crate::operations;
use crate::paths::resolve_myopenpanels_paths;
use crate::selection::read_selection_asset_for_panel;
use crate::server::run_server;
use crate::storage::Storage;
use crate::studio::{
    discard_studio_session_binding, find_open_port, open_browser, record_current_studio,
    resolve_current_studio_session, reuse_existing_studio, start_studio, stop_studio_session,
    studio_status, wait_for_existing_studio, write_studio_session, StudioServerStatus,
    StudioSession, StudioStartOptions, StudioStartResult,
};
use crate::tasks;
use crate::trace::{self, TraceEventInput};
use crate::types::{PanelKind, ProjectBootstrap};
use crate::update::{
    check_for_update, download_update, install_update, maybe_notify_update, UpdateCheckPayload,
    UpdateDownloadPayload, UpdateInstallPayload, DEFAULT_MANIFEST_URL,
};
use crate::wiki;
use serde::Serialize;
use serde_json::Value;
mod args;
pub(crate) mod registry;
mod support;
#[cfg(test)]
mod tests;
mod update;

use self::support::*;
use self::update::run_update_command;
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
#[cfg(test)]
use std::path::PathBuf;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Eq, PartialEq)]
struct Invocation {
    flags: BTreeMap<String, FlagValue>,
    intent: String,
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
#[serde(rename_all = "camelCase")]
struct ResponseMeta<'a> {
    cli_version: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SuccessPayload<'a, T: Serialize> {
    ok: bool,
    schema_version: u32,
    intent: &'a str,
    data: &'a T,
    meta: ResponseMeta<'a>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorPayload<'a> {
    ok: bool,
    schema_version: u32,
    intent: &'a str,
    error: ErrorDetail<'a>,
    meta: ResponseMeta<'a>,
}

#[derive(Debug, Serialize)]
struct ErrorDetail<'a> {
    code: &'a str,
    message: &'a str,
    retryable: bool,
    recovery: RecoveryDetail<'a>,
}

#[derive(Debug, Serialize)]
struct RecoveryDetail<'a> {
    instruction: &'a str,
    command: Option<&'a str>,
}

pub fn run_cli(argv: &[String]) -> i32 {
    let stdout = io::stdout();
    let stderr = io::stderr();
    if let Some(trace_url) = trace_url_for_cli(argv) {
        std::env::set_var("MYOPENPANELS_TRACE_URL", trace_url);
        trace::emit_cli_event(TraceEventInput {
            audience: None,
            category: Some("cli".to_owned()),
            detail: Some(serde_json::json!({ "argv": argv })),
            direction: Some("start".to_owned()),
            release_summary: None,
            run_id: std::env::var("MYOPENPANELS_TRACE_RUN_ID").ok(),
            source: Some("myopenpanels".to_owned()),
            summary: Some(format!("myopenpanels {}", argv.join(" "))),
            task_id: std::env::var("MYOPENPANELS_TRACE_RUN_ID").ok(),
        });
        let code = run_cli_with_io(
            argv,
            TracedWriter::new(stdout.lock(), "stdout"),
            TracedWriter::new(stderr.lock(), "stderr"),
        );
        trace::emit_cli_event(TraceEventInput {
            audience: None,
            category: Some(if code == 0 { "cli" } else { "error" }.to_owned()),
            detail: Some(serde_json::json!({ "code": code })),
            direction: Some("exit".to_owned()),
            release_summary: if code == 0 {
                None
            } else {
                Some("A local MyOpenPanels command failed".to_owned())
            },
            run_id: std::env::var("MYOPENPANELS_TRACE_RUN_ID").ok(),
            source: Some("myopenpanels".to_owned()),
            summary: Some(format!("myopenpanels exited with {code}")),
            task_id: std::env::var("MYOPENPANELS_TRACE_RUN_ID").ok(),
        });
        return code;
    }
    run_cli_with_io(argv, stdout.lock(), stderr.lock())
}

fn trace_url_for_cli(argv: &[String]) -> Option<String> {
    let args::ParseOutcome::Invocation(parsed) = args::parse(argv) else {
        return None;
    };
    let command = parsed.positionals.first().map(String::as_str);
    if command.is_none()
        || command == Some("__serve-studio")
        || command == Some("help")
        || command == Some("version")
        || has_flag(&parsed, "help")
        || has_flag(&parsed, "version")
    {
        return None;
    }

    if let Ok(url) = std::env::var("MYOPENPANELS_TRACE_URL") {
        if !url.is_empty() {
            return Some(url);
        }
    }

    let paths = parsed_paths(&parsed).ok()?;
    let status = studio_status(&paths).ok()?;
    if status.server != StudioServerStatus::Running {
        return None;
    }
    let session = status.session?;
    Some(trace_url_for_studio_session(&session))
}

fn trace_url_for_studio_session(session: &StudioSession) -> String {
    let server_url = session
        .local_server_url
        .as_deref()
        .unwrap_or(&session.server_url)
        .trim_end_matches('/');
    format!("{server_url}/api/trace/events")
}

struct TracedWriter<W: Write> {
    inner: W,
    stream: &'static str,
}

impl<W: Write> TracedWriter<W> {
    fn new(inner: W, stream: &'static str) -> Self {
        Self { inner, stream }
    }
}

impl<W: Write> Write for TracedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        if written > 0 {
            let text = String::from_utf8_lossy(&buf[..written]).to_string();
            trace::emit_cli_event(TraceEventInput {
                audience: None,
                category: Some("cli".to_owned()),
                detail: Some(serde_json::json!({
                    "stream": self.stream,
                    "text": text,
                })),
                direction: Some(self.stream.to_owned()),
                release_summary: None,
                run_id: std::env::var("MYOPENPANELS_TRACE_RUN_ID").ok(),
                source: Some("myopenpanels".to_owned()),
                summary: Some(format!("cli {}: {}", self.stream, text)),
                task_id: std::env::var("MYOPENPANELS_TRACE_RUN_ID").ok(),
            });
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn run_cli_with_io(argv: &[String], mut stdout: impl Write, mut stderr: impl Write) -> i32 {
    let parsed = match args::parse(argv) {
        args::ParseOutcome::Invocation(parsed) => parsed,
        args::ParseOutcome::Display(text) => {
            let _ = write_text(&mut stdout, &text);
            return 0;
        }
        args::ParseOutcome::Error(message) => {
            let parsed = parse_error_args(argv);
            let error = CliError::with_recovery(
                "invalid_argument",
                message.trim().to_owned(),
                false,
                "Review the generated command help and retry with valid arguments.",
            );
            write_error(&parsed, &mut stdout, &mut stderr, &error);
            return error.exit_code();
        }
    };
    match run_parsed_cli(&parsed, &mut stdout, &mut stderr) {
        Ok(()) => 0,
        Err(error) => {
            write_error(&parsed, &mut stdout, &mut stderr, &error);
            error.exit_code()
        }
    }
}

fn run_parsed_cli(
    parsed: &Invocation,
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
        if let Some(delay_ms) = string_flag(parsed, "restart-delay-ms") {
            let delay_ms = delay_ms
                .parse::<u64>()
                .map_err(|_| CliError::new("Expected --restart-delay-ms to be a number."))?;
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
        let exit_code = run_server(host, port, paths, static_dir)?;
        std::process::exit(exit_code);
    }

    if command == Some("studio") {
        return run_studio_command(parsed, stdout);
    }

    if command == Some("agent") {
        return run_agent_command(parsed, stdout);
    }

    if command == Some("project") {
        return run_project_command(parsed, stdout);
    }

    if command == Some("panel") {
        return run_panel_command(parsed, stdout);
    }

    if command == Some("canvas") {
        return run_canvas_command(parsed, stdout);
    }

    if command == Some("wiki") {
        return run_wiki_command(parsed, stdout);
    }

    if command == Some("operation") {
        return run_operation_command(parsed, stdout);
    }

    if command == Some("tasks") {
        return run_tasks_command(parsed, stdout);
    }

    if should_check_for_updates(parsed, command) {
        maybe_notify_update(VERSION, stderr);
    }

    Err(CliError::new(format!(
        "Unknown command: {}",
        command.unwrap_or_default()
    )))
}

fn run_tasks_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        Some("list") => {
            let paths = parsed_current_paths(parsed)?;
            let result = with_task_next_actions(
                tasks::list_tasks(&paths, task_list_filter(parsed))?,
                true,
            );
            let count = result["tasks"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} task(s)"))
        }
        Some("next") => {
            let paths = parsed_current_paths(parsed)?;
            let result = with_task_next_actions(
                tasks::next_task(&paths, task_list_filter(parsed))?,
                true,
            );
            let text = result["task"]["id"].as_str().unwrap_or("No pending task");
            write_result(parsed, stdout, &result, text)
        }
        Some("inspect") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = with_task_next_actions(tasks::inspect_task(&paths, task_id)?, false);
            write_result(parsed, stdout, &result, task_id)
        }
        Some("claim-next") => {
            let paths = parsed_current_paths(parsed)?;
            let target_id = required_flag(parsed, "target-id")?;
            let wait_ms = string_flag(parsed, "wait-ms")
                .map(|value| {
                    value
                        .parse::<u64>()
                        .map_err(|_| CliError::new("Expected --wait-ms to be a number."))
                })
                .transpose()?;
            let result = tasks::claim_next(
                &paths,
                target_id,
                string_flag(parsed, "capability"),
                wait_ms,
            )?;
            let text = result["task"]["id"]
                .as_str()
                .unwrap_or("No matching task");
            write_result(parsed, stdout, &result, text)
        }
        Some("claim") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let target_id = required_flag(parsed, "target-id")?;
            let result = tasks::claim_task(&paths, task_id, target_id)?;
            write_result(parsed, stdout, &result, &format!("Claimed {task_id}"))
        }
        Some("heartbeat") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let lease_token = required_flag(parsed, "lease-token")?;
            let result = tasks::heartbeat_task(&paths, task_id, lease_token)?;
            write_result(parsed, stdout, &result, &format!("Heartbeat {task_id}"))
        }
        Some("complete") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let lease_token = required_flag(parsed, "lease-token")?;
            let result_value = string_flag(parsed, "result-file")
                .map(|path| {
                    let raw = fs::read_to_string(path)
                        .map_err(|error| CliError::new(error.to_string()))?;
                    serde_json::from_str::<Value>(&raw)
                        .map_err(|error| CliError::new(error.to_string()))
                })
                .transpose()?;
            let result = tasks::complete_task(&paths, task_id, lease_token, result_value)?;
            write_result(parsed, stdout, &result, &format!("Completed {task_id}"))
        }
        Some("fail") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let lease_token = required_flag(parsed, "lease-token")?;
            let message = required_flag(parsed, "message")?;
            let result = tasks::fail_task(
                &paths,
                task_id,
                lease_token,
                message,
                string_flag(parsed, "retry-after"),
            )?;
            write_result(parsed, stdout, &result, &format!("Failed {task_id}"))
        }
        Some("release") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let lease_token = required_flag(parsed, "lease-token")?;
            let result = tasks::release_task(&paths, task_id, lease_token)?;
            write_result(parsed, stdout, &result, &format!("Released {task_id}"))
        }
        Some("retry") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = tasks::retry_task(&paths, task_id)?;
            write_result(parsed, stdout, &result, &format!("Retried {task_id}"))
        }
        Some("cancel") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = tasks::cancel_task(&paths, task_id)?;
            write_result(parsed, stdout, &result, &format!("Cancelled {task_id}"))
        }
        Some("deliveries") => {
            let paths = parsed_current_paths(parsed)?;
            let result = tasks::list_deliveries(&paths, string_flag(parsed, "task-id"))?;
            let count = result["deliveries"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} delivery record(s)"))
        }
        _ => Err(CliError::new(
            "Expected tasks subcommand: list, next, inspect, claim-next, claim, heartbeat, complete, fail, release, retry, cancel, or deliveries.",
        )),
    }
}

fn task_list_filter(parsed: &Invocation) -> tasks::TaskListFilter<'_> {
    tasks::TaskListFilter {
        pending: has_flag(parsed, "pending"),
        queue: string_flag(parsed, "queue"),
        status: string_flag(parsed, "status"),
    }
}

fn run_project_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        Some("current") => {
            let paths = parsed_current_paths(parsed)?;
            let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new())?;
            let payload = serde_json::json!({
                "project": bootstrap.session,
                "activePanel": {
                    "id": bootstrap.active_panel_id,
                    "kind": bootstrap.active_panel_kind,
                    "title": bootstrap.panel.title,
                },
                "panels": bootstrap.panels.iter().map(|snapshot| &snapshot.panel).collect::<Vec<_>>(),
            });
            write_result(parsed, stdout, &payload, &bootstrap.session.title)
        }
        Some("list") => {
            let fallback = parsed_paths(parsed)?;
            let paths = match parsed_current_paths(parsed) {
                Ok(paths) => paths,
                Err(error) if error.code() == Some("no_current_project") => fallback,
                Err(error) => return Err(error),
            };
            let storage = Storage::open(&paths)?;
            let sessions = storage.list_sessions()?;
            let current_id = read_active_session_id(&paths)?;
            let projects = sessions
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
                "project": bootstrap.session,
                "activePanel": {
                    "id": bootstrap.active_panel_id,
                    "kind": bootstrap.active_panel_kind,
                    "title": bootstrap.panel.title,
                },
                "panels": bootstrap.panels.iter().map(|snapshot| &snapshot.panel).collect::<Vec<_>>(),
            });
            write_result(parsed, stdout, &payload, &bootstrap.session.title)
        }
        Some("select") => {
            let fallback = parsed_paths(parsed)?;
            let paths = match parsed_current_paths(parsed) {
                Ok(paths) => paths,
                Err(error) if error.code() == Some("no_current_project") => fallback,
                Err(error) => return Err(error),
            };
            let session_id = required_flag(parsed, "id")?;
            let bootstrap = ensure_project_bootstrap(
                &paths,
                BootstrapRequest {
                    requested_panel_id: None,
                    requested_panel_kind: None,
                    requested_session_id: Some(session_id.to_owned()),
                },
            )?;
            let payload = serde_json::json!({
                "project": bootstrap.session,
                "activePanel": {
                    "id": bootstrap.active_panel_id,
                    "kind": bootstrap.active_panel_kind,
                    "title": bootstrap.panel.title,
                },
                "focusRevision": read_focus_revision(&paths)?,
            });
            write_result(parsed, stdout, &payload, session_id)
        }
        _ => Err(CliError::new(
            "Expected project subcommand: current, list, create, or select.",
        )),
    }
}

fn run_panel_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        Some("current") => run_project_read_command(parsed, stdout, ProjectReadView::Current),
        Some("list") => run_project_read_command(parsed, stdout, ProjectReadView::List),
        Some("switch") => {
            let _ = required_flag(parsed, "kind")?;
            run_project_read_command(parsed, stdout, ProjectReadView::Activate)
        }
        Some("context") => {
            let paths = parsed_current_paths(parsed)?;
            let payload = crate::panel::read_context(&paths)?;
            write_result(parsed, stdout, &payload, "Current panel context")
        }
        Some("state") => {
            let paths = parsed_current_paths(parsed)?;
            let payload = crate::panel::read_state(&paths)?;
            write_result(parsed, stdout, &payload, "Current panel state")
        }
        Some("selection") => {
            let paths = parsed_current_paths(parsed)?;
            let payload = crate::panel::read_selection(&paths)?;
            write_result(parsed, stdout, &payload, "Current panel selection")
        }
        _ => Err(CliError::new(
            "Expected panel subcommand: current, list, or switch.",
        )),
    }
}

fn run_canvas_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    let action = parsed.positionals.get(2).map(String::as_str);
    match (subcommand, action) {
        (Some("generation"), Some("begin")) => {
            let paths = parsed_current_paths(parsed)?;
            require_focus(parsed, &paths, PanelKind::Canvas)?;
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
            let output = required_flag(parsed, "output")?;
            let result = read_selection_asset_for_panel(
                &paths,
                &bootstrap.session.id,
                &bootstrap.panel.id,
                output,
            )?;
            let text = format!("Wrote {}", result.output_path);
            write_result(parsed, stdout, &result, &text)
        }
        (Some("image"), Some("insert")) => {
            let paths = parsed_current_paths(parsed)?;
            require_focus(parsed, &paths, PanelKind::Canvas)?;
            let image_path = required_flag(parsed, "image")?;
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
            "Expected canvas subcommand: selection export, image insert, or generation begin.",
        )),
    }
}

fn require_focus(
    parsed: &Invocation,
    paths: &crate::paths::MyOpenPanelsPaths,
    kind: PanelKind,
) -> Result<ProjectBootstrap, CliError> {
    let expected = string_flag(parsed, "expect-focus-revision")
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| CliError::new("Expected --expect-focus-revision to be an integer."))
        })
        .transpose()?;
    require_active_panel(paths, kind, expected)
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
            let bootstrap = match bind_and_bootstrap_studio(&paths, &result.session) {
                Ok(bootstrap) => bootstrap,
                Err(error) => {
                    if !result.reused_existing {
                        let _ = stop_studio_session(&paths);
                    }
                    return Err(error);
                }
            };
            let payload = studio_launch_payload(&result, &bootstrap, None, true)?;
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
            let bootstrap = match bind_and_bootstrap_studio(&paths, &result.session) {
                Ok(bootstrap) => bootstrap,
                Err(error) => {
                    if !result.reused_existing {
                        let _ = stop_studio_session(&paths);
                    }
                    return Err(error);
                }
            };
            let payload = studio_system_browser_payload(&result, &bootstrap, open_browser)?;
            let system_url = payload["systemBrowserUrl"].as_str().unwrap_or("");
            write_result(parsed, stdout, &payload, &format!("Opened {system_url}"))
        }
        Some("serve") => {
            let paths = parsed_paths(parsed)?;
            sync_builtin_agent_skills(&paths)?;
            if let Some(session) = reuse_existing_studio(&paths)? {
                let bootstrap = bind_and_bootstrap_studio(&paths, &session)?;
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
                let payload =
                    studio_launch_payload(&result, &bootstrap, Some(("foreground", false)), true)?;
                let text = payload["embeddedBrowserUrl"].as_str().unwrap_or("");
                return write_result(parsed, stdout, &payload, text);
            }
            let host = studio_host(parsed);
            let port = studio_port(parsed)?.unwrap_or(find_open_port(&host)?);
            let local_server_url = format!("http://127.0.0.1:{port}");
            let session = StudioSession {
                system_browser_url: Some(local_server_url.clone()),
                context_dir: paths.context_dir.display().to_string(),
                context_id: paths.context_id.clone(),
                context_id_source: paths.context_id_source.clone(),
                host: Some(host.clone()),
                lan_server_urls: Some(Vec::new()),
                local_server_url: Some(local_server_url.clone()),
                log_path: paths.context_dir.join("studio.log").display().to_string(),
                pid: std::process::id(),
                port,
                project_dir: paths.project_dir.display().to_string(),
                server_url: local_server_url.clone(),
                started_at: crate::control::now_iso(),
                storage_dir: paths.storage_dir.display().to_string(),
            };
            let bootstrap = ensure_project_bootstrap(&paths, BootstrapRequest::new())?;
            write_studio_session(&paths, &session)?;
            if let Err(error) = record_current_studio(&paths, &session) {
                let _ = discard_studio_session_binding(&paths);
                return Err(studio_binding_error(error));
            }
            let result = StudioStartResult {
                session,
                reused_existing: false,
                server_version: VERSION.to_owned(),
                lifecycle: crate::studio::StudioLifecycle::Started,
                previous_version: None,
                browser_refresh_required: false,
            };
            let payload =
                studio_launch_payload(&result, &bootstrap, Some(("foreground", true)), true)?;
            let text = payload["embeddedBrowserUrl"].as_str().unwrap_or("");
            write_result(parsed, stdout, &payload, text)?;
            stdout
                .flush()
                .map_err(|error| CliError::new(error.to_string()))?;
            let static_dir = string_flag(parsed, "static-dir").map(std::path::PathBuf::from);
            let exit_code = run_server(&host, port, paths, static_dir)?;
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
                "unbound": result.unbound,
            });
            let text = if result.stopped {
                "Stopped MyOpenPanels"
            } else if result.unbound {
                "Unbound MyOpenPanels studio"
            } else {
                "No MyOpenPanels studio binding"
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
            let payload = with_operation_next_actions(
                operations::list(&paths, string_flag(parsed, "status"))?,
                true,
            );
            write_result(parsed, stdout, &payload, "Operations")
        }
        Some("read") => {
            let id = required_flag(parsed, "operation-id")?;
            let payload = with_operation_next_actions(operations::inspect(&paths, id)?, false);
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

fn run_agent_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        Some("bootstrap") => {
            let paths = parsed_current_paths(parsed)?;
            let payload = agent_bootstrap(&paths, VERSION)?;
            write_result(parsed, stdout, &payload, "MyOpenPanels agent protocol v4 bootstrap")
        }
        Some("entry-skill") => match parsed.positionals.get(2).map(String::as_str) {
            Some("acknowledge") => {
                let paths = parsed_current_paths(parsed)?;
                let payload = crate::agent_control::acknowledge_entry_skill_update(
                    &paths,
                    required_flag(parsed, "event-id")?,
                    required_flag(parsed, "installed-version")?,
                )?;
                write_result(parsed, stdout, &payload, "Entry Skill update acknowledged")
            }
            _ => Err(CliError::new(
                "Expected agent entry-skill subcommand: acknowledge.",
            )),
        },
        Some("operations") => {
            let paths = parsed_current_paths(parsed)?;
            match parsed.positionals.get(2).map(String::as_str) {
                Some("list") => {
                    let payload = operations::list(&paths, string_flag(parsed, "status"))?;
                    write_result(parsed, stdout, &payload, "Agent operations")
                }
                Some("inspect") => {
                    let payload = operations::inspect(&paths, required_flag(parsed, "operation-id")?)?;
                    write_result(parsed, stdout, &payload, payload["status"].as_str().unwrap_or("unknown"))
                }
                _ => Err(CliError::new("Expected agent operations subcommand: list or inspect.")),
            }
        }
        Some("capabilities") => {
            let payload = match string_flag(parsed, "scope") {
                Some(scope) => registry::scope_capabilities(scope).ok_or_else(|| {
                    CliError::with_recovery_command(
                        "capability_scope_not_found",
                        format!("MyOpenPanels capability scope not found: {scope}"),
                        false,
                        "Run `myopenpanels agent capability list --format json` to list valid scopes.",
                        "myopenpanels agent capability list --format json",
                    )
                })?,
                None => registry::scope_index(),
            };
            let text = render_agent_discovery_summary(&payload);
            write_result(parsed, stdout, &payload, &text)
        }
        Some("capability") => {
            let intent = required_flag(parsed, "intent")?;
            let capability = registry::capability_payload(intent).ok_or_else(|| {
                CliError::with_code(
                    "capability_not_found",
                    format!("MyOpenPanels capability not found: {intent}"),
                )
            })?;
            write_result(parsed, stdout, &capability, intent)
        }
        Some("bridge") => {
            let paths = parsed_current_paths(parsed)?;
            if parsed.positionals.get(2).map(String::as_str) == Some("status") {
                let result = read_bridge_status(&paths)?;
                let text = result["status"].as_str().unwrap_or("idle");
                return write_result(parsed, stdout, &result, text);
            }
            let interval_ms = string_flag(parsed, "interval-ms")
                .map(|value| {
                    value
                        .parse::<u64>()
                        .map_err(|_| CliError::new("Expected --interval-ms to be a number."))
                })
                .transpose()?
                .unwrap_or(2000);
            let timeout_ms = string_flag(parsed, "timeout-ms")
                .map(|value| {
                    value
                        .parse::<u64>()
                        .map_err(|_| CliError::new("Expected --timeout-ms to be a number."))
                })
                .transpose()?
                .unwrap_or(600_000);
            let result = run_bridge(
                &paths,
                BridgeOptions {
                    agent_prompt: false,
                    capabilities: string_list_flag(parsed, "capability"),
                    command: string_flag(parsed, "command"),
                    host: None,
                    interval_ms,
                    manual_lifecycle: has_flag(parsed, "manual-lifecycle"),
                    name: string_flag(parsed, "name"),
                    once: has_flag(parsed, "once"),
                    priority: 0,
                    queue: string_flag(parsed, "queue"),
                    timeout_ms,
                },
            )?;
            let text = if result["ran"].as_bool().unwrap_or(false) {
                "Bridge command ran"
            } else {
                "No pending task"
            };
            write_result(parsed, stdout, &result, text)
        }
        Some("targets") => {
            let paths = parsed_current_paths(parsed)?;
            match parsed.positionals.get(2).map(String::as_str) {
                Some("list") => {
                    let result = tasks::list_targets(&paths)?;
                    let count = result["targets"].as_array().map(Vec::len).unwrap_or(0);
                    write_result(parsed, stdout, &result, &format!("{count} target(s)"))
                }
                Some("register") => {
                    let priority = string_flag(parsed, "priority")
                        .map(|value| {
                            value.parse::<i64>().map_err(|_| {
                                CliError::new("Expected --priority to be an integer.")
                            })
                        })
                        .transpose()?
                        .unwrap_or(0);
                    let result = tasks::register_target(
                        &paths,
                        tasks::TargetRegistration {
                            name: required_flag(parsed, "name")?,
                            host: string_flag(parsed, "host"),
                            transport: required_flag(parsed, "transport")?,
                            endpoint: string_flag(parsed, "endpoint"),
                            capabilities: string_list_flag(parsed, "capability"),
                            priority,
                        },
                    )?;
                    write_result(
                        parsed,
                        stdout,
                        &result,
                        result["target"]["id"].as_str().unwrap_or("Registered target"),
                    )
                }
                Some("heartbeat") => {
                    let target_id = required_flag(parsed, "target-id")?;
                    let result = tasks::heartbeat_target(&paths, target_id)?;
                    write_result(parsed, stdout, &result, &format!("Heartbeat {target_id}"))
                }
                Some("remove") => {
                    let target_id = required_flag(parsed, "target-id")?;
                    let result = tasks::remove_target(&paths, target_id)?;
                    write_result(parsed, stdout, &result, &format!("Removed {target_id}"))
                }
                _ => Err(CliError::new(
                    "Expected agent targets subcommand: list, register, heartbeat, or remove.",
                )),
            }
        }
        Some("guides") => {
            let guides = list_agent_guide_summaries(
                string_flag(parsed, "panel-kind"),
                string_flag(parsed, "task-type"),
            )?;
            let next_actions = discovery_read_actions(
                &guides,
                "agent.guide.read",
                "--guide-id",
                "Read this Guide when its loadWhen condition applies.",
            );
            let payload = serde_json::json!({
                "guides": guides,
                "nextActions": next_actions,
                "nextRequiredAction": {
                    "intent": "select-guide",
                    "instruction": "Choose the nextActions entry whose loadWhen matches the user request.",
                },
            });
            let text = render_agent_discovery_summary(&payload);
            write_result(parsed, stdout, &payload, &text)
        }
        Some("guide") => {
            let guide_id = parsed
                .positionals
                .get(2)
                .ok_or_else(|| CliError::new("Missing guide id."))?;
            let paths = parsed_current_paths(parsed)?;
            let payload = read_agent_guide(&paths, guide_id, string_flag(parsed, "task-id"))?;
            let markdown = payload.markdown.clone();
            write_result(parsed, stdout, &payload, &markdown)
        }
        Some("skills") => {
            let skills = list_agent_skill_summaries(
                string_flag(parsed, "panel-kind"),
                string_flag(parsed, "task-type"),
            )?;
            let next_actions = discovery_read_actions(
                &skills,
                "agent.skill.read",
                "--skill-id",
                "Read this Skill when its loadWhen condition applies.",
            );
            let payload = serde_json::json!({
                "skills": skills,
                "nextActions": next_actions,
                "nextRequiredAction": {
                    "intent": "select-skill",
                    "instruction": "Choose the nextActions entry whose loadWhen matches the user request.",
                },
            });
            let text = render_agent_discovery_summary(&payload);
            write_result(parsed, stdout, &payload, &text)
        }
        Some("skill") => {
            let skill_id = parsed
                .positionals
                .get(2)
                .ok_or_else(|| CliError::new("Missing skill id."))?;
            let paths = parsed_current_paths(parsed)?;
            let payload = read_agent_skill(&paths, skill_id, string_flag(parsed, "task-id"))?;
            let markdown = payload.markdown.clone();
            write_result(parsed, stdout, &payload, &markdown)
        }
        _ => Err(CliError::new(
            "Expected agent subcommand: bootstrap, capabilities, operations, bridge, guides, guide, skills, or skill.",
        )),
    }
}

fn discovery_read_actions(
    items: &[Value],
    intent: &str,
    id_flag: &str,
    load_when: &str,
) -> Vec<Value> {
    items
        .iter()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .map(|id| {
            let mut action = registry::command_action(
                intent,
                vec![
                    id_flag.to_owned(),
                    id.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            )
            .unwrap_or_else(|| panic!("missing Command Registry action for {intent}"));
            action["loadWhen"] = serde_json::json!(load_when);
            action
        })
        .collect()
}

fn with_task_next_actions(mut payload: Value, list_mode: bool) -> Value {
    let mut actions = Vec::new();
    if list_mode {
        let tasks = payload
            .get("tasks")
            .and_then(Value::as_array)
            .cloned()
            .or_else(|| {
                payload
                    .get("task")
                    .filter(|task| !task.is_null())
                    .map(|task| vec![task.clone()])
            })
            .unwrap_or_default();
        for task in tasks {
            let Some(task_id) = task.get("id").and_then(Value::as_str) else {
                continue;
            };
            let mut action = registry::command_action(
                "task.read",
                vec![
                    "--task-id".to_owned(),
                    task_id.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            )
            .expect("registered Task read action");
            action["loadWhen"] =
                serde_json::json!("The user request should inspect or continue this Task.");
            actions.push(action);
        }
    } else if let Some(task) = payload.get("task") {
        if let Some(capability) = task
            .get("capability")
            .and_then(Value::as_str)
            .filter(|intent| registry::capability(intent).is_some())
        {
            actions.push(capability_read_action(
                capability,
                "The Task requires this capability.",
            ));
        }
        let status = task.get("status").and_then(Value::as_str);
        let lifecycle = match status {
            Some("queued") => &["task.claim", "task.cancel"][..],
            Some("failed") => &["task.claim", "task.retry", "task.cancel"][..],
            Some("reserved" | "running" | "claimed" | "converting" | "indexing") => &[
                "task.heartbeat",
                "task.complete",
                "task.fail",
                "task.release",
            ][..],
            _ => &[][..],
        };
        actions.extend(lifecycle.iter().map(|intent| {
            capability_read_action(intent, "The Task lifecycle requires this action.")
        }));
    }
    payload["nextActions"] = serde_json::json!(actions);
    payload["nextRequiredAction"] = serde_json::json!({
        "intent": "continue-task",
        "instruction": "Choose an applicable nextActions entry, execute its argv with the same resolved CLI executable, and follow the returned response.",
    });
    payload
}

fn with_operation_next_actions(mut payload: Value, list_mode: bool) -> Value {
    let mut actions = Vec::new();
    if list_mode {
        for operation in payload
            .get("operations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(operation_id) = operation.get("id").and_then(Value::as_str) else {
                continue;
            };
            let mut action = registry::command_action(
                "operation.read",
                vec![
                    "--operation-id".to_owned(),
                    operation_id.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            )
            .expect("registered Operation read action");
            action["loadWhen"] =
                serde_json::json!("The user request should inspect or continue this Operation.");
            actions.push(action);
        }
    } else {
        if let Some(skill_id) = payload.get("skillId").and_then(Value::as_str) {
            let mut action = registry::command_action(
                "agent.skill.read",
                vec![
                    "--skill-id".to_owned(),
                    skill_id.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            )
            .expect("registered Skill read action");
            action["loadWhen"] = serde_json::json!("The Operation requires this Skill.");
            actions.push(action);
        }
        if let Some(guide_id) = payload.get("guideId").and_then(Value::as_str) {
            let mut action = registry::command_action(
                "agent.guide.read",
                vec![
                    "--guide-id".to_owned(),
                    guide_id.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            )
            .expect("registered Guide read action");
            action["loadWhen"] = serde_json::json!("The Operation requires this Guide.");
            actions.push(action);
        }
        if matches!(
            payload.get("status").and_then(Value::as_str),
            Some("active" | "failed")
        ) {
            actions.extend(
                ["operation.complete", "operation.fail", "operation.cancel"].map(|intent| {
                    capability_read_action(intent, "The Operation requires this lifecycle action.")
                }),
            );
        }
    }
    payload["nextActions"] = serde_json::json!(actions);
    payload["nextRequiredAction"] = serde_json::json!({
        "intent": "continue-operation",
        "instruction": "Choose an applicable nextActions entry, execute its argv with the same resolved CLI executable, and follow the returned response.",
    });
    payload
}

fn capability_read_action(intent: &str, load_when: &str) -> Value {
    let mut action = registry::command_action(
        "agent.capability.read",
        vec![
            "--intent".to_owned(),
            intent.to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ],
    )
    .expect("registered capability read action");
    action["loadWhen"] = serde_json::json!(load_when);
    action
}

fn run_wiki_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    let action = parsed.positionals.get(2).map(String::as_str);
    if string_flag(parsed, "task-id").is_none()
        && matches!(
            parsed.intent.as_str(),
            "wiki.raw-document.add"
                | "wiki.raw-document.create-markdown"
                | "wiki.raw-document.markdown.write"
                | "wiki.generated-document.create"
                | "wiki.generated-document.write"
                | "wiki.generated-document.rename"
                | "wiki.generated-document.delete"
                | "wiki.generated-document.publish"
                | "wiki.space.activate"
                | "wiki.page.write"
                | "wiki.generation.begin"
        )
    {
        let paths = parsed_current_paths(parsed)?;
        require_focus(parsed, &paths, PanelKind::Wiki)?;
    }
    match (subcommand, action) {
        (Some("generation"), Some("begin")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = operations::begin_wiki(
                &paths,
                required_flag(parsed, "title")?,
                string_flag(parsed, "document-format").unwrap_or("markdown"),
                string_flag(parsed, "document-id"),
            )?;
            write_result(
                parsed,
                stdout,
                &result,
                result["operation"]["id"]
                    .as_str()
                    .unwrap_or("Started wiki generation"),
            )
        }
        (Some("documents"), Some("create-markdown")) => {
            let paths = parsed_current_paths(parsed)?;
            let title = required_flag(parsed, "title")?;
            let file_name = string_flag(parsed, "file-name").unwrap_or(title);
            let content = if let Some(file) = string_flag(parsed, "file") {
                std::fs::read(file).map_err(|error| CliError::new(error.to_string()))?
            } else {
                string_flag(parsed, "content")
                    .unwrap_or("")
                    .as_bytes()
                    .to_vec()
            };
            let result = wiki::add_raw_document(
                &paths,
                file_name,
                Some(title),
                Some("text/markdown"),
                "agent",
                string_flag(parsed, "wiki-space-id"),
                &content,
            )?;
            let text = format!(
                "Created markdown {}",
                result["document"]["id"].as_str().unwrap_or("")
            );
            write_result(parsed, stdout, &result, &text)
        }
        (Some("documents"), Some("add")) => {
            let paths = parsed_current_paths(parsed)?;
            let file = required_flag(parsed, "file")?;
            let content = std::fs::read(file).map_err(|error| CliError::new(error.to_string()))?;
            let file_name = string_flag(parsed, "file-name")
                .or_else(|| {
                    std::path::Path::new(file)
                        .file_name()
                        .and_then(|value| value.to_str())
                })
                .unwrap_or("document");
            let result = wiki::add_raw_document(
                &paths,
                file_name,
                string_flag(parsed, "title"),
                string_flag(parsed, "mime-type"),
                "agent",
                string_flag(parsed, "wiki-space-id"),
                &content,
            )?;
            let text = format!(
                "Added raw document {}",
                result["document"]["id"].as_str().unwrap_or("")
            );
            write_result(parsed, stdout, &result, &text)
        }
        (Some("documents"), Some("list")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = wiki::wiki_context(&paths)?;
            let documents = result["state"]["rawDocuments"].clone();
            let payload = serde_json::json!({ "documents": documents });
            let count = payload["documents"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &payload, &format!("{count} document(s)"))
        }
        (Some("generated-documents"), Some("list")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = wiki::list_generated_documents(&paths)?;
            let count = result["documents"].as_array().map(Vec::len).unwrap_or(0);
            write_result(
                parsed,
                stdout,
                &result,
                &format!("{count} generated document(s)"),
            )
        }
        (Some("generated-documents"), Some("create")) => {
            let paths = parsed_current_paths(parsed)?;
            let file = required_flag(parsed, "file")?;
            let content = fs::read(file).map_err(|error| CliError::new(error.to_string()))?;
            let file_name = std::path::Path::new(file)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("document.md");
            let result = wiki::create_generated_document(
                &paths,
                file_name,
                string_flag(parsed, "title"),
                string_flag(parsed, "mime-type"),
                string_flag(parsed, "task-id"),
                string_flag(parsed, "thread-id"),
                &content,
            )?;
            let id = result["document"]["id"].as_str().unwrap_or_default();
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Created generated document {id}"),
            )
        }
        (Some("generated-documents"), Some("read")) => {
            let paths = parsed_current_paths(parsed)?;
            let document_id = required_flag(parsed, "document-id")?;
            let result = wiki::read_generated_document(&paths, document_id)?;
            let text = result["content"].as_str().unwrap_or("");
            write_result(parsed, stdout, &result, text)
        }
        (Some("generated-documents"), Some("write")) => {
            let paths = parsed_current_paths(parsed)?;
            let document_id = required_flag(parsed, "document-id")?;
            let file = required_flag(parsed, "file")?;
            let content = fs::read(file).map_err(|error| CliError::new(error.to_string()))?;
            let file_name = std::path::Path::new(file)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("document.md");
            let result = wiki::write_generated_document(
                &paths,
                document_id,
                file_name,
                string_flag(parsed, "mime-type"),
                &content,
            )?;
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Updated generated document {document_id}"),
            )
        }
        (Some("generated-documents"), Some("rename")) => {
            let paths = parsed_current_paths(parsed)?;
            let document_id = required_flag(parsed, "document-id")?;
            let result = wiki::rename_generated_document(
                &paths,
                document_id,
                required_flag(parsed, "title")?,
            )?;
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Renamed generated document {document_id}"),
            )
        }
        (Some("generated-documents"), Some("delete")) => {
            let paths = parsed_current_paths(parsed)?;
            let document_id = required_flag(parsed, "document-id")?;
            let result = wiki::delete_generated_document(&paths, document_id)?;
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Deleted generated document {document_id}"),
            )
        }
        (Some("generated-documents"), Some("publish")) => {
            let paths = parsed_current_paths(parsed)?;
            let document_id = required_flag(parsed, "document-id")?;
            let result = wiki::publish_generated_document(
                &paths,
                document_id,
                string_flag(parsed, "wiki-space-id"),
            )?;
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Published generated document {document_id}"),
            )
        }
        (Some("markdown"), Some("read")) => {
            let paths = parsed_current_paths(parsed)?;
            let document_id = required_flag(parsed, "document-id")?;
            let result = wiki::read_markdown(&paths, document_id)?;
            let text = result["markdown"].as_str().unwrap_or("");
            write_result(parsed, stdout, &result, text)
        }
        (Some("markdown"), Some("write")) => {
            let paths = parsed_current_paths(parsed)?;
            let document_id = required_flag(parsed, "document-id")?;
            let file = required_flag(parsed, "file")?;
            let content =
                std::fs::read_to_string(file).map_err(|error| CliError::new(error.to_string()))?;
            let result = wiki::write_markdown(
                &paths,
                document_id,
                &content,
                string_flag(parsed, "task-id"),
            )?;
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Wrote markdown {document_id}"),
            )
        }
        (Some("spaces"), Some("switch")) => {
            let paths = parsed_current_paths(parsed)?;
            let wiki_space_id = required_flag(parsed, "wiki-space-id")?;
            let result = wiki::set_active_space(&paths, wiki_space_id)?;
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Active wiki space {wiki_space_id}"),
            )
        }
        (Some("spaces"), Some("list")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = wiki::list_spaces(&paths)?;
            let count = result["spaces"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} wiki space(s)"))
        }
        (Some("pages"), Some("read")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = wiki::read_page(
                &paths,
                required_flag(parsed, "wiki-space-id")?,
                required_flag(parsed, "path")?,
            )?;
            let text = result["markdown"].as_str().unwrap_or("");
            write_result(parsed, stdout, &result, text)
        }
        (Some("pages"), Some("search")) => {
            let paths = parsed_current_paths(parsed)?;
            let limit = number_flag(parsed, "limit")?.unwrap_or(20.0) as usize;
            let result = wiki::search_pages(
                &paths,
                required_flag(parsed, "wiki-space-id")?,
                required_flag(parsed, "query")?,
                limit,
            )?;
            let count = result["matches"].as_array().map(Vec::len).unwrap_or(0);
            write_result(
                parsed,
                stdout,
                &result,
                &format!("{count} matching page(s)"),
            )
        }
        (Some("pages"), Some("write")) => {
            let paths = parsed_current_paths(parsed)?;
            let wiki_space_id = required_flag(parsed, "wiki-space-id")?;
            let page_path = required_flag(parsed, "path")?;
            let file = required_flag(parsed, "file")?;
            let content =
                std::fs::read_to_string(file).map_err(|error| CliError::new(error.to_string()))?;
            let result = wiki::write_page(
                &paths,
                wiki_space_id,
                page_path,
                &content,
                string_flag(parsed, "title"),
                string_flag(parsed, "task-id"),
            )?;
            write_result(parsed, stdout, &result, &format!("Wrote page {page_path}"))
        }
        (Some("pages"), Some("list")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = wiki::list_pages(&paths, required_flag(parsed, "wiki-space-id")?)?;
            let count = result["pages"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} page(s)"))
        }
        _ => Err(CliError::new("Unknown wiki command.")),
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ProjectReadView {
    Current,
    List,
    Activate,
}

fn run_project_read_command(
    parsed: &Invocation,
    stdout: &mut impl Write,
    view: ProjectReadView,
) -> Result<(), CliError> {
    let paths = parsed_current_paths(parsed)?;
    let mut request = BootstrapRequest::new();
    request.requested_panel_kind = match view {
        ProjectReadView::Activate => string_flag(parsed, "kind")
            .map(parse_panel_kind)
            .transpose()?,
        ProjectReadView::Current | ProjectReadView::List => None,
    };
    let bootstrap = read_project_bootstrap(&paths, request)?;

    if view == ProjectReadView::Activate {
        let payload = serde_json::json!({
            "project": bootstrap.session,
            "panel": bootstrap.panel,
            "focus": {
                "focusRevision": read_focus_revision(&paths)?,
                "projectId": bootstrap.session.id,
                "panelId": bootstrap.panel.id,
                "panelKind": bootstrap.panel.kind,
            }
        });
        return write_result(parsed, stdout, &payload, "Panel activated.");
    }

    match view {
        ProjectReadView::List => {
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
        ProjectReadView::Current => {
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
        ProjectReadView::Activate => unreachable!(),
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
            "no_current_project",
            "No running MyOpenPanels Studio is bound to this project directory.",
            true,
            "Run `myopenpanels studio start --project-dir <dir> --format json`, then retry.",
        ));
    };
    resolve_myopenpanels_paths(
        Some(&session.project_dir),
        Some(&session.storage_dir),
        Some(&session.context_id),
    )
}

#[cfg(test)]
fn resolve_panel_paths_for_active_studio(
    paths: crate::paths::MyOpenPanelsPaths,
    has_explicit_context_id: bool,
) -> Result<crate::paths::MyOpenPanelsPaths, CliError> {
    if has_explicit_context_id {
        return Ok(paths);
    }
    let Ok(status) = studio_status(&paths) else {
        return Ok(paths);
    };
    if status.server != StudioServerStatus::Running {
        return Ok(paths);
    }
    let Some(session) = status.session else {
        return Ok(paths);
    };
    if PathBuf::from(&session.project_dir) != paths.project_dir
        || PathBuf::from(&session.storage_dir) != paths.storage_dir
        || session.context_id == paths.context_id
    {
        return Ok(paths);
    }
    resolve_myopenpanels_paths(
        Some(&session.project_dir),
        Some(&session.storage_dir),
        Some(&session.context_id),
    )
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

fn bind_and_bootstrap_studio(
    paths: &crate::paths::MyOpenPanelsPaths,
    session: &StudioSession,
) -> Result<ProjectBootstrap, CliError> {
    record_current_studio(paths, session).map_err(studio_binding_error)?;
    ensure_project_bootstrap(paths, BootstrapRequest::new())
}

fn studio_binding_error(error: CliError) -> CliError {
    CliError::with_recovery(
        "studio_binding_failed",
        format!(
            "Failed to bind the MyOpenPanels Studio: {}",
            error.message()
        ),
        true,
        "Fix access to the MyOpenPanels storage directory, then retry `studio start`.",
    )
}

fn studio_launch_payload(
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
            "id": result.session.context_id,
            "source": result.session.context_id_source,
            "projectDir": result.session.project_dir,
            "storageDir": result.session.storage_dir,
        },
        "project": {
            "id": bootstrap.session.id,
            "title": bootstrap.session.title,
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
            "studio.open-system-browser",
            vec![
                "--local-only".to_owned(),
                "--project-dir".to_owned(),
                result.session.project_dir.clone(),
                "--format".to_owned(),
                "json".to_owned(),
            ],
        )
        .expect("registered Studio browser fallback action");
        payload["nextRequiredAction"] = serde_json::json!({
            "intent": "studio.open",
            "required": true,
            "url": system_browser_url,
            "preferredTarget": "in_app_browser",
            "fallback": {
                "intent": "studio.open-system-browser",
                "executor": fallback_action["executor"],
                "command": "myopenpanels studio open-system-browser",
                "argv": fallback_action["argv"],
                "args": [
                    "--local-only",
                    "--project-dir",
                    result.session.project_dir,
                    "--format",
                    "json",
                ],
            },
            "completionCriterion": "The Studio URL was accepted by an in-app or system browser opener.",
        });
    }
    Ok(payload)
}

fn studio_system_browser_payload(
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
    let mut payload = studio_launch_payload(result, bootstrap, None, false)?;
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
            let envelope = SuccessPayload {
                ok: true,
                schema_version: 1,
                intent: &parsed.intent,
                data: payload,
                meta: ResponseMeta {
                    cli_version: VERSION,
                },
            };
            let json = serde_json::to_string(&envelope)
                .map_err(|error| CliError::new(error.to_string()))?;
            let envelope_bytes = json.len() + 1;
            if parsed.intent == "agent.bootstrap.read"
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
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    error: &CliError,
) {
    match output_format(parsed) {
        OutputFormat::Json => {
            let payload = ErrorPayload {
                ok: false,
                schema_version: 1,
                intent: &parsed.intent,
                error: ErrorDetail {
                    code: error.code().unwrap_or("command_failed"),
                    message: error.message(),
                    retryable: error.retryable(),
                    recovery: RecoveryDetail {
                        instruction: error
                            .recovery()
                            .unwrap_or("Review the command help and retry with valid arguments."),
                        command: error.recovery_command(),
                    },
                },
                meta: ResponseMeta {
                    cli_version: VERSION,
                },
            };
            let _ = serde_json::to_writer(&mut *stdout, &payload);
            let _ = stdout.write_all(b"\n");
        }
        OutputFormat::Text => {
            let _ = write_text(stderr, &format!("Error: {}\n", error.message()));
        }
    }
}

fn parse_error_args(argv: &[String]) -> Invocation {
    let json = argv.windows(2).any(|parts| parts == ["--format", "json"])
        || argv.iter().any(|arg| arg == "--format=json");
    let mut flags = BTreeMap::new();
    if json {
        flags.insert("format".to_owned(), FlagValue::String("json".to_owned()));
    }
    Invocation {
        flags,
        intent: "cli.parse".to_owned(),
        positionals: Vec::new(),
    }
}

fn write_text(stream: &mut impl Write, text: &str) -> Result<(), CliError> {
    stream
        .write_all(text.as_bytes())
        .map_err(|error| CliError::new(error.to_string()))
}
