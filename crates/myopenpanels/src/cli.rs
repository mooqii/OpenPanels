use crate::agent::{
    agent_bootstrap, list_agent_skill_summaries, read_agent_skill, sync_builtin_agent_skills,
};
use crate::bridge::{read_bridge_status, run_bridge, BridgeOptions};
use crate::canvas::{insert_image, InsertImageInput};
use crate::control::{
    activate_project_panel, create_project, ensure_project_bootstrap, read_active_project_id,
    read_focus_revision, read_project_bootstrap, require_active_panel, BootstrapRequest,
};
use crate::error::{CliError, CliErrorCategory, CliRecoveryAction};
use crate::operations;
use crate::paths::{resolve_myopenpanels_paths, resolve_studio_service_paths};
use crate::selection::read_selection_asset_for_panel;
use crate::server::run_server;
use crate::storage::Storage;
use crate::studio::{
    acquire_studio_transition_lock, find_open_port, open_browser, resolve_current_studio_session,
    reuse_existing_studio, start_studio, stop_studio_session, studio_status,
    wait_for_existing_studio, write_studio_session, StudioServerStatus, StudioSession,
    StudioStartOptions, StudioStartResult,
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
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const CLI_ENVELOPE_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, Eq, PartialEq)]
struct Invocation {
    command_id: registry::CommandId,
    flags: BTreeMap<String, FlagValue>,
    positionals: Vec<String>,
}

impl Invocation {
    fn intent(&self) -> &'static str {
        self.command_id.intent()
    }
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
    actions: &'a ResponseActions,
    meta: ResponseMeta<'a>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorPayload<'a> {
    ok: bool,
    schema_version: u32,
    intent: &'a str,
    error: ErrorDetail<'a>,
    actions: ResponseActions,
    meta: ResponseMeta<'a>,
}

#[derive(Debug, Serialize)]
struct ErrorDetail<'a> {
    #[serde(rename = "type")]
    category: CliErrorCategory,
    subtype: &'a str,
    message: &'a str,
    retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    param: Option<&'a str>,
    hint: &'a str,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResponseActions {
    required: Vec<Value>,
    suggested: Vec<Value>,
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
            let mut error = CliError::with_recovery(
                "invalid_argument",
                concise_parse_error_message(&message),
                false,
                "Review the generated command help and retry with valid arguments.",
            )
            .with_recovery_action(CliRecoveryAction::cli(["--help"]));
            if let Some(param) = parse_error_param(&message) {
                error = error.with_param(param);
            }
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
    use registry::CommandGroup;

    match parsed.command_id.group() {
        CommandGroup::Version => write_result(
            parsed,
            stdout,
            &VersionPayload { version: VERSION },
            VERSION,
        ),
        CommandGroup::Update => run_update_command(parsed, stdout),
        CommandGroup::Studio => run_studio_command(parsed, stdout),
        CommandGroup::Agent => run_agent_command(parsed, stdout),
        CommandGroup::Project => run_project_command(parsed, stdout),
        CommandGroup::Panel => run_panel_command(parsed, stdout),
        CommandGroup::Canvas => run_canvas_command(parsed, stdout),
        CommandGroup::Wiki => run_wiki_command(parsed, stdout),
        CommandGroup::Writing => run_writing_command(parsed, stdout),
        CommandGroup::Operation => run_operation_command(parsed, stdout),
        CommandGroup::Task => run_tasks_command(parsed, stdout),
        CommandGroup::Workflow => run_workflows_command(parsed, stdout),
        CommandGroup::InternalStudioServe => {
            let paths = parsed_paths(parsed)?;
            let service_paths = resolve_studio_service_paths(&paths)?;
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
            let exit_code = run_server(host, port, service_paths, static_dir)?;
            std::process::exit(exit_code);
        }
        CommandGroup::ParseError => {
            let command = parsed.positionals.first().map(String::as_str);
            if should_check_for_updates(parsed, command) {
                maybe_notify_update(VERSION, stderr);
            }
            Err(CliError::new(format!(
                "Unknown command: {}",
                command.unwrap_or_default()
            )))
        }
    }
}

