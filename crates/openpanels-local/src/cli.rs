use crate::agent::{
    agent_context, capabilities, list_agent_guides, read_agent_guide, render_agent_guides_markdown,
};
use crate::bridge::{read_bridge_status, run_bridge, BridgeOptions};
use crate::canvas::{insert_image, insert_placeholder, InsertImageInput, InsertPlaceholderInput};
use crate::control::{create_project, read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::resolve_openpanels_paths;
use crate::selection::{read_selection, read_selection_asset_to_file};
use crate::server::run_server;
use crate::storage::Storage;
use crate::studio::{
    find_open_port, record_current_studio, resolve_current_studio_session, reuse_existing_studio,
    start_studio, stop_studio_session, studio_status, wait_for_existing_studio,
    write_studio_session, StudioServerStatus, StudioSession, StudioStartOptions,
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

pub const HELP_TEXT: &str = concat!(
    "openpanels-local <command> [options]\n\n",
    "Commands:\n",
    "  agent context             Read current agent context and instructions\n",
    "  agent capabilities        List the current agent-facing command set\n",
    "  agent bridge              Run a generic task bridge command\n",
    "  agent guides              List loadable agent guides\n",
    "  agent guide <id>          Print one full agent guide\n",
    "  project current           Read the current user-visible project\n",
    "  project list              List available projects\n",
    "  project create            Create a project explicitly\n",
    "  panel list                List panels in the current project\n",
    "  panel current             Read the current panel\n",
    "  panel switch              Switch the active panel by kind\n",
    "  canvas state              Read current canvas state\n",
    "  canvas selection read     Read current canvas selection\n",
    "  canvas selection export   Export selected canvas pixels to a file\n",
    "  canvas placeholder create Insert a generation placeholder\n",
    "  canvas image insert       Insert a local image into the current canvas\n",
    "  tasks list|next|inspect   Read project tasks across panels\n",
    "  wiki context              Read current wiki context\n",
    "  wiki documents list       List raw wiki documents\n",
    "  wiki documents add        Add a raw wiki document\n",
    "  wiki documents create-markdown Create markdown raw document\n",
    "  wiki markdown read        Read source markdown for a document\n",
    "  wiki markdown write       Write source markdown for a document\n",
    "  wiki tasks list|next|claim|complete|fail\n",
    "  wiki spaces list|switch\n",
    "  wiki pages list|read|write\n",
    "  studio start              Start or reuse the local studio\n",
    "  studio status             Show local studio status\n",
    "  studio open               Open the local studio in a browser\n",
    "  studio serve              Run the local studio in the foreground\n",
    "  studio wait               Wait for the local studio to become ready\n",
    "  studio stop               Stop the local studio\n",
    "  update                    Install the latest GitHub Releases binary\n",
    "  update check              Check GitHub Releases for a newer binary\n\n",
    "Options:\n",
    "  --project <dir>           Project directory (default: cwd or OPENPANELS_PROJECT_DIR; data is global)\n",
    "  --host <host>             Studio bind host (default: 0.0.0.0; set 127.0.0.1 for local-only)\n",
    "  --local-only              Bind the studio to 127.0.0.1\n",
    "  --port <port>             Studio port for foreground serving\n",
    "  --metadata-file <json>    Attach image asset metadata when inserting an image\n",
    "  --allow-fallback          Allow fallback assets for canvas selection export\n",
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
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'a str>,
    error: &'a str,
}

pub fn run_cli(argv: &[String]) -> i32 {
    let stdout = io::stdout();
    let stderr = io::stderr();
    if let Some(trace_url) = trace_url_for_cli(argv) {
        std::env::set_var("OPENPANELS_TRACE_URL", trace_url);
        trace::emit_cli_event(TraceEventInput {
            audience: None,
            category: Some("cli".to_owned()),
            detail: Some(serde_json::json!({ "argv": argv })),
            direction: Some("start".to_owned()),
            release_summary: None,
            run_id: std::env::var("OPENPANELS_TRACE_RUN_ID").ok(),
            source: Some("openpanels-local".to_owned()),
            summary: Some(format!("openpanels-local {}", argv.join(" "))),
            task_id: std::env::var("OPENPANELS_TRACE_RUN_ID").ok(),
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
                Some("A local OpenPanels command failed".to_owned())
            },
            run_id: std::env::var("OPENPANELS_TRACE_RUN_ID").ok(),
            source: Some("openpanels-local".to_owned()),
            summary: Some(format!("openpanels-local exited with {code}")),
            task_id: std::env::var("OPENPANELS_TRACE_RUN_ID").ok(),
        });
        return code;
    }
    run_cli_with_io(argv, stdout.lock(), stderr.lock())
}

fn trace_url_for_cli(argv: &[String]) -> Option<String> {
    let parsed = parse_args(argv);
    let command = parsed.positionals.first().map(String::as_str);
    if command.is_none()
        || command == Some("__serve-studio")
        || command == Some("help")
        || has_flag(&parsed, "help")
        || has_flag(&parsed, "version")
    {
        return None;
    }

    if let Ok(url) = std::env::var("OPENPANELS_TRACE_URL") {
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
                run_id: std::env::var("OPENPANELS_TRACE_RUN_ID").ok(),
                source: Some("openpanels-local".to_owned()),
                summary: Some(format!("cli {}: {}", self.stream, text)),
                task_id: std::env::var("OPENPANELS_TRACE_RUN_ID").ok(),
            });
        }
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
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

    if has_flag(parsed, "help") {
        write_text(stdout, &command_help_text(parsed))?;
        return Ok(());
    }

    if command.is_none() || command == Some("help") {
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

fn command_help_text(parsed: &ParsedArgs) -> String {
    let parts = parsed
        .positionals
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    match parts.as_slice() {
        [command, rest @ ..] if *command == "project" => match rest {
            ["current", ..] => help_block(
                "openpanels-local project current",
                "Read the current user-visible Project. Does not create a Project.",
                &[],
            ),
            ["list", ..] => help_block(
                "openpanels-local project list",
                "List available Projects. Does not create a Project.",
                &[],
            ),
            ["create", ..] => help_block(
                "openpanels-local project create",
                "Create a Project explicitly.",
                &["--title <title>"],
            ),
            _ => help_block(
                "openpanels-local project <current|list|create>",
                "Project commands.",
                &[],
            ),
        },
        [command, rest @ ..] if *command == "panel" => match rest {
            ["list", ..] => help_block(
                "openpanels-local panel list",
                "List panels in the current Project. Does not create a Project.",
                &[],
            ),
            ["current", ..] => help_block(
                "openpanels-local panel current",
                "Read the current panel. Does not create a Project.",
                &[],
            ),
            ["switch", ..] => help_block(
                "openpanels-local panel switch",
                "Switch the current panel by kind. Does not create a Project.",
                &["--kind <wiki|canvas|image|diff|preview|files>"],
            ),
            _ => help_block(
                "openpanels-local panel <list|current|switch>",
                "Panel commands for the current Project.",
                &[],
            ),
        },
        [command, rest @ ..] if *command == "canvas" => match rest {
            ["state", ..] => help_block(
                "openpanels-local canvas state",
                "Read current canvas state. Does not create a Project.",
                &[],
            ),
            ["selection", "read", ..] => help_block(
                "openpanels-local canvas selection read",
                "Read current canvas selection. Does not create a Project.",
                &["--include-image-base64"],
            ),
            ["selection", "export", ..] => help_block(
                "openpanels-local canvas selection export",
                "Export selected canvas pixels to a file. Does not create a Project.",
                &["--output <path>", "--allow-fallback"],
            ),
            ["placeholder", "create", ..] => help_block(
                "openpanels-local canvas placeholder create",
                "Insert a generation placeholder. Does not create a Project.",
                &[
                    "--display-width <number>",
                    "--display-height <number>",
                    "--text <text>",
                    "--anchor-shape-id <id>",
                ],
            ),
            ["image", "insert", ..] => help_block(
                "openpanels-local canvas image insert",
                "Insert a local image into the current canvas. Does not create a Project.",
                &[
                    "--image <path>",
                    "--placement <auto|right|below|left>",
                    "--metadata-file <json>",
                    "--replace-shape-id <id>",
                ],
            ),
            _ => help_block(
                "openpanels-local canvas <state|selection|placeholder|image>",
                "Canvas commands for the current Project.",
                &[],
            ),
        },
        [command, rest @ ..] if *command == "wiki" => match rest {
            ["context", ..] => help_block(
                "openpanels-local wiki context",
                "Read current wiki context. Does not create a Project.",
                &[],
            ),
            ["documents", "list", ..] => help_block(
                "openpanels-local wiki documents list",
                "List raw wiki documents. Does not create a Project.",
                &[],
            ),
            ["documents", "add", ..] => help_block(
                "openpanels-local wiki documents add",
                "Add a raw wiki document. Does not create a Project.",
                &["--file <path>", "--title <title>", "--mime-type <mime>"],
            ),
            ["documents", "create-markdown", ..] => help_block(
                "openpanels-local wiki documents create-markdown",
                "Create a markdown raw document. Does not create a Project.",
                &["--title <title>", "--file <path>", "--content <text>"],
            ),
            ["markdown", "read", ..] => help_block(
                "openpanels-local wiki markdown read",
                "Read markdown for a raw document. Does not create a Project.",
                &["--document-id <id>"],
            ),
            ["markdown", "write", ..] => help_block(
                "openpanels-local wiki markdown write",
                "Write markdown for a raw document. Does not create a Project.",
                &["--document-id <id>", "--file <path>", "--task-id <id>"],
            ),
            ["tasks", action, ..] => help_block(
                &format!("openpanels-local wiki tasks {action}"),
                "Operate on wiki tasks. Does not create a Project.",
                &["--task-id <id>", "--message <message>"],
            ),
            ["spaces", "switch", ..] => help_block(
                "openpanels-local wiki spaces switch",
                "Switch active wiki space. Does not create a Project.",
                &["--wiki-space-id <id>"],
            ),
            ["spaces", "list", ..] => help_block(
                "openpanels-local wiki spaces list",
                "List wiki spaces. Does not create a Project.",
                &[],
            ),
            ["pages", action, ..] => help_block(
                &format!("openpanels-local wiki pages {action}"),
                "Read, list, or write wiki pages. Does not create a Project.",
                &["--wiki-space-id <id>", "--path <path>", "--file <path>"],
            ),
            _ => help_block(
                "openpanels-local wiki <context|documents|markdown|tasks|spaces|pages>",
                "Wiki commands for the current Project.",
                &[],
            ),
        },
        [command, rest @ ..] if *command == "agent" => match rest {
            ["context", ..] => help_block(
                "openpanels-local agent context",
                "Read current agent context. Does not create a Project.",
                &[],
            ),
            ["capabilities", ..] => help_block(
                "openpanels-local agent capabilities",
                "List the current agent-facing command set.",
                &[],
            ),
            ["bridge", ..] => help_block(
                "openpanels-local agent bridge [status] [--command <command>]",
                "Run the task bridge or read bridge status. Does not create a Project.",
                &[
                    "--command <command>",
                    "--once",
                    "--queue <queue>",
                    "--interval-ms <ms>",
                    "--timeout-ms <ms>",
                ],
            ),
            ["guides", ..] => help_block(
                "openpanels-local agent guides",
                "List loadable agent guides.",
                &[],
            ),
            ["guide", ..] => help_block(
                "openpanels-local agent guide <guide-id>",
                "Read one full agent guide. Does not create a Project.",
                &["--task-id <id>"],
            ),
            _ => help_block(
                "openpanels-local agent <context|capabilities|bridge|guides|guide>",
                "Agent discovery and context commands.",
                &[],
            ),
        },
        [command, rest @ ..] if *command == "tasks" => match rest {
            ["list", ..] | [] => help_block(
                "openpanels-local tasks list",
                "List project tasks across panels. Does not create a Project.",
                &["--queue <queue>", "--status <status>", "--pending"],
            ),
            ["next", ..] => help_block(
                "openpanels-local tasks next",
                "Read the next pending project task. Does not create a Project.",
                &["--queue <queue>", "--status <status>"],
            ),
            ["inspect", ..] => help_block(
                "openpanels-local tasks inspect --task-id <id>",
                "Read one project task by id. Does not create a Project.",
                &["--task-id <id>"],
            ),
            _ => HELP_TEXT.to_owned(),
        },
        _ => HELP_TEXT.to_owned(),
    }
}

fn help_block(usage: &str, description: &str, flags: &[&str]) -> String {
    let flags = if flags.is_empty() {
        "Flags:\n  --format json\n".to_owned()
    } else {
        format!("Flags:\n  {}\n  --format json\n", flags.join("\n  "))
    };
    format!("{usage}\n\n{description}\n\n{flags}")
}

fn run_tasks_command(parsed: &ParsedArgs, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        None | Some("list") => {
            let paths = parsed_current_paths(parsed)?;
            let result = tasks::list_tasks(&paths, task_list_filter(parsed))?;
            let count = result["tasks"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} task(s)"))
        }
        Some("next") => {
            let paths = parsed_current_paths(parsed)?;
            let result = tasks::next_task(&paths, task_list_filter(parsed))?;
            let text = result["task"]["id"].as_str().unwrap_or("No pending task");
            write_result(parsed, stdout, &result, text)
        }
        Some("inspect") => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = tasks::inspect_task(&paths, task_id)?;
            write_result(parsed, stdout, &result, task_id)
        }
        _ => Err(CliError::new(
            "Expected tasks subcommand: list, next, or inspect.",
        )),
    }
}

