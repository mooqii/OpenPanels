use super::{FlagValue, Invocation};
use clap::{Args, Command, CommandFactory, Parser, Subcommand};
use std::collections::BTreeMap;

#[derive(Debug)]
pub(super) enum ParseOutcome {
    Invocation(Invocation),
    Display(String),
    Error(String),
}

#[derive(Debug, Parser)]
#[command(name = "myopenpanels", version = super::VERSION, disable_help_subcommand = true)]
pub(super) struct CliArgs {
    #[arg(long, global = true)]
    project_dir: Option<String>,
    #[arg(long, global = true, hide = true)]
    storage_dir: Option<String>,
    #[arg(long, global = true, hide = true)]
    context_id: Option<String>,
    #[arg(long, global = true, default_value = "text", value_parser = ["text", "json"])]
    format: String,
    #[command(subcommand)]
    command: RootCommand,
}

#[derive(Debug, Subcommand)]
enum RootCommand {
    Studio(StudioArgs),
    Update(UpdateArgs),
    Project(ProjectArgs),
    Panel(PanelArgs),
    Canvas(CanvasArgs),
    Wiki(WikiArgs),
    Writing(WritingArgs),
    Task(TaskArgs),
    Workflow(WorkflowArgs),
    Operation(OperationArgs),
    Agent(AgentArgs),
    Version,
    #[command(name = "__serve-studio", hide = true)]
    InternalServe(InternalServeArgs),
}

#[derive(Debug, Args)]
struct StudioArgs {
    #[command(subcommand)]
    command: StudioCommand,
}

#[derive(Debug, Subcommand)]
enum StudioCommand {
    Start(StudioStartArgs),
    Status,
    #[command(name = "open-system-browser")]
    OpenSystemBrowser(StudioStartArgs),
    Serve(StudioServeArgs),
    Wait(StudioWaitArgs),
    Stop,
}

#[derive(Debug, Args)]
struct StudioStartArgs {
    #[arg(long)]
    host: Option<String>,
    #[arg(long)]
    local_only: bool,
    #[arg(long)]
    static_dir: Option<String>,
}

#[derive(Debug, Args)]
struct StudioServeArgs {
    #[command(flatten)]
    launch: StudioStartArgs,
    #[arg(long)]
    port: Option<u16>,
}

#[derive(Debug, Args)]
struct StudioWaitArgs {
    #[arg(long)]
    timeout: Option<u64>,
}

#[derive(Debug, Args)]
struct InternalServeArgs {
    #[arg(long)]
    host: Option<String>,
    #[arg(long)]
    port: u16,
    #[arg(long)]
    static_dir: Option<String>,
    #[arg(long)]
    restart_delay_ms: Option<u64>,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Environment:\n  MYOPENPANELS_UPDATE_MANIFEST_URL  Override the release manifest URL\n  MYOPENPANELS_UPDATE_CACHE_DIR     Override the update cache directory\n  MYOPENPANELS_DISABLE_UPDATE_CHECK Disable opportunistic update checks"
)]
struct UpdateArgs {
    #[command(subcommand)]
    command: UpdateCommand,
}

#[derive(Debug, Subcommand)]
enum UpdateCommand {
    Check,
    Download,
    Install,
}

#[derive(Debug, Args)]
struct ProjectArgs {
    #[command(subcommand)]
    command: ProjectCommand,
}

#[derive(Debug, Subcommand)]
enum ProjectCommand {
    Read,
    List,
    Create {
        #[arg(long)]
        title: Option<String>,
    },
    Activate {
        #[arg(long)]
        project_id: String,
    },
}

#[derive(Debug, Args)]
struct PanelArgs {
    #[command(subcommand)]
    command: PanelCommand,
}

#[derive(Debug, Subcommand)]
enum PanelCommand {
    List,
    Activate {
        #[arg(long, value_parser = panel_kind_values())]
        panel_kind: String,
    },
    Read {
        #[arg(long, value_parser = panel_kind_values())]
        panel_kind: Option<String>,
        #[arg(long, default_value = "summary", value_parser = ["summary", "full"])]
        detail: String,
    },
    Selection(ReadArgs),
}

#[derive(Debug, Args)]
struct ReadArgs {
    #[command(subcommand)]
    command: ReadCommand,
}

#[derive(Debug, Subcommand)]
enum ReadCommand {
    Read,
}

#[derive(Debug, Args)]
struct CanvasArgs {
    #[command(subcommand)]
    command: CanvasCommand,
}

#[derive(Debug, Subcommand)]
enum CanvasCommand {
    Selection(CanvasSelectionArgs),
    Image(CanvasImageArgs),
}

#[derive(Debug, Args)]
struct CanvasSelectionArgs {
    #[command(subcommand)]
    command: CanvasSelectionCommand,
}

#[derive(Debug, Subcommand)]
enum CanvasSelectionCommand {
    Export {
        #[arg(long)]
        output_file: String,
    },
}

#[derive(Debug, Args)]
struct CanvasImageArgs {
    #[command(subcommand)]
    command: CanvasImageCommand,
}

#[derive(Debug, Subcommand)]
enum CanvasImageCommand {
    Create(CanvasImageCreateArgs),
    Generate(CanvasImageGenerateArgs),
}

