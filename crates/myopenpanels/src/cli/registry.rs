use clap::{ArgAction, Command};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const COMMAND_CATALOG_VERSION: u32 = 3;

#[derive(Clone, Copy)]
struct CommandSpec {
    intent: &'static str,
    path: &'static [&'static str],
    title: &'static str,
    scope: &'static str,
    target_mode: &'static str,
    mutates: bool,
    required_panel_kind: Option<&'static str>,
    large_output: bool,
    agent_exposed: bool,
}

macro_rules! spec {
    ($intent:literal, [$($path:literal),+], $title:literal, $scope:literal, $target:literal, $mutates:literal) => {
        CommandSpec { intent: $intent, path: &[$($path),+], title: $title, scope: $scope, target_mode: $target, mutates: $mutates, required_panel_kind: None, large_output: false, agent_exposed: true }
    };
    ($intent:literal, [$($path:literal),+], $title:literal, $scope:literal, $target:literal, $mutates:literal, panel=$panel:literal) => {
        CommandSpec { intent: $intent, path: &[$($path),+], title: $title, scope: $scope, target_mode: $target, mutates: $mutates, required_panel_kind: Some($panel), large_output: false, agent_exposed: true }
    };
}

const SPECS: &[CommandSpec] = &[
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
        "project.current.read",
        ["project", "current"],
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
        "panel.current.read",
        ["panel", "current"],
        "Read the active Panel",
        "panel",
        "current-focus",
        false
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
        "panel.context.read",
        ["panel", "context", "read"],
        "Read compact Panel context",
        "panel",
        "current-focus",
        false
    ),
    CommandSpec {
        large_output: true,
        ..spec!(
            "panel.state.read",
            ["panel", "state", "read"],
            "Read raw Panel state",
            "panel",
            "current-focus",
            false
        )
    },
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
        "current-focus",
        false,
        panel = "canvas"
    ),
    spec!(
        "canvas.image.insert",
        ["canvas", "image", "insert"],
        "Insert an image into Canvas",
        "canvas",
        "current-focus",
        true,
        panel = "canvas"
    ),
    spec!(
        "canvas.generation.begin",
        ["canvas", "generation", "begin"],
        "Begin Canvas image generation",
        "canvas",
        "current-focus",
        true,
        panel = "canvas"
    ),
    spec!(
        "wiki.raw-document.list",
        ["wiki", "raw-document", "list"],
        "List Wiki Raw Documents",
        "wiki",
        "current-focus",
        false,
        panel = "wiki"
    ),
    spec!(
        "wiki.raw-document.add",
        ["wiki", "raw-document", "add"],
        "Add a Wiki Raw Document",
        "wiki",
        "current-focus",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.raw-document.create-markdown",
        ["wiki", "raw-document", "create-markdown"],
        "Create a Markdown Raw Document",
        "wiki",
        "current-focus",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.raw-document.markdown.read",
        ["wiki", "raw-document", "markdown", "read"],
        "Read Raw Document Markdown",
        "wiki",
        "current-focus",
        false,
        panel = "wiki"
    ),
    spec!(
        "wiki.raw-document.markdown.write",
        ["wiki", "raw-document", "markdown", "write"],
        "Write Raw Document Markdown",
        "wiki",
        "focus-or-task",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.generated-document.list",
        ["wiki", "generated-document", "list"],
        "List Generated Documents",
        "wiki",
        "current-focus",
        false,
        panel = "wiki"
    ),
    spec!(
        "wiki.generated-document.create",
        ["wiki", "generated-document", "create"],
        "Create a Generated Document",
        "wiki",
        "focus-or-task",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.generated-document.read",
        ["wiki", "generated-document", "read"],
        "Read a Generated Document",
        "wiki",
        "current-focus",
        false,
        panel = "wiki"
    ),
    spec!(
        "wiki.generated-document.write",
        ["wiki", "generated-document", "write"],
        "Write a Generated Document",
        "wiki",
        "current-focus",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.generated-document.rename",
        ["wiki", "generated-document", "rename"],
        "Rename a Generated Document",
        "wiki",
        "current-focus",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.generated-document.delete",
        ["wiki", "generated-document", "delete"],
        "Delete a Generated Document",
        "wiki",
        "current-focus",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.generated-document.publish",
        ["wiki", "generated-document", "publish"],
        "Publish a Generated Document",
        "wiki",
        "current-focus",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.space.list",
        ["wiki", "space", "list"],
        "List Wiki Spaces",
        "wiki",
        "current-focus",
        false,
        panel = "wiki"
    ),
    spec!(
        "wiki.space.activate",
        ["wiki", "space", "activate"],
        "Activate a Wiki Space",
        "wiki",
        "current-focus",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.page.list",
        ["wiki", "page", "list"],
        "List Wiki Pages",
        "wiki",
        "current-focus",
        false,
        panel = "wiki"
    ),
    spec!(
        "wiki.page.search",
        ["wiki", "page", "search"],
        "Search Wiki Pages",
        "wiki",
        "current-focus",
        false,
        panel = "wiki"
    ),
    spec!(
        "wiki.page.read",
        ["wiki", "page", "read"],
        "Read a Wiki Page",
        "wiki",
        "current-focus",
        false,
        panel = "wiki"
    ),
    spec!(
        "wiki.page.write",
        ["wiki", "page", "write"],
        "Write a Wiki Page",
        "wiki",
        "focus-or-task",
        true,
        panel = "wiki"
    ),
    spec!(
        "wiki.generation.begin",
        ["wiki", "generation", "begin"],
        "Begin Wiki document generation",
        "wiki",
        "current-focus",
        true,
        panel = "wiki"
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
        "task.delivery.list",
        ["task", "delivery", "list"],
        "List Task deliveries",
        "task",
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
        "agent.capability.list",
        ["agent", "capability", "list"],
        "List Agent capabilities",
        "agent",
        "none",
        false
    ),
    spec!(
        "agent.capability.read",
        ["agent", "capability", "read"],
        "Read one Agent capability",
        "agent",
        "none",
        false
    ),
    spec!(
        "agent.guide.list",
        ["agent", "guide", "list"],
        "List Agent Guides",
        "agent",
        "none",
        false
    ),
    spec!(
        "agent.guide.read",
        ["agent", "guide", "read"],
        "Read an Agent Guide",
        "agent",
        "current-project",
        false
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
];