fn task_list_filter<'a>(parsed: &'a ParsedArgs) -> tasks::TaskListFilter<'a> {
    tasks::TaskListFilter {
        pending: has_flag(parsed, "pending"),
        queue: string_flag(parsed, "queue"),
        status: string_flag(parsed, "status"),
    }
}

fn run_project_command(parsed: &ParsedArgs, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        None | Some("current") => {
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
            let paths = parsed_current_paths(parsed).unwrap_or(fallback);
            let storage = Storage::open(&paths)?;
            let sessions = storage.list_sessions()?;
            let payload = serde_json::json!({ "projects": sessions });
            let count = payload["projects"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &payload, &format!("{count} project(s)"))
        }
        Some("create") => {
            let fallback = parsed_paths(parsed)?;
            let paths = parsed_current_paths(parsed).unwrap_or(fallback);
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
        _ => Err(CliError::new(
            "Expected project subcommand: current, list, or create.",
        )),
    }
}

fn run_panel_command(parsed: &ParsedArgs, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        None | Some("current") => run_project_read_command(parsed, stdout, "active-panel"),
        Some("list") => run_project_read_command(parsed, stdout, "panels"),
        Some("switch") => {
            let _ = required_flag(parsed, "kind")?;
            run_project_read_command(parsed, stdout, "active-panel")
        }
        _ => Err(CliError::new(
            "Expected panel subcommand: current, list, or switch.",
        )),
    }
}