fn run_tasks_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        Some("list") => {
            let paths = parsed_current_paths(parsed)?;
            let result = with_task_actions(
                tasks::list_tasks(&paths, task_list_filter(parsed))?,
                true,
            );
            let count = result["tasks"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} task(s)"))
        }
        Some("next") => {
            let paths = parsed_current_paths(parsed)?;
            let result = with_task_actions(
                tasks::next_task(&paths, task_list_filter(parsed))?,
                true,
            );
            let text = result["task"]["id"].as_str().unwrap_or("No pending task");
            write_result(parsed, stdout, &result, text)
        }
        Some("read") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = with_task_actions(tasks::inspect_task(&paths, task_id)?, false);
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
            let failure_class = string_flag(parsed, "failure-class")
                .map(|value| {
                    tasks::TaskFailureClass::parse(value).ok_or_else(|| {
                        CliError::with_code(
                            "invalid_argument",
                            "--failure-class must be retryable_channel, retryable_output, or terminal_task.",
                        )
                        .with_param("--failure-class")
                    })
                })
                .transpose()?
                .unwrap_or(tasks::TaskFailureClass::RetryableChannel);
            let result = tasks::fail_task_with_class(
                &paths,
                task_id,
                lease_token,
                message,
                string_flag(parsed, "retry-after"),
                failure_class,
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
        Some("archive") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = tasks::archive_task(&paths, task_id)?;
            write_result(parsed, stdout, &result, &format!("Archived {task_id}"))
        }
        Some("events") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = tasks::list_task_events(&paths, task_id)?;
            let count = result["events"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} event(s)"))
        }
        Some("attempts") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = tasks::list_task_attempts(&paths, task_id)?;
            let count = result["attempts"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} attempt(s)"))
        }
        _ => Err(CliError::new(
            "Expected task subcommand: list, next, read, claim-next, claim, heartbeat, complete, fail, release, retry, cancel, archive, events, or attempts.",
        )),
    }
}

