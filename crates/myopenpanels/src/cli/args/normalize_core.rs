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
        RootCommand::Typesetting(args) => normalize_typesetting(args.command, flags),
        RootCommand::Publishing(args) => match args.command {
            PublishingCommand::Checkpoint { task_id, phase } => {
                put(flags, "task-id", Some(task_id));
                put(flags, "phase", Some(phase));
                (
                    vec!["publishing".into(), "checkpoint".into()],
                    "publishing.checkpoint",
                )
            }
        },
        RootCommand::Task(args) => normalize_task(args.command, flags),
        RootCommand::Workflow(args) => match args.command {
            WorkflowCommand::Run(args) => match args.command {
                WorkflowRunCommand::List => (
                    vec!["workflow".into(), "run".into(), "list".into()],
                    "workflow.run.list",
                ),
                WorkflowRunCommand::Read { workflow_run_id } => {
                    put(flags, "workflow-run-id", Some(workflow_run_id));
                    (
                        vec!["workflow".into(), "run".into(), "read".into()],
                        "workflow.run.read",
                    )
                }
            },
        },
        RootCommand::Operation(args) => normalize_operation(args.command, flags),
        RootCommand::Agent(args) => normalize_agent(args.command, flags),
    }
}

fn normalize_typesetting(
    command: TypesettingCommand,
    flags: &mut BTreeMap<String, FlagValue>,
) -> (Vec<String>, &'static str) {
    match command {
        TypesettingCommand::Title(args) => match args.command {
            TypesettingTitleCommand::Generate {
                publication_id,
                skill_id,
                instruction,
                request_id,
            } => {
                put(flags, "publication-id", Some(publication_id));
                put(flags, "skill-id", Some(skill_id));
                put(flags, "instruction", instruction);
                put(flags, "request-id", request_id);
                (
                    vec!["typesetting".into(), "title".into(), "generate".into()],
                    "typesetting.title.generate",
                )
            }
            TypesettingTitleCommand::Skill(args) => match args.command {
                TypesettingTitleSkillCommand::List => (
                    vec![
                        "typesetting".into(),
                        "title".into(),
                        "skill".into(),
                        "list".into(),
                    ],
                    "typesetting.title.skill.list",
                ),
            },
        },
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