fn run_canvas_command(parsed: &ParsedArgs, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    let action = parsed.positionals.get(2).map(String::as_str);
    match (subcommand, action) {
        (None, _) | (Some("state"), _) => run_project_read_command(parsed, stdout, "canvas-state"),
        (Some("selection"), None | Some("read")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = read_selection(&paths, None, has_flag(parsed, "include-image-base64"))?;
            let text = format!(
                "Selection contains {} shape(s)",
                selected_shape_count(&result.selection)
            );
            write_result(parsed, stdout, &result, &text)
        }
        (Some("selection"), Some("export")) => {
            let paths = parsed_current_paths(parsed)?;
            let output = required_flag(parsed, "output")?;
            let result =
                read_selection_asset_to_file(&paths, None, output, has_flag(parsed, "allow-fallback"))?;
            let text = format!("Wrote {}", result.output_path);
            write_result(parsed, stdout, &result, &text)
        }
        (Some("placeholder"), Some("create")) => {
            let paths = parsed_current_paths(parsed)?;
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
            write_result(parsed, stdout, &result, &text)
        }
        (Some("image"), Some("insert")) => {
            let paths = parsed_current_paths(parsed)?;
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
            "Expected canvas subcommand: state, selection read, selection export, placeholder create, or image insert.",
        )),
    }
}

