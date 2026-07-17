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
            WikiSpaceCommand::Materialize { space_id } => {
                put(flags, "space-id", Some(space_id));
                (
                    vec!["wiki".into(), "space".into(), "materialize".into()],
                    "wiki.space.materialize",
                )
            }
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
