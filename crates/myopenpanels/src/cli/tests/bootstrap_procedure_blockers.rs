#[test]
fn procedure_bootstrap_reports_selection_blockers_and_structured_fallbacks() {
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
        "--procedure",
        "canvas.image.edit",
    ];
    edit.extend(base);
    let (code, stdout, stderr) = run(&edit);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("blocked procedure");
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

    let mut unknown = vec!["agent", "bootstrap", "--procedure", "panel.unknown"];
    unknown.extend(base);
    let (code, stdout, stderr) = run_raw(&unknown);
    assert_eq!(code, 0, "{stdout}{stderr}");
    let fallback = serde_json::from_str::<Value>(&stdout).expect("generic fallback");
    assert_eq!(
        fallback["data"]["procedureFallback"]["requestedKey"],
        "panel.unknown"
    );
    assert_eq!(
        fallback["data"]["procedureFallback"]["reason"],
        "agent_procedure_not_found"
    );
    assert!(fallback["data"].get("agentProcedure").is_none());

}

#[test]
fn procedure_bootstrap_blocks_when_its_target_panel_is_missing() {
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
    let canvas_panel_id = bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == PanelKind::Canvas)
        .expect("canvas panel")
        .panel
        .id
        .clone();
    let storage = crate::storage::Storage::open(&paths).expect("storage");
    storage
        .connection()
        .execute(
            "DELETE FROM panels WHERE project_id = ? AND id = ?",
            rusqlite::params![bootstrap.project.id, canvas_panel_id],
        )
        .expect("remove canvas panel");

    let (code, stdout, stderr) = run(&[
        "agent",
        "bootstrap",
        "--procedure",
        "canvas.image.insert",
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
    let payload = serde_json::from_str::<Value>(&stdout).expect("blocked procedure");
    assert_eq!(payload["readiness"], "blocked");
    assert_eq!(payload["blockers"][0]["code"], "target_panel_required");
    assert_eq!(payload["focus"]["panelKind"], "wiki");
    assert_eq!(payload["target"]["panelKind"], "canvas");
    assert_eq!(payload["target"]["panelId"], Value::Null);
}

#[test]
fn module_procedure_materializes_selected_resource_versions() {
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
    let document = crate::wiki::create_my_document(
        &paths,
        "selected.md",
        Some("Selected"),
        Some("text/markdown"),
        None,
        None,
        b"# Selected\n",
    )
    .expect("document");
    let document_id = document["document"]["id"].as_str().unwrap().to_owned();
    crate::wiki::write_agent_selection(&paths, std::slice::from_ref(&document_id))
        .expect("selection");

    let (code, stdout, stderr) = run(&[
        "agent",
        "bootstrap",
        "--procedure",
        "my-document.read",
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
    let payload = serde_json::from_str::<Value>(&stdout).expect("Procedure");
    assert_eq!(payload["readiness"], "ready");
    assert_eq!(payload["target"]["kind"], "module");
    assert_eq!(payload["target"]["moduleKey"], "my-document");
    assert_eq!(
        payload["target"]["resourceVersions"][0]["resourceId"],
        document_id
    );
    assert_eq!(
        payload["target"]["resourceVersions"][0]["contentVersion"],
        1
    );
    assert_eq!(payload["focus"]["panelKind"], "wiki");
    assert!(payload["target"]["selectionSource"]["revision"].is_i64());
    assert_eq!(
        payload["panel"]["selection"]["value"]["selectedMyDocuments"][0]["id"],
        payload["target"]["resourceVersions"][0]["resourceId"]
    );
}
