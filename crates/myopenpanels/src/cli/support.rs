use super::*;

pub(super) fn has_flag(parsed: &Invocation, name: &str) -> bool {
    parsed.flags.contains_key(name)
}

pub(super) fn string_flag<'a>(parsed: &'a Invocation, name: &str) -> Option<&'a str> {
    match parsed.flags.get(name) {
        Some(FlagValue::String(value)) => Some(value),
        _ => None,
    }
}

pub(super) fn string_list_flag(parsed: &Invocation, name: &str) -> Vec<String> {
    string_flag(parsed, name)
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(super) fn required_flag<'a>(parsed: &'a Invocation, name: &str) -> Result<&'a str, CliError> {
    string_flag(parsed, name)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CliError::new(format!("Missing required --{name} <value>.")))
}

pub(super) fn number_flag(parsed: &Invocation, name: &str) -> Result<Option<f64>, CliError> {
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

pub(super) fn output_format(parsed: &Invocation) -> OutputFormat {
    match string_flag(parsed, "format") {
        Some("json") => OutputFormat::Json,
        _ => OutputFormat::Text,
    }
}

pub(super) fn studio_status_text(status: StudioServerStatus) -> String {
    match status {
        StudioServerStatus::Running => "MyOpenPanels studio running".to_owned(),
        StudioServerStatus::Missing => "MyOpenPanels studio missing".to_owned(),
        StudioServerStatus::Stale => "MyOpenPanels studio stale".to_owned(),
        StudioServerStatus::Unavailable => "MyOpenPanels studio unavailable".to_owned(),
    }
}

pub(super) fn render_agent_discovery_summary(payload: &Value) -> String {
    if let Some(scope) = payload.get("scope").and_then(Value::as_str) {
        let count = payload
            .get("capabilities")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        return format!("{count} capability(s) in scope {scope}");
    }
    for key in ["scopes", "guides", "skills"] {
        if let Some(items) = payload.get(key).and_then(Value::as_array) {
            return format!("{} {key}", items.len());
        }
    }
    "Agent discovery result".to_owned()
}

pub(super) fn should_check_for_updates(parsed: &Invocation, command: Option<&str>) -> bool {
    if output_format(parsed) == OutputFormat::Json {
        return false;
    }

    matches!(
        command,
        Some("studio" | "agent" | "project" | "panel" | "canvas" | "tasks" | "wiki")
    )
}
