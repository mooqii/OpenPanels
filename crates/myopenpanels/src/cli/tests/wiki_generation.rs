#[test]
fn wiki_commands_create_markdown_tasks_and_pages() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);

    let (code, stdout, stderr) = run(&[
        "wiki",
        "raw",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--title",
        "Research note",
        "--file-name",
        "research-note.md",
        "--content",
        "# Research note\n\nA useful source.",
        "--space-id",
        "wiki:default",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let raw = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(raw["document"]["conversion"]["status"], "not_required");
    assert_eq!(
        raw["document"]["ingestionByWikiSpace"]["wiki:default"]["status"],
        "queued"
    );

    let (code, stdout, stderr) = run(&[
        "task",
        "next",
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
    let next = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(next["task"]["type"], "ingest_markdown_into_wiki");
    assert_eq!(next["task"]["source"]["agentSkillId"], "karpathy-llm-wiki");
    let task_id = next["task"]["id"].as_str().unwrap();

    let (code, stdout, stderr) = run(&[
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
    assert_eq!(code, 0, "{stderr}");
    let context = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(context["tasks"]["next"]["taskId"], task_id);
    assert!(context["tasks"]["next"].get("readCommand").is_none());
    assert!(context["tasks"]["next"].get("readAction").is_none());
    let task_action = context["actions"]["suggested"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["intent"] == "task.read")
        .expect("Task read action");
    assert_action_parses(task_action);

    update_wiki_state_field(
        &storage_dir,
        "wikiAgentSkillId",
        json!("karpathy-llm-wiki-zh"),
    );
    let (code, stdout, stderr) = run(&[
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
    assert_eq!(code, 0, "{stderr}");
    let context = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(context["tasks"]["next"]["taskId"], task_id);
    assert!(context.get("state").is_none());
    let task_queue_action = context["actions"]["suggested"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["skillId"] == "myopenpanels-task-queue")
        .expect("Task queue Skill action");
    assert_action_parses(task_queue_action);

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "list",
        "--task-type",
        "ingest_markdown_into_wiki",
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
    let authoring_skills = serde_json::from_str::<Value>(&stdout).expect("authoring skills");
    assert_eq!(authoring_skills["skills"].as_array().unwrap().len(), 2);
    for skill in authoring_skills["skills"].as_array().unwrap() {
        assert!(skill.get("name").and_then(Value::as_str).is_some());
        assert!(skill.get("title").is_none());
    }

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "read",
        "--skill-id",
        "myopenpanels-wiki-panel",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let panel_skill = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(panel_skill["skill"]["name"], "MyOpenPanels Wiki Panel");
    assert!(panel_skill["skill"].get("title").is_none());
    assert!(panel_skill["markdown"]
        .as_str()
        .unwrap_or("")
        .contains("`agent.skill.read`"));

    let (code, _, _) = run(&[
        "agent",
        "skill",
        "read",
        "--skill-id",
        "wiki-panel",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_ne!(code, 0, "legacy platform Skill id must be rejected");

    let (code, stdout, stderr) = run(&[
        "task",
        "list",
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
    let project_tasks = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(project_tasks["pendingCount"], 1);
    assert_eq!(project_tasks["readyCount"], 1);
    assert_eq!(project_tasks["blockedCount"], 0);
    assert_eq!(project_tasks["tasks"][0]["queue"], "wiki");
    assert_eq!(project_tasks["tasks"][0]["id"], task_id);
    assert_eq!(project_tasks["tasks"][0]["ready"], true);
    assert_eq!(
        project_tasks["tasks"][0]["type"],
        "ingest_markdown_into_wiki"
    );
    assert_eq!(
        project_tasks["tasks"][0]["capability"],
        "wiki.ingestMarkdown"
    );
    assert_eq!(
        project_tasks["tasks"][0]["input"]["documentId"],
        raw["document"]["id"]
    );
    assert_eq!(
        project_tasks["tasks"][0]["source"]["wikiSpaceId"],
        "wiki:default"
    );
    assert_eq!(project_tasks["tasks"][0]["attempt"], 0);
    assert_eq!(project_tasks["tasks"][0]["maxAttempts"], 8);
    assert!(project_tasks["tasks"][0]["lease"]["owner"].is_null());
    assert!(project_tasks["tasks"][0]["retryAfter"].is_null());
    let (code, stdout, stderr) = run(&[
        "task",
        "list",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--status",
        "queued",
        "--pending",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let filtered_project_tasks = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(filtered_project_tasks["pendingCount"], 1);
    assert_eq!(filtered_project_tasks["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(filtered_project_tasks["tasks"][0]["id"], task_id);
    let (code, stdout, stderr) = run(&[
        "task",
        "next",
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
    let project_next = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(project_next["task"]["id"], task_id);
    assert_eq!(project_next["task"]["ready"], true);

    let future = (chrono::Utc::now() + chrono::Duration::minutes(10))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let past = (chrono::Utc::now() - chrono::Duration::minutes(10))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    update_task_in_panel_state(
        &storage_dir,
        task_id,
        &[
            ("leaseOwner", json!("agent:test")),
            ("leaseExpiresAt", json!(future)),
        ],
    );
    let (code, stdout, stderr) = run(&[
        "task",
        "next",
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
    let leased_next = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(leased_next["task"].is_null());
    let (code, stdout, stderr) = run(&[
        "task",
        "list",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--pending",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let leased_list = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(leased_list["readyCount"], 0);
    assert_eq!(leased_list["blockedCount"], 1);
    assert_eq!(leased_list["tasks"][0]["blockedReason"], "leased");
    update_task_in_panel_state(&storage_dir, task_id, &[("leaseExpiresAt", json!(past))]);
    let (code, stdout, stderr) = run(&[
        "task",
        "next",
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
    let expired_lease_next = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(expired_lease_next["task"]["id"], task_id);
    update_task_in_panel_state(
        &storage_dir,
        task_id,
        &[
            ("leaseOwner", Value::Null),
            ("leaseExpiresAt", Value::Null),
            ("retryAfter", json!(future)),
        ],
    );
    let (code, stdout, stderr) = run(&[
        "task",
        "next",
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
    let retry_next = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(retry_next["task"].is_null());
    update_task_in_panel_state(
        &storage_dir,
        task_id,
        &[
            ("retryAfter", Value::Null),
            ("status", json!("failed")),
            ("attempt", json!(3)),
            ("maxAttempts", json!(3)),
        ],
    );
    let (code, stdout, stderr) = run(&[
        "task",
        "next",
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
    let attempts_next = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(attempts_next["task"].is_null());
    update_task_in_panel_state(
        &storage_dir,
        task_id,
        &[
            ("status", json!("queued")),
            ("attempt", json!(0)),
            ("maxAttempts", json!(3)),
        ],
    );

    let (code, stdout, stderr) = run(&[
        "agent",
        "bridge",
        "run",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--once",
        "--command",
        print_myopenpanels_task_id_command(),
        "--manual-lifecycle",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let bridge = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(bridge["ran"], true);
    assert_eq!(bridge["task"]["id"], task_id);
    assert_eq!(bridge["stdout"], task_id);
    let first_lease_token = bridge["leaseToken"].as_str().unwrap();
    let (code, _, stderr) = run(&[
        "task",
        "release",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
        "--lease-token",
        first_lease_token,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");

    let (code, stdout, stderr) = run(&[
        "agent",
        "bridge",
        "run",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--once",
        "--timeout-ms",
        "50",
        "--command",
        "sleep 1",
        "--manual-lifecycle",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let timed_out_bridge = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(timed_out_bridge["timedOut"], true);
    assert_eq!(timed_out_bridge["success"], false);
    let timeout_lease_token = timed_out_bridge["leaseToken"].as_str().unwrap();
    let (code, _, stderr) = run(&[
        "task",
        "release",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
        "--lease-token",
        timeout_lease_token,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");

    let (code, _, stderr) = run(&[
        "task",
        "retry",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");

    let (code, stdout, stderr) = run(&[
        "agent",
        "bridge",
        "run",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--once",
        "--command",
        "yes x | head -c 70000",
        "--manual-lifecycle",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let truncated_bridge = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(truncated_bridge["stdout"]
        .as_str()
        .unwrap()
        .contains("output truncated"));

    let (code, stdout, stderr) = run(&[
        "task",
        "read",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let inspected_task = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(inspected_task["task"]["id"], task_id);
    assert_eq!(inspected_task["task"]["dispatchState"], "running");
    assert!(inspected_task["task"]["assignedTarget"].is_object());
    assert!(!storage_dir
        .join("contexts")
        .join("ctx")
        .join("wakeups")
        .join(format!(
            "{}.json",
            crate::paths::sanitize_path_part(task_id)
        ))
        .exists());

    let truncated_lease_token = truncated_bridge["leaseToken"].as_str().unwrap();

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "read",
        "--skill-id",
        "karpathy-llm-wiki",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains(&format!("- task id: {task_id}")));
    assert!(!stdout.contains("`task.claim`"));
    assert!(stdout.contains("Read `SKILL.md` directly from the local path above"));
    assert!(stdout.contains("# Skill: karpathy-llm-wiki"));

    let (code, panel_stdout, stderr) = run(&[
        "agent",
        "skill",
        "read",
        "--skill-id",
        "myopenpanels-wiki-panel",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(panel_stdout.contains("`task.claim`"));
    assert!(panel_stdout.contains("`wiki.page.create`"));

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "read",
        "--skill-id",
        "karpathy-llm-wiki",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let skill_payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(skill_payload["skill"]["id"], "karpathy-llm-wiki");
    assert!(
        Path::new(skill_payload["localPath"].as_str().unwrap_or("")).ends_with(
            Path::new(".myopenpanels")
                .join("skills")
                .join("karpathy-llm-wiki")
                .join("SKILL.md")
        )
    );
    assert!(skill_payload["markdown"]
        .as_str()
        .unwrap_or("")
        .contains(&format!("- task id: {task_id}")));

    let page_file = project_dir.join("topic.md");
    fs::write(&page_file, "# Topic\n\nStructured page.").expect("page");
    let (code, stdout, stderr) = run(&[
        "wiki",
        "page",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--space-id",
        "wiki:default",
        "--path",
        "topics/topic.md",
        "--content-file",
        page_file.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let page = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(page["task"]["type"], "maintain_wiki");
    assert_eq!(page["task"]["wikiSpaceId"], "wiki:default");
    assert_eq!(
        page["task"]["changeEvents"],
        json!([{
            "kind": "wiki_page_written",
            "path": "topics/topic.md",
            "operation": "created"
        }])
    );
    let page_index_item = page["wikiSpace"]["pageIndex"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["path"] == "topics/topic.md"))
        .expect("wiki page index item");
    assert_eq!(page_index_item["wordCount"], 21);

    let (code, stdout, stderr) = run(&[
        "task",
        "complete",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
        "--lease-token",
        truncated_lease_token,
        "--format",
        "json",
    ]);
    assert_eq!(code, 1, "{stdout}\n{stderr}");
    let complete = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(complete["code"], "execution_fenced");
    let (code, stdout, stderr) = run(&[
        "task",
        "list",
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
    let project_tasks = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(project_tasks["pendingCount"], 1);

    let db = Connection::open(storage_dir.join("main.sqlite3")).expect("db");
    let stored_status: String = db
        .query_row(
            "SELECT status FROM tasks WHERE id = ? AND queue = 'wiki'",
            params![task_id],
            |row| row.get(0),
        )
        .expect("task row");
    assert_eq!(stored_status, "superseded");

    let binary_path = project_dir.join("archive.bin");
    fs::write(&binary_path, [1_u8, 2, 3]).expect("binary");
    let (code, stdout, stderr) = run(&[
        "wiki",
        "raw",
        "create",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--source-file",
        binary_path.to_str().unwrap(),
        "--mime-type",
        "application/octet-stream",
        "--space-id",
        "wiki:default",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let binary = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(binary["document"]["conversion"]["status"], "queued");

    let convert_task_id = binary["document"]["conversion"]["taskId"]
        .as_str()
        .expect("task id");
    let (code, stdout, stderr) = run(&[
        "agent",
        "target",
        "register",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--name",
        "wiki-converter",
        "--capability",
        "wiki.*",
        "--priority",
        "100",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let target = serde_json::from_str::<Value>(&stdout).expect("json");
    let target_id = target["target"]["id"].as_str().expect("target id");
    let (code, stdout, stderr) = run(&[
        "task",
        "claim",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        convert_task_id,
        "--target-id",
        target_id,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}\n{stdout}");
    let conversion_claim = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(conversion_claim["task"]["status"], "running");
    let conversion_lease = conversion_claim["leaseToken"]
        .as_str()
        .expect("lease token");

    let converted_file = project_dir.join("converted.md");
    fs::write(&converted_file, "# Archive\n\nConverted.").expect("converted");
    let (code, stdout, stderr) = run(&[
        "wiki",
        "raw",
        "update",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--raw-document-id",
        binary["document"]["id"].as_str().unwrap(),
        "--content-file",
        converted_file.to_str().unwrap(),
        "--task-id",
        convert_task_id,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stdout}\n{stderr}");
    let markdown = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(markdown["task"].is_null());

    let (code, stdout, stderr) = run(&[
        "task",
        "complete",
        "--project-dir",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        convert_task_id,
        "--lease-token",
        conversion_lease,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let conversion_complete = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(conversion_complete["task"]["status"], "succeeded");

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
    let conversion_state = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(
        conversion_state["state"]["rawDocuments"][0]["conversion"]["status"],
        "ready"
    );
    assert_eq!(
        conversion_state["state"]["rawDocuments"][0]["ingestionByWikiSpace"]["wiki:default"]
            ["status"],
        "queued"
    );
}
