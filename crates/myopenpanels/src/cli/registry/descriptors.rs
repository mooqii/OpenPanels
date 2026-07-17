fn collect_public_leaves(command: &Command, path: &mut Vec<String>, leaves: &mut BTreeSet<String>) {
    let children = command
        .get_subcommands()
        .filter(|child| !child.is_hide_set())
        .collect::<Vec<_>>();
    if children.is_empty() {
        if !path.is_empty() {
            leaves.insert(path.join(" "));
        }
        return;
    }
    for child in children {
        path.push(child.get_name().to_owned());
        collect_public_leaves(child, path, leaves);
        path.pop();
    }
}

fn catalog_domain(spec: &CommandDefinition) -> Option<&'static str> {
    match spec.audience {
        CommandAudience::Agent => Some(spec.scope),
        CommandAudience::Operator => Some("worker"),
        CommandAudience::Host | CommandAudience::Protocol | CommandAudience::Internal => None,
    }
}

fn descriptor(spec: &CommandDefinition) -> Value {
    let root = super::args::clap_command();
    let leaf = find_leaf(&root, spec.path).unwrap_or_else(|| {
        panic!(
            "registered command path is unavailable: {}",
            spec.path.join(" ")
        )
    });
    let command_args = leaf
        .get_arguments()
        .filter(|arg| {
            let id = arg.get_id().as_str();
            !arg.is_hide_set()
                && !matches!(
                    id,
                    "project_dir" | "storage_dir" | "context_id" | "format" | "help" | "version"
                )
        })
        .collect::<Vec<_>>();
    let args = command_args
        .iter()
        .map(|arg| {
            let values = arg
                .get_value_parser()
                .possible_values()
                .map(|values| {
                    values
                        .map(|value| value.get_name().to_owned())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let default_values = arg
                .get_default_values()
                .iter()
                .map(|value| value.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            json!({
                "name": arg.get_id().as_str(),
                "flag": arg.get_long().map(|value| format!("--{value}")),
                "description": arg.get_help().map(|value| value.to_string()),
                "type": argument_type(arg.get_id().as_str(), arg.get_action(), &values),
                "required": arg.is_required_set(),
                "repeatable": matches!(arg.get_action(), ArgAction::Append | ArgAction::Count),
                "values": values,
                "defaultValues": default_values,
                "sensitive": is_sensitive_argument(arg.get_id().as_str()),
            })
        })
        .collect::<Vec<_>>();
    let risk = risk_for(spec);
    let requires_active_panel = requires_active_panel(spec);
    let target_mode = if requires_active_panel {
        "active-selection"
    } else {
        match spec.target_mode {
            "task" | "panel-kind-or-task" => "task-bound",
            "operation" => "operation-bound",
            mode if mode.starts_with("panel-kind") => "panel-kind",
            mode => mode,
        }
    };
    json!({
        "intent": spec.intent,
        "description": spec.title,
        "argv": example_argv(spec, &command_args),
        "args": args,
        "risk": risk,
        "target": {
            "mode": target_mode,
            "panelKind": spec.required_panel_kind,
            "selection": if requires_active_panel {
                "active-required"
            } else if spec.intent == "canvas.image.generate" {
                "active-when-requested"
            } else {
                "none"
            },
        },
        "retry": if risk == "read" { "safe" } else { "revalidate" },
    })
}

fn argument_type(name: &str, action: &ArgAction, values: &[String]) -> &'static str {
    if matches!(action, ArgAction::SetTrue | ArgAction::SetFalse) {
        "bool"
    } else if matches!(action, ArgAction::Count) {
        "integer"
    } else if !values.is_empty() {
        "enum"
    } else if is_integer_argument(name) {
        "integer"
    } else if is_path_argument(name) && matches!(action, ArgAction::Append) {
        "path-array"
    } else if is_path_argument(name) {
        "path"
    } else if matches!(action, ArgAction::Append) {
        "string-array"
    } else {
        "string"
    }
}

fn is_integer_argument(name: &str) -> bool {
    name.ends_with("_revision")
        || name.ends_with("_ms")
        || matches!(
            name,
            "limit"
                | "max_concurrency"
                | "port"
                | "priority"
                | "protocol_version"
                | "required_protocol_version"
                | "timeout"
        )
}

fn is_path_argument(name: &str) -> bool {
    name.ends_with("_dir") || name.ends_with("_file")
}

fn is_sensitive_argument(name: &str) -> bool {
    name.contains("token") || name.contains("secret") || name.contains("password")
}

fn example_argv(spec: &CommandDefinition, args: &[&clap::Arg]) -> Vec<String> {
    let mut argv = spec
        .path
        .iter()
        .map(|part| (*part).to_owned())
        .collect::<Vec<_>>();
    for arg in args.iter().filter(|arg| arg.is_required_set()) {
        if let Some(flag) = arg.get_long() {
            argv.push(format!("--{flag}"));
        }
        if !matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse) {
            let example_value = arg
                .get_value_parser()
                .possible_values()
                .and_then(|mut values| values.next())
                .map(|value| value.get_name().to_owned())
                .unwrap_or_else(|| {
                    let value_name = arg
                        .get_value_names()
                        .and_then(|names| names.first())
                        .map(|name| name.to_string())
                        .unwrap_or_else(|| arg.get_id().as_str().to_ascii_uppercase());
                    format!("<{value_name}>")
                });
            argv.push(example_value);
        }
    }
    match spec.intent {
        "wiki.raw.create" => argv.extend(["--content".to_owned(), "<CONTENT>".to_owned()]),
        "wiki.document.update" => argv.extend(["--title".to_owned(), "<TITLE>".to_owned()]),
        _ => {}
    }
    argv.extend(["--format".to_owned(), "json".to_owned()]);
    argv
}

fn risk_for(spec: &CommandDefinition) -> &'static str {
    if spec.intent == "canvas.selection.export" {
        return "write";
    }
    if !spec.mutates {
        return "read";
    }
    if matches!(
        spec.intent,
        "update.install"
            | "studio.stop"
            | "wiki.document.delete"
            | "wiki.document.publish"
            | "writing.skill.install"
            | "task.cancel"
            | "task.archive"
            | "agent.target.remove"
            | "agent.route.remove"
    ) {
        "high-risk-write"
    } else {
        "write"
    }
}

fn requires_active_panel(spec: &CommandDefinition) -> bool {
    matches!(
        spec.intent,
        "panel.selection.read" | "canvas.selection.export"
    )
}

fn find_leaf<'a>(root: &'a Command, path: &[&str]) -> Option<&'a Command> {
    let mut command = root;
    for part in path {
        command = command
            .get_subcommands()
            .find(|child| child.get_name() == *part)?;
    }
    Some(command)
}

