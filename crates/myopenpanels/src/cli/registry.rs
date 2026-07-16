use clap::{ArgAction, Command};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const COMMAND_CATALOG_VERSION: u32 = 1;
const COMMAND_CATALOG_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandId {
    Catalog(usize),
    InternalStudioServe,
    ParseError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandGroup {
    Version,
    Studio,
    Update,
    Project,
    Panel,
    Canvas,
    Wiki,
    Writing,
    Task,
    Workflow,
    Operation,
    Agent,
    InternalStudioServe,
    ParseError,
}

impl CommandId {
    pub(crate) fn from_intent(intent: &str) -> Option<Self> {
        match intent {
            value if value == INTERNAL_STUDIO_DEFINITION.intent => Some(Self::InternalStudioServe),
            "cli.parse" => Some(Self::ParseError),
            _ => SPECS
                .iter()
                .position(|spec| spec.intent == intent)
                .map(Self::Catalog),
        }
    }

    pub(crate) fn intent(self) -> &'static str {
        match self {
            Self::Catalog(index) => SPECS[index].intent,
            Self::InternalStudioServe => INTERNAL_STUDIO_DEFINITION.intent,
            Self::ParseError => "cli.parse",
        }
    }

    pub(crate) fn registered(intent: &str) -> Self {
        Self::from_intent(intent)
            .filter(|id| matches!(id, Self::Catalog(_)))
            .unwrap_or_else(|| panic!("command is not registered: {intent}"))
    }

    pub(crate) fn group(self) -> CommandGroup {
        match self {
            Self::InternalStudioServe => CommandGroup::InternalStudioServe,
            Self::ParseError => CommandGroup::ParseError,
            Self::Catalog(index) => match SPECS[index].path[0] {
                "version" => CommandGroup::Version,
                "studio" => CommandGroup::Studio,
                "update" => CommandGroup::Update,
                "project" => CommandGroup::Project,
                "panel" => CommandGroup::Panel,
                "canvas" => CommandGroup::Canvas,
                "wiki" => CommandGroup::Wiki,
                "writing" => CommandGroup::Writing,
                "task" => CommandGroup::Task,
                "workflow" => CommandGroup::Workflow,
                "operation" => CommandGroup::Operation,
                "agent" => CommandGroup::Agent,
                path => panic!("unsupported registered command group: {path}"),
            },
        }
    }
}

#[derive(Clone, Copy)]
struct CommandDefinition {
    intent: &'static str,
    path: &'static [&'static str],
    title: &'static str,
    scope: &'static str,
    target_mode: &'static str,
    mutates: bool,
    required_panel_kind: Option<&'static str>,
    audience: CommandAudience,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommandAudience {
    Agent,
    Host,
    Protocol,
    Operator,
    #[allow(dead_code)]
    Internal,
}

macro_rules! spec {
    ($intent:literal, [$($path:tt),+], $title:literal, $scope:literal, $target:literal, $mutates:literal) => {
        CommandDefinition { intent: $intent, path: &[$($path),+], title: $title, scope: $scope, target_mode: $target, mutates: $mutates, required_panel_kind: None, audience: command_audience!($($path),+) }
    };
    ($intent:literal, [$($path:tt),+], $title:literal, $scope:literal, $target:literal, $mutates:literal, panel=$panel:literal) => {
        CommandDefinition { intent: $intent, path: &[$($path),+], title: $title, scope: $scope, target_mode: $target, mutates: $mutates, required_panel_kind: Some($panel), audience: command_audience!($($path),+) }
    };
}

macro_rules! command_audience {
    ("studio", $($rest:literal),*) => {
        CommandAudience::Host
    };
    ("update", $($rest:literal),*) => {
        CommandAudience::Host
    };
    ("version") => {
        CommandAudience::Host
    };
    ("__serve-studio") => {
        CommandAudience::Internal
    };
    ("agent", "bridge", $($rest:literal),*) => {
        CommandAudience::Operator
    };
    ("agent", "target", $($rest:literal),*) => {
        CommandAudience::Operator
    };
    ("agent", "route", $($rest:literal),*) => {
        CommandAudience::Operator
    };
    ("agent", "skill", $($rest:literal),*) => {
        CommandAudience::Agent
    };
    ("agent", $($rest:literal),*) => {
        CommandAudience::Protocol
    };
    ($($rest:literal),*) => {
        CommandAudience::Agent
    };
}

