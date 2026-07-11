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
    format!("# OpenPanels Agent Capabilities\n\n| Intent | Command |\n| --- | --- |\n{rows}\n")
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