fn run_studio_command(parsed: &ParsedArgs, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        Some("start") => {
            let paths = parsed_paths(parsed)?;
            let result = start_studio(
                &paths,
                StudioStartOptions {
                    host: studio_host(parsed),
                    open_browser: !has_flag(parsed, "no-open"),
                    static_dir: string_flag(parsed, "static-dir").map(std::path::PathBuf::from),
                },
            )?;
            let _ = record_current_studio(&paths, &result.session);
            let payload = studio_session_payload(
                &result.session,
                &[
                    ("ok", serde_json::json!(true)),
                    ("reusedExisting", serde_json::json!(result.reused_existing)),
                ],
            )?;
            write_result(parsed, stdout, &payload, &result.session.server_url)
        }
        Some("status") => {
            let paths = parsed_paths(parsed)?;
            let status = studio_status(&paths)?;
            let text = studio_status_text(status.server);
            write_result(parsed, stdout, &status, &text)
        }
        Some("open") => {
            let paths = parsed_paths(parsed)?;
            let result = start_studio(
                &paths,
                StudioStartOptions {
                    host: studio_host(parsed),
                    open_browser: true,
                    static_dir: string_flag(parsed, "static-dir").map(std::path::PathBuf::from),
                },
            )?;
            let _ = record_current_studio(&paths, &result.session);
            let payload = studio_session_payload(
                &result.session,
                &[
                    ("ok", serde_json::json!(true)),
                    ("opened", serde_json::json!(true)),
                    ("reusedExisting", serde_json::json!(result.reused_existing)),
                ],
            )?;
            write_result(
                parsed,
                stdout,
                &payload,
                &format!("Opened {}", result.session.server_url),
            )
        }
        Some("serve") => {
            let paths = parsed_paths(parsed)?;
            if let Some(session) = reuse_existing_studio(&paths, false)? {
                let _ = record_current_studio(&paths, &session);
                let payload = studio_session_payload(
                    &session,
                    &[
                        ("ok", serde_json::json!(true)),
                        ("foreground", serde_json::json!(false)),
                        ("reusedExisting", serde_json::json!(true)),
                    ],
                )?;
                return write_result(parsed, stdout, &payload, &session.server_url);
            }
            let host = studio_host(parsed);
            let port = studio_port(parsed)?.unwrap_or(find_open_port(&host)?);
            let local_server_url = format!("http://127.0.0.1:{port}");
            let session = StudioSession {
                browser_url: Some(local_server_url.clone()),
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
            write_studio_session(&paths, &session)?;
            let _ = record_current_studio(&paths, &session);
            let payload = studio_session_payload(
                &session,
                &[
                    ("ok", serde_json::json!(true)),
                    ("foreground", serde_json::json!(true)),
                    ("reusedExisting", serde_json::json!(false)),
                ],
            )?;
            write_result(parsed, stdout, &payload, &session.server_url)?;
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
            "Expected studio subcommand: start, status, open, serve, wait, or stop.",
        )),
    }
}