fn run_workflows_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let paths = parsed_current_paths(parsed)?;
    match parsed.positionals.get(1).map(String::as_str) {
        Some("list") => {
            let result = tasks::list_workflows(&paths)?;
            let count = result["workflows"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} workflow(s)"))
        }
        Some("read") => {
            let workflow_id = required_flag(parsed, "workflow-id")?;
            let result = tasks::read_workflow(&paths, workflow_id)?;
            write_result(parsed, stdout, &result, workflow_id)
        }
        _ => Err(CliError::new("Expected workflow subcommand: list or read.")),
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

fn run_agent_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        Some("bootstrap") => {
            let paths = parsed_bootstrap_paths(parsed)?;
            let payload = agent_bootstrap(&paths, VERSION)?;
            write_result(parsed, stdout, &payload, "MyOpenPanels agent protocol v6 bootstrap")
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
        Some("catalog") => {
            let payload = registry::catalog(string_flag(parsed, "domain")).ok_or_else(|| {
                CliError::with_code(
                    "catalog_domain_not_found",
                    format!(
                        "MyOpenPanels command catalog domain not found: {}",
                        string_flag(parsed, "domain").unwrap_or_default()
                    ),
                )
            })?;
            let text = render_agent_discovery_summary(&payload);
            write_result(parsed, stdout, &payload, &text)
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
        Some("target") => {
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
                    let protocol_version = string_flag(parsed, "protocol-version")
                        .map(|value| value.parse::<i64>().map_err(|_| CliError::new("Expected --protocol-version to be an integer.")))
                        .transpose()?
                        .unwrap_or(2);
                    let max_concurrency = string_flag(parsed, "max-concurrency")
                        .map(|value| value.parse::<i64>().map_err(|_| CliError::new("Expected --max-concurrency to be an integer.")))
                        .transpose()?
                        .unwrap_or(1);
                    let result = tasks::register_target(
                        &paths,
                        tasks::TargetRegistration {
                            name: required_flag(parsed, "name")?,
                            host: string_flag(parsed, "host"),
                            transport: required_flag(parsed, "transport")?,
                            capabilities: string_list_flag(parsed, "capability"),
                            priority,
                            protocol_version,
                            max_concurrency,
                            model_gateway_connection_id: None,
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
                    "Expected agent target subcommand: list, register, heartbeat, or remove.",
                )),
            }
        }
        Some("route") => {
            let paths = parsed_current_paths(parsed)?;
            match parsed.positionals.get(2).map(String::as_str) {
                Some("list") => {
                    let result = tasks::list_agent_routes(&paths)?;
                    let count = result["routes"].as_array().map(Vec::len).unwrap_or(0);
                    write_result(parsed, stdout, &result, &format!("{count} route(s)"))
                }
                Some("set") => {
                    let result = tasks::set_agent_route(
                        &paths,
                        required_flag(parsed, "capability")?,
                        &string_list_flag(parsed, "target-id"),
                    )?;
                    write_result(parsed, stdout, &result, "Agent route updated")
                }
                Some("remove") => {
                    let result = tasks::remove_agent_route(
                        &paths,
                        required_flag(parsed, "capability")?,
                    )?;
                    write_result(parsed, stdout, &result, "Agent route removed")
                }
                _ => Err(CliError::new("Expected agent route subcommand: list, set, or remove.")),
            }
        }
        Some("skill") if parsed.positionals.get(2).map(String::as_str) == Some("list") => {
            let skills = list_agent_skill_summaries(
                &parsed_current_paths(parsed)?,
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
                "actions": { "required": [], "suggested": next_actions },
            });
            let text = render_agent_discovery_summary(&payload);
            write_result(parsed, stdout, &payload, &text)
        }
        Some("skill") if parsed.positionals.get(2).map(String::as_str) == Some("read") => {
            let skill_id = required_flag(parsed, "skill-id")?;
            let paths = parsed_current_paths(parsed)?;
            let payload = read_agent_skill(&paths, skill_id, string_flag(parsed, "task-id"))?;
            let markdown = payload.markdown.clone();
            write_result(parsed, stdout, &payload, &markdown)
        }
        _ => Err(CliError::new(
            "Expected agent subcommand: bootstrap, catalog, entry-skill, bridge, skill, target, or route.",
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
                registry::CommandId::registered(intent),
                vec![
                    id_flag.to_owned(),
                    id.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            )
            .unwrap_or_else(|| panic!("missing Command Registry action for {intent}"));
            action["condition"] = serde_json::json!({
                "type": "agent-judgment",
                "description": load_when,
            });
            action
        })
        .collect()
}

