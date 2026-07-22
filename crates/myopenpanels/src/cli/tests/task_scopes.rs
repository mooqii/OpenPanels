fn scope_test_project() -> (
    tempfile::TempDir,
    crate::paths::MyOpenPanelsPaths,
    crate::types::ProjectBootstrap,
) {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project dir");
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("task-scope-test"),
    )
    .expect("paths");
    let bootstrap = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    (temp, paths, bootstrap)
}

fn scope_test_target(paths: &crate::paths::MyOpenPanelsPaths, project_id: &str) -> Value {
    tasks::register_target(
        paths,
        tasks::TargetRegistration {
            name: "task-scope-target",
            host: Some("test"),
            project_id: Some(project_id),
            capabilities: vec!["*".to_owned()],
            priority: 10,
            protocol_version: 3,
            max_concurrency: 1,
            model_gateway_connection_id: None,
        },
    )
    .expect("target")
}

fn add_scope_markdown(
    paths: &crate::paths::MyOpenPanelsPaths,
    name: &str,
    content: &[u8],
) -> Value {
    crate::wiki::add_raw_document(
        paths,
        name,
        Some(name),
        Some("text/markdown"),
        "user",
        Some("wiki:default"),
        content,
    )
    .expect("raw document")
}

#[test]
fn task_handoff_cli_starts_a_project_drain_with_one_execution_bundle() {
    let (_temp, paths, bootstrap) = scope_test_project();
    let (code, stdout, stderr) = run(&[
        "task",
        "handoff",
        "start",
        "--scope",
        "project-drain",
        "--project-id",
        &bootstrap.project.id,
        "--project-dir",
        paths.project_dir.to_str().unwrap(),
        "--storage-dir",
        paths.storage_dir.to_str().unwrap(),
        "--context-id",
        "task-scope-test",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let payload: Value = serde_json::from_str(&stdout).expect("scope payload");
    assert_eq!(payload["scopeState"], "complete");
    assert_eq!(payload["scope"]["kind"], "project-drain");
    assert!(payload["executionBundle"].is_null());
    add_scope_markdown(&paths, "after-empty.md", b"New work after the empty read.");
    let live = tasks::read_task_scope(
        &paths,
        &tasks::TaskExecutionScope::ProjectDrain {
            project_id: bootstrap.project.id.clone(),
        },
    )
    .expect("live project scope");
    assert_eq!(live["scopeState"], "ready");

    let _broker = crate::content::enable_test_task_broker();
    let (code, stdout, stderr) = run(&[
        "task",
        "handoff",
        "start",
        "--scope",
        "project-drain",
        "--project-id",
        &bootstrap.project.id,
        "--project-dir",
        paths.project_dir.to_str().unwrap(),
        "--storage-dir",
        paths.storage_dir.to_str().unwrap(),
        "--context-id",
        "task-scope-test",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let ready: Value = serde_json::from_str(&stdout).expect("ready scope payload");
    assert_eq!(ready["scopeState"], "running");
    assert_eq!(ready["taskHandoffVersion"], 1);
    assert_eq!(ready["executionBundle"]["schemaVersion"], 2);
    assert_eq!(
        ready["executionBundle"]["handlerKey"],
        "handler.wiki.authoring"
    );
    assert_eq!(
        ready["executionBundle"]["allowedAgentCommandIntents"],
        json!(["wiki.page.read"])
    );
    assert!(ready["executionBundle"]["allowedCommandIntents"].is_null());
    assert_eq!(ready["delivery"]["mode"], "agent-message");
    assert!(ready["delivery"]["prompt"]
        .as_str()
        .unwrap()
        .contains("continue with every Task returned within the Project drain scope"));
    let serialized = serde_json::to_string(&ready).unwrap();
    assert!(!serialized.contains("leaseToken"));
    assert!(!serialized.contains("executionToken"));
    let denied = tasks::execute_task_handoff_command(
        &paths,
        ready["handoff"]["id"].as_str().expect("handoff id"),
        &[
            "wiki".to_owned(),
            "page".to_owned(),
            "create".to_owned(),
        ],
    )
    .expect_err("Task Handler output writes must be Runtime-owned");
    assert_eq!(denied.code(), Some("task_handoff_command_not_allowed"));
    tasks::stop_task_handoff(
        &paths,
        ready["handoff"]["id"].as_str().expect("handoff id"),
    )
    .expect("stop handoff");
}

#[test]
fn removed_task_scope_cli_is_rejected() {
    let (_temp, paths, bootstrap) = scope_test_project();
    let (code, stdout, stderr) = run(&[
        "task",
        "scope",
        "read",
        "--scope",
        "project-drain",
        "--project-id",
        &bootstrap.project.id,
        "--project-dir",
        paths.project_dir.to_str().unwrap(),
        "--storage-dir",
        paths.storage_dir.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_ne!(code, 0, "{stderr}{stdout}");
    assert!(stdout.contains("unrecognized subcommand 'scope'"));
}

#[test]
fn exact_task_handoff_completion_validates_and_finishes_the_scope() {
    let (_temp, paths, bootstrap) = scope_test_project();
    add_scope_markdown(&paths, "complete.md", b"Complete this Wiki update.");
    let task_id = Storage::open(&paths)
        .expect("storage")
        .list_tasks(&bootstrap.project.id)
        .expect("tasks")
        .into_iter()
        .find(|task| task["type"] == "ingest_markdown_into_wiki")
        .and_then(|task| task["id"].as_str().map(str::to_owned))
        .expect("wiki task");
    let _broker = crate::content::enable_test_task_broker();
    let started = tasks::start_task_handoff(
        &paths,
        &tasks::TaskExecutionScope::ExactTask {
            task_id: task_id.clone(),
        },
    )
    .expect("start handoff");
    let handoff_id = started["handoff"]["id"].as_str().unwrap();
    let result_path = started["executionBundle"]["workspace"]["resultFilePath"]
        .as_str()
        .unwrap();
    fs::write(
        result_path,
        serde_json::to_vec(&json!({
            "schemaVersion": 2,
            "outcome": "no_change",
            "summary": "The source requires no Wiki change.",
            "changedPaths": [],
            "artifacts": [],
        }))
        .unwrap(),
    )
    .expect("result file");

    let completed = tasks::complete_task_handoff(&paths, handoff_id).expect("complete handoff");
    assert_eq!(completed["scopeState"], "complete", "{completed}");
    assert!(completed["executionBundle"].is_null());
    assert_eq!(completed["previousExecution"]["status"], "succeeded");
    assert_eq!(
        tasks::inspect_task(&paths, &task_id).unwrap()["task"]["status"],
        "succeeded"
    );
}

#[test]
fn task_handoff_runtime_stages_declared_wiki_artifacts() {
    let (_temp, paths, bootstrap) = scope_test_project();
    add_scope_markdown(&paths, "runtime-output.md", b"Create a runtime-owned page.");
    let task_id = Storage::open(&paths)
        .expect("storage")
        .list_tasks(&bootstrap.project.id)
        .expect("tasks")
        .into_iter()
        .find(|task| task["type"] == "ingest_markdown_into_wiki")
        .and_then(|task| task["id"].as_str().map(str::to_owned))
        .expect("wiki task");
    let _broker = crate::content::enable_test_task_broker();
    let started = tasks::start_task_handoff(
        &paths,
        &tasks::TaskExecutionScope::ExactTask {
            task_id: task_id.clone(),
        },
    )
    .expect("start handoff");
    let handoff_id = started["handoff"]["id"].as_str().unwrap();
    let workspace = std::path::PathBuf::from(
        started["executionBundle"]["workspace"]["rootPath"]
            .as_str()
            .unwrap(),
    );
    let page_path = workspace.join("outputs/wiki/runtime/owned.md");
    fs::create_dir_all(page_path.parent().unwrap()).expect("output directory");
    fs::write(&page_path, "# Runtime owned\n\nCommitted by the Finalizer.\n")
        .expect("page artifact");
    fs::write(
        workspace.join("execution-result.json"),
        serde_json::to_vec(&json!({
            "schemaVersion": 2,
            "outcome": "changed",
            "summary": "Created the runtime-owned page.",
            "changedPaths": ["runtime/owned.md"],
            "artifacts": [{
                "role": "wiki-page",
                "relativePath": "outputs/wiki/runtime/owned.md",
                "logicalPath": "runtime/owned.md"
            }]
        }))
        .unwrap(),
    )
    .expect("result file");

    let completed = tasks::complete_task_handoff(&paths, handoff_id).expect("complete handoff");
    assert_eq!(completed["scopeState"], "complete", "{completed}");
    assert_eq!(completed["previousExecution"]["status"], "succeeded");
    assert!(completed["previousExecution"]["runtimeFinalization"]
        .is_object());
    let page = crate::wiki::read_page(&paths, "wiki:default", "runtime/owned.md")
        .expect("committed Wiki page");
    assert_eq!(page["markdown"], "# Runtime owned\n\nCommitted by the Finalizer.\n");
    let serialized = serde_json::to_string(&completed["previousExecution"]).unwrap();
    assert!(!serialized.contains(workspace.to_str().unwrap()));
}

#[test]
fn task_handoff_runtime_completes_every_compatible_wiki_batch_member() {
    let (_temp, paths, bootstrap) = scope_test_project();
    add_scope_markdown(&paths, "batch-one.md", b"First compatible source.");
    add_scope_markdown(&paths, "batch-two.md", b"Second compatible source.");
    let mut wiki_tasks = Storage::open(&paths)
        .expect("storage")
        .list_tasks(&bootstrap.project.id)
        .expect("tasks")
        .into_iter()
        .filter(|task| task["type"] == "ingest_markdown_into_wiki")
        .collect::<Vec<_>>();
    wiki_tasks.sort_by_key(|task| task["mutationSequence"].as_i64().unwrap());
    let task_ids = wiki_tasks
        .iter()
        .map(|task| task["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let mutation_key = wiki_tasks[0]["mutationKey"].as_str().unwrap().to_owned();
    let _broker = crate::content::enable_test_task_broker();
    let started = tasks::start_task_handoff(
        &paths,
        &tasks::TaskExecutionScope::WikiMutationDrain {
            project_id: bootstrap.project.id.clone(),
            mutation_key,
        },
    )
    .expect("start batch handoff");
    assert_eq!(
        started["executionBundle"]["executionUnit"]["kind"],
        "wiki-update-batch"
    );
    assert_eq!(
        started["executionBundle"]["executionUnit"]["taskIds"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let handoff_id = started["handoff"]["id"].as_str().unwrap();
    let workspace = std::path::PathBuf::from(
        started["executionBundle"]["workspace"]["rootPath"]
            .as_str()
            .unwrap(),
    );
    fs::write(
        workspace.join("execution-result.json"),
        serde_json::to_vec(&json!({
            "schemaVersion": 2,
            "outcome": "no_change",
            "summary": "The compatible batch required no Wiki changes.",
            "changedPaths": [],
            "artifacts": []
        }))
        .unwrap(),
    )
    .expect("result file");

    let completed = tasks::complete_task_handoff(&paths, handoff_id).expect("complete batch");
    assert_eq!(completed["scopeState"], "complete", "{completed}");
    assert_eq!(
        completed["previousExecution"]["result"]["batch"]["taskCount"],
        2
    );
    for task_id in task_ids {
        assert_eq!(
            tasks::inspect_task(&paths, &task_id).unwrap()["task"]["status"],
            "succeeded"
        );
    }
}

#[test]
fn task_handoff_runtime_creates_and_commits_the_writing_operation() {
    let (_temp, paths, _bootstrap) = scope_test_project();
    crate::writing::write_selection(&paths, false, &[]).expect("writing selection");
    let created = crate::writing::create_requests(
        &paths,
        "Write a short runtime finalization test.",
        "create",
        None,
        &["writing-default".to_owned()],
    )
    .expect("writing request");
    let task = created["tasks"][0].clone();
    let task_id = task["id"].as_str().unwrap().to_owned();
    let document_id = task["input"]["targetGeneratedDocumentId"]
        .as_str()
        .unwrap()
        .to_owned();
    let _broker = crate::content::enable_test_task_broker();
    let started = tasks::start_task_handoff(
        &paths,
        &tasks::TaskExecutionScope::ExactTask {
            task_id: task_id.clone(),
        },
    )
    .expect("start handoff");
    let handoff_id = started["handoff"]["id"].as_str().unwrap();
    let workspace = std::path::PathBuf::from(
        started["executionBundle"]["workspace"]["rootPath"]
            .as_str()
            .unwrap(),
    );
    fs::create_dir_all(workspace.join("outputs")).expect("output directory");
    fs::write(
        workspace.join("outputs/document.md"),
        "# Runtime Finalizer\n\nThe Runtime created this Operation.\n",
    )
    .expect("document artifact");
    fs::write(
        workspace.join("execution-result.json"),
        serde_json::to_vec(&json!({
            "schemaVersion": 2,
            "outcome": "generated",
            "summary": "Generated a Runtime-owned document.",
            "title": "Runtime Finalizer",
            "artifacts": [{
                "role": "generated-document",
                "relativePath": "outputs/document.md"
            }]
        }))
        .unwrap(),
    )
    .expect("result file");

    let completed = tasks::complete_task_handoff(&paths, handoff_id).expect("complete handoff");
    assert_eq!(completed["scopeState"], "complete", "{completed}");
    let operation_ids = completed["previousExecution"]["runtimeFinalization"]["operationIds"]
        .as_array()
        .expect("Operation ids");
    assert_eq!(operation_ids.len(), 1);
    let operation = crate::operations::inspect(&paths, operation_ids[0].as_str().unwrap())
        .expect("Runtime Operation");
    assert_eq!(operation["status"], "completed");
    assert_eq!(operation["target"]["writingTaskId"], task_id);
    let document = crate::wiki::read_generated_document(&paths, &document_id)
        .expect("generated document");
    assert_eq!(document["document"]["title"], "Runtime Finalizer");
    assert_eq!(
        document["content"],
        "# Runtime Finalizer\n\nThe Runtime created this Operation.\n"
    );
}

#[test]
fn task_handoff_runtime_stages_converted_markdown() {
    let (_temp, paths, bootstrap) = scope_test_project();
    let uploaded = crate::wiki::add_raw_document(
        &paths,
        "runtime-conversion.pdf",
        Some("Runtime conversion"),
        Some("application/pdf"),
        "user",
        Some("wiki:default"),
        b"binary source",
    )
    .expect("raw document");
    let document_id = uploaded["document"]["id"].as_str().unwrap().to_owned();
    let task_id = Storage::open(&paths)
        .expect("storage")
        .list_tasks(&bootstrap.project.id)
        .expect("tasks")
        .into_iter()
        .find(|task| {
            task["type"] == "convert_document_to_markdown"
                && task["input"]["documentId"] == document_id
        })
        .and_then(|task| task["id"].as_str().map(str::to_owned))
        .expect("conversion task");
    let _broker = crate::content::enable_test_task_broker();
    let started = tasks::start_task_handoff(
        &paths,
        &tasks::TaskExecutionScope::ExactTask {
            task_id: task_id.clone(),
        },
    )
    .expect("start handoff");
    let handoff_id = started["handoff"]["id"].as_str().unwrap();
    let workspace = std::path::PathBuf::from(
        started["executionBundle"]["workspace"]["rootPath"]
            .as_str()
            .unwrap(),
    );
    fs::create_dir_all(workspace.join("outputs")).expect("output directory");
    fs::write(
        workspace.join("outputs/source.md"),
        "# Converted by Runtime\n",
    )
    .expect("converted artifact");
    fs::write(
        workspace.join("execution-result.json"),
        serde_json::to_vec(&json!({
            "schemaVersion": 2,
            "outcome": "converted",
            "summary": "Converted the source document.",
            "artifacts": [{
                "role": "source-markdown",
                "relativePath": "outputs/source.md"
            }]
        }))
        .unwrap(),
    )
    .expect("result file");

    let completed = tasks::complete_task_handoff(&paths, handoff_id).expect("complete handoff");
    assert_eq!(completed["scopeState"], "complete", "{completed}");
    assert_eq!(
        crate::wiki::read_markdown(&paths, &document_id).unwrap()["markdown"],
        "# Converted by Runtime\n"
    );
    assert_eq!(
        tasks::inspect_task(&paths, &task_id).unwrap()["task"]["status"],
        "succeeded"
    );
}

#[test]
fn task_handoff_runtime_validates_and_installs_the_refined_writing_skill() {
    let (_temp, paths, _bootstrap) = scope_test_project();
    let generated = crate::wiki::create_generated_document(
        &paths,
        "refinement-source.md",
        Some("Refinement source"),
        Some("text/markdown"),
        None,
        None,
        b"# Source\n\nLead with the point and keep paragraphs short.\n",
    )
    .expect("generated source");
    let source_id = generated["document"]["id"].as_str().unwrap();
    crate::writing::write_selection(&paths, false, &[source_id.to_owned()])
        .expect("writing selection");
    let created = crate::writing::create_refinement_request(&paths, "Runtime House Style")
        .expect("refinement request");
    let task = created["task"].clone();
    let task_id = task["id"].as_str().unwrap().to_owned();
    let skill_id = task["input"]["skillId"].as_str().unwrap().to_owned();
    let _broker = crate::content::enable_test_task_broker();
    let started = tasks::start_task_handoff(
        &paths,
        &tasks::TaskExecutionScope::ExactTask {
            task_id: task_id.clone(),
        },
    )
    .expect("start handoff");
    let handoff_id = started["handoff"]["id"].as_str().unwrap();
    let workspace = std::path::PathBuf::from(
        started["executionBundle"]["workspace"]["rootPath"]
            .as_str()
            .unwrap(),
    );
    fs::create_dir_all(workspace.join("outputs")).expect("output directory");
    fs::write(
        workspace.join("outputs/SKILL.md"),
        format!(
            "---\nname: {skill_id}\ndescription: Write direct documents with short paragraphs.\n---\n\nLead with the main point. Use short paragraphs and concrete examples.\n"
        ),
    )
    .expect("Writing Skill artifact");
    fs::write(
        workspace.join("execution-result.json"),
        serde_json::to_vec(&json!({
            "schemaVersion": 2,
            "outcome": "refined",
            "summary": "Created a reusable house style.",
            "artifacts": [{
                "role": "writing-skill",
                "relativePath": "outputs/SKILL.md"
            }]
        }))
        .unwrap(),
    )
    .expect("result file");

    let completed = tasks::complete_task_handoff(&paths, handoff_id).expect("complete handoff");
    assert_eq!(completed["scopeState"], "complete", "{completed}");
    let skill = crate::agent::writing_agent_skill(&paths, &skill_id).expect("installed Skill");
    assert_eq!(skill.skill.name, "Runtime House Style");
    assert_eq!(
        completed["previousExecution"]["runtimeFinalization"]["artifacts"][0]["logicalPath"],
        "SKILL.md"
    );
}

#[test]
fn task_handoff_rejects_execution_result_v1_as_retryable_output() {
    let (_temp, paths, bootstrap) = scope_test_project();
    let uploaded = crate::wiki::add_raw_document(
        &paths,
        "legacy-result.pdf",
        Some("Legacy result"),
        Some("application/pdf"),
        "user",
        Some("wiki:default"),
        b"binary source",
    )
    .expect("raw document");
    let document_id = uploaded["document"]["id"].as_str().unwrap();
    let task_id = Storage::open(&paths)
        .expect("storage")
        .list_tasks(&bootstrap.project.id)
        .expect("tasks")
        .into_iter()
        .find(|task| {
            task["type"] == "convert_document_to_markdown"
                && task["input"]["documentId"] == document_id
        })
        .and_then(|task| task["id"].as_str().map(str::to_owned))
        .expect("conversion task");
    let _broker = crate::content::enable_test_task_broker();
    let started = tasks::start_task_handoff(
        &paths,
        &tasks::TaskExecutionScope::ExactTask {
            task_id: task_id.clone(),
        },
    )
    .expect("start handoff");
    let handoff_id = started["handoff"]["id"].as_str().unwrap();
    let workspace = std::path::PathBuf::from(
        started["executionBundle"]["workspace"]["rootPath"]
            .as_str()
            .unwrap(),
    );
    fs::create_dir_all(workspace.join("outputs")).expect("output directory");
    fs::write(workspace.join("outputs/source.md"), "# Legacy\n").expect("artifact");
    fs::write(
        workspace.join("execution-result.json"),
        serde_json::to_vec(&json!({
            "schemaVersion": 1,
            "outcome": "converted",
            "summary": "Used the retired result contract.",
            "artifacts": [{
                "role": "source-markdown",
                "relativePath": "outputs/source.md"
            }]
        }))
        .unwrap(),
    )
    .expect("result file");

    let continued = tasks::complete_task_handoff(&paths, handoff_id).expect("complete call");
    assert_eq!(continued["previousExecution"]["status"], "failed");
    assert_eq!(
        continued["previousExecution"]["finalizationState"]["phase"],
        "failed"
    );
    assert_eq!(
        continued["previousExecution"]["finalizationState"]["failedAt"],
        "validating"
    );
    assert_eq!(continued["previousExecution"]["error"]["code"], "invalid_output");
    let attempts = tasks::list_task_attempts(&paths, &task_id).expect("Task attempts");
    assert!(
        attempts["attempts"].as_array().unwrap().iter().any(|attempt| {
            attempt["status"] == "invalid_output"
                && attempt["failureClass"] == "retryable_output"
        }),
        "{attempts}"
    );
    if continued["scopeState"] == "running" {
        tasks::stop_task_handoff(&paths, handoff_id).expect("stop retried handoff");
    }
}

#[test]
fn exact_task_scope_does_not_absorb_wiki_successors() {
    let (_temp, paths, bootstrap) = scope_test_project();
    add_scope_markdown(&paths, "one.md", b"First source.");
    add_scope_markdown(&paths, "two.md", b"Second source.");
    let mut wiki_tasks = Storage::open(&paths)
        .expect("storage")
        .list_tasks(&bootstrap.project.id)
        .expect("tasks")
        .into_iter()
        .filter(|task| task["type"] == "ingest_markdown_into_wiki")
        .collect::<Vec<_>>();
    wiki_tasks.sort_by_key(|task| task["mutationSequence"].as_i64().unwrap());
    let first_id = wiki_tasks[0]["id"].as_str().unwrap().to_owned();
    let second_id = wiki_tasks[1]["id"].as_str().unwrap().to_owned();
    let target = scope_test_target(&paths, &bootstrap.project.id);
    let _broker = crate::content::enable_test_task_broker();
    let scope = tasks::TaskExecutionScope::ExactTask {
        task_id: first_id.clone(),
    };
    let claim = tasks::claim_task_scope(
        &paths,
        &scope,
        target["target"]["id"].as_str().unwrap(),
    )
    .expect("exact claim");
    assert_eq!(claim["task"]["id"], first_id);
    assert!(claim["batch"].is_null());
    assert_eq!(claim["scopeState"], "running");

    tasks::release_task(
        &paths,
        &first_id,
        claim["leaseToken"].as_str().unwrap(),
    )
    .expect("release exact task");
    tasks::cancel_task(&paths, &first_id).expect("cancel exact task");
    assert_eq!(tasks::read_task_scope(&paths, &scope).unwrap()["scopeState"], "complete");
    assert_eq!(tasks::inspect_task(&paths, &second_id).unwrap()["task"]["status"], "queued");
}

#[test]
fn exact_task_scope_does_not_claim_a_prerequisite() {
    let (_temp, paths, bootstrap) = scope_test_project();
    crate::wiki::add_raw_document(
        &paths,
        "blocked.pdf",
        Some("Blocked"),
        Some("application/pdf"),
        "user",
        Some("wiki:default"),
        b"not-a-real-pdf",
    )
    .expect("raw document");
    let tasks = Storage::open(&paths)
        .expect("storage")
        .list_tasks(&bootstrap.project.id)
        .expect("tasks");
    let conversion = tasks
        .iter()
        .find(|task| task["type"] == "convert_document_to_markdown")
        .expect("conversion");
    let ingest = tasks
        .iter()
        .find(|task| task["type"] == "ingest_markdown_into_wiki")
        .expect("ingest");
    let target = scope_test_target(&paths, &bootstrap.project.id);
    let claim = tasks::claim_task_scope(
        &paths,
        &tasks::TaskExecutionScope::ExactTask {
            task_id: ingest["id"].as_str().unwrap().to_owned(),
        },
        target["target"]["id"].as_str().unwrap(),
    )
    .expect("blocked exact claim");

    assert!(claim["task"].is_null());
    assert_eq!(claim["scopeState"], "blocked");
    assert_eq!(claim["blockers"][0]["reason"], "prerequisite");
    assert_eq!(
        tasks::inspect_task(&paths, conversion["id"].as_str().unwrap()).unwrap()["task"]
            ["status"],
        "queued"
    );
}

#[test]
fn project_drain_claims_work_added_while_the_scope_is_running() {
    let (_temp, paths, bootstrap) = scope_test_project();
    add_scope_markdown(&paths, "initial.md", b"Initial drain work.");
    let first_id = Storage::open(&paths)
        .expect("storage")
        .list_tasks(&bootstrap.project.id)
        .expect("tasks")[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let target = scope_test_target(&paths, &bootstrap.project.id);
    let target_id = target["target"]["id"].as_str().unwrap();
    let scope = tasks::TaskExecutionScope::ProjectDrain {
        project_id: bootstrap.project.id.clone(),
    };
    let _broker = crate::content::enable_test_task_broker();
    let first = tasks::claim_task_scope(&paths, &scope, target_id).expect("first drain claim");
    assert_eq!(first["task"]["id"], first_id);

    add_scope_markdown(&paths, "arrived-later.md", b"Work added during the drain.");
    let second_id = Storage::open(&paths)
        .expect("storage")
        .list_tasks(&bootstrap.project.id)
        .expect("tasks")
        .into_iter()
        .find_map(|task| (task["id"] != first_id).then(|| task["id"].as_str().unwrap().to_owned()))
        .expect("second task");
    tasks::release_task(&paths, &first_id, first["leaseToken"].as_str().unwrap())
        .expect("release first drain task");
    tasks::cancel_task(&paths, &first_id).expect("cancel first drain task");

    let second = tasks::claim_task_scope(&paths, &scope, target_id).expect("second drain claim");
    assert_eq!(second["task"]["id"], second_id);
    assert_eq!(second["scopeState"], "running");
}

#[test]
fn wiki_mutation_handoff_bundles_the_required_conversion_first() {
    let (_temp, paths, bootstrap) = scope_test_project();
    crate::wiki::add_raw_document(
        &paths,
        "source.pdf",
        Some("Source"),
        Some("application/pdf"),
        "user",
        Some("wiki:default"),
        b"not-a-real-pdf",
    )
    .expect("raw document");
    let tasks = Storage::open(&paths)
        .expect("storage")
        .list_tasks(&bootstrap.project.id)
        .expect("tasks");
    let ingest = tasks
        .iter()
        .find(|task| task["type"] == "ingest_markdown_into_wiki")
        .expect("ingest");
    let mutation_key = ingest["mutationKey"].as_str().unwrap().to_owned();
    let dispatch = tasks::set_wiki_update_group_dispatch(
        &paths,
        &mutation_key,
        "auto",
        None,
    )
    .expect("mutation dispatch");
    assert_eq!(dispatch["updatedTaskCount"], 2);
    let _broker = crate::content::enable_test_task_broker();
    let (code, stdout, stderr) = run(&[
        "task",
        "handoff",
        "start",
        "--scope",
        "wiki-mutation-drain",
        "--project-id",
        &bootstrap.project.id,
        "--mutation-key",
        &mutation_key,
        "--project-dir",
        paths.project_dir.to_str().unwrap(),
        "--storage-dir",
        paths.storage_dir.to_str().unwrap(),
        "--context-id",
        "task-scope-test",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let claim: Value = serde_json::from_str(&stdout).expect("mutation claim");
    assert_eq!(claim["scopeState"], "running");
    assert_eq!(
        claim["executionBundle"]["handlerKey"],
        "handler.wiki.document-conversion"
    );
    assert_eq!(
        claim["executionBundle"]["objective"]["taskType"],
        "convert_document_to_markdown"
    );
    assert!(claim["delivery"]["prompt"]
        .as_str()
        .unwrap()
        .contains("task handoff exec"));
    assert!(claim["delivery"]["prompt"]
        .as_str()
        .unwrap()
        .contains("limited to the current Wiki mutation scope"));
    assert!(!claim["delivery"]["prompt"]
        .as_str()
        .unwrap()
        .contains("Project drain scope"));
    tasks::stop_task_handoff(
        &paths,
        claim["handoff"]["id"].as_str().expect("handoff id"),
    )
    .expect("stop handoff");
}

#[test]
fn wiki_mutation_scope_serializes_targets_and_stops_at_a_failed_predecessor() {
    let (_temp, paths, bootstrap) = scope_test_project();
    add_scope_markdown(&paths, "serial-one.md", b"First serial source.");
    add_scope_markdown(&paths, "serial-two.md", b"Second serial source.");
    let mut wiki_tasks = Storage::open(&paths)
        .expect("storage")
        .list_tasks(&bootstrap.project.id)
        .expect("tasks")
        .into_iter()
        .filter(|task| task["type"] == "ingest_markdown_into_wiki")
        .collect::<Vec<_>>();
    wiki_tasks.sort_by_key(|task| task["mutationSequence"].as_i64().unwrap());
    let mutation_key = wiki_tasks[0]["mutationKey"].as_str().unwrap().to_owned();
    let first_id = wiki_tasks[0]["id"].as_str().unwrap().to_owned();
    let second_id = wiki_tasks[1]["id"].as_str().unwrap().to_owned();
    let first_target = scope_test_target(&paths, &bootstrap.project.id);
    let second_target = tasks::register_target(
        &paths,
        tasks::TargetRegistration {
            name: "second-task-scope-target",
            host: Some("test"),
            project_id: Some(&bootstrap.project.id),
            capabilities: vec!["*".to_owned()],
            priority: 10,
            protocol_version: 3,
            max_concurrency: 1,
            model_gateway_connection_id: None,
        },
    )
    .expect("second target");
    let scope = tasks::TaskExecutionScope::WikiMutationDrain {
        project_id: bootstrap.project.id.clone(),
        mutation_key,
    };
    let _broker = crate::content::enable_test_task_broker();
    let first_claim = tasks::claim_task_scope(
        &paths,
        &scope,
        first_target["target"]["id"].as_str().unwrap(),
    )
    .expect("first mutation claim");
    let concurrent = tasks::claim_task_scope(
        &paths,
        &scope,
        second_target["target"]["id"].as_str().unwrap(),
    )
    .expect("concurrent mutation claim");
    assert!(concurrent["task"].is_null());
    assert_eq!(concurrent["scopeState"], "running");

    tasks::release_task(
        &paths,
        &first_id,
        first_claim["leaseToken"].as_str().unwrap(),
    )
    .expect("release batched leader");
    let exact = tasks::claim_task_scope(
        &paths,
        &tasks::TaskExecutionScope::ExactTask {
            task_id: first_id.clone(),
        },
        second_target["target"]["id"].as_str().unwrap(),
    )
    .expect("exact predecessor claim");
    tasks::fail_task_with_class(
        &paths,
        &first_id,
        exact["leaseToken"].as_str().unwrap(),
        "terminal predecessor failure",
        None,
        tasks::TaskFailureClass::TerminalTask,
    )
    .expect("terminal predecessor failure");

    let blocked = tasks::claim_task_scope(
        &paths,
        &scope,
        first_target["target"]["id"].as_str().unwrap(),
    )
    .expect("blocked mutation scope");
    assert!(blocked["task"].is_null());
    assert_eq!(blocked["scopeState"], "blocked");
    assert_eq!(tasks::inspect_task(&paths, &second_id).unwrap()["task"]["status"], "queued");
    assert!(blocked["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|blocker| blocker["taskId"] == first_id));
}

#[test]
fn wiki_mutation_window_has_no_task_count_limit() {
    let (_temp, paths, bootstrap) = scope_test_project();
    for index in 0..20 {
        add_scope_markdown(
            &paths,
            &format!("source-{index}.md"),
            format!("Small source {index}.").as_bytes(),
        );
    }
    let tasks = Storage::open(&paths)
        .expect("storage")
        .list_tasks(&bootstrap.project.id)
        .expect("tasks");
    let mutation_key = tasks
        .iter()
        .find_map(|task| task["mutationKey"].as_str())
        .unwrap()
        .to_owned();
    let target = scope_test_target(&paths, &bootstrap.project.id);
    let _broker = crate::content::enable_test_task_broker();
    let claim = tasks::claim_task_scope(
        &paths,
        &tasks::TaskExecutionScope::WikiMutationDrain {
            project_id: bootstrap.project.id.clone(),
            mutation_key,
        },
        target["target"]["id"].as_str().unwrap(),
    )
    .expect("mutation claim");
    assert_eq!(claim["batch"]["taskCount"], 20);
}

#[test]
fn wiki_mutation_window_splits_on_input_bytes_not_task_count() {
    let (_temp, paths, bootstrap) = scope_test_project();
    let source = vec![b'a'; 140 * 1024];
    add_scope_markdown(&paths, "large-one.md", &source);
    add_scope_markdown(&paths, "large-two.md", &source);
    let tasks = Storage::open(&paths)
        .expect("storage")
        .list_tasks(&bootstrap.project.id)
        .expect("tasks");
    let mutation_key = tasks
        .iter()
        .find_map(|task| task["mutationKey"].as_str())
        .unwrap()
        .to_owned();
    let target = scope_test_target(&paths, &bootstrap.project.id);
    let _broker = crate::content::enable_test_task_broker();
    let claim = tasks::claim_task_scope(
        &paths,
        &tasks::TaskExecutionScope::WikiMutationDrain {
            project_id: bootstrap.project.id.clone(),
            mutation_key,
        },
        target["target"]["id"].as_str().unwrap(),
    )
    .expect("mutation claim");
    assert!(claim["batch"].is_null());
    assert_eq!(claim["scopeState"], "running");
}