#[derive(Debug, Args)]
struct CanvasImageCreateArgs {
    #[arg(long)]
    image_file: String,
    #[arg(long, default_value = "auto", value_parser = ["auto", "right", "below", "left"])]
    placement: String,
    #[arg(long)]
    metadata_file: Option<String>,
    #[arg(long)]
    replace_shape_id: Option<String>,
    #[arg(long)]
    anchor_shape_id: Option<String>,
    #[arg(long)]
    display_width: Option<f64>,
    #[arg(long)]
    display_height: Option<f64>,
    #[arg(long)]
    file_name: Option<String>,
}

#[derive(Debug, Args)]
struct CanvasImageGenerateArgs {
    #[arg(long)]
    display_width: Option<f64>,
    #[arg(long)]
    display_height: Option<f64>,
    #[arg(long)]
    use_selection: bool,
    #[arg(long)]
    text: Option<String>,
}

#[derive(Debug, Args)]
struct WikiArgs {
    #[command(subcommand)]
    command: WikiCommand,
}

#[derive(Debug, Subcommand)]
enum WikiCommand {
    Raw(WikiRawArgs),
    Document(WikiDocumentArgs),
    Space(WikiSpaceArgs),
    Page(WikiPageArgs),
}

#[derive(Debug, Args)]
struct WikiRawArgs {
    #[command(subcommand)]
    command: WikiRawCommand,
}

#[derive(Debug, Subcommand)]
enum WikiRawCommand {
    List,
    Create {
        #[arg(long, conflicts_with = "content", required_unless_present = "content")]
        source_file: Option<String>,
        #[arg(long, conflicts_with = "source_file")]
        content: Option<String>,
        #[arg(long)]
        file_name: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        mime_type: Option<String>,
        #[arg(long)]
        space_id: String,
    },
    Read {
        #[arg(long)]
        raw_document_id: String,
    },
    Update {
        #[arg(long)]
        raw_document_id: String,
        #[arg(long)]
        content_file: String,
        #[arg(long)]
        task_id: Option<String>,
    },
}

#[derive(Debug, Args)]
struct WikiDocumentArgs {
    #[command(subcommand)]
    command: WikiDocumentCommand,
}

#[derive(Debug, Subcommand)]
enum WikiDocumentCommand {
    List,
    Create {
        #[arg(long)]
        content_file: String,
        #[arg(long)]
        mime_type: Option<String>,
        #[arg(long)]
        task_id: Option<String>,
        #[arg(long)]
        thread_id: Option<String>,
        #[arg(long)]
        title: Option<String>,
    },
    Read {
        #[arg(long)]
        document_id: String,
    },
    Update {
        #[arg(long)]
        document_id: String,
        #[arg(long, required_unless_present = "title")]
        content_file: Option<String>,
        #[arg(long)]
        mime_type: Option<String>,
        #[arg(long)]
        title: Option<String>,
    },
    Delete {
        #[arg(long)]
        document_id: String,
    },
    Publish {
        #[arg(long)]
        document_id: String,
        #[arg(long)]
        space_id: String,
    },
    Generate {
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "markdown")]
        document_format: String,
        #[arg(long)]
        document_id: Option<String>,
    },
}

#[derive(Debug, Args)]
struct WikiSpaceArgs {
    #[command(subcommand)]
    command: WikiSpaceCommand,
}

#[derive(Debug, Subcommand)]
enum WikiSpaceCommand {
    List,
    Activate {
        #[arg(long)]
        space_id: String,
    },
}

#[derive(Debug, Args)]
struct WikiPageArgs {
    #[command(subcommand)]
    command: WikiPageCommand,
}

#[derive(Debug, Subcommand)]
enum WikiPageCommand {
    List {
        #[arg(long)]
        space_id: String,
    },
    Search {
        #[arg(long)]
        space_id: String,
        #[arg(long)]
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Read {
        #[arg(long)]
        space_id: String,
        #[arg(long)]
        path: String,
    },
    Create {
        #[arg(long)]
        space_id: String,
        #[arg(long)]
        path: String,
        #[arg(long)]
        content_file: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        task_id: Option<String>,
    },
    Update {
        #[arg(long)]
        space_id: String,
        #[arg(long)]
        path: String,
        #[arg(long)]
        content_file: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        task_id: Option<String>,
    },
}

#[derive(Debug, Args)]
struct WritingArgs {
    #[command(subcommand)]
    command: WritingCommand,
}

#[derive(Debug, Subcommand)]
enum WritingCommand {
    Request(WritingRequestArgs),
    Refinement(WritingRefinementArgs),
    Generate {
        #[arg(long)]
        task_id: String,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "markdown")]
        document_format: String,
    },
    Skill(WritingSkillArgs),
}

#[derive(Debug, Args)]
struct WritingRequestArgs {
    #[command(subcommand)]
    command: WritingRequestCommand,
}

#[derive(Debug, Subcommand)]
enum WritingRequestCommand {
    Read {
        #[arg(long)]
        task_id: String,
    },
}

#[derive(Debug, Args)]
struct WritingRefinementArgs {
    #[command(subcommand)]
    command: WritingRefinementCommand,
}

#[derive(Debug, Subcommand)]
enum WritingRefinementCommand {
    Read {
        #[arg(long)]
        task_id: String,
    },
}

