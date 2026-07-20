#[test]
fn workflow_run_commands_use_the_nested_breaking_interface() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let bootstrap = crate::control::read_project_bootstrap(&paths, BootstrapRequest::new())
        .expect("bootstrap");
    let task = crate::storage::Storage::open(&paths)
        .expect("storage")
        .insert_task(
            &bootstrap.project.id,
            &bootstrap.panel.id,
            "wiki",
            "maintain_wiki",
            "wiki.maintain",
            "wiki",
            &json!({}),
            &json!({}),
        )
        .expect("task");
    let workflow_run_id = task["workflowRunId"].as_str().expect("Workflow Run id");
    assert!(workflow_run_id.starts_with("workflow-run:"));
    crate::storage::Storage::open(&paths)
        .expect("storage")
        .ensure_workflow_run(
            &bootstrap.project.id,
            &bootstrap.panel.id,
            "workflow:legacy",
            "legacy.definition",
            "succeeded",
            &json!({}),
        )
        .expect("legacy Workflow Run");

    let common = [
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ];
    let mut list = vec!["workflow", "run", "list"];
    list.extend(common);
    let (code, stdout, stderr) = run(&list);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let list_payload = serde_json::from_str::<Value>(&stdout).expect("Workflow Run list");
    let created = list_payload["workflowRuns"]
        .as_array()
        .expect("Workflow Runs")
        .iter()
        .find(|workflow_run| workflow_run["workflowRunId"] == workflow_run_id)
        .expect("created Workflow Run");
    assert_eq!(created["definitionKey"], "wiki.maintain_wiki");
    assert!(created.get("id").is_none());
    assert!(created.get("type").is_none());
    assert!(created.get("sourceWorkflowId").is_none());
    assert!(list_payload.get("workflows").is_none());

    let mut read = vec![
        "workflow",
        "run",
        "read",
        "--workflow-run-id",
        workflow_run_id,
    ];
    read.extend(common);
    let (code, stdout, stderr) = run(&read);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let read_payload = serde_json::from_str::<Value>(&stdout).expect("Workflow Run");
    assert_eq!(
        read_payload["workflowRun"]["workflowRunId"],
        workflow_run_id
    );
    assert!(read_payload.get("workflow").is_none());
    assert!(read_payload["workflowRun"].get("id").is_none());
    assert!(read_payload["tasks"][0].get("workflowId").is_none());

    let mut legacy_read = vec![
        "workflow",
        "run",
        "read",
        "--workflow-run-id",
        "workflow:legacy",
    ];
    legacy_read.extend(common);
    let (code, stdout, stderr) = run(&legacy_read);
    assert_eq!(code, 0, "{stderr}{stdout}");
    assert_eq!(
        serde_json::from_str::<Value>(&stdout).expect("legacy Workflow Run")["workflowRun"]
            ["workflowRunId"],
        "workflow:legacy"
    );

    let mut removed = vec!["workflow", "list"];
    removed.extend(common);
    let (code, stdout, stderr) = run_raw(&removed);
    assert_ne!(code, 0, "{stdout}{stderr}");

    let mut removed_read = vec![
        "workflow",
        "read",
        "--workflow-id",
        workflow_run_id,
    ];
    removed_read.extend(common);
    let (code, stdout, stderr) = run_raw(&removed_read);
    assert_ne!(code, 0, "{stdout}{stderr}");
}