pub(crate) fn capabilities() -> Vec<Value> {
    debug_assert!(validate().is_ok());
    SPECS
        .iter()
        .filter(|spec| spec.agent_exposed)
        .filter_map(descriptor)
        .collect()
}

pub(crate) fn command_action(intent: &str, args: Vec<String>) -> Option<Value> {
    let spec = SPECS.iter().find(|spec| spec.intent == intent)?;
    let mut argv = spec
        .path
        .iter()
        .map(|part| (*part).to_owned())
        .collect::<Vec<_>>();
    argv.extend(args);
    Some(json!({
        "intent": intent,
        "executor": "cli",
        "argv": argv,
    }))
}

pub(crate) fn scope_index() -> Value {
    debug_assert!(validate().is_ok());
    let mut counts = BTreeMap::<&str, usize>::new();
    for spec in SPECS.iter().filter(|spec| spec.agent_exposed) {
        *counts.entry(spec.scope).or_default() += 1;
    }
    let mut next_actions = Vec::new();
    let scopes = counts
        .into_iter()
        .map(|(scope, count)| {
            let mut action = command_action(
                "agent.capability.list",
                vec![
                    "--scope".to_owned(),
                    scope.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            )
            .expect("registered capability list action");
            action["loadWhen"] = json!(format!("The user request matches the {scope} scope."));
            next_actions.push(action);
            json!({
                "scope": scope,
                "count": count,
                "listCommand": format!("myopenpanels agent capability list --scope {scope} --format json"),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "catalogVersion": COMMAND_CATALOG_VERSION,
        "scopes": scopes,
        "nextActions": next_actions,
        "nextRequiredAction": {
            "intent": "select-capability-scope",
            "instruction": "Choose the nextActions entry whose loadWhen matches the user request.",
        },
    })
}

pub(crate) fn scope_capabilities(scope: &str) -> Option<Value> {
    let specs = SPECS
        .iter()
        .filter(|spec| spec.agent_exposed && spec.scope == scope)
        .collect::<Vec<_>>();
    if specs.is_empty() {
        return None;
    }
    let capabilities = specs.iter().map(|spec| summary(spec)).collect::<Vec<_>>();
    let next_actions = specs
        .iter()
        .map(|spec| {
            let mut action = command_action(
                "agent.capability.read",
                vec![
                    "--intent".to_owned(),
                    spec.intent.to_owned(),
                    "--format".to_owned(),
                    "json".to_owned(),
                ],
            )
            .expect("registered capability read action");
            action["loadWhen"] = json!(format!("The user request requires {}.", spec.title));
            action
        })
        .collect::<Vec<_>>();
    Some(json!({
        "catalogVersion": COMMAND_CATALOG_VERSION,
        "scope": scope,
        "capabilities": capabilities,
        "nextActions": next_actions,
        "nextRequiredAction": {
            "intent": "select-capability",
            "instruction": "Choose the nextActions entry whose loadWhen matches the user request.",
        },
    }))
}

pub(crate) fn capability_payload(intent: &str) -> Option<Value> {
    capability(intent).map(|capability| {
        json!({
            "catalogVersion": COMMAND_CATALOG_VERSION,
            "capability": capability,
            "nextActions": [],
            "nextRequiredAction": {
                "intent": "execute-capability",
                "instruction": "Use the capability command path and argument schema in this response. Supply values from the user request and current context, then execute it with the same resolved CLI executable.",
            },
        })
    })
}

pub(crate) fn capability(intent: &str) -> Option<Value> {
    SPECS
        .iter()
        .find(|spec| spec.intent == intent)
        .and_then(descriptor)
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

fn descriptor(spec: &CommandSpec) -> Option<Value> {
    let root = super::args::clap_command();
    let leaf = find_leaf(&root, spec.path)?;
    let args = leaf
        .get_arguments()
        .filter(|arg| {
            let id = arg.get_id().as_str();
            !matches!(id, "project_dir" | "storage_dir" | "context_id" | "format" | "help" | "version")
        })
        .map(|arg| {
            let values = arg
                .get_value_parser()
                .possible_values()
                .map(|values| values.map(|value| value.get_name().to_owned()).collect::<Vec<_>>())
                .unwrap_or_default();
            json!({
                "name": arg.get_id().as_str(),
                "flag": arg.get_long().map(|value| format!("--{value}")),
                "type": if matches!(arg.get_action(), ArgAction::SetTrue) { "bool" } else if values.is_empty() { "string" } else { "enum" },
                "required": arg.is_required_set(),
                "values": values,
            })
        })
        .collect::<Vec<_>>();
    Some(json!({
        "intent": spec.intent,
        "title": spec.title,
        "command": format!("myopenpanels {}", spec.path.join(" ")),
        "scope": spec.scope,
        "audience": "agent",
        "targetMode": spec.target_mode,
        "mutates": spec.mutates,
        "requiredPanelKind": spec.required_panel_kind,
        "largeOutput": spec.large_output,
        "args": args,
        "outputSchemaId": format!("myopenpanels.{}.v1", spec.intent),
        "preconditions": if spec.mutates && spec.target_mode == "current-focus" { json!(["matching-active-panel", "expected-focus-revision"]) } else { json!([]) },
    }))
}

fn summary(spec: &CommandSpec) -> Value {
    json!({
        "intent": spec.intent,
        "title": spec.title,
        "command": format!("myopenpanels {}", spec.path.join(" ")),
        "mutates": spec.mutates,
        "targetMode": spec.target_mode,
        "requiredPanelKind": spec.required_panel_kind,
        "largeOutput": spec.large_output,
        "readCommand": format!("myopenpanels agent capability read --intent {} --format json", spec.intent),
    })
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
        let capabilities = capabilities();
        assert_eq!(capabilities.len(), SPECS.len());
        assert!(capabilities.iter().any(|value| {
            value["intent"] == "panel.state.read" && value["largeOutput"] == true
        }));
        assert!(capability("agent.capability.read").is_some());
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
