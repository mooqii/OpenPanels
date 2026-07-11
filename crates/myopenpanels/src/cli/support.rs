use super::*;

pub(super) fn parse_args(argv: &[String]) -> ParsedArgs {
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
            insert_flag(&mut flags, name, FlagValue::String(value.to_owned()));
            index += 1;
            continue;
        }

        if let Some(next) = argv.get(index + 1) {
            if !next.starts_with("--") {
                insert_flag(&mut flags, raw, FlagValue::String(next.clone()));
                index += 2;
                continue;
            }
        }

        insert_flag(&mut flags, raw, FlagValue::Bool);
        index += 1;
    }

    ParsedArgs { flags, positionals }
}

pub(super) fn validate_args(parsed: &ParsedArgs) -> Result<(), CliError> {
    let command = parsed.positionals.first().map(String::as_str);
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    let action = parsed.positionals.get(2).map(String::as_str);
    let mut allowed = vec!["format", "help"];

    if command.is_none() || command == Some("help") || command == Some("version") {
        allowed.push("version");
    } else if command == Some("update") {
        allowed.extend(["check", "version"]);
    } else {
        allowed.extend(["project-dir", "storage-dir", "context-id"]);
        match (command, subcommand, action) {
            (Some("__serve-studio"), _, _) => {
                allowed.extend(["host", "port", "static-dir", "restart-delay-ms"]);
            }
            (Some("studio"), Some("start" | "open-system-browser"), _) => {
                allowed.extend(["host", "local-only", "static-dir"]);
            }
            (Some("studio"), Some("serve"), _) => {
                allowed.extend(["host", "local-only", "port", "static-dir"]);
            }
            (Some("studio"), Some("wait"), _) => allowed.push("timeout"),
            (Some("studio"), Some("status" | "stop"), _) => {}
            (Some("project"), Some("create"), _) => allowed.push("title"),
            (Some("project"), Some("select"), _) => allowed.push("id"),
            (Some("project"), Some("current" | "list"), _) => {}
            (Some("panel"), Some("switch"), _) => allowed.push("kind"),
            (Some("panel"), Some("current" | "list"), _) => {}
            (Some("canvas"), Some("generation"), Some("begin")) => {
                allowed.extend(["display-width", "display-height", "use-selection", "text"]);
            }
            (Some("canvas"), Some("generation"), Some("complete")) => {
                allowed.extend(["operation-id", "image", "metadata-file", "metadata-json"]);
            }
            (Some("canvas"), Some("generation"), Some("fail")) => {
                allowed.extend(["operation-id", "message"]);
            }
            (Some("canvas"), Some("generation"), Some("cancel" | "inspect")) => {
                allowed.push("operation-id");
            }
            (Some("canvas"), Some("selection"), Some("read")) => {
                allowed.push("include-image-base64");
            }
            (Some("canvas"), Some("selection"), Some("export")) => {
                allowed.extend(["output", "allow-fallback"]);
            }
            (Some("canvas"), Some("placeholder"), Some("create")) => {
                allowed.extend(["anchor-shape-id", "display-height", "display-width", "text"]);
            }
            (Some("canvas"), Some("image"), Some("insert")) => {
                allowed.extend([
                    "anchor-shape-id",
                    "display-height",
                    "display-width",
                    "file-name",
                    "image",
                    "metadata-file",
                    "metadata-json",
                    "placement",
                    "replace-shape-id",
                ]);
            }
            (Some("canvas"), Some("state"), _) => {}
            (Some("tasks"), Some("list" | "next"), _) => {
                allowed.extend(["pending", "queue", "status"]);
            }
            (Some("tasks"), Some("inspect" | "retry" | "cancel" | "deliveries"), _) => {
                allowed.push("task-id");
            }
            (Some("tasks"), Some("claim-next"), _) => {
                allowed.extend(["target-id", "capability", "wait-ms"]);
            }
            (Some("tasks"), Some("claim"), _) => allowed.extend(["task-id", "target-id"]),
            (Some("tasks"), Some("heartbeat" | "release"), _) => {
                allowed.extend(["task-id", "lease-token"]);
            }
            (Some("tasks"), Some("complete"), _) => {
                allowed.extend(["task-id", "lease-token", "result-file"]);
            }
            (Some("tasks"), Some("fail"), _) => {
                allowed.extend(["task-id", "lease-token", "message", "retry-after"]);
            }
            (Some("agent"), Some("operations"), Some("list")) => allowed.push("status"),
            (Some("agent"), Some("operations"), Some("inspect")) => allowed.push("operation-id"),
            (Some("agent"), Some("bridge"), _) => allowed.extend([
                "capability",
                "command",
                "interval-ms",
                "manual-lifecycle",
                "name",
                "once",
                "queue",
                "timeout-ms",
            ]),
            (Some("agent"), Some("targets"), Some("register")) => allowed.extend([
                "capability",
                "endpoint",
                "host",
                "name",
                "priority",
                "transport",
            ]),
            (Some("agent"), Some("targets"), Some("heartbeat" | "remove")) => {
                allowed.push("target-id");
            }
            (Some("agent"), Some("guide" | "skill"), _) => allowed.push("task-id"),
            (Some("agent"), Some("bootstrap" | "capabilities" | "guides" | "skills"), _) => {}
            (Some("wiki"), Some("generation"), Some("begin")) => {
                allowed.extend(["title", "document-format", "document-id"]);
            }
            (Some("wiki"), Some("generation"), Some("complete")) => {
                allowed.extend(["operation-id", "file"]);
            }
            (Some("wiki"), Some("generation"), Some("fail")) => {
                allowed.extend(["operation-id", "message"]);
            }
            (Some("wiki"), Some("generation"), Some("cancel" | "inspect")) => {
                allowed.push("operation-id");
            }
            (Some("wiki"), Some("context"), _) => {}
            (Some("wiki"), Some("selection"), Some("read")) => {}
            (Some("wiki"), Some("documents"), Some("create-markdown")) => {
                allowed.extend(["content", "file", "file-name", "title", "wiki-space-id"]);
            }
            (Some("wiki"), Some("documents"), Some("add")) => {
                allowed.extend(["file", "file-name", "mime-type", "title", "wiki-space-id"]);
            }
            (Some("wiki"), Some("documents"), Some("list")) => {}
            (Some("wiki"), Some("generated-documents"), Some("list")) => {}
            (Some("wiki"), Some("generated-documents"), Some("create")) => {
                allowed.extend(["file", "mime-type", "task-id", "thread-id", "title"]);
            }
            (Some("wiki"), Some("generated-documents"), Some("read" | "delete")) => {
                allowed.push("document-id");
            }
            (Some("wiki"), Some("generated-documents"), Some("write")) => {
                allowed.extend(["document-id", "file", "mime-type"]);
            }
            (Some("wiki"), Some("generated-documents"), Some("rename")) => {
                allowed.extend(["document-id", "title"]);
            }
            (Some("wiki"), Some("generated-documents"), Some("publish")) => {
                allowed.extend(["document-id", "wiki-space-id"]);
            }
            (Some("wiki"), Some("markdown"), Some("read")) => allowed.push("document-id"),
            (Some("wiki"), Some("markdown"), Some("write")) => {
                allowed.extend(["document-id", "file", "task-id"]);
            }
            (Some("wiki"), Some("tasks"), Some("list")) => allowed.push("status"),
            (Some("wiki"), Some("tasks"), Some("next")) => {}
            (Some("wiki"), Some("tasks"), Some("claim")) => {
                allowed.extend(["agent-host", "task-id", "thread-id"]);
            }
            (Some("wiki"), Some("tasks"), Some("complete")) => allowed.push("task-id"),
            (Some("wiki"), Some("tasks"), Some("fail")) => {
                allowed.extend(["message", "task-id"]);
            }
            (Some("wiki"), Some("spaces"), Some("switch")) => allowed.push("wiki-space-id"),
            (Some("wiki"), Some("spaces"), Some("list")) => {}
            (Some("wiki"), Some("pages"), Some("read")) => {
                allowed.extend(["path", "wiki-space-id"]);
            }
            (Some("wiki"), Some("pages"), Some("list")) => allowed.push("wiki-space-id"),
            (Some("wiki"), Some("pages"), Some("search")) => {
                allowed.extend(["limit", "query", "wiki-space-id"]);
            }
            (Some("wiki"), Some("pages"), Some("write")) => {
                allowed.extend(["file", "path", "task-id", "title", "wiki-space-id"]);
            }
            _ => {}
        }
    }

    allowed.sort_unstable();
    allowed.dedup();
    for name in parsed.flags.keys() {
        if allowed.contains(&name.as_str()) {
            continue;
        }
        let suggestion = closest_flag(name, &allowed)
            .map(|candidate| format!(" Did you mean `--{candidate}`?"))
            .unwrap_or_default();
        return Err(CliError::with_recovery(
            "invalid_argument",
            format!("Unknown or inapplicable argument `--{name}`.{suggestion}"),
            false,
            format!("Run the command with only its documented arguments.{suggestion}"),
        ));
    }
    Ok(())
}