#[derive(Debug, Args)]
struct WritingSkillArgs {
    #[command(subcommand)]
    command: WritingSkillCommand,
}

#[derive(Debug, Subcommand)]
enum WritingSkillCommand {
    Install {
        #[arg(long)]
        task_id: String,
        #[arg(long)]
        skill_file: String,
    },
}

#[derive(Debug, Args)]
struct TaskArgs {
    #[command(subcommand)]
    command: TaskCommand,
}

#[derive(Debug, Subcommand)]
enum TaskCommand {
    List(TaskFilterArgs),
    Next(TaskFilterArgs),
    Read(TaskIdArgs),
    #[command(name = "claim-next")]
    ClaimNext {
        #[arg(long)]
        target_id: String,
        #[arg(long)]
        capability: Vec<String>,
        #[arg(long)]
        wait_ms: Option<u64>,
    },
    Claim {
        #[command(flatten)]
        task: TaskIdArgs,
        #[arg(long)]
        target_id: String,
    },
    Heartbeat(TaskLeaseArgs),
    Complete {
        #[command(flatten)]
        lease: TaskLeaseArgs,
        #[arg(long)]
        result_file: Option<String>,
    },
    Fail {
        #[command(flatten)]
        lease: TaskLeaseArgs,
        #[arg(long)]
        message: String,
        #[arg(long)]
        retry_after: Option<String>,
        #[arg(long)]
        failure_class: Option<String>,
    },
    Release(TaskLeaseArgs),
    Retry(TaskIdArgs),
    Cancel(TaskIdArgs),
    Archive(TaskIdArgs),
    Events(TaskIdArgs),
    Attempts(TaskIdArgs),
}

#[derive(Debug, Args)]
struct WorkflowArgs {
    #[command(subcommand)]
    command: WorkflowCommand,
}

#[derive(Debug, Subcommand)]
enum WorkflowCommand {
    List,
    Read {
        #[arg(long)]
        workflow_id: String,
    },
}

#[derive(Debug, Args)]
struct TaskFilterArgs {
    #[arg(long)]
    pending: bool,
    #[arg(long)]
    queue: Option<String>,
    #[arg(long)]
    status: Option<String>,
}

#[derive(Debug, Args)]
struct TaskIdArgs {
    #[arg(long)]
    task_id: String,
}

#[derive(Debug, Args)]
struct TaskLeaseArgs {
    #[arg(long)]
    task_id: String,
    #[arg(long)]
    lease_token: String,
}

#[derive(Debug, Args)]
struct OperationArgs {
    #[command(subcommand)]
    command: OperationCommand,
}

#[derive(Debug, Subcommand)]
enum OperationCommand {
    List {
        #[arg(long)]
        status: Option<String>,
    },
    Read(OperationIdArgs),
    Complete {
        #[command(flatten)]
        operation: OperationIdArgs,
        #[arg(long)]
        artifact_file: String,
        #[arg(long)]
        metadata_file: Option<String>,
    },
    Fail {
        #[command(flatten)]
        operation: OperationIdArgs,
        #[arg(long)]
        message: String,
    },
    Cancel(OperationIdArgs),
}

#[derive(Debug, Args)]
struct OperationIdArgs {
    #[arg(long)]
    operation_id: String,
}

#[derive(Debug, Args)]
struct AgentArgs {
    #[command(subcommand)]
    command: AgentCommand,
}

#[derive(Debug, Subcommand)]
enum AgentCommand {
    Bootstrap,
    Catalog {
        #[arg(long)]
        domain: Option<String>,
    },
    EntrySkill(AgentEntrySkillArgs),
    Skill(AgentSkillArgs),
    Bridge(AgentBridgeArgs),
    Target(AgentTargetArgs),
    Route(AgentRouteArgs),
}

#[derive(Debug, Args)]
struct AgentRouteArgs {
    #[command(subcommand)]
    command: AgentRouteCommand,
}

#[derive(Debug, Subcommand)]
enum AgentRouteCommand {
    List,
    Set {
        #[arg(long)]
        capability: String,
        #[arg(long = "target-id")]
        target_ids: Vec<String>,
    },
    Remove {
        #[arg(long)]
        capability: String,
    },
}

#[derive(Debug, Args)]
struct AgentEntrySkillArgs {
    #[command(subcommand)]
    command: AgentEntrySkillCommand,
}

#[derive(Debug, Subcommand)]
enum AgentEntrySkillCommand {
    Acknowledge {
        #[arg(long)]
        event_id: String,
        #[arg(long)]
        installed_version: String,
    },
}

#[derive(Debug, Args)]
struct AgentSkillArgs {
    #[command(subcommand)]
    command: AgentSkillCommand,
}

#[derive(Debug, Subcommand)]
enum AgentSkillCommand {
    List {
        #[arg(long, value_parser = panel_kind_values())]
        panel_kind: Option<String>,
        #[arg(long)]
        task_type: Option<String>,
    },
    Read {
        #[arg(long)]
        skill_id: String,
        #[arg(long)]
        task_id: Option<String>,
    },
}

#[derive(Debug, Args)]
struct AgentBridgeArgs {
    #[command(subcommand)]
    command: AgentBridgeCommand,
}

#[derive(Debug, Subcommand)]
enum AgentBridgeCommand {
    Run(AgentBridgeRunArgs),
    Status,
}

