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
    find_open_port, reuse_existing_studio, start_studio, stop_studio_session, studio_status,
    wait_for_existing_studio, write_studio_session, StudioServerStatus, StudioSession,
    StudioStartOptions,
};
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
use std::io::{self, Write};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const HELP_TEXT: &str = concat!(
    "openpanels-local <command> [options]\n\n",
    "Commands:\n",
    "  studio start              Start or reuse the local studio\n",
    "  studio status             Show local studio status\n",
    "  studio open               Open the local studio in a browser\n",
    "  studio serve              Run the local studio in the foreground\n",
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
    "  --port <port>             Studio port for foreground serving\n",
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
    if let Ok(url) = std::env::var("OPENPANELS_TRACE_URL") {
        if !url.is_empty() {
            return Some(url);
        }
    }

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
            let result = start_studio(
                &paths,
                StudioStartOptions {
                    host: studio_host(parsed),
                    open_browser: !has_flag(parsed, "no-open"),
                    static_dir: string_flag(parsed, "static-dir").map(std::path::PathBuf::from),
                },
            )?;
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
    let action = parsed.positionals.get(2).map(String::as_str);
    match (subcommand, action) {
        (None, _) | (Some("context"), _) => {
            let paths = parsed_paths(parsed)?;
            let (payload, markdown) = agent_context(&paths, VERSION, Some(PanelKind::Wiki))?;
            write_result(parsed, stdout, &payload, &markdown)
        }
        (Some("agent-target"), Some("register")) => {
            let paths = parsed_paths(parsed)?;
            let result = wiki::register_agent_target(
                &paths,
                string_flag(parsed, "host").unwrap_or("unknown"),
                string_flag(parsed, "thread-id").unwrap_or("default"),
                string_flag(parsed, "wake-url"),
            )?;
            let text = format!(
                "Registered {}",
                result["target"]["host"].as_str().unwrap_or("")
            );
            write_result(parsed, stdout, &result, &text)
        }
        (Some("agent-target"), None | Some("list")) => {
            let paths = parsed_paths(parsed)?;
            let result = wiki::list_agent_targets(&paths)?;
            let text = result["targets"]
                .as_array()
                .map(|targets| {
                    targets
                        .iter()
                        .map(|target| {
                            format!(
                                "{}:{}",
                                target["host"].as_str().unwrap_or(""),
                                target["threadId"].as_str().unwrap_or("")
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();
            write_result(parsed, stdout, &result, &text)
        }
        (Some("raw"), Some("new-markdown") | Some("add-text")) => {
            let paths = parsed_paths(parsed)?;
            let title = string_flag(parsed, "title").unwrap_or("Untitled");
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
        (Some("raw"), Some("add")) => {
            let paths = parsed_paths(parsed)?;
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
        (Some("raw"), None | Some("list")) => {
            let paths = parsed_paths(parsed)?;
            let result = wiki::wiki_context(&paths)?;
            let documents = result["state"]["rawDocuments"].clone();
            let payload = serde_json::json!({ "documents": documents });
            let count = payload["documents"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &payload, &format!("{count} document(s)"))
        }
        (Some("markdown"), Some("read")) => {
            let paths = parsed_paths(parsed)?;
            let document_id = required_flag(parsed, "document-id")?;
            let result = wiki::read_markdown(&paths, document_id)?;
            let text = result["markdown"].as_str().unwrap_or("");
            write_result(parsed, stdout, &result, text)
        }
        (Some("markdown"), Some("write")) => {
            let paths = parsed_paths(parsed)?;
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
            let paths = parsed_paths(parsed)?;
            let result = wiki::list_tasks(&paths, string_flag(parsed, "status"))?;
            let count = result["tasks"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} task(s)"))
        }
        (Some("tasks"), Some("next")) => {
            let paths = parsed_paths(parsed)?;
            let result = wiki::next_task(&paths)?;
            let text = result["task"]["id"].as_str().unwrap_or("No queued task");
            write_result(parsed, stdout, &result, text)
        }
        (Some("tasks"), Some("claim")) => {
            let paths = parsed_paths(parsed)?;
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
            let paths = parsed_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = wiki::complete_task(&paths, task_id, None)?;
            write_result(parsed, stdout, &result, &format!("Completed {task_id}"))
        }
        (Some("tasks"), Some("fail")) => {
            let paths = parsed_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let result = wiki::fail_task(
                &paths,
                task_id,
                string_flag(parsed, "message").unwrap_or("Wiki task failed"),
            )?;
            write_result(parsed, stdout, &result, &format!("Failed {task_id}"))
        }
        (Some("spaces"), Some("active")) => {
            let paths = parsed_paths(parsed)?;
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
            let paths = parsed_paths(parsed)?;
            let result = wiki::list_spaces(&paths)?;
            let count = result["spaces"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} wiki space(s)"))
        }
        (Some("pages"), Some("read")) => {
            let paths = parsed_paths(parsed)?;
            let result = wiki::read_page(
                &paths,
                required_flag(parsed, "wiki-space-id")?,
                required_flag(parsed, "path")?,
            )?;
            let text = result["markdown"].as_str().unwrap_or("");
            write_result(parsed, stdout, &result, text)
        }
        (Some("pages"), Some("create") | Some("write")) => {
            let paths = parsed_paths(parsed)?;
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
            let paths = parsed_paths(parsed)?;
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