fn run_agent_command(parsed: &ParsedArgs, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    match subcommand {
        None | Some("context") => {
            let paths = parsed_current_paths(parsed)?;
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
                    command: string_flag(parsed, "command"),
                    interval_ms,
                    once: has_flag(parsed, "once"),
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
            let paths = parsed_current_paths(parsed)?;
            let payload = read_agent_guide(&paths, guide_id, string_flag(parsed, "task-id"))?;
            let markdown = payload.markdown.clone();
            write_result(parsed, stdout, &payload, &markdown)
        }
        _ => Err(CliError::new(
            "Expected agent subcommand: context, capabilities, bridge, guides, or guide.",
        )),
    }
}

fn run_wiki_command(parsed: &ParsedArgs, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    let action = parsed.positionals.get(2).map(String::as_str);
    match (subcommand, action) {
        (None, _) | (Some("context"), _) => {
            let paths = parsed_current_paths(parsed)?;
            let (payload, markdown) = agent_context(&paths, VERSION, Some(PanelKind::Wiki))?;
            write_result(parsed, stdout, &payload, &markdown)
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
        (Some("documents"), None | Some("list")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = wiki::wiki_context(&paths)?;
            let documents = result["state"]["rawDocuments"].clone();
            let payload = serde_json::json!({ "documents": documents });
            let count = payload["documents"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &payload, &format!("{count} document(s)"))
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
        (Some("tasks"), None | Some("list")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = wiki::list_tasks(&paths, string_flag(parsed, "status"))?;
            let count = result["tasks"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} task(s)"))
        }
        (Some("tasks"), Some("next")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = wiki::next_task(&paths)?;
            let text = result["task"]["id"].as_str().unwrap_or("No queued task");
            write_result(parsed, stdout, &result, text)
        }
        (Some("tasks"), Some("claim")) => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = wiki::claim_task(
                &paths,
                task_id,
                string_flag(parsed, "agent-host"),
                string_flag(parsed, "thread-id"),
            )?;
            write_result(parsed, stdout, &result, &format!("Claimed {task_id}"))
        }
        (Some("tasks"), Some("complete")) => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = wiki::complete_task(&paths, task_id, None)?;
            write_result(parsed, stdout, &result, &format!("Completed {task_id}"))
        }
        (Some("tasks"), Some("fail")) => {
            let paths = parsed_current_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = wiki::fail_task(
                &paths,
                task_id,
                string_flag(parsed, "message").unwrap_or("Wiki task failed"),
            )?;
            write_result(parsed, stdout, &result, &format!("Failed {task_id}"))
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
        (Some("spaces"), None | Some("list")) => {
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
        (Some("pages"), None | Some("list")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = wiki::list_pages(&paths, required_flag(parsed, "wiki-space-id")?)?;
            let count = result["pages"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} page(s)"))
        }
        _ => Err(CliError::new("Unknown wiki command.")),
    }
}

fn run_project_read_command(
    parsed: &ParsedArgs,
    stdout: &mut impl Write,
    command: &str,
) -> Result<(), CliError> {
    let paths = parsed_current_paths(parsed)?;
    let mut request = BootstrapRequest::new();
    request.requested_panel_kind = if command == "canvas-state" {
        Some(PanelKind::Canvas)
    } else {
        string_flag(parsed, "kind")
            .map(parse_panel_kind)
            .transpose()?
    };
    let bootstrap = read_project_bootstrap(&paths, request)?;

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
                "revision": bootstrap.revision,
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
    revision: i64,
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
            revision: bootstrap.revision,
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

fn parsed_current_paths(parsed: &ParsedArgs) -> Result<crate::paths::OpenPanelsPaths, CliError> {
    let paths = parsed_paths(parsed)?;
    if string_flag(parsed, "context-id").is_some() {
        return Ok(paths);
    }
    let Some(session) = resolve_current_studio_session(&paths)? else {
        return Err(CliError::with_code(
            "no_current_project",
            "No current MyOpenPanels project is available. Focus an open Studio window or create a project explicitly with `openpanels-local project create`.",
        ));
    };
    resolve_openpanels_paths(
        Some(&session.project_dir),
        Some(&session.storage_dir),
        Some(&session.context_id),
    )
}

#[cfg(test)]
fn resolve_panel_paths_for_active_studio(
    paths: crate::paths::OpenPanelsPaths,
    has_explicit_context_id: bool,
) -> Result<crate::paths::OpenPanelsPaths, CliError> {
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
    resolve_openpanels_paths(
        Some(&session.project_dir),
        Some(&session.storage_dir),
        Some(&session.context_id),
    )
}

fn image_metadata_flag(parsed: &ParsedArgs) -> Result<Option<Value>, CliError> {
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

fn studio_host(parsed: &ParsedArgs) -> String {
    if has_flag(parsed, "local-only") {
        return "127.0.0.1".to_owned();
    }
    string_flag(parsed, "host")
        .map(str::to_owned)
        .or_else(|| std::env::var("OPENPANELS_STUDIO_HOST").ok())
        .unwrap_or_else(|| "0.0.0.0".to_owned())
}

fn studio_port(parsed: &ParsedArgs) -> Result<Option<u16>, CliError> {
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
                code: error.code(),
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
