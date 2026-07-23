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
        "release.list",
        ["release", "list"],
        "List Releases",
        "release",
        "current-project",
        false
    ),
    spec!(
        "release.checkpoint",
        ["release", "checkpoint"],
        "Checkpoint a Release attempt",
        "release",
        "task",
        true
    ),
    spec!(
        "publication.list",
        ["publication", "list"],
        "List Publications",
        "publication",
        "current-project",
        false
    ),
    spec!(
        "publication.title.skill.list",
        ["publication", "title", "skill", "list"],
        "List title generation Skills",
        "publication",
        "current-project",
        false
    ),
    spec!(
        "publication.title.generate",
        ["publication", "title", "generate"],
        "Create a title generation Task",
        "publication",
        "current-project",
        true
    ),
    spec!(
        "asset.list",
        ["asset", "list"],
        "List Project Assets",
        "asset",
        "current-project",
        false
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
        "my-document.list",
        ["my-document", "list"],
        "List My Documents",
        "my-document",
        "current-project",
        false
    ),
    spec!(
        "my-document.import",
        ["my-document", "import"],
        "Import a My Document",
        "my-document",
        "current-project",
        true
    ),
    spec!(
        "my-document.read",
        ["my-document", "read"],
        "Read a My Document",
        "my-document",
        "current-project",
        false
    ),
    spec!(
        "my-document.update",
        ["my-document", "update"],
        "Update a My Document",
        "my-document",
        "current-project",
        true
    ),
    spec!(
        "my-document.delete",
        ["my-document", "delete"],
        "Delete a My Document",
        "my-document",
        "current-project",
        true
    ),
    spec!(
        "wiki-source.create-from-my-document",
        ["wiki-source", "create-from-my-document"],
        "Create a Wiki Source from a My Document",
        "wiki-source",
        "current-project",
        true
    ),
    spec!(
        "my-document.create",
        ["my-document", "create"],
        "Begin My Document creation",
        "my-document",
        "current-project",
        true
    ),
    spec!(
        "my-document.revise",
        ["my-document", "revise"],
        "Begin My Document revision",
        "my-document",
        "current-project",
        true
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
        "wiki.space.materialize",
        ["wiki", "space", "materialize"],
        "Materialize a Wiki Space as local Markdown files",
        "wiki",
        "panel-kind",
        false,
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
        "writing.request.read",
        ["writing", "request", "read"],
        "Read a submitted Writing request",
        "writing",
        "task",
        false
    ),
    spec!(
        "writing.write",
        ["writing", "write"],
        "Begin My Document writing",
        "writing",
        "task",
        true
    ),
    spec!(
        "writing.distillation.read",
        ["writing", "distillation", "read"],
        "Read a submitted Writing Skill distillation",
        "writing",
        "task",
        false
    ),
    spec!(
        "writing.skill.install",
        ["writing", "skill", "install"],
        "Install a distilled shared Writing Skill",
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
        "task.handoff.start",
        ["task", "handoff", "start"],
        "Start a Task Handoff",
        "task",
        "task-handoff",
        true
    ),
    spec!(
        "task.handoff.exec",
        ["task", "handoff", "exec"],
        "Execute an allowed Task Handoff command",
        "task",
        "task-handoff",
        true
    ),
    spec!(
        "task.handoff.heartbeat",
        ["task", "handoff", "heartbeat"],
        "Heartbeat a Task Handoff",
        "task",
        "task-handoff",
        true
    ),
    spec!(
        "task.handoff.complete",
        ["task", "handoff", "complete"],
        "Complete and advance a Task Handoff",
        "task",
        "task-handoff",
        true
    ),
    spec!(
        "task.handoff.fail",
        ["task", "handoff", "fail"],
        "Fail and advance a Task Handoff",
        "task",
        "task-handoff",
        true
    ),
    spec!(
        "task.handoff.stop",
        ["task", "handoff", "stop"],
        "Stop a Task Handoff",
        "task",
        "task-handoff",
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
        "domains": counts.into_iter().map(|(domain, count)| json!({ "domain": domain, "count": count })).collect::<Vec<_>>(),
    }))
}

pub(crate) fn catalog_domain_for_intent(intent: &str) -> Option<&'static str> {
    SPECS
        .iter()
        .find(|spec| spec.intent == intent)
        .and_then(catalog_domain)
}

pub(crate) fn descriptors_for_intents(intents: &[String]) -> Result<Vec<Value>, CliError> {
    intents
        .iter()
        .map(|intent| {
            let spec = SPECS
                .iter()
                .find(|spec| spec.intent == intent)
                .ok_or_else(|| {
                    CliError::with_code(
                        "agent_procedure_command_not_found",
                        format!("Agent Procedure command intent is not registered: {intent}"),
                    )
                })?;
            Ok(descriptor(spec))
        })
        .collect()
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