const INTERNAL_STUDIO_DEFINITION: CommandDefinition = CommandDefinition {
    intent: "internal.studio.serve",
    path: &["__serve-studio"],
    title: "Serve Studio internally",
    scope: "internal",
    target_mode: "none",
    mutates: true,
    required_panel_kind: None,
    audience: CommandAudience::Internal,
};

const SPECS: &[CommandDefinition] = &[
    spec!(
        "cli.version.read",
        ["version"],
        "Read CLI version",
        "system",
        "none",
        false
    ),
    spec!(
        "studio.start",
        ["studio", "start"],
        "Start or reuse Studio",
        "studio",
        "project-directory",
        true
    ),
    spec!(
        "studio.status",
        ["studio", "status"],
        "Read Studio status",
        "studio",
        "project-directory",
        false
    ),
    spec!(
        "studio.open-system-browser",
        ["studio", "open-system-browser"],
        "Open Studio in the system browser",
        "studio",
        "project-directory",
        true
    ),
    spec!(
        "studio.serve",
        ["studio", "serve"],
        "Serve Studio in the foreground",
        "studio",
        "project-directory",
        true
    ),
    spec!(
        "studio.wait",
        ["studio", "wait"],
        "Wait for Studio readiness",
        "studio",
        "project-directory",
        false
    ),
    spec!(
        "studio.stop",
        ["studio", "stop"],
        "Stop Studio",
        "studio",
        "project-directory",
        true
    ),
    spec!(
        "update.check",
        ["update", "check"],
        "Check for a CLI update",
        "update",
        "none",
        false
    ),
    spec!(
        "update.download",
        ["update", "download"],
        "Download a CLI update",
        "update",
        "none",
        true
    ),
    spec!(
        "update.install",
        ["update", "install"],
        "Install a CLI update",
        "update",
        "none",
        true
    ),
    spec!(
        "project.read",
        ["project", "read"],
        "Read the current Project",
        "project",
        "current-project",
        false
    ),
    spec!(
        "project.list",
        ["project", "list"],
        "List Projects",
        "project",
        "storage",
        false
    ),
    spec!(
        "project.create",
        ["project", "create"],
        "Create a Project",
        "project",
        "storage",
        true
    ),
    spec!(
        "project.activate",
        ["project", "activate"],
        "Activate a Project",
        "project",
        "storage",
        true
    ),
    spec!(
        "panel.list",
        ["panel", "list"],
        "List Panels",
        "panel",
        "current-project",
        false
    ),
    spec!(
        "panel.activate",
        ["panel", "activate"],
        "Activate a Panel",
        "panel",
        "current-project",
        true
    ),
    spec!(
        "panel.read",
        ["panel", "read"],
        "Read Panel summary or full state",
        "panel",
        "panel-kind-or-current",
        false
    ),
    spec!(
        "panel.selection.read",
        ["panel", "selection", "read"],
        "Read active Panel selection",
        "panel",
        "current-focus",
        false
    ),
    spec!(
        "canvas.selection.export",
        ["canvas", "selection", "export"],
        "Export explicit Canvas selection",
        "canvas",
        "current-selection",
        false,
        panel = "canvas"
    ),
    spec!(
        "canvas.image.create",
        ["canvas", "image", "create"],
        "Insert an image into Canvas",
        "canvas",
        "panel-kind",
        true,
        panel = "canvas"
    ),
    spec!(
        "canvas.image.generate",
        ["canvas", "image", "generate"],
        "Begin Canvas image generation",
        "canvas",
        "panel-kind",
        true,
        panel = "canvas"
    ),
    spec!(
        "wiki.raw.list",
        ["wiki", "raw", "list"],
        "List Wiki Raw Documents",
        "wiki",
        "panel-kind",
        false,
        panel = "wiki"
    ),
    spec!(
        "wiki.raw.create",
        ["wiki", "raw", "create"],
        "Create a Wiki Raw Document",
        "wiki",
        "panel-kind",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.raw.read",
        ["wiki", "raw", "read"],
        "Read Raw Document Markdown",
        "wiki",
        "panel-kind",
        false,
        panel = "wiki"
    ),
    spec!(
        "wiki.raw.update",
        ["wiki", "raw", "update"],
        "Write Raw Document Markdown",
        "wiki",
        "panel-kind-or-task",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.document.list",
        ["wiki", "document", "list"],
        "List Generated Documents",
        "wiki",
        "panel-kind",
        false,
        panel = "wiki"
    ),
    spec!(
        "wiki.document.create",
        ["wiki", "document", "create"],
        "Create a Generated Document",
        "wiki",
        "panel-kind-or-task",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.document.read",
        ["wiki", "document", "read"],
        "Read a Generated Document",
        "wiki",
        "panel-kind",
        false,
        panel = "wiki"
    ),
    spec!(
        "wiki.document.update",
        ["wiki", "document", "update"],
        "Update a Generated Document",
        "wiki",
        "panel-kind",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.document.delete",
        ["wiki", "document", "delete"],
        "Delete a Generated Document",
        "wiki",
        "panel-kind",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.document.publish",
        ["wiki", "document", "publish"],
        "Publish a Generated Document",
        "wiki",
        "panel-kind",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.space.list",
        ["wiki", "space", "list"],
        "List Wiki Spaces",
        "wiki",
        "panel-kind",
        false,
        panel = "wiki"
    ),
    spec!(
        "wiki.space.activate",
        ["wiki", "space", "activate"],
        "Activate a Wiki Space",
        "wiki",
        "panel-kind",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.page.list",
        ["wiki", "page", "list"],
        "List Wiki Pages",
        "wiki",
        "panel-kind",
        false,
        panel = "wiki"
    ),
    spec!(
        "wiki.page.search",
        ["wiki", "page", "search"],
        "Search Wiki Pages",
        "wiki",
        "panel-kind",
        false,
        panel = "wiki"
    ),
    spec!(
        "wiki.page.read",
        ["wiki", "page", "read"],
        "Read a Wiki Page",
        "wiki",
        "panel-kind",
        false,
        panel = "wiki"
    ),
    spec!(
        "wiki.page.create",
        ["wiki", "page", "create"],
        "Create a Wiki Page",
        "wiki",
        "panel-kind-or-task",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.page.update",
        ["wiki", "page", "update"],
        "Update a Wiki Page",
        "wiki",
        "panel-kind-or-task",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.document.generate",
        ["wiki", "document", "generate"],
        "Begin Wiki document generation",
        "wiki",
        "panel-kind",
        true,
        panel = "wiki"
    ),
    spec!(
        "writing.request.read",
        ["writing", "request", "read"],
        "Read a submitted Writing request",
        "writing",
        "task",
        false
    ),
    spec!(
        "writing.generate",
        ["writing", "generate"],
        "Begin Writing document generation",
        "writing",
        "task",
        true
    ),
    spec!(
        "writing.refinement.read",
        ["writing", "refinement", "read"],
        "Read a submitted Writing Skill refinement",
        "writing",
        "task",
        false
    ),
    spec!(
        "writing.skill.install",
        ["writing", "skill", "install"],
        "Install a refined shared Writing Skill",
        "writing",
        "task",
        true
    ),
    spec!(
        "task.list",
        ["task", "list"],
        "List Tasks",
        "task",
        "current-project",
        false
    ),
    spec!(
        "task.next",
        ["task", "next"],
        "Read the next Task",
        "task",
        "current-project",
        false
    ),
    spec!(
        "task.read",
        ["task", "read"],
        "Read a Task",
        "task",
        "task",
        false
    ),
    spec!(
        "task.claim-next",
        ["task", "claim-next"],
        "Claim the next Task",
        "task",
        "current-project",
        true
    ),
    spec!(
        "task.claim",
        ["task", "claim"],
        "Claim a Task",
        "task",
        "task",
        true
    ),
    spec!(
        "task.heartbeat",
        ["task", "heartbeat"],
        "Heartbeat a Task lease",
        "task",
        "task",
        true
    ),
    spec!(
        "task.complete",
        ["task", "complete"],
        "Complete a Task",
        "task",
        "task",
        true
    ),
    spec!(
        "task.fail",
        ["task", "fail"],
        "Fail a Task",
        "task",
        "task",
        true
    ),
    spec!(
        "task.release",
        ["task", "release"],
        "Release a Task",
        "task",
        "task",
        true
    ),
    spec!(
        "task.retry",
        ["task", "retry"],
        "Retry a Task",
        "task",
        "task",
        true
    ),
    spec!(
        "task.cancel",
        ["task", "cancel"],
        "Cancel a Task",
        "task",
        "task",
        true
    ),
    spec!(
        "task.archive",
        ["task", "archive"],
        "Archive a terminal Task",
        "task",
        "task",
        true
    ),
    spec!(
        "task.events",
        ["task", "events"],
        "List Task events",
        "task",
        "task",
        false
    ),
    spec!(
        "task.attempts",
        ["task", "attempts"],
        "List Task attempts",
        "task",
        "task",
        false
    ),
    spec!(
        "workflow.list",
        ["workflow", "list"],
        "List Workflows",
        "workflow",
        "current-project",
        false
    ),
    spec!(
        "workflow.read",
        ["workflow", "read"],
        "Read a Workflow DAG",
        "workflow",
        "current-project",
        false
    ),
    spec!(
        "operation.list",
        ["operation", "list"],
        "List Operations",
        "operation",
        "current-context",
        false
    ),
    spec!(
        "operation.read",
        ["operation", "read"],
        "Read an Operation",
        "operation",
        "operation",
        false
    ),
    spec!(
        "operation.complete",
        ["operation", "complete"],
        "Complete an Operation",
        "operation",
        "operation",
        true
    ),
    spec!(
        "operation.fail",
        ["operation", "fail"],
        "Fail an Operation",
        "operation",
        "operation",
        true
    ),
    spec!(
        "operation.cancel",
        ["operation", "cancel"],
        "Cancel an Operation",
        "operation",
        "operation",
        true
    ),
    spec!(
        "agent.bootstrap.read",
        ["agent", "bootstrap"],
        "Read Agent Bootstrap",
        "agent",
        "current-context",
        false
    ),
    spec!(
        "agent.catalog",
        ["agent", "catalog"],
        "Read the Agent command catalog",
        "agent",
        "none",
        false
    ),
    spec!(
        "agent.entry-skill.acknowledge",
        ["agent", "entry-skill", "acknowledge"],
        "Acknowledge the installed Entry Skill version",
        "agent",
        "current-context",
        true
    ),
    spec!(
        "agent.skill.list",
        ["agent", "skill", "list"],
        "List Agent Skills",
        "agent",
        "current-project",
        false
    ),
    spec!(
        "agent.skill.read",
        ["agent", "skill", "read"],
        "Read an Agent Skill",
        "agent",
        "current-project",
        false
    ),
    spec!(
        "agent.bridge.run",
        ["agent", "bridge", "run"],
        "Run the Agent task bridge",
        "agent",
        "current-project",
        true
    ),
    spec!(
        "agent.bridge.status",
        ["agent", "bridge", "status"],
        "Read Agent bridge status",
        "agent",
        "current-project",
        false
    ),
    spec!(
        "agent.target.list",
        ["agent", "target", "list"],
        "List Agent Targets",
        "agent",
        "current-project",
        false
    ),
    spec!(
        "agent.target.register",
        ["agent", "target", "register"],
        "Register an Agent Target",
        "agent",
        "current-project",
        true
    ),
    spec!(
        "agent.target.heartbeat",
        ["agent", "target", "heartbeat"],
        "Heartbeat an Agent Target",
        "agent",
        "current-project",
        true
    ),
    spec!(
        "agent.target.remove",
        ["agent", "target", "remove"],
        "Remove an Agent Target",
        "agent",
        "current-project",
        true
    ),
    spec!(
        "agent.route.list",
        ["agent", "route", "list"],
        "List Agent routes",
        "agent",
        "current-project",
        false
    ),
    spec!(
        "agent.route.set",
        ["agent", "route", "set"],
        "Set an Agent route",
        "agent",
        "current-project",
        true
    ),
    spec!(
        "agent.route.remove",
        ["agent", "route", "remove"],
        "Remove an Agent route",
        "agent",
        "current-project",
        true
    ),
];

