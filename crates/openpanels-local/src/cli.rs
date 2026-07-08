use crate::error::CliError;
use serde::Serialize;
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

fn run_cli_with_io(
    argv: &[String],
    mut stdout: impl Write,
    mut stderr: impl Write,
) -> i32 {
    let parsed = parse_args(argv);
    match run_parsed_cli(&parsed, &mut stdout) {
        Ok(()) => 0,
        Err(error) => {
            write_error(&parsed, &mut stdout, &mut stderr, &error);
            error.exit_code()
        }
    }
}

fn run_parsed_cli(parsed: &ParsedArgs, stdout: &mut impl Write) -> Result<(), CliError> {
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

    Err(CliError::new(format!(
        "Unknown command: {}",
        command.unwrap_or_default()
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

fn output_format(parsed: &ParsedArgs) -> OutputFormat {
    match string_flag(parsed, "format") {
        Some("json") => OutputFormat::Json,
        _ => OutputFormat::Text,
    }
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
}
