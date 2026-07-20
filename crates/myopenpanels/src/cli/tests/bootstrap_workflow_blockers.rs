#[test]
fn workflow_bootstrap_reports_selection_blockers_and_structured_fallbacks() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);

    let base = [
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ];
    let mut edit = vec![
        "agent",
        "bootstrap",
        "--workflow",
        "panel.canvas.image.edit",
    ];
    edit.extend(base);
    let (code, stdout, stderr) = run(&edit);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("blocked workflow");
    assert_eq!(payload["readiness"], "blocked");
    assert_eq!(payload["blockers"][0]["code"], "active_panel_required");
    assert_eq!(payload["panel"]["selection"]["value"], Value::Null);

    let (code, stdout, stderr) = run(&[
        "panel",
        "activate",
        "--panel-kind",
        "canvas",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let (code, stdout, stderr) = run(&edit);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("selection blocker");
    assert_eq!(payload["readiness"], "blocked");
    assert_eq!(
        payload["blockers"][0]["code"],
        "explicit_selection_required"
    );
    assert_eq!(payload["panel"]["selection"]["value"], Value::Null);

    let mut unknown = vec!["agent", "bootstrap", "--workflow", "panel.unknown"];
    unknown.extend(base);
    let (code, stdout, stderr) = run_raw(&unknown);
    assert_eq!(code, 1, "{stdout}{stderr}");
    let error = serde_json::from_str::<Value>(&stderr).expect("unknown workflow error");
    assert_eq!(error["error"]["subtype"], "agent_workflow_not_found");
    assert_eq!(error["actions"]["suggested"][0]["intent"], "agent.bootstrap.read");
    assert_eq!(
        &error["actions"]["suggested"][0]["argv"]
            .as_array()
            .unwrap()[..2],
        &[json!("agent"), json!("bootstrap")]
    );

    let mut handoff = vec!["agent", "bootstrap", "--workflow", "task.scope.execute"];
    handoff.extend(base);
    let (code, stdout, stderr) = run_raw(&handoff);
    assert_eq!(code, 1, "{stdout}{stderr}");
    assert_eq!(
        serde_json::from_str::<Value>(&stderr).expect("handoff error")["error"]["subtype"],
        "agent_workflow_not_bootstrappable"
    );
}