pub(crate) fn command_action(command_id: CommandId, args: Vec<String>) -> Option<Value> {
    let CommandId::Catalog(index) = command_id else {
        return None;
    };
    let spec = SPECS.get(index)?;
    let mut argv = spec
        .path
        .iter()
        .map(|part| (*part).to_owned())
        .collect::<Vec<_>>();
    argv.extend(args);
    Some(json!({
        "intent": spec.intent,
        "executor": "cli",
        "argv": argv,
    }))
}

pub(crate) fn catalog(domain: Option<&str>) -> Option<Value> {
    debug_assert!(validate().is_ok());
    if let Some(domain) = domain {
        let commands = SPECS
            .iter()
            .filter(|spec| catalog_domain(spec) == Some(domain))
            .map(descriptor)
            .collect::<Vec<_>>();
        return (!commands.is_empty()).then(|| {
            json!({
                "schemaVersion": COMMAND_CATALOG_SCHEMA_VERSION,
                "catalogVersion": COMMAND_CATALOG_VERSION,
                "domain": domain,
                "commands": commands,
            })
        });
    }
    let mut counts = BTreeMap::<&str, usize>::new();
    for spec in SPECS.iter().filter(|spec| catalog_domain(spec).is_some()) {
        *counts.entry(catalog_domain(spec).unwrap()).or_default() += 1;
    }
    Some(json!({
        "schemaVersion": COMMAND_CATALOG_SCHEMA_VERSION,
        "catalogVersion": COMMAND_CATALOG_VERSION,
        "domains": counts.into_iter().map(|(domain, count)| json!({ "domain": domain, "count": count })).collect::<Vec<_>>(),
    }))
}

