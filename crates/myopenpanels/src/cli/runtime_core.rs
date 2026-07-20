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
use crate::paths::{
    redirect_deprecated_default_storage_dir, resolve_myopenpanels_paths,
    resolve_studio_service_paths,
};
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
        CommandGroup::Publishing => run_publishing_command(parsed, stdout),
        CommandGroup::Operation => run_operation_command(parsed, stdout),
        CommandGroup::Task => run_tasks_command(parsed, stdout),
        CommandGroup::Workflow => run_workflow_runs_command(parsed, stdout),
        CommandGroup::InternalStudioServe => {
            let storage_dir =
                redirect_deprecated_default_storage_dir(string_flag(parsed, "storage-dir"))?;
            let paths = resolve_myopenpanels_paths(
                string_flag(parsed, "project-dir"),
                storage_dir.as_deref().and_then(std::path::Path::to_str),
                string_flag(parsed, "context-id"),
            )?;
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
