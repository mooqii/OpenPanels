use crate::agent::{
    agent_bootstrap, capabilities, list_agent_guides, list_agent_skills, read_agent_guide,
    read_agent_skill, render_agent_guides_markdown, render_agent_skills_markdown,
    sync_builtin_agent_skills,
};
use crate::bridge::{read_bridge_status, run_bridge, BridgeOptions};
use crate::canvas::{insert_image, insert_placeholder, InsertImageInput, InsertPlaceholderInput};
use crate::control::{create_project, read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::operations;
use crate::paths::resolve_myopenpanels_paths;
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
    "myopenpanels <command> [options]\n\n",
    "Commands:\n",
    "  agent bootstrap           Read protocol v2 context, guides, and operations\n",
    "  agent operations list|inspect\n",
    "  agent capabilities        List the current agent-facing command set\n",
    "  agent bridge              Run a registered command task bridge\n",
    "  agent targets list|register|heartbeat|remove\n",
    "  agent guides              List loadable agent guides\n",
    "  agent guide <id>          Print one full agent guide\n",
    "  agent skills              List loadable agent skills\n",
    "  agent skill <id>          Print one full agent skill\n",
    "  project current           Read the current user-visible project\n",
    "  project list              List available projects\n",
    "  project create            Create a project explicitly\n",
    "  panel list                List panels in the current project\n",
    "  panel current             Read the current panel\n",
    "  panel switch              Switch the active panel by kind\n",
    "  canvas state              Read current canvas state\n",
    "  canvas selection read     Read current canvas selection\n",
    "  canvas selection export   Export selected canvas pixels to a file\n",
    "  canvas generation begin|complete|fail|cancel|inspect\n",
    "  canvas image insert       Insert a non-generated local image\n",
    "  tasks list|next|inspect   Read project tasks across panels\n",
    "  tasks claim-next|claim|heartbeat|complete|fail|release|retry|cancel\n",
    "  wiki context              Read current wiki context\n",
    "  wiki selection read       Read the user-selected Wiki and raw documents\n",
    "  wiki documents list       List raw wiki documents\n",
    "  wiki documents add        Add a raw wiki document\n",
    "  wiki documents create-markdown Create markdown raw document\n",
    "  wiki markdown read        Read source markdown for a document\n",
    "  wiki markdown write       Write source markdown for a document\n",
    "  wiki generated-documents list|read|rename|delete|publish\n",
    "  wiki generation begin|complete|fail|cancel|inspect\n",
    "  wiki tasks list|next|claim|complete|fail\n",
    "  wiki spaces list|switch\n",
    "  wiki pages list|search|read|write\n",
    "  studio start              Start or reuse the MyOpenPanels Studio\n",
    "  studio status             Show MyOpenPanels Studio status\n",
    "  studio open               Open the MyOpenPanels Studio in a browser\n",
    "  studio serve              Run the MyOpenPanels Studio in the foreground\n",
    "  studio wait               Wait for the MyOpenPanels Studio to become ready\n",
    "  studio stop               Stop the MyOpenPanels Studio\n",
    "  update                    Install the latest GitHub Releases binary\n",
    "  update check              Check GitHub Releases for a newer binary\n\n",
    "Options:\n",
    "  --project <dir>           Project directory (default: cwd or MYOPENPANELS_PROJECT_DIR; data is global)\n",
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
                "myopenpanels project current",
                "Read the current user-visible Project. Does not create a Project.",
                &[],
            ),
            ["list", ..] => help_block(
                "myopenpanels project list",
                "List available Projects. Does not create a Project.",
                &[],
            ),
            ["create", ..] => help_block(
                "myopenpanels project create",
                "Create a Project explicitly.",
                &["--title <title>"],
            ),
            _ => help_block(
                "myopenpanels project <current|list|create>",
                "Project commands.",
                &[],
            ),
        },
        [command, rest @ ..] if *command == "panel" => match rest {
            ["list", ..] => help_block(
                "myopenpanels panel list",
                "List panels in the current Project. Does not create a Project.",
                &[],
            ),
            ["current", ..] => help_block(
                "myopenpanels panel current",
                "Read the current panel. Does not create a Project.",
                &[],
            ),
            ["switch", ..] => help_block(
                "myopenpanels panel switch",
                "Switch the current panel by kind. Does not create a Project.",
                &["--kind <wiki|canvas|image|diff|preview|files>"],
            ),
            _ => help_block(
                "myopenpanels panel <list|current|switch>",
                "Panel commands for the current Project.",
                &[],
            ),
        },
        [command, rest @ ..] if *command == "canvas" => match rest {
            ["state", ..] => help_block(
                "myopenpanels canvas state",
                "Read current canvas state. Does not create a Project.",
                &[],
            ),
            ["selection", "read", ..] => help_block(
                "myopenpanels canvas selection read",
                "Read current canvas selection. Does not create a Project.",
                &["--include-image-base64"],
            ),
            ["selection", "export", ..] => help_block(
                "myopenpanels canvas selection export",
                "Export selected canvas pixels to a file. Does not create a Project.",
                &["--output <path>", "--allow-fallback"],
            ),
            ["placeholder", "create", ..] => help_block(
                "myopenpanels canvas placeholder create",
                "Insert a generation placeholder. Does not create a Project.",
                &[
                    "--display-width <number>",
                    "--display-height <number>",
                    "--text <text>",
                    "--anchor-shape-id <id>",
                ],
            ),
            ["image", "insert", ..] => help_block(
                "myopenpanels canvas image insert",
                "Insert a local image into the current canvas. Does not create a Project.",
                &[
                    "--image <path>",
                    "--placement <auto|right|below|left>",
                    "--metadata-file <json>",
                    "--replace-shape-id <id>",
                ],
            ),
            _ => help_block(
                "myopenpanels canvas <state|selection|placeholder|image>",
                "Canvas commands for the current Project.",
                &[],
            ),
        },
        [command, rest @ ..] if *command == "wiki" => match rest {
            ["context", ..] => help_block(
                "myopenpanels wiki context",
                "Read current wiki context. Does not create a Project.",
                &[],
            ),
            ["documents", "list", ..] => help_block(
                "myopenpanels wiki documents list",
                "List raw wiki documents. Does not create a Project.",
                &[],
            ),
            ["documents", "add", ..] => help_block(
                "myopenpanels wiki documents add",
                "Add a raw wiki document. Does not create a Project.",
                &["--file <path>", "--title <title>", "--mime-type <mime>"],
            ),
            ["documents", "create-markdown", ..] => help_block(
                "myopenpanels wiki documents create-markdown",
                "Create a markdown raw document. Does not create a Project.",
                &["--title <title>", "--file <path>", "--content <text>"],
            ),
            ["markdown", "read", ..] => help_block(
                "myopenpanels wiki markdown read",
                "Read markdown for a raw document. Does not create a Project.",
                &["--document-id <id>"],
            ),
            ["markdown", "write", ..] => help_block(
                "myopenpanels wiki markdown write",
                "Write markdown for a raw document. Does not create a Project.",
                &["--document-id <id>", "--file <path>", "--task-id <id>"],
            ),
            ["generated-documents", action, ..] => help_block(
                &format!("myopenpanels wiki generated-documents {action}"),
                "Manage agent-generated Markdown and text documents. Does not create a Project.",
                &[
                    "--document-id <id>",
                    "--file <path>",
                    "--title <title>",
                    "--task-id <id>",
                    "--thread-id <id>",
                    "--wiki-space-id <id>",
                ],
            ),
            ["tasks", action, ..] => help_block(
                &format!("myopenpanels wiki tasks {action}"),
                "Operate on wiki tasks. Does not create a Project.",
                &["--task-id <id>", "--message <message>"],
            ),
            ["spaces", "switch", ..] => help_block(
                "myopenpanels wiki spaces switch",
                "Switch active wiki space. Does not create a Project.",
                &["--wiki-space-id <id>"],
            ),
            ["spaces", "list", ..] => help_block(
                "myopenpanels wiki spaces list",
                "List wiki spaces. Does not create a Project.",
                &[],
            ),
            ["pages", action, ..] => help_block(
                &format!("myopenpanels wiki pages {action}"),
                "Read, list, or write wiki pages. Does not create a Project.",
                &["--wiki-space-id <id>", "--path <path>", "--file <path>"],
            ),
            _ => help_block(
                "myopenpanels wiki <context|documents|generated-documents|markdown|tasks|spaces|pages>",
                "Wiki commands for the current Project.",
                &[],
            ),
        },
        [command, rest @ ..] if *command == "agent" => match rest {
            ["bootstrap", ..] => help_block(
                "myopenpanels agent bootstrap",
                "Read the current Agent protocol, focus, guides, and operations.",
                &[],
            ),
            ["capabilities", ..] => help_block(
                "myopenpanels agent capabilities",
                "List the current agent-facing command set.",
                &[],
            ),
            ["bridge", ..] => help_block(
                "myopenpanels agent bridge [status] [--command <command>]",
                "Run the task bridge or read bridge status. Does not create a Project.",
                &[
                    "--command <command>",
                    "--capability <name>",
                    "--name <name>",
                    "--once",
                    "--queue <queue>",
                    "--interval-ms <ms>",
                    "--timeout-ms <ms>",
                    "--manual-lifecycle",
                ],
            ),
            ["targets", action, ..] => help_block(
                &format!("myopenpanels agent targets {action}"),
                "Register, inspect, heartbeat, or remove project agent targets.",
                &[
                    "--target-id <id>",
                    "--name <name>",
                    "--host <host>",
                    "--transport <webhook|poll|command>",
                    "--endpoint <url>",
                    "--capability <name>",
                    "--priority <number>",
                ],
            ),
            ["guides", ..] => help_block(
                "myopenpanels agent guides",
                "List loadable agent guides.",
                &[],
            ),
            ["guide", ..] => help_block(
                "myopenpanels agent guide <guide-id>",
                "Read one full agent guide. Does not create a Project.",
                &["--task-id <id>"],
            ),
            ["skills", ..] => help_block(
                "myopenpanels agent skills",
                "List loadable agent skills.",
                &[],
            ),
            ["skill", ..] => help_block(
                "myopenpanels agent skill <skill-id>",
                "Read one full agent skill. Does not create a Project.",
                &["--task-id <id>"],
            ),
            _ => help_block(
                "myopenpanels agent <context|capabilities|bridge|guides|guide|skills|skill>",
                "Agent discovery and context commands.",
                &[],
            ),
        },
        [command, rest @ ..] if *command == "tasks" => match rest {
            ["list", ..] => help_block(
                "myopenpanels tasks list",
                "List project tasks across panels. Does not create a Project.",
                &["--queue <queue>", "--status <status>", "--pending"],
            ),
            ["next", ..] => help_block(
                "myopenpanels tasks next",
                "Read the next pending project task. Does not create a Project.",
                &["--queue <queue>", "--status <status>"],
            ),
            ["inspect", ..] => help_block(
                "myopenpanels tasks inspect --task-id <id>",
                "Read one project task by id. Does not create a Project.",
                &["--task-id <id>"],
            ),
            ["claim-next", ..] => help_block(
                "myopenpanels tasks claim-next --target-id <id>",
                "Atomically claim the next matching project task.",
                &["--target-id <id>", "--capability <name>", "--wait-ms <ms>"],
            ),
            ["claim", ..] => help_block(
                "myopenpanels tasks claim --task-id <id> --target-id <id>",
                "Atomically claim one project task.",
                &["--task-id <id>", "--target-id <id>"],
            ),
            ["heartbeat", ..] | ["complete", ..] | ["fail", ..] | ["release", ..] => help_block(
                &format!("myopenpanels tasks {}", rest[0]),
                "Update a claimed project task using its lease token.",
                &[
                    "--task-id <id>",
                    "--lease-token <token>",
                    "--result-file <json>",
                    "--message <text>",
                    "--retry-after <time>",
                ],
            ),
            ["retry", ..] | ["cancel", ..] => help_block(
                &format!("myopenpanels tasks {}", rest[0]),
                "Manually retry or cancel a project task.",
                &["--task-id <id>"],
            ),
            ["deliveries", ..] => help_block(
                "myopenpanels tasks deliveries",
                "Read task delivery history.",
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
        Some("list") => {
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
        Some("current") => run_project_read_command(parsed, stdout, ProjectReadView::PanelCurrent),
        Some("list") => run_project_read_command(parsed, stdout, ProjectReadView::PanelList),
        Some("switch") => {
            let _ = required_flag(parsed, "kind")?;
            run_project_read_command(parsed, stdout, ProjectReadView::PanelSwitch)
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
        (Some("generation"), Some("begin")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = operations::begin_canvas(
                &paths,
                number_flag(parsed, "display-width")?,
                number_flag(parsed, "display-height")?,
                has_flag(parsed, "use-selection"),
                string_flag(parsed, "text"),
            )?;
            write_result(parsed, stdout, &result, result["operation"]["id"].as_str().unwrap_or("Started canvas generation"))
        }
        (Some("generation"), Some("complete")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = operations::complete_canvas(
                &paths,
                required_flag(parsed, "operation-id")?,
                required_flag(parsed, "image")?,
                image_metadata_flag(parsed)?.ok_or_else(|| CliError::with_code("generation_metadata_required", "Missing --metadata-file"))?,
            )?;
            write_result(parsed, stdout, &result, "Completed canvas generation")
        }
        (Some("generation"), Some("fail")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = operations::finish_canvas(&paths, required_flag(parsed, "operation-id")?, "failed", Some(required_flag(parsed, "message")?))?;
            write_result(parsed, stdout, &result, "Failed canvas generation")
        }
        (Some("generation"), Some("cancel")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = operations::finish_canvas(&paths, required_flag(parsed, "operation-id")?, "cancelled", None)?;
            write_result(parsed, stdout, &result, "Cancelled canvas generation")
        }
        (Some("generation"), Some("inspect")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = operations::inspect(&paths, required_flag(parsed, "operation-id")?)?;
            write_result(parsed, stdout, &result, result["status"].as_str().unwrap_or("unknown"))
        }
        (Some("state"), _) => run_project_read_command(parsed, stdout, ProjectReadView::CanvasState),
        (Some("selection"), Some("read")) => {
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
            sync_builtin_agent_skills(&paths)?;
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
            sync_builtin_agent_skills(&paths)?;
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
            sync_builtin_agent_skills(&paths)?;
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
        Some("bootstrap") => {
            let paths = parsed_current_paths(parsed)?;
            let payload = agent_bootstrap(&paths, VERSION)?;
            write_result(parsed, stdout, &payload, "MyOpenPanels agent protocol v2 bootstrap")
        }
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
        Some("skills") => {
            let paths = parsed_current_paths(parsed)?;
            let skills = list_agent_skills(&paths)?;
            let payload = serde_json::json!({ "skills": skills });
            write_result(
                parsed,
                stdout,
                &payload,
                &render_agent_skills_markdown(&skills),
            )
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

fn run_wiki_command(parsed: &ParsedArgs, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    let action = parsed.positionals.get(2).map(String::as_str);
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
        (Some("generation"), Some("complete")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = operations::complete_wiki(
                &paths,
                required_flag(parsed, "operation-id")?,
                required_flag(parsed, "file")?,
            )?;
            write_result(parsed, stdout, &result, "Completed wiki generation")
        }
        (Some("generation"), Some("fail")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = operations::finish_wiki(
                &paths,
                required_flag(parsed, "operation-id")?,
                "failed",
                Some(required_flag(parsed, "message")?),
            )?;
            write_result(parsed, stdout, &result, "Failed wiki generation")
        }
        (Some("generation"), Some("cancel")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = operations::finish_wiki(
                &paths,
                required_flag(parsed, "operation-id")?,
                "cancelled",
                None,
            )?;
            write_result(parsed, stdout, &result, "Cancelled wiki generation")
        }
        (Some("generation"), Some("inspect")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = operations::inspect(&paths, required_flag(parsed, "operation-id")?)?;
            write_result(
                parsed,
                stdout,
                &result,
                result["status"].as_str().unwrap_or("unknown"),
            )
        }
        (Some("context"), _) => {
            let paths = parsed_current_paths(parsed)?;
            let payload = wiki::wiki_context(&paths)?;
            write_result(parsed, stdout, &payload, "Wiki context")
        }
        (Some("selection"), Some("read")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = wiki::read_agent_selection(&paths)?;
            let text = format!(
                "Wiki selected: {}; raw documents selected: {}; generated documents selected: {}",
                result["selection"]["isWikiSelected"]
                    .as_bool()
                    .unwrap_or(false),
                result["selectedRawDocuments"]
                    .as_array()
                    .map(Vec::len)
                    .unwrap_or(0),
                result["selectedGeneratedDocuments"]
                    .as_array()
                    .map(Vec::len)
                    .unwrap_or(0)
            );
            write_result(parsed, stdout, &result, &text)
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
        (Some("tasks"), Some("list")) => {
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
    CanvasState,
    PanelCurrent,
    PanelList,
    PanelSwitch,
}

fn run_project_read_command(
    parsed: &ParsedArgs,
    stdout: &mut impl Write,
    view: ProjectReadView,
) -> Result<(), CliError> {
    let paths = parsed_current_paths(parsed)?;
    let mut request = BootstrapRequest::new();
    request.requested_panel_kind = match view {
        ProjectReadView::CanvasState => Some(PanelKind::Canvas),
        ProjectReadView::PanelSwitch => string_flag(parsed, "kind")
            .map(parse_panel_kind)
            .transpose()?,
        ProjectReadView::PanelCurrent | ProjectReadView::PanelList => None,
    };
    let bootstrap = read_project_bootstrap(&paths, request)?;

    if view == ProjectReadView::PanelSwitch {
        let payload = agent_bootstrap(&paths, VERSION)?;
        return write_result(
            parsed,
            stdout,
            &payload,
            "Panel switched; use the returned protocol v2 focus and guides.",
        );
    }

    match view {
        ProjectReadView::PanelList => {
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
        ProjectReadView::PanelCurrent => {
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
        ProjectReadView::PanelSwitch => unreachable!(),
        ProjectReadView::CanvasState => {
            let text = format!("Canvas ready at {}", bootstrap.storage_dir);
            write_result(
                parsed,
                stdout,
                &CanvasBootstrapPayload::from(bootstrap),
                &text,
            )
        }
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

fn parsed_paths(parsed: &ParsedArgs) -> Result<crate::paths::MyOpenPanelsPaths, CliError> {
    resolve_myopenpanels_paths(
        string_flag(parsed, "project"),
        string_flag(parsed, "storage-dir"),
        string_flag(parsed, "context-id"),
    )
}

fn parsed_current_paths(parsed: &ParsedArgs) -> Result<crate::paths::MyOpenPanelsPaths, CliError> {
    let paths = parsed_paths(parsed)?;
    if string_flag(parsed, "context-id").is_some() {
        return Ok(paths);
    }
    let Some(session) = resolve_current_studio_session(&paths)? else {
        return Err(CliError::with_code(
            "no_current_project",
            "No current MyOpenPanels project is available. Focus an open Studio window or create a project explicitly with `myopenpanels project create`.",
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
        .or_else(|| std::env::var("MYOPENPANELS_STUDIO_HOST").ok())
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