fn closest_flag<'a>(name: &str, allowed: &'a [&str]) -> Option<&'a str> {
    let mut ranked = allowed
        .iter()
        .map(|candidate| (*candidate, edit_distance(name, candidate)))
        .filter(|(_, distance)| *distance <= 4)
        .collect::<Vec<_>>();
    ranked.sort_by_key(|(_, distance)| *distance);
    match ranked.as_slice() {
        [(candidate, _)] => Some(*candidate),
        [(candidate, first), (_, second), ..] if first < second => Some(*candidate),
        _ => None,
    }
}

fn edit_distance(left: &str, right: &str) -> usize {
    let mut row = (0..=right.len()).collect::<Vec<_>>();
    for (left_index, left_char) in left.chars().enumerate() {
        let mut previous = row[0];
        row[0] = left_index + 1;
        for (right_index, right_char) in right.chars().enumerate() {
            let replaced = previous + usize::from(left_char != right_char);
            previous = row[right_index + 1];
            row[right_index + 1] = (row[right_index] + 1)
                .min(row[right_index + 1] + 1)
                .min(replaced);
        }
    }
    row[right.len()]
}

fn insert_flag(flags: &mut BTreeMap<String, FlagValue>, name: &str, value: FlagValue) {
    if let (Some(FlagValue::String(current)), FlagValue::String(next)) =
        (flags.get_mut(name), &value)
    {
        current.push(',');
        current.push_str(next);
        return;
    }
    flags.insert(name.to_owned(), value);
}

