#[test]
fn publication_title_commands_list_skills_and_create_a_task() {
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
    let bootstrap = ensure_project_bootstrap(
        &paths,
        BootstrapRequest {
            requested_project_id: None,
            requested_panel_id: None,
            requested_panel_kind: Some(PanelKind::Typesetting),
        },
    )
    .expect("typesetting bootstrap");
    Storage::open(&paths)
        .expect("storage")
        .write_panel_state(
            &bootstrap.project.id,
            &bootstrap.panel.id,
            &json!({
                "publications": [{
                    "id": "publication:cli-title",
                    "title": "CLI title target",
                    "titles": [{ "id": "title:primary", "value": "CLI title target" }],
                    "selectedTitleId": "title:primary",
                    "covers": [],
                    "content": {
                        "type": "doc",
                        "content": [{
                            "type": "paragraph",
                            "content": [{ "type": "text", "text": "A captured article body" }]
                        }]
                    },
                    "createdAt": "2026-07-22T00:00:00Z",
                    "updatedAt": "2026-07-22T00:00:00Z"
                }]
            }),
        )
        .expect("typesetting state");

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
    let mut list_args = vec!["publication", "title", "skill", "list"];
    list_args.extend(common);
    let (code, stdout, stderr) = run(&list_args);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let skills = serde_json::from_str::<Value>(&stdout).expect("skills");
    assert!(skills.as_array().is_some_and(|items| items.iter().any(|item| {
        item.pointer("/skill/id").and_then(Value::as_str)
            == Some(crate::publication::DEFAULT_TITLE_SKILL_ID)
    })));

    let mut generate_args = vec![
        "publication",
        "title",
        "generate",
        "--publication-id",
        "publication:cli-title",
        "--instruction",
        "Prefer direct language",
        "--request-id",
        "title-request:cli",
    ];
    generate_args.extend(common);
    let (code, stdout, stderr) = run(&generate_args);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("task payload");
    assert_eq!(
        payload["task"]["type"],
        json!(crate::publication::TITLE_TASK_TYPE)
    );
    assert_eq!(
        payload["task"]["input"]["titleSkillId"],
        json!(crate::publication::DEFAULT_TITLE_SKILL_ID)
    );
    assert_eq!(
        payload["task"]["input"]["instruction"],
        json!("Prefer direct language")
    );
}
