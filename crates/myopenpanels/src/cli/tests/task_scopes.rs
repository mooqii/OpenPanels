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
fn task_scope_cli_reads_a_project_drain_with_required_actions() {
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
        "--context-id",
        "task-scope-test",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let payload: Value = serde_json::from_str(&stdout).expect("scope payload");
    assert_eq!(payload["scopeState"], "complete");
    assert_eq!(payload["scope"]["kind"], "project-drain");
    assert!(payload["task"].is_null());
    assert!(payload["batch"].is_null());
    assert!(payload["lease"].is_null());
    assert!(payload["executionToken"].is_null());
    assert_eq!(payload["inputManifest"], json!([]));
    assert_eq!(
        payload["actions"]["required"][0]["intent"],
        "agent.skill.read"
    );
    add_scope_markdown(&paths, "after-empty.md", b"New work after the empty read.");
    let live = tasks::read_task_scope(
        &paths,
        &tasks::TaskExecutionScope::ProjectDrain {
            project_id: bootstrap.project.id.clone(),
        },
    )
    .expect("live project scope");
    assert_eq!(live["scopeState"], "ready");

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
        "--context-id",
        "task-scope-test",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let ready: Value = serde_json::from_str(&stdout).expect("ready scope payload");
    assert_eq!(ready["scopeState"], "ready");
    assert_eq!(ready["actions"]["required"][1]["intent"], "agent.target.register");
    assert!(ready["actions"]["required"][1]["argv"]
        .as_array()
        .unwrap()
        .windows(2)
        .any(|pair| pair[0] == "--project-id" && pair[1] == bootstrap.project.id));
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
fn wiki_mutation_scope_claims_required_conversion_first() {
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
    let target = scope_test_target(&paths, &bootstrap.project.id);
    let _broker = crate::content::enable_test_task_broker();
    let (code, stdout, stderr) = run(&[
        "task",
        "scope",
        "claim",
        "--scope",
        "wiki-mutation-drain",
        "--project-id",
        &bootstrap.project.id,
        "--mutation-key",
        &mutation_key,
        "--target-id",
        target["target"]["id"].as_str().unwrap(),
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
    assert_eq!(claim["task"]["type"], "convert_document_to_markdown");
    assert_eq!(claim["scopeState"], "running");
    let skill_ids = claim["actions"]["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|action| action["skillId"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        skill_ids,
        [
            "myopenpanels-task-queue",
            "myopenpanels-wiki-panel",
            "karpathy-llm-wiki"
        ]
    );
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
