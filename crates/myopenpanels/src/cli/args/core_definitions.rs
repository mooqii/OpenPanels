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
    Materialize {
        #[arg(long)]
        space_id: String,
    },
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