pub(super) fn has_flag(parsed: &ParsedArgs, name: &str) -> bool {
    parsed.flags.contains_key(name)
}

pub(super) fn string_flag<'a>(parsed: &'a ParsedArgs, name: &str) -> Option<&'a str> {
    match parsed.flags.get(name) {
        Some(FlagValue::String(value)) => Some(value),
        _ => None,
    }
}

pub(super) fn string_list_flag(parsed: &ParsedArgs, name: &str) -> Vec<String> {
    string_flag(parsed, name)
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(super) fn required_flag<'a>(parsed: &'a ParsedArgs, name: &str) -> Result<&'a str, CliError> {
    string_flag(parsed, name)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CliError::new(format!("Missing required --{name} <value>.")))
}

pub(super) fn number_flag(parsed: &ParsedArgs, name: &str) -> Result<Option<f64>, CliError> {
    string_flag(parsed, name)
        .map(|value| {
            value
                .parse::<f64>()
                .map_err(|_| CliError::new(format!("Expected --{name} to be a number.")))
        })
        .transpose()
}

pub(super) fn parse_panel_kind(value: &str) -> Result<PanelKind, CliError> {
    PanelKind::parse(value).ok_or_else(|| {
        CliError::new("Expected --kind to be one of: wiki, canvas, image, diff, preview, files.")
    })
}

pub(super) fn output_format(parsed: &ParsedArgs) -> OutputFormat {
    match string_flag(parsed, "format") {
        Some("json") => OutputFormat::Json,
        _ => OutputFormat::Text,
    }
}

pub(super) fn selected_shape_count(selection: &Value) -> usize {
    selection
        .get("selectedShapes")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

pub(super) fn studio_status_text(status: StudioServerStatus) -> String {
    match status {
        StudioServerStatus::Running => "MyOpenPanels studio running".to_owned(),
        StudioServerStatus::Missing => "MyOpenPanels studio missing".to_owned(),
        StudioServerStatus::Stale => "MyOpenPanels studio stale".to_owned(),
        StudioServerStatus::Unavailable => "MyOpenPanels studio unavailable".to_owned(),
    }
}

pub(super) fn render_capabilities_summary(capabilities: &[Value]) -> String {
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
    format!("# MyOpenPanels Agent Capabilities\n\n| Intent | Command |\n| --- | --- |\n{rows}\n")
}

pub(super) fn should_check_for_updates(parsed: &ParsedArgs, command: Option<&str>) -> bool {
    if output_format(parsed) == OutputFormat::Json {
        return false;
    }

    matches!(
        command,
        Some("studio" | "agent" | "project" | "panel" | "canvas" | "tasks" | "wiki")
    )
}