fn with_task_actions(mut payload: Value, list_mode: bool) -> Value {
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
                registry::CommandId::registered("task.read"),
                vec![
                    "--task-id".to_owned(),
                    task_id.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            )
            .expect("registered Task read action");
            action["condition"] = serde_json::json!({
                "type": "agent-judgment",
                "description": "The user request should inspect or continue this Task."
            });
            actions.push(action);
        }
    } else if let Some(task) = payload.get("task") {
        if let Some(domain) = task
            .get("capability")
            .and_then(Value::as_str)
            .and_then(registry::catalog_domain_for_intent)
        {
            actions.push(catalog_domain_action(
                domain,
                "The Task requires a command from this domain.",
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
        if !lifecycle.is_empty() {
            actions.push(catalog_domain_action(
                "task",
                "The Task lifecycle requires a task command.",
            ));
        }
    }
    payload["actions"] = serde_json::json!({ "required": [], "suggested": actions });
    payload
}

fn with_operation_actions(mut payload: Value, list_mode: bool) -> Value {
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
                registry::CommandId::registered("operation.read"),
                vec![
                    "--operation-id".to_owned(),
                    operation_id.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            )
            .expect("registered Operation read action");
            action["condition"] = serde_json::json!({
                "type": "agent-judgment",
                "description": "The user request should inspect or continue this Operation."
            });
            actions.push(action);
        }
    } else {
        if let Some(skill_id) = payload.get("skillId").and_then(Value::as_str) {
            let mut action = registry::command_action(
                registry::CommandId::registered("agent.skill.read"),
                vec![
                    "--skill-id".to_owned(),
                    skill_id.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            )
            .expect("registered Skill read action");
            action["condition"] = serde_json::json!({
                "type": "resource-field",
                "field": "skillId",
                "operator": "present"
            });
            actions.push(action);
        }
        if matches!(
            payload.get("status").and_then(Value::as_str),
            Some("active" | "failed")
        ) {
            actions.push(catalog_domain_action(
                "operation",
                "The Operation requires a lifecycle command.",
            ));
        }
    }
    payload["actions"] = serde_json::json!({ "required": [], "suggested": actions });
    payload
}

fn catalog_domain_action(domain: &str, load_when: &str) -> Value {
    let mut action = registry::command_action(
        registry::CommandId::registered("agent.catalog"),
        vec![
            "--domain".to_owned(),
            domain.to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ],
    )
    .expect("registered catalog action");
    action["condition"] = serde_json::json!({
        "type": "agent-judgment",
        "description": load_when,
    });
    action
}

fn run_wiki_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    let action = parsed.positionals.get(2).map(String::as_str);
    match (subcommand, action) {
        (Some("document"), Some("generate")) => {
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
        (Some("raw"), Some("create")) => {
            let paths = parsed_current_paths(parsed)?;
            let source_file = string_flag(parsed, "source-file");
            let content = if let Some(file) = source_file {
                std::fs::read(file).map_err(|error| CliError::new(error.to_string()))?
            } else {
                required_flag(parsed, "content")?.as_bytes().to_vec()
            };
            let file_name = string_flag(parsed, "file-name")
                .or_else(|| {
                    source_file.and_then(|file| {
                        std::path::Path::new(file)
                            .file_name()
                            .and_then(|value| value.to_str())
                    })
                })
                .or_else(|| string_flag(parsed, "title"))
                .unwrap_or("document.md");
            let result = wiki::add_raw_document(
                &paths,
                file_name,
                string_flag(parsed, "title"),
                string_flag(parsed, "mime-type")
                    .or_else(|| source_file.is_none().then_some("text/markdown")),
                "agent",
                string_flag(parsed, "space-id"),
                &content,
            )?;
            let text = format!(
                "Added raw document {}",
                result["document"]["id"].as_str().unwrap_or("")
            );
            write_result(parsed, stdout, &result, &text)
        }
        (Some("raw"), Some("list")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = wiki::wiki_context(&paths)?;
            let documents = result["state"]["rawDocuments"].clone();
            let payload = serde_json::json!({ "documents": documents });
            let count = payload["documents"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &payload, &format!("{count} document(s)"))
        }
        (Some("document"), Some("list")) => {
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
        (Some("document"), Some("create")) => {
            let paths = parsed_current_paths(parsed)?;
            let file = required_flag(parsed, "content-file")?;
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
        (Some("document"), Some("read")) => {
            let paths = parsed_current_paths(parsed)?;
            let document_id = required_flag(parsed, "document-id")?;
            let result = wiki::read_generated_document(&paths, document_id)?;
            let text = result["content"].as_str().unwrap_or("");
            write_result(parsed, stdout, &result, text)
        }
        (Some("document"), Some("update")) => {
            let paths = parsed_current_paths(parsed)?;
            let document_id = required_flag(parsed, "document-id")?;
            let mut result = if let Some(file) = string_flag(parsed, "content-file") {
                let content = fs::read(file).map_err(|error| CliError::new(error.to_string()))?;
                let file_name = std::path::Path::new(file)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("document.md");
                wiki::write_generated_document_for_agent(
                    &paths,
                    document_id,
                    file_name,
                    string_flag(parsed, "mime-type"),
                    &content,
                )?
            } else {
                wiki::read_generated_document(&paths, document_id)?
            };
            if let Some(title) = string_flag(parsed, "title") {
                result = wiki::rename_generated_document(&paths, document_id, title)?;
            }
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Updated generated document {document_id}"),
            )
        }
        (Some("document"), Some("delete")) => {
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
        (Some("document"), Some("publish")) => {
            let paths = parsed_current_paths(parsed)?;
            let document_id = required_flag(parsed, "document-id")?;
            let result = wiki::publish_generated_document(
                &paths,
                document_id,
                string_flag(parsed, "space-id"),
            )?;
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Published generated document {document_id}"),
            )
        }
        (Some("raw"), Some("read")) => {
            let paths = parsed_current_paths(parsed)?;
            let document_id = required_flag(parsed, "raw-document-id")?;
            let result = wiki::read_markdown(&paths, document_id)?;
            let text = result["markdown"].as_str().unwrap_or("");
            write_result(parsed, stdout, &result, text)
        }
        (Some("raw"), Some("update")) => {
            let paths = parsed_current_paths(parsed)?;
            let document_id = required_flag(parsed, "raw-document-id")?;
            let file = required_flag(parsed, "content-file")?;
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
        (Some("space"), Some("activate")) => {
            let paths = parsed_current_paths(parsed)?;
            let wiki_space_id = required_flag(parsed, "space-id")?;
            let result = wiki::set_active_space(&paths, wiki_space_id)?;
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Active wiki space {wiki_space_id}"),
            )
        }
        (Some("space"), Some("list")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = wiki::list_spaces(&paths)?;
            let count = result["spaces"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} wiki space(s)"))
        }
        (Some("page"), Some("read")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = wiki::read_page(
                &paths,
                required_flag(parsed, "space-id")?,
                required_flag(parsed, "path")?,
            )?;
            let text = result["markdown"].as_str().unwrap_or("");
            write_result(parsed, stdout, &result, text)
        }
        (Some("page"), Some("search")) => {
            let paths = parsed_current_paths(parsed)?;
            let limit = number_flag(parsed, "limit")?.unwrap_or(20.0) as usize;
            let result = wiki::search_pages(
                &paths,
                required_flag(parsed, "space-id")?,
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
        (Some("page"), Some(mode @ ("create" | "update"))) => {
            let paths = parsed_current_paths(parsed)?;
            let wiki_space_id = required_flag(parsed, "space-id")?;
            let page_path = required_flag(parsed, "path")?;
            let pages = wiki::list_pages(&paths, wiki_space_id)?;
            let exists = pages["pages"].as_array().is_some_and(|pages| {
                pages
                    .iter()
                    .any(|page| page["path"].as_str() == Some(page_path))
            });
            if mode == "create" && exists {
                return Err(CliError::with_code(
                    "content_conflict",
                    format!("Wiki page already exists: {page_path}"),
                ));
            }
            if mode == "update" && !exists {
                return Err(CliError::with_code(
                    "wiki_page_not_found",
                    format!("Wiki page not found: {page_path}"),
                ));
            }
            let file = required_flag(parsed, "content-file")?;
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
            write_result(
                parsed,
                stdout,
                &result,
                &format!("{mode}d page {page_path}"),
            )
        }
        (Some("page"), Some("list")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = wiki::list_pages(&paths, required_flag(parsed, "space-id")?)?;
            let count = result["pages"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} page(s)"))
        }
        _ => Err(CliError::new("Unknown wiki command.")),
    }
}

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
        "writing.generate" => {
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
            write_result(parsed, stdout, &result, "Started writing generation")
        }
        "writing.refinement.read" => {
            let task_id = required_flag(parsed, "task-id")?;
            let result = crate::writing::read_refinement(&paths, task_id)?;
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Writing Skill refinement {task_id}"),
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
            "Run `myopenpanels studio start --local-only --project-dir <dir> --format json`, then retry.",
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
            "Run `myopenpanels studio start --local-only --project-dir <dir> --format json`, open the returned URL, then retry `myopenpanels agent bootstrap --format json`.",
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
