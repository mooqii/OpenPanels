#[derive(Debug, Args)]
struct WritingArgs {
    #[command(subcommand)]
    command: WritingCommand,
}

#[derive(Debug, Subcommand)]
enum WritingCommand {
    Request(WritingRequestArgs),
    Distillation(WritingDistillationArgs),
    Write {
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
struct WritingDistillationArgs {
    #[command(subcommand)]
    command: WritingDistillationCommand,
}

#[derive(Debug, Subcommand)]
enum WritingDistillationCommand {
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
struct PublicationArgs {
    #[command(subcommand)]
    command: PublicationCommand,
}

#[derive(Debug, Subcommand)]
enum PublicationCommand {
    List,
    Title(PublicationTitleArgs),
}

#[derive(Debug, Args)]
struct PublicationTitleArgs {
    #[command(subcommand)]
    command: PublicationTitleCommand,
}

#[derive(Debug, Subcommand)]
enum PublicationTitleCommand {
    Generate {
        #[arg(long)]
        publication_id: String,
        #[arg(long, default_value = "publication-title-default")]
        skill_id: String,
        #[arg(long)]
        instruction: Option<String>,
        #[arg(long)]
        request_id: Option<String>,
    },
    Skill(PublicationTitleSkillArgs),
}

#[derive(Debug, Args)]
struct PublicationTitleSkillArgs {
    #[command(subcommand)]
    command: PublicationTitleSkillCommand,
}

#[derive(Debug, Subcommand)]
enum PublicationTitleSkillCommand {
    List,
}

#[derive(Debug, Args)]
struct ReleaseArgs {
    #[command(subcommand)]
    command: ReleaseCommand,
}

#[derive(Debug, Subcommand)]
enum ReleaseCommand {
    List,
    Checkpoint {
        #[arg(long)]
        task_id: String,
        #[arg(long, value_parser = ["prepared", "committing"])]
        phase: String,
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
    Handoff(TaskHandoffArgs),
    Retry(TaskIdArgs),
    Cancel(TaskIdArgs),
    Archive(TaskIdArgs),
}

#[derive(Debug, Args)]
struct TaskHandoffArgs {
    #[command(subcommand)]
    command: TaskHandoffCommand,
}

#[derive(Debug, Subcommand)]
enum TaskHandoffCommand {
    Start(TaskScopeSelectorArgs),
    Exec {
        #[arg(long)]
        handoff_id: String,
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    Heartbeat {
        #[arg(long)]
        handoff_id: String,
    },
    Complete {
        #[arg(long)]
        handoff_id: String,
    },
    Fail {
        #[arg(long)]
        handoff_id: String,
        #[arg(long)]
        message: String,
        #[arg(long)]
        failure_class: Option<String>,
    },
    Stop {
        #[arg(long)]
        handoff_id: String,
    },
}

#[derive(Debug, Args)]
struct TaskScopeSelectorArgs {
    #[arg(long, value_parser = ["project-drain", "exact-task", "wiki-mutation-drain"])]
    scope: String,
    #[arg(long)]
    project_id: Option<String>,
    #[arg(long)]
    task_id: Option<String>,
    #[arg(long)]
    mutation_key: Option<String>,
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
    Bootstrap {
        #[arg(long)]
        procedure: Option<String>,
    },
    Catalog {
        #[arg(long)]
        domain: Option<String>,
    },
    EntrySkill(AgentEntrySkillArgs),
    Skill(AgentSkillArgs),
    Bridge(AgentBridgeArgs),
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