#[derive(Debug, Args)]
struct AgentBridgeRunArgs {
    #[arg(long)]
    capability: Vec<String>,
    #[arg(long)]
    command: Option<String>,
    #[arg(long)]
    interval_ms: Option<u64>,
    #[arg(long)]
    manual_lifecycle: bool,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    once: bool,
    #[arg(long)]
    queue: Option<String>,
    #[arg(long)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Args)]
struct AgentTargetArgs {
    #[command(subcommand)]
    command: AgentTargetCommand,
}

#[derive(Debug, Subcommand)]
enum AgentTargetCommand {
    List,
    Register {
        #[arg(long)]
        name: String,
        #[arg(long)]
        host: Option<String>,
        #[arg(long, value_parser = ["poll", "command"])]
        transport: String,
        #[arg(long)]
        capability: Vec<String>,
        #[arg(long, default_value_t = 0, allow_hyphen_values = true)]
        priority: i64,
        #[arg(long, default_value_t = 2)]
        protocol_version: i64,
        #[arg(long, default_value_t = 1)]
        max_concurrency: i64,
    },
    Heartbeat {
        #[arg(long)]
        target_id: String,
    },
    Remove {
        #[arg(long)]
        target_id: String,
    },
}

pub(super) fn parse(argv: &[String]) -> ParseOutcome {
    if argv.is_empty() {
        return ParseOutcome::Display(clap_command().render_long_help().to_string());
    }
    let json = argv.windows(2).any(|parts| parts == ["--format", "json"])
        || argv.iter().any(|arg| arg == "--format=json");
    if argv.iter().any(|arg| arg == "--version") {
        let mut flags = BTreeMap::new();
        if json {
            put(&mut flags, "format", Some("json".to_owned()));
        }
        return ParseOutcome::Invocation(Invocation {
            command_id: super::registry::CommandId::from_intent("cli.version.read")
                .expect("version command is registered"),
            flags,
            positionals: vec!["version".to_owned()],
        });
    }
    let mut args = Vec::with_capacity(argv.len() + 1);
    args.push("myopenpanels".to_owned());
    args.extend(argv.iter().cloned());
    match CliArgs::try_parse_from(args) {
        Ok(cli) => ParseOutcome::Invocation(normalize(cli)),
        Err(error)
            if error.kind() == clap::error::ErrorKind::DisplayHelp
                || error.kind() == clap::error::ErrorKind::DisplayVersion =>
        {
            ParseOutcome::Display(error.to_string())
        }
        Err(error) => ParseOutcome::Error(error.to_string()),
    }
}

pub(super) fn clap_command() -> Command {
    CliArgs::command()
}

fn normalize(cli: CliArgs) -> Invocation {
    let mut flags = BTreeMap::new();
    put(&mut flags, "project-dir", cli.project_dir);
    put(&mut flags, "storage-dir", cli.storage_dir);
    put(&mut flags, "context-id", cli.context_id);
    put(&mut flags, "format", Some(cli.format));
    let (positionals, intent) = normalize_command(cli.command, &mut flags);
    Invocation {
        command_id: super::registry::CommandId::from_intent(intent)
            .expect("normalized command is registered"),
        flags,
        positionals,
    }
}

fn normalize_command(
    command: RootCommand,
    flags: &mut BTreeMap<String, FlagValue>,
) -> (Vec<String>, &'static str) {
    match command {
        RootCommand::Version => (vec!["version".into()], "cli.version.read"),
        RootCommand::InternalServe(args) => {
            put(flags, "host", args.host);
            put(flags, "port", Some(args.port.to_string()));
            put(flags, "static-dir", args.static_dir);
            put_num(flags, "restart-delay-ms", args.restart_delay_ms);
            (vec!["__serve-studio".into()], "internal.studio.serve")
        }
        RootCommand::Studio(args) => normalize_studio(args.command, flags),
        RootCommand::Update(args) => {
            let action = match args.command {
                UpdateCommand::Check => "check",
                UpdateCommand::Download => "download",
                UpdateCommand::Install => "install",
            };
            (
                vec!["update".into(), action.into()],
                match action {
                    "check" => "update.check",
                    "download" => "update.download",
                    _ => "update.install",
                },
            )
        }
        RootCommand::Project(args) => normalize_project(args.command, flags),
        RootCommand::Panel(args) => normalize_panel(args.command, flags),
        RootCommand::Canvas(args) => normalize_canvas(args.command, flags),
        RootCommand::Wiki(args) => normalize_wiki(args.command, flags),
        RootCommand::Writing(args) => normalize_writing(args.command, flags),
        RootCommand::Task(args) => normalize_task(args.command, flags),
        RootCommand::Workflow(args) => match args.command {
            WorkflowCommand::List => (vec!["workflow".into(), "list".into()], "workflow.list"),
            WorkflowCommand::Read { workflow_id } => {
                put(flags, "workflow-id", Some(workflow_id));
                (vec!["workflow".into(), "read".into()], "workflow.read")
            }
        },
        RootCommand::Operation(args) => normalize_operation(args.command, flags),
        RootCommand::Agent(args) => normalize_agent(args.command, flags),
    }
}

fn normalize_writing(
    command: WritingCommand,
    flags: &mut BTreeMap<String, FlagValue>,
) -> (Vec<String>, &'static str) {
    match command {
        WritingCommand::Request(args) => match args.command {
            WritingRequestCommand::Read { task_id } => {
                put(flags, "task-id", Some(task_id));
                (
                    vec!["writing".into(), "request".into(), "read".into()],
                    "writing.request.read",
                )
            }
        },
        WritingCommand::Refinement(args) => match args.command {
            WritingRefinementCommand::Read { task_id } => {
                put(flags, "task-id", Some(task_id));
                (
                    vec!["writing".into(), "refinement".into(), "read".into()],
                    "writing.refinement.read",
                )
            }
        },
        WritingCommand::Generate {
            task_id,
            title,
            document_format,
        } => {
            put(flags, "task-id", Some(task_id));
            put(flags, "title", Some(title));
            put(flags, "document-format", Some(document_format));
            (
                vec!["writing".into(), "generate".into()],
                "writing.generate",
            )
        }
        WritingCommand::Skill(args) => match args.command {
            WritingSkillCommand::Install {
                task_id,
                skill_file,
            } => {
                put(flags, "task-id", Some(task_id));
                put(flags, "skill-file", Some(skill_file));
                (
                    vec!["writing".into(), "skill".into(), "install".into()],
                    "writing.skill.install",
                )
            }
        },
    }
}

fn normalize_studio(
    command: StudioCommand,
    flags: &mut BTreeMap<String, FlagValue>,
) -> (Vec<String>, &'static str) {
    let (action, intent) = match &command {
        StudioCommand::Start(_) => ("start", "studio.start"),
        StudioCommand::Status => ("status", "studio.status"),
        StudioCommand::OpenSystemBrowser(_) => {
            ("open-system-browser", "studio.open-system-browser")
        }
        StudioCommand::Serve(_) => ("serve", "studio.serve"),
        StudioCommand::Wait(_) => ("wait", "studio.wait"),
        StudioCommand::Stop => ("stop", "studio.stop"),
    };
    match command {
        StudioCommand::Start(args) | StudioCommand::OpenSystemBrowser(args) => {
            studio_start_flags(flags, args)
        }
        StudioCommand::Serve(args) => {
            studio_start_flags(flags, args.launch);
            put_num(flags, "port", args.port);
        }
        StudioCommand::Wait(args) => put_num(flags, "timeout", args.timeout),
        StudioCommand::Status | StudioCommand::Stop => {}
    }
    (vec!["studio".into(), action.into()], intent)
}

fn studio_start_flags(flags: &mut BTreeMap<String, FlagValue>, args: StudioStartArgs) {
    put(flags, "host", args.host);
    put_bool(flags, "local-only", args.local_only);
    put(flags, "static-dir", args.static_dir);
}

fn normalize_project(
    command: ProjectCommand,
    flags: &mut BTreeMap<String, FlagValue>,
) -> (Vec<String>, &'static str) {
    match command {
        ProjectCommand::Read => (vec!["project".into(), "read".into()], "project.read"),
        ProjectCommand::List => (vec!["project".into(), "list".into()], "project.list"),
        ProjectCommand::Create { title } => {
            put(flags, "title", title);
            (vec!["project".into(), "create".into()], "project.create")
        }
        ProjectCommand::Activate { project_id } => {
            put(flags, "project-id", Some(project_id));
            (
                vec!["project".into(), "activate".into()],
                "project.activate",
            )
        }
    }
}

fn normalize_panel(
    command: PanelCommand,
    flags: &mut BTreeMap<String, FlagValue>,
) -> (Vec<String>, &'static str) {
    match command {
        PanelCommand::List => (vec!["panel".into(), "list".into()], "panel.list"),
        PanelCommand::Activate { panel_kind } => {
            put(flags, "panel-kind", Some(panel_kind));
            (vec!["panel".into(), "activate".into()], "panel.activate")
        }
        PanelCommand::Read { panel_kind, detail } => {
            put(flags, "panel-kind", panel_kind);
            put(flags, "detail", Some(detail));
            (vec!["panel".into(), "read".into()], "panel.read")
        }
        PanelCommand::Selection(_) => (
            vec!["panel".into(), "selection".into(), "read".into()],
            "panel.selection.read",
        ),
    }
}

fn normalize_canvas(
    command: CanvasCommand,
    flags: &mut BTreeMap<String, FlagValue>,
) -> (Vec<String>, &'static str) {
    match command {
        CanvasCommand::Selection(args) => match args.command {
            CanvasSelectionCommand::Export { output_file } => {
                put(flags, "output-file", Some(output_file));
                (
                    vec!["canvas".into(), "selection".into(), "export".into()],
                    "canvas.selection.export",
                )
            }
        },
        CanvasCommand::Image(args) => match args.command {
            CanvasImageCommand::Create(args) => {
                put(flags, "image-file", Some(args.image_file));
                put(flags, "placement", Some(args.placement));
                put(flags, "metadata-file", args.metadata_file);
                put(flags, "replace-shape-id", args.replace_shape_id);
                put(flags, "anchor-shape-id", args.anchor_shape_id);
                put_num(flags, "display-width", args.display_width);
                put_num(flags, "display-height", args.display_height);
                put(flags, "file-name", args.file_name);
                (
                    vec!["canvas".into(), "image".into(), "create".into()],
                    "canvas.image.create",
                )
            }
            CanvasImageCommand::Generate(args) => {
                put_num(flags, "display-width", args.display_width);
                put_num(flags, "display-height", args.display_height);
                put_bool(flags, "use-selection", args.use_selection);
                put(flags, "text", args.text);
                (
                    vec!["canvas".into(), "image".into(), "generate".into()],
                    "canvas.image.generate",
                )
            }
        },
    }
}

fn normalize_wiki(
    command: WikiCommand,
    flags: &mut BTreeMap<String, FlagValue>,
) -> (Vec<String>, &'static str) {
    match command {
        WikiCommand::Raw(args) => normalize_wiki_raw(args.command, flags),
        WikiCommand::Document(args) => normalize_wiki_document(args.command, flags),
        WikiCommand::Space(args) => match args.command {
            WikiSpaceCommand::List => (
                vec!["wiki".into(), "space".into(), "list".into()],
                "wiki.space.list",
            ),
            WikiSpaceCommand::Activate { space_id } => {
                put(flags, "space-id", Some(space_id));
                (
                    vec!["wiki".into(), "space".into(), "activate".into()],
                    "wiki.space.activate",
                )
            }
        },
        WikiCommand::Page(args) => normalize_wiki_page(args.command, flags),
    }
}

fn normalize_wiki_raw(
    command: WikiRawCommand,
    flags: &mut BTreeMap<String, FlagValue>,
) -> (Vec<String>, &'static str) {
    match command {
        WikiRawCommand::List => (
            vec!["wiki".into(), "raw".into(), "list".into()],
            "wiki.raw.list",
        ),
        WikiRawCommand::Create {
            source_file,
            content,
            file_name,
            title,
            mime_type,
            space_id,
        } => {
            put(flags, "source-file", source_file);
            put(flags, "content", content);
            put(flags, "file-name", file_name);
            put(flags, "title", title);
            put(flags, "mime-type", mime_type);
            put(flags, "space-id", Some(space_id));
            (
                vec!["wiki".into(), "raw".into(), "create".into()],
                "wiki.raw.create",
            )
        }
        WikiRawCommand::Read { raw_document_id } => {
            put(flags, "raw-document-id", Some(raw_document_id));
            (
                vec!["wiki".into(), "raw".into(), "read".into()],
                "wiki.raw.read",
            )
        }
        WikiRawCommand::Update {
            raw_document_id,
            content_file,
            task_id,
        } => {
            put(flags, "raw-document-id", Some(raw_document_id));
            put(flags, "content-file", Some(content_file));
            put(flags, "task-id", task_id);
            (
                vec!["wiki".into(), "raw".into(), "update".into()],
                "wiki.raw.update",
            )
        }
    }
}

fn normalize_wiki_document(
    command: WikiDocumentCommand,
    flags: &mut BTreeMap<String, FlagValue>,
) -> (Vec<String>, &'static str) {
    let base = vec!["wiki".to_owned(), "document".to_owned()];
    match command {
        WikiDocumentCommand::List => (with_action(&base, "list"), "wiki.document.list"),
        WikiDocumentCommand::Create {
            content_file,
            mime_type,
            task_id,
            thread_id,
            title,
        } => {
            put(flags, "content-file", Some(content_file));
            put(flags, "mime-type", mime_type);
            put(flags, "task-id", task_id);
            put(flags, "thread-id", thread_id);
            put(flags, "title", title);
            (with_action(&base, "create"), "wiki.document.create")
        }
        WikiDocumentCommand::Read { document_id } => {
            put(flags, "document-id", Some(document_id));
            (with_action(&base, "read"), "wiki.document.read")
        }
        WikiDocumentCommand::Update {
            document_id,
            content_file,
            mime_type,
            title,
        } => {
            put(flags, "document-id", Some(document_id));
            put(flags, "content-file", content_file);
            put(flags, "mime-type", mime_type);
            put(flags, "title", title);
            (with_action(&base, "update"), "wiki.document.update")
        }
        WikiDocumentCommand::Delete { document_id } => {
            put(flags, "document-id", Some(document_id));
            (with_action(&base, "delete"), "wiki.document.delete")
        }
        WikiDocumentCommand::Publish {
            document_id,
            space_id,
        } => {
            put(flags, "document-id", Some(document_id));
            put(flags, "space-id", Some(space_id));
            (with_action(&base, "publish"), "wiki.document.publish")
        }
        WikiDocumentCommand::Generate {
            title,
            document_format,
            document_id,
        } => {
            put(flags, "title", Some(title));
            put(flags, "document-format", Some(document_format));
            put(flags, "document-id", document_id);
            (with_action(&base, "generate"), "wiki.document.generate")
        }
    }
}

fn normalize_wiki_page(
    command: WikiPageCommand,
    flags: &mut BTreeMap<String, FlagValue>,
) -> (Vec<String>, &'static str) {
    let base = vec!["wiki".to_owned(), "page".to_owned()];
    match command {
        WikiPageCommand::List { space_id } => {
            put(flags, "space-id", Some(space_id));
            (with_action(&base, "list"), "wiki.page.list")
        }
        WikiPageCommand::Search {
            space_id,
            query,
            limit,
        } => {
            put(flags, "space-id", Some(space_id));
            put(flags, "query", Some(query));
            put_num(flags, "limit", Some(limit));
            (with_action(&base, "search"), "wiki.page.search")
        }
        WikiPageCommand::Read { space_id, path } => {
            put(flags, "space-id", Some(space_id));
            put(flags, "path", Some(path));
            (with_action(&base, "read"), "wiki.page.read")
        }
        WikiPageCommand::Create {
            space_id,
            path,
            content_file,
            title,
            task_id,
        } => {
            put(flags, "space-id", Some(space_id));
            put(flags, "path", Some(path));
            put(flags, "content-file", Some(content_file));
            put(flags, "title", title);
            put(flags, "task-id", task_id);
            (with_action(&base, "create"), "wiki.page.create")
        }
        WikiPageCommand::Update {
            space_id,
            path,
            content_file,
            title,
            task_id,
        } => {
            put(flags, "space-id", Some(space_id));
            put(flags, "path", Some(path));
            put(flags, "content-file", Some(content_file));
            put(flags, "title", title);
            put(flags, "task-id", task_id);
            (with_action(&base, "update"), "wiki.page.update")
        }
    }
}

fn normalize_task(
    command: TaskCommand,
    flags: &mut BTreeMap<String, FlagValue>,
) -> (Vec<String>, &'static str) {
    let (action, intent) = match command {
        TaskCommand::List(args) => {
            task_filter(flags, args);
            ("list", "task.list")
        }
        TaskCommand::Next(args) => {
            task_filter(flags, args);
            ("next", "task.next")
        }
        TaskCommand::Read(args) => {
            task_id(flags, args);
            ("read", "task.read")
        }
        TaskCommand::ClaimNext {
            target_id,
            capability,
            wait_ms,
        } => {
            put(flags, "target-id", Some(target_id));
            put_many(flags, "capability", capability);
            put_num(flags, "wait-ms", wait_ms);
            ("claim-next", "task.claim-next")
        }
        TaskCommand::Claim { task, target_id } => {
            task_id(flags, task);
            put(flags, "target-id", Some(target_id));
            ("claim", "task.claim")
        }
        TaskCommand::Heartbeat(args) => {
            task_lease(flags, args);
            ("heartbeat", "task.heartbeat")
        }
        TaskCommand::Complete { lease, result_file } => {
            task_lease(flags, lease);
            put(flags, "result-file", result_file);
            ("complete", "task.complete")
        }
        TaskCommand::Fail {
            lease,
            message,
            retry_after,
            failure_class,
        } => {
            task_lease(flags, lease);
            put(flags, "message", Some(message));
            put(flags, "retry-after", retry_after);
            put(flags, "failure-class", failure_class);
            ("fail", "task.fail")
        }
        TaskCommand::Release(args) => {
            task_lease(flags, args);
            ("release", "task.release")
        }
        TaskCommand::Retry(args) => {
            task_id(flags, args);
            ("retry", "task.retry")
        }
        TaskCommand::Cancel(args) => {
            task_id(flags, args);
            ("cancel", "task.cancel")
        }
        TaskCommand::Archive(args) => {
            task_id(flags, args);
            ("archive", "task.archive")
        }
        TaskCommand::Events(args) => {
            task_id(flags, args);
            ("events", "task.events")
        }
        TaskCommand::Attempts(args) => {
            task_id(flags, args);
            ("attempts", "task.attempts")
        }
    };
    (vec!["task".into(), action.into()], intent)
}

fn normalize_operation(
    command: OperationCommand,
    flags: &mut BTreeMap<String, FlagValue>,
) -> (Vec<String>, &'static str) {
    let action = match command {
        OperationCommand::List { status } => {
            put(flags, "status", status);
            "list"
        }
        OperationCommand::Read(args) => {
            operation_id(flags, args);
            "read"
        }
        OperationCommand::Complete {
            operation,
            artifact_file,
            metadata_file,
        } => {
            operation_id(flags, operation);
            put(flags, "artifact-file", Some(artifact_file));
            put(flags, "metadata-file", metadata_file);
            "complete"
        }
        OperationCommand::Fail { operation, message } => {
            operation_id(flags, operation);
            put(flags, "message", Some(message));
            "fail"
        }
        OperationCommand::Cancel(args) => {
            operation_id(flags, args);
            "cancel"
        }
    };
    let intent = match action {
        "list" => "operation.list",
        "read" => "operation.read",
        "complete" => "operation.complete",
        "fail" => "operation.fail",
        _ => "operation.cancel",
    };
    (vec!["operation".into(), action.into()], intent)
}

fn normalize_agent(
    command: AgentCommand,
    flags: &mut BTreeMap<String, FlagValue>,
) -> (Vec<String>, &'static str) {
    match command {
        AgentCommand::Bootstrap => (
            vec!["agent".into(), "bootstrap".into()],
            "agent.bootstrap.read",
        ),
        AgentCommand::Catalog { domain } => {
            put(flags, "domain", domain);
            (vec!["agent".into(), "catalog".into()], "agent.catalog")
        }
        AgentCommand::EntrySkill(args) => match args.command {
            AgentEntrySkillCommand::Acknowledge {
                event_id,
                installed_version,
            } => {
                put(flags, "event-id", Some(event_id));
                put(flags, "installed-version", Some(installed_version));
                (
                    vec!["agent".into(), "entry-skill".into(), "acknowledge".into()],
                    "agent.entry-skill.acknowledge",
                )
            }
        },
        AgentCommand::Skill(args) => match args.command {
            AgentSkillCommand::List {
                panel_kind,
                task_type,
            } => {
                put(flags, "panel-kind", panel_kind);
                put(flags, "task-type", task_type);
                (
                    vec!["agent".into(), "skill".into(), "list".into()],
                    "agent.skill.list",
                )
            }
            AgentSkillCommand::Read { skill_id, task_id } => {
                put(flags, "skill-id", Some(skill_id));
                put(flags, "task-id", task_id);
                (
                    vec!["agent".into(), "skill".into(), "read".into()],
                    "agent.skill.read",
                )
            }
        },
        AgentCommand::Bridge(args) => match args.command {
            AgentBridgeCommand::Status => (
                vec!["agent".into(), "bridge".into(), "status".into()],
                "agent.bridge.status",
            ),
            AgentBridgeCommand::Run(args) => {
                put_many(flags, "capability", args.capability);
                put(flags, "command", args.command);
                put_num(flags, "interval-ms", args.interval_ms);
                put_bool(flags, "manual-lifecycle", args.manual_lifecycle);
                put(flags, "name", args.name);
                put_bool(flags, "once", args.once);
                put(flags, "queue", args.queue);
                put_num(flags, "timeout-ms", args.timeout_ms);
                (
                    vec!["agent".into(), "bridge".into(), "run".into()],
                    "agent.bridge.run",
                )
            }
        },
        AgentCommand::Target(args) => match args.command {
            AgentTargetCommand::List => (
                vec!["agent".into(), "target".into(), "list".into()],
                "agent.target.list",
            ),
            AgentTargetCommand::Register {
                name,
                host,
                transport,
                capability,
                priority,
                protocol_version,
                max_concurrency,
            } => {
                put(flags, "name", Some(name));
                put(flags, "host", host);
                put(flags, "transport", Some(transport));
                put_many(flags, "capability", capability);
                put_num(flags, "priority", Some(priority));
                put_num(flags, "protocol-version", Some(protocol_version));
                put_num(flags, "max-concurrency", Some(max_concurrency));
                (
                    vec!["agent".into(), "target".into(), "register".into()],
                    "agent.target.register",
                )
            }
            AgentTargetCommand::Heartbeat { target_id } => {
                put(flags, "target-id", Some(target_id));
                (
                    vec!["agent".into(), "target".into(), "heartbeat".into()],
                    "agent.target.heartbeat",
                )
            }
            AgentTargetCommand::Remove { target_id } => {
                put(flags, "target-id", Some(target_id));
                (
                    vec!["agent".into(), "target".into(), "remove".into()],
                    "agent.target.remove",
                )
            }
        },
        AgentCommand::Route(args) => match args.command {
            AgentRouteCommand::List => (
                vec!["agent".into(), "route".into(), "list".into()],
                "agent.route.list",
            ),
            AgentRouteCommand::Set {
                capability,
                target_ids,
            } => {
                put(flags, "capability", Some(capability));
                put_many(flags, "target-id", target_ids);
                (
                    vec!["agent".into(), "route".into(), "set".into()],
                    "agent.route.set",
                )
            }
            AgentRouteCommand::Remove { capability } => {
                put(flags, "capability", Some(capability));
                (
                    vec!["agent".into(), "route".into(), "remove".into()],
                    "agent.route.remove",
                )
            }
        },
    }
}

fn task_filter(flags: &mut BTreeMap<String, FlagValue>, args: TaskFilterArgs) {
    put_bool(flags, "pending", args.pending);
    put(flags, "queue", args.queue);
    put(flags, "status", args.status);
}

fn task_id(flags: &mut BTreeMap<String, FlagValue>, args: TaskIdArgs) {
    put(flags, "task-id", Some(args.task_id));
}
fn task_lease(flags: &mut BTreeMap<String, FlagValue>, args: TaskLeaseArgs) {
    put(flags, "task-id", Some(args.task_id));
    put(flags, "lease-token", Some(args.lease_token));
}
fn operation_id(flags: &mut BTreeMap<String, FlagValue>, args: OperationIdArgs) {
    put(flags, "operation-id", Some(args.operation_id));
}

fn with_action(base: &[String], action: &str) -> Vec<String> {
    let mut value = base.to_vec();
    value.push(action.to_owned());
    value
}

fn put(flags: &mut BTreeMap<String, FlagValue>, name: &str, value: Option<String>) {
    if let Some(value) = value {
        flags.insert(name.to_owned(), FlagValue::String(value));
    }
}

fn put_bool(flags: &mut BTreeMap<String, FlagValue>, name: &str, value: bool) {
    if value {
        flags.insert(name.to_owned(), FlagValue::Bool);
    }
}

fn put_num<T: ToString>(flags: &mut BTreeMap<String, FlagValue>, name: &str, value: Option<T>) {
    put(flags, name, value.map(|value| value.to_string()));
}

fn put_many(flags: &mut BTreeMap<String, FlagValue>, name: &str, values: Vec<String>) {
    if !values.is_empty() {
        put(flags, name, Some(values.join(",")));
    }
}

fn panel_kind_values() -> [&'static str; 5] {
    ["wiki", "writing", "canvas", "typesetting", "publishing"]
}