pub(crate) fn catalog_domain_for_intent(intent: &str) -> Option<&'static str> {
    SPECS
        .iter()
        .find(|spec| spec.intent == intent)
        .and_then(catalog_domain)
}

pub(crate) fn validate() -> Result<(), String> {
    let mut intents = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for spec in SPECS {
        if !intents.insert(spec.intent) {
            return Err(format!("Duplicate command intent: {}", spec.intent));
        }
        if !paths.insert(spec.path.join(" ")) {
            return Err(format!("Duplicate command path: {}", spec.path.join(" ")));
        }
        if find_leaf(&super::args::clap_command(), spec.path).is_none() {
            return Err(format!(
                "Command registry path does not parse: {}",
                spec.path.join(" ")
            ));
        }
    }
    let registered = SPECS
        .iter()
        .map(|spec| spec.path.join(" "))
        .collect::<BTreeSet<_>>();
    let mut public_leaves = BTreeSet::new();
    collect_public_leaves(
        &super::args::clap_command(),
        &mut Vec::new(),
        &mut public_leaves,
    );
    if registered != public_leaves {
        let missing = public_leaves
            .difference(&registered)
            .cloned()
            .collect::<Vec<_>>();
        let stale = registered
            .difference(&public_leaves)
            .cloned()
            .collect::<Vec<_>>();
        return Err(format!(
            "Command registry mismatch; missing: {}; stale: {}",
            missing.join(", "),
            stale.join(", ")
        ));
    }
    Ok(())
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_covers_every_public_clap_leaf_once() {
        validate().expect("complete command registry");
        let index = catalog(None).expect("catalog index");
        assert!(index["domains"]
            .as_array()
            .is_some_and(|domains| !domains.is_empty()));
        assert!(catalog(Some("studio")).is_none());

        let wiki = catalog(Some("wiki")).expect("wiki catalog");
        let commands = wiki["commands"].as_array().expect("commands");
        let delete = commands
            .iter()
            .find(|command| command["intent"] == "wiki.document.delete")
            .expect("document delete");
        assert_eq!(delete["risk"], "high-risk-write");
        let allowed = [
            "intent",
            "description",
            "argv",
            "args",
            "risk",
            "target",
            "retry",
        ];
        for command in commands {
            assert!(command
                .as_object()
                .unwrap()
                .keys()
                .all(|key| allowed.contains(&key.as_str())));
            assert_eq!(
                command["argv"].as_array().unwrap().last(),
                Some(&json!("json"))
            );
        }

        for spec in SPECS.iter().filter(|spec| catalog_domain(spec).is_some()) {
            let descriptor = descriptor(spec);
            let argv = descriptor["argv"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            assert!(
                matches!(
                    super::super::args::parse(&argv),
                    super::super::args::ParseOutcome::Invocation(_)
                ),
                "catalog argv must parse for {}: {argv:?}",
                spec.intent
            );
        }

        let canvas = catalog(Some("canvas")).expect("canvas catalog");
        let create = canvas["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|command| command["intent"] == "canvas.image.create")
            .unwrap();
        assert_eq!(create["target"]["mode"], "panel-kind");
        let selection = canvas["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|command| command["intent"] == "canvas.selection.export")
            .unwrap();
        assert_eq!(selection["target"]["mode"], "active-selection");
        for spec in SPECS {
            let mut argv = spec
                .path
                .iter()
                .map(|part| (*part).to_owned())
                .collect::<Vec<_>>();
            argv.push("--help".to_owned());
            assert!(
                matches!(
                    super::super::args::parse(&argv),
                    super::super::args::ParseOutcome::Display(_)
                ),
                "{} must expose Clap help",
                spec.path.join(" ")
            );
        }
    }
}
