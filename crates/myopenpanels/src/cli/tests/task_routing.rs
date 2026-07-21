#[test]
fn agent_bootstrap_delivers_entry_skill_update_until_the_context_acknowledges_it() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project_unacknowledged(&project_dir, &storage_dir);

    let bootstrap_args = [
        "agent",
        "bootstrap",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ];
    let (code, stdout, stderr) = run(&bootstrap_args);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let pending = serde_json::from_str::<Value>(&stdout).expect("pending bootstrap");
    assert_eq!(
        pending["entrySkillUpdate"]["requiredVersion"],
        crate::agent_control::ENTRY_SKILL_VERSION
    );
    let required_steps = pending["actions"]["required"].as_array().unwrap();
    assert_eq!(required_steps[0]["executor"], "agent-host");
    assert_eq!(
        required_steps[0]["intent"],
        "agent-host.skill.update-required"
    );
    assert_action_parses(&required_steps[1]);
    assert_eq!(pending["actions"]["suggested"], json!([]));

    let event_id = pending["entrySkillUpdate"]["eventId"]
        .as_str()
        .expect("event id");
    let (code, repeated, stderr) = run(&bootstrap_args);
    assert_eq!(code, 0, "{stderr}{repeated}");
    assert_eq!(
        serde_json::from_str::<Value>(&repeated).unwrap()["entrySkillUpdate"]["eventId"],
        event_id
    );

    let (code, stdout, stderr) = run(&[
        "agent",
        "entry-skill",
        "acknowledge",
        "--event-id",
        event_id,
        "--installed-version",
        "0.0",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 1, "{stderr}{stdout}");
    assert_eq!(
        serde_json::from_str::<Value>(&stdout).unwrap()["code"],
        "entry_skill_version_too_old"
    );

    let (code, stdout, stderr) = run(&[
        "agent",
        "entry-skill",
        "acknowledge",
        "--event-id",
        event_id,
        "--installed-version",
        crate::agent_control::ENTRY_SKILL_VERSION,
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
    assert_eq!(
        serde_json::from_str::<Value>(&stdout).unwrap()["acknowledged"],
        true
    );

    let (code, stdout, stderr) = run(&bootstrap_args);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let normal = serde_json::from_str::<Value>(&stdout).expect("normal bootstrap");
    assert!(normal.get("entrySkillUpdate").is_none());
    assert_eq!(normal["skills"][0]["id"], "myopenpanels-panels");
}

#[test]
fn agent_bootstrap_prepares_panel_and_wiki_task_authoring_skills() {
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
    let task = crate::wiki::maintain_wiki_space(&paths, None).expect("maintenance task");
    let task_id = task["task"]["id"].as_str().expect("task id");

    let (code, stdout, stderr) = run_raw(&[
        "agent",
        "bootstrap",
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
    assert!(
        stdout.len() <= crate::agent::MAX_BOOTSTRAP_ENVELOPE_BYTES,
        "Bootstrap was {} bytes",
        stdout.len()
    );
    let envelope = serde_json::from_str::<Value>(&stdout).expect("bootstrap");
    let payload = &envelope["data"];
    let required_skills = payload["skills"].as_array().unwrap();
    assert_eq!(required_skills.len(), 2);
    assert_eq!(required_skills[0]["id"], "myopenpanels-panels");
    assert_eq!(required_skills[1]["id"], "karpathy-llm-wiki");
    assert_eq!(required_skills[1]["taskId"], task_id);
    for skill in required_skills {
        let context_path = Path::new(skill["contextPath"].as_str().unwrap());
        let local_path = Path::new(skill["localPath"].as_str().unwrap());
        assert!(context_path.is_file());
        assert!(local_path.is_file());
    }
    let authoring_context = fs::read_to_string(required_skills[1]["contextPath"].as_str().unwrap())
        .expect("authoring loader context");
    assert!(authoring_context.contains(task_id));
}

#[test]
fn agent_bootstrap_rejects_protocol_selection_without_target_side_effects() {
    let temp = tempfile::tempdir().expect("temp dir");
    let untouched_project = temp.path().join("project");
    let untouched_storage = temp.path().join("storage");
    let (code, stdout, stderr) = run_raw(&[
        "agent",
        "bootstrap",
        "--protocol-version",
        "99",
        "--project-dir",
        untouched_project.to_str().unwrap(),
        "--storage-dir",
        untouched_storage.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 2, "{stderr}{stdout}");
    assert_eq!(stdout, "");
    let error = serde_json::from_str::<Value>(&stderr).unwrap();
    assert_eq!(error["error"]["type"], "validation");
    assert_eq!(error["error"]["subtype"], "invalid_argument");
    assert_eq!(error["error"]["subtype"], "invalid_argument");
    assert!(error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("unexpected argument '--protocol-version'"));
    assert!(!untouched_project.exists());
    assert!(!untouched_storage.exists());
}

#[test]
fn canvas_write_commands_insert_and_replace_shapes() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    let image_path = project_dir.join("image.png");
    let metadata_path = project_dir.join("image-metadata.json");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let (code, _, stderr) = run(&[
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
    assert_eq!(code, 0, "{stderr}");
    fs::write(&image_path, tiny_png()).expect("image");
    fs::write(
        &metadata_path,
        serde_json::to_string_pretty(&json!({
            "generateOptions": {
                "prompt": "Soft editorial product photo",
                "model": "test-image-model",
                "referenceImages": [
                    {
                        "source": "local_path",
                        "path": image_path.to_str().unwrap(),
                        "role": "reference"
                    }
                ]
            },
            "generatedBy": "agent"
        }))
        .unwrap(),
    )
    .expect("metadata");

    let (code, stdout, stderr) = run(&[
        "canvas",
        "image",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--image-file",
        image_path.to_str().unwrap(),
        "--display-width",
        "512",
        "--display-height",
        "512",
        "--metadata-file",
        metadata_path.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let inserted = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(
        inserted["bounds"],
        json!({ "x": 160.0, "y": 160.0, "width": 512.0, "height": 512.0 })
    );

    let (code, stdout, stderr) = run(&[
        "canvas",
        "image",
        "generate",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--display-width",
        "512",
        "--display-height",
        "512",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let generation = serde_json::from_str::<Value>(&stdout).expect("json");
    let placeholder = json!({
        "shapeId": generation["operation"]["target"]["placeholderShapeId"],
        "bounds": generation["operation"]["target"]["bounds"],
    });
    assert_eq!(
        placeholder["bounds"],
        json!({ "x": 160.0, "y": 752.0, "width": 512.0, "height": 512.0 })
    );

    let (code, stdout, stderr) = run(&[
        "canvas",
        "image",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--image-file",
        image_path.to_str().unwrap(),
        "--replace-shape-id",
        placeholder["shapeId"].as_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let replaced = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(replaced["replacedShapeId"], placeholder["shapeId"]);
    assert_eq!(replaced["bounds"], placeholder["bounds"]);

    let (code, stdout, stderr) = run(&[
        "panel",
        "read",
        "--detail",
        "full",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let state = serde_json::from_str::<Value>(&stdout).expect("json")["state"].clone();
    assert_eq!(state["selectedShapeIds"], json!([replaced["shapeId"]]));
    assert!(state["store"][placeholder["shapeId"].as_str().unwrap()].is_null());
    let inserted_asset_id = inserted["assetId"].as_str().unwrap();
    assert_eq!(
        state["store"][inserted_asset_id]["meta"]["generateOptions"]["prompt"],
        "Soft editorial product photo"
    );
    assert_eq!(
        state["store"][inserted_asset_id]["meta"]["generateOptions"]["referenceImages"][0]["path"],
        image_path.to_str().unwrap()
    );
    assert!(state["store"][inserted_asset_id]["meta"]["assetRef"]
        .as_str()
        .is_some_and(|value| value.starts_with("projects/")));

    let (code, stdout, stderr) = run(&[
        "canvas",
        "image",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--image-file",
        image_path.to_str().unwrap(),
        "--replace-shape-id",
        "shape:missing-placeholder",
        "--display-width",
        "256",
        "--display-height",
        "128",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let fallback_insert = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(fallback_insert["replacedShapeId"].is_null());
    assert_eq!(
        fallback_insert["bounds"],
        json!({ "x": 160.0, "y": 1344.0, "width": 256.0, "height": 128.0 })
    );

    let (code, stdout, stderr) = run(&[
        "panel",
        "read",
        "--detail",
        "full",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let state = serde_json::from_str::<Value>(&stdout).expect("json")["state"].clone();
    assert_eq!(
        state["selectedShapeIds"],
        json!([fallback_insert["shapeId"]])
    );
    assert!(state["store"][fallback_insert["shapeId"].as_str().unwrap()].is_object());
}
