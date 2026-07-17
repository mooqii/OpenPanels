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
        #[arg(long, default_value_t = 3)]
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
