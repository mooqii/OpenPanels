use super::*;
use base64::Engine;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::fs;
use std::io::Read;
use std::net::TcpListener;
use std::path::Path;
use std::thread;

fn run(args: &[&str]) -> (i32, String, String) {
    let argv = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_cli_with_io(&argv, &mut stdout, &mut stderr);
    (
        code,
        String::from_utf8(stdout).expect("stdout should be utf8"),
        String::from_utf8(stderr).expect("stderr should be utf8"),
    )
}

fn fake_studio_server(request_count: usize) -> (u16, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
    let port = listener.local_addr().expect("local addr").port();
    let server = thread::spawn(move || {
        for _ in 0..request_count {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buffer = [0_u8; 1024];
            let _ = stream.read(&mut buffer);
            stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 11\r\n\r\n{\"ok\":true}",
                    )
                    .expect("response");
        }
    });
    (port, server)
}

fn create_cli_project(project_dir: &Path, storage_dir: &Path) {
    let (code, stdout, stderr) = run(&[
        "project",
        "create",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
}

#[test]
fn cli_trace_url_falls_back_to_running_studio_session() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    let paths = resolve_openpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let (port, server) = fake_studio_server(1);
    let server_url = format!("http://127.0.0.1:{port}");
    write_studio_session(
        &paths,
        &StudioSession {
            browser_url: Some(server_url.clone()),
            context_dir: paths.context_dir.display().to_string(),
            context_id: paths.context_id.clone(),
            context_id_source: paths.context_id_source.clone(),
            host: Some("127.0.0.1".to_owned()),
            lan_server_urls: Some(Vec::new()),
            local_server_url: Some(server_url.clone()),
            log_path: paths.context_dir.join("studio.log").display().to_string(),
            pid: std::process::id(),
            port,
            project_dir: paths.project_dir.display().to_string(),
            server_url: server_url.clone(),
            started_at: "2026-07-09T00:00:00.000Z".to_owned(),
            storage_dir: paths.storage_dir.display().to_string(),
        },
    )
    .expect("studio session");

    let argv = vec![
        "canvas".to_owned(),
        "selection".to_owned(),
        "read".to_owned(),
        "--project".to_owned(),
        project_dir.to_str().unwrap().to_owned(),
        "--storage-dir".to_owned(),
        storage_dir.to_str().unwrap().to_owned(),
        "--context-id".to_owned(),
        "ctx".to_owned(),
        "--format".to_owned(),
        "json".to_owned(),
    ];

    assert_eq!(
        trace_url_for_cli(&argv),
        Some(format!("{server_url}/api/trace/events"))
    );
    server.join().expect("server thread");
}

#[test]
fn studio_start_json_reuses_same_project_studio() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    let owner_paths = resolve_openpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("owner"),
    )
    .expect("owner paths");
    let (port, server) = fake_studio_server(1);
    let server_url = format!("http://127.0.0.1:{port}");
    write_studio_session(
        &owner_paths,
        &StudioSession {
            browser_url: Some(server_url.clone()),
            context_dir: owner_paths.context_dir.display().to_string(),
            context_id: owner_paths.context_id.clone(),
            context_id_source: owner_paths.context_id_source.clone(),
            host: Some("127.0.0.1".to_owned()),
            lan_server_urls: Some(Vec::new()),
            local_server_url: Some(server_url.clone()),
            log_path: owner_paths
                .context_dir
                .join("studio.log")
                .display()
                .to_string(),
            pid: std::process::id(),
            port,
            project_dir: owner_paths.project_dir.display().to_string(),
            server_url: server_url.clone(),
            started_at: "2026-07-09T00:00:00.000Z".to_owned(),
            storage_dir: owner_paths.storage_dir.display().to_string(),
        },
    )
    .expect("studio session");

    let (code, stdout, stderr) = run(&[
        "studio",
        "start",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "borrower",
        "--format",
        "json",
        "--no-open",
    ]);

    assert_eq!(code, 0, "{stderr}{stdout}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(payload["browserUrl"], server_url);
    assert_eq!(payload["contextId"], "owner");
    assert_eq!(payload["reusedExisting"], true);
    assert!(storage_dir
        .join("contexts")
        .join("borrower")
        .join("studio-session.json")
        .exists());
    server.join().expect("server thread");
}

#[test]
fn panel_commands_follow_borrowed_running_studio_context() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    let owner_paths = resolve_openpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("owner"),
    )
    .expect("owner paths");
    let borrower_paths = resolve_openpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("borrower"),
    )
    .expect("borrower paths");
    let (port, server) = fake_studio_server(1);
    let server_url = format!("http://127.0.0.1:{port}");
    write_studio_session(
        &borrower_paths,
        &StudioSession {
            browser_url: Some(server_url.clone()),
            context_dir: owner_paths.context_dir.display().to_string(),
            context_id: owner_paths.context_id.clone(),
            context_id_source: owner_paths.context_id_source.clone(),
            host: Some("127.0.0.1".to_owned()),
            lan_server_urls: Some(Vec::new()),
            local_server_url: Some(server_url.clone()),
            log_path: owner_paths
                .context_dir
                .join("studio.log")
                .display()
                .to_string(),
            pid: std::process::id(),
            port,
            project_dir: owner_paths.project_dir.display().to_string(),
            server_url,
            started_at: "2026-07-09T00:00:00.000Z".to_owned(),
            storage_dir: owner_paths.storage_dir.display().to_string(),
        },
    )
    .expect("borrowed studio session");

    let resolved =
        resolve_panel_paths_for_active_studio(borrower_paths.clone(), false).expect("resolved");
    assert_eq!(resolved.context_id, "owner");
    assert_eq!(resolved.context_dir, owner_paths.context_dir);

    let explicit =
        resolve_panel_paths_for_active_studio(borrower_paths.clone(), true).expect("explicit");
    assert_eq!(explicit.context_id, "borrower");
    assert_eq!(explicit.context_dir, borrower_paths.context_dir);
    server.join().expect("server thread");
}

#[test]
fn studio_serve_reuses_existing_studio_without_foreground_server() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    let owner_paths = resolve_openpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("owner"),
    )
    .expect("owner paths");
    let (port, server) = fake_studio_server(1);
    let server_url = format!("http://127.0.0.1:{port}");
    write_studio_session(
        &owner_paths,
        &StudioSession {
            browser_url: Some(server_url.clone()),
            context_dir: owner_paths.context_dir.display().to_string(),
            context_id: owner_paths.context_id.clone(),
            context_id_source: owner_paths.context_id_source.clone(),
            host: Some("127.0.0.1".to_owned()),
            lan_server_urls: Some(Vec::new()),
            local_server_url: Some(server_url.clone()),
            log_path: owner_paths
                .context_dir
                .join("studio.log")
                .display()
                .to_string(),
            pid: std::process::id(),
            port,
            project_dir: owner_paths.project_dir.display().to_string(),
            server_url: server_url.clone(),
            started_at: "2026-07-09T00:00:00.000Z".to_owned(),
            storage_dir: owner_paths.storage_dir.display().to_string(),
        },
    )
    .expect("studio session");

    let (code, stdout, stderr) = run(&[
        "studio",
        "serve",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "borrower",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "{stderr}{stdout}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(payload["browserUrl"], server_url);
    assert_eq!(payload["foreground"], false);
    assert_eq!(payload["reusedExisting"], true);
    server.join().expect("server thread");
}

#[test]
fn project_read_commands_bootstrap_project() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);

    let (code, stdout, stderr) = run(&[
        "panel",
        "list",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "{stderr}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(payload["activePanelKind"], "wiki");
    assert_eq!(
        payload["panels"]
            .as_array()
            .expect("panels")
            .iter()
            .map(|panel| panel["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["wiki", "canvas"]
    );
    assert_eq!(stderr, "");

    let (code, stdout, stderr) = run(&[
        "panel",
        "switch",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--kind",
        "canvas",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        serde_json::from_str::<Value>(&stdout).expect("json")["activePanelKind"],
        "canvas"
    );
}

#[test]
fn agent_commands_emit_context_guides_and_capabilities() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);

    let (code, stdout, stderr) = run(&[
        "agent",
        "context",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}{stdout}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(payload["protocolVersion"], 1);
    assert_eq!(payload["cliVersion"], VERSION);
    assert_eq!(payload["activePanel"]["kind"], "wiki");
    assert_eq!(payload["state"]["wiki"]["language"], Value::Null);
    assert!(payload["capabilities"]
        .as_array()
        .expect("capabilities")
        .iter()
        .any(
            |capability| capability["intent"] == "canvas.placeholder.create"
                && capability["target"] == "current_user_project.current_canvas"
        ));
    assert!(payload["availableGuides"]
        .as_array()
        .expect("available guides")
        .iter()
        .any(|guide| guide["id"] == "canvas.image-generation" && guide["source"] == "builtin"));

    let (code, stdout, stderr) = run(&[
        "agent",
        "context",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("# OpenPanels Agent Context"));
    assert!(stdout.contains("## Capabilities"));

    let (code, stdout, stderr) = run(&[
        "agent",
        "guides",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("wiki.index-document"));

    let (code, stdout, stderr) = run(&[
        "agent",
        "guide",
        "canvas.image-generation",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("# Guide: canvas.image-generation"));
    assert!(stdout.contains("## Instructions"));

    let (code, stdout, stderr) = run(&[
        "wiki",
        "context",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(
        serde_json::from_str::<Value>(&stdout).expect("json")["activePanel"]["kind"],
        "wiki"
    );
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
        "insert",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--image",
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
        "placeholder",
        "create",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--anchor-shape-id",
        inserted["shapeId"].as_str().unwrap(),
        "--display-width",
        "512",
        "--display-height",
        "512",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let placeholder = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(
        placeholder["bounds"],
        json!({ "x": 752.0, "y": 160.0, "width": 512.0, "height": 512.0 })
    );

    let (code, stdout, stderr) = run(&[
        "canvas",
        "image",
        "insert",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--image",
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
        "canvas",
        "state",
        "--project",
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
        .is_some_and(|value| value.starts_with("sessions/")));

    let (code, stdout, stderr) = run(&[
        "canvas",
        "image",
        "insert",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--image",
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
        json!({ "x": 160.0, "y": 752.0, "width": 256.0, "height": 128.0 })
    );

    let (code, stdout, stderr) = run(&[
        "canvas",
        "state",
        "--project",
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

#[test]
fn wiki_commands_create_markdown_tasks_and_pages() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);

    let (code, stdout, stderr) = run(&[
        "wiki",
        "documents",
        "create-markdown",
        "--project",
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
        "wiki",
        "tasks",
        "next",
        "--project",
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
    let task_id = next["task"]["id"].as_str().unwrap();
    let (code, stdout, stderr) = run(&[
        "tasks",
        "list",
        "--project",
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
    assert_eq!(project_tasks["tasks"][0]["queue"], "wiki");
    assert_eq!(project_tasks["tasks"][0]["id"], task_id);
    assert_eq!(
        project_tasks["tasks"][0]["type"],
        "ingest_markdown_into_wiki"
    );
    assert!(storage_dir
        .join("contexts")
        .join("ctx")
        .join("wakeups")
        .join(format!(
            "{}.json",
            crate::paths::sanitize_path_part(task_id)
        ))
        .exists());

    let (code, stdout, stderr) = run(&[
        "agent",
        "guide",
        "wiki.index-document",
        "--project",
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
    assert!(stdout.contains("openpanels-local wiki tasks claim"));

    let page_file = project_dir.join("topic.md");
    fs::write(&page_file, "# Topic\n\nStructured page.").expect("page");
    let (code, stdout, stderr) = run(&[
        "wiki",
        "pages",
        "write",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--wiki-space-id",
        "wiki:default",
        "--path",
        "topics/topic.md",
        "--file",
        page_file.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let page = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(page["task"]["type"], "rebuild_wiki_index");
    assert_eq!(page["task"]["wikiSpaceId"], "wiki:default");

    let (code, stdout, stderr) = run(&[
        "wiki",
        "tasks",
        "claim",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
        "--agent-host",
        "test-host",
        "--thread-id",
        "thread-1",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let claim = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(claim["task"]["status"], "claimed");
    assert_eq!(
        claim["state"]["rawDocuments"][0]["ingestionByWikiSpace"]["wiki:default"]["status"],
        "ingesting"
    );
    assert_eq!(claim["state"]["agentProcesses"][0]["status"], "running");

    let (code, stdout, stderr) = run(&[
        "wiki",
        "tasks",
        "complete",
        "--project",
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
    let complete = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(complete["task"]["status"], "succeeded");
    assert_eq!(
        complete["state"]["rawDocuments"][0]["ingestionByWikiSpace"]["wiki:default"]["status"],
        "ingested"
    );
    let (code, stdout, stderr) = run(&[
        "tasks",
        "list",
        "--project",
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
            "SELECT status FROM wiki_tasks WHERE id = ?",
            params![task_id],
            |row| row.get(0),
        )
        .expect("wiki task row");
    assert_eq!(stored_status, "succeeded");
    let stored_project_status: String = db
        .query_row(
            "SELECT status FROM project_tasks WHERE id = ? AND queue = 'wiki'",
            params![task_id],
            |row| row.get(0),
        )
        .expect("project task row");
    assert_eq!(stored_project_status, "succeeded");

    let binary_path = project_dir.join("archive.bin");
    fs::write(&binary_path, [1_u8, 2, 3]).expect("binary");
    let (code, stdout, stderr) = run(&[
        "wiki",
        "documents",
        "add",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--file",
        binary_path.to_str().unwrap(),
        "--mime-type",
        "application/octet-stream",
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
        "wiki",
        "tasks",
        "claim",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        convert_task_id,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let conversion_claim = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(conversion_claim["task"]["status"], "running");
    assert_eq!(
        conversion_claim["state"]["rawDocuments"][0]["conversion"]["status"],
        "converting"
    );

    let converted_file = project_dir.join("converted.md");
    fs::write(&converted_file, "# Archive\n\nConverted.").expect("converted");
    let (code, stdout, stderr) = run(&[
        "wiki",
        "markdown",
        "write",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--document-id",
        binary["document"]["id"].as_str().unwrap(),
        "--file",
        converted_file.to_str().unwrap(),
        "--task-id",
        convert_task_id,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let markdown = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(markdown["task"].is_null());

    let (code, stdout, stderr) = run(&[
        "wiki",
        "tasks",
        "complete",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        convert_task_id,
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let conversion_complete = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(
        conversion_complete["state"]["rawDocuments"][0]["conversion"]["status"],
        "ready"
    );
    assert_eq!(
        conversion_complete["state"]["rawDocuments"][0]["ingestionByWikiSpace"]["wiki:default"]
            ["status"],
        "queued"
    );
}

#[test]
fn studio_status_reports_missing_session() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");

    let (code, stdout, stderr) = run(&[
        "studio",
        "status",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "{stderr}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["server"], "missing");
    assert_eq!(payload["contextId"], "ctx");
    assert_eq!(stderr, "");
}

#[test]
fn selection_reads_sqlite_panel_selection() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    let context_dir = storage_dir.join("contexts").join("ctx");
    fs::create_dir_all(&context_dir).expect("context dir");
    fs::create_dir_all(&project_dir).expect("project dir");
    seed_selection_database(
        &storage_dir,
        "session:1",
        "panel:canvas",
        serde_json::json!({
            "sessionId": "session:1",
            "panelId": "panel:canvas",
            "selectedShapeIds": ["shape:1"],
            "selectedShapes": [{
                "id": "shape:1",
                "type": "geo",
                "parentId": "page:main",
                "props": {},
                "bounds": { "x": 1, "y": 2, "width": 3, "height": 4 }
            }],
            "assetRef": null,
            "updatedAt": "2026-07-08T00:00:00.000Z"
        }),
        None,
    );
    fs::write(
        context_dir.join("active-session.json"),
        r#"{"sessionId":"session:1"}"#,
    )
    .expect("active session");

    let (code, stdout, stderr) = run(&[
        "canvas",
        "selection",
        "read",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "{stderr}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(
        payload["selection"]["selectedShapeIds"],
        serde_json::json!(["shape:1"])
    );
    assert_eq!(payload["selection"]["isExplicitSelection"], true);
    assert_eq!(payload["contextId"], "ctx");
    assert_eq!(stderr, "");
}

#[test]
fn selection_fallback_is_not_explicit_and_asset_export_requires_opt_in() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    let context_dir = storage_dir.join("contexts").join("ctx");
    let asset_dir = storage_dir
        .join("sessions")
        .join("session:1")
        .join("panels")
        .join("panel:canvas")
        .join("assets");
    fs::create_dir_all(&context_dir).expect("context dir");
    fs::create_dir_all(&project_dir).expect("project dir");
    fs::create_dir_all(&asset_dir).expect("asset dir");
    fs::write(asset_dir.join("fallback.png"), tiny_png()).expect("asset");
    seed_selection_database(
        &storage_dir,
        "session:1",
        "panel:canvas",
        serde_json::json!({
            "sessionId": "session:1",
            "panelId": "panel:canvas",
            "selectedShapeIds": [],
            "selectedShapes": [],
            "assetRef": null,
            "updatedAt": "2026-07-08T00:00:00.000Z"
        }),
        Some(serde_json::json!({
            "schema": { "schemaVersion": 1, "recordVersions": {} },
            "currentPageId": "page:main",
            "selectedShapeIds": [],
            "store": {
                "page:main": { "id": "page:main", "typeName": "page", "name": "Page 1", "index": 1 },
                "asset:fallback": {
                    "id": "asset:fallback",
                    "typeName": "asset",
                    "type": "image",
                    "props": {
                        "name": "fallback.png",
                        "src": "/api/panels/session:1/panel:canvas/assets/fallback.png",
                        "w": 1,
                        "h": 1,
                        "mimeType": "image/png",
                        "isAnimated": false
                    },
                    "meta": {
                        "assetRef": "sessions/session:1/panels/panel:canvas/assets/fallback.png"
                    }
                },
                "shape:fallback": {
                    "id": "shape:fallback",
                    "typeName": "shape",
                    "type": "image",
                    "parentId": "page:main",
                    "index": 1,
                    "props": {
                        "assetId": "asset:fallback",
                        "x": 1,
                        "y": 2,
                        "width": 3,
                        "height": 4
                    }
                }
            }
        })),
    );
    fs::write(
        context_dir.join("active-session.json"),
        r#"{"sessionId":"session:1"}"#,
    )
    .expect("active session");

    let (code, stdout, stderr) = run(&[
        "canvas",
        "selection",
        "read",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(payload["selection"]["isExplicitSelection"], false);
    assert_eq!(payload["selection"]["fallback"], "last-image");
    assert_eq!(
        payload["selection"]["assetRef"],
        "sessions/session:1/panels/panel:canvas/assets/fallback.png"
    );

    let output_path = temp.path().join("out.png");
    let (code, _stdout, stderr) = run(&[
        "canvas",
        "selection",
        "export",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--output",
        output_path.to_str().unwrap(),
    ]);
    assert_eq!(code, 1);
    assert!(stderr.contains("No explicit MyOpenPanels selection asset is available"));

    let (code, stdout, stderr) = run(&[
        "canvas",
        "selection",
        "export",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--output",
        output_path.to_str().unwrap(),
        "--allow-fallback",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(payload["outputPath"], output_path.to_str().unwrap());
    assert!(output_path.exists());
}

#[test]
fn version_prints_text() {
    let (code, stdout, stderr) = run(&["version"]);

    assert_eq!(code, 0);
    assert_eq!(stdout, format!("{VERSION}\n"));
    assert_eq!(stderr, "");
}

#[test]
fn version_prints_json() {
    let (code, stdout, stderr) = run(&["--version", "--format", "json"]);

    assert_eq!(code, 0);
    assert_eq!(stdout, format!("{{\n  \"version\": \"{VERSION}\"\n}}\n"));
    assert_eq!(stderr, "");
}

#[test]
fn help_prints_current_command_map() {
    let (code, stdout, stderr) = run(&[]);

    assert_eq!(code, 0);
    assert!(stdout.contains("openpanels-local <command> [options]"));
    assert!(stdout.contains("studio start"));
    assert!(stdout.contains("canvas image insert"));
    assert!(stdout.contains("update check"));
    assert_eq!(stderr, "");
}

#[test]
fn update_help_prints_manifest_controls() {
    let (code, stdout, stderr) = run(&["update", "help"]);

    assert_eq!(code, 0);
    assert!(stdout.contains("openpanels-local update"));
    assert!(stdout.contains("OPENPANELS_UPDATE_MANIFEST_URL"));
    assert_eq!(stderr, "");
}

#[test]
fn unknown_command_prints_text_error() {
    let (code, stdout, stderr) = run(&["nope"]);

    assert_eq!(code, 1);
    assert_eq!(stdout, "");
    assert_eq!(stderr, "Error: Unknown command: nope\n");
}

#[test]
fn unknown_command_prints_json_error() {
    let (code, stdout, stderr) = run(&["nope", "--format=json"]);

    assert_eq!(code, 1);
    assert_eq!(
        stdout,
        "{\n  \"ok\": false,\n  \"error\": \"Unknown command: nope\"\n}\n"
    );
    assert_eq!(stderr, "");
}

fn seed_selection_database(
    storage_dir: &Path,
    session_id: &str,
    panel_id: &str,
    selection: Value,
    state: Option<Value>,
) {
    fs::create_dir_all(storage_dir).expect("storage dir");
    let connection = Connection::open(storage_dir.join("main.sqlite3")).expect("db");
    connection
        .execute_batch(
            r#"
                CREATE TABLE sessions (
                  id TEXT PRIMARY KEY NOT NULL,
                  title TEXT NOT NULL,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL,
                  panel_ids_json TEXT NOT NULL DEFAULT '[]',
                  session_json TEXT NOT NULL
                );
                CREATE TABLE panels (
                  id TEXT NOT NULL,
                  session_id TEXT NOT NULL,
                  kind TEXT NOT NULL,
                  title TEXT NOT NULL,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL,
                  state_ref TEXT,
                  panel_json TEXT NOT NULL,
                  PRIMARY KEY (session_id, id)
                );
                CREATE TABLE panel_states (
                  session_id TEXT NOT NULL,
                  panel_id TEXT NOT NULL,
                  schema_version INTEGER,
                  state_json TEXT NOT NULL,
                  updated_at TEXT NOT NULL,
                  PRIMARY KEY (session_id, panel_id)
                );
                CREATE TABLE panel_selections (
                  session_id TEXT NOT NULL,
                  panel_id TEXT NOT NULL,
                  asset_ref TEXT,
                  selected_shape_ids_json TEXT NOT NULL DEFAULT '[]',
                  selection_json TEXT NOT NULL,
                  updated_at TEXT NOT NULL,
                  PRIMARY KEY (session_id, panel_id)
                );
                "#,
        )
        .expect("schema");
    connection
            .execute(
                "INSERT INTO sessions (id, title, created_at, updated_at, panel_ids_json, session_json) VALUES (?, 'Project 1', '2026-07-08T00:00:00.000Z', '2026-07-08T00:00:00.000Z', ?, ?)",
                params![
                    session_id,
                    serde_json::json!([panel_id]).to_string(),
                    serde_json::json!({
                        "id": session_id,
                        "title": "Project 1",
                        "panelIds": [panel_id],
                        "createdAt": "2026-07-08T00:00:00.000Z",
                        "updatedAt": "2026-07-08T00:00:00.000Z"
                    }).to_string()
                ],
            )
            .expect("session");
    connection
            .execute(
                "INSERT INTO panels (id, session_id, kind, title, created_at, updated_at, state_ref, panel_json) VALUES (?, ?, 'canvas', 'Design canvas', '2026-07-08T00:00:00.000Z', '2026-07-08T00:00:00.000Z', NULL, ?)",
                params![
                    panel_id,
                    session_id,
                    serde_json::json!({
                        "id": panel_id,
                        "sessionId": session_id,
                        "kind": "canvas",
                        "title": "Design canvas",
                        "createdAt": "2026-07-08T00:00:00.000Z",
                        "updatedAt": "2026-07-08T00:00:00.000Z"
                    }).to_string()
                ],
            )
            .expect("panel");
    if let Some(state) = state {
        connection
                .execute(
                    "INSERT INTO panel_states (session_id, panel_id, schema_version, state_json, updated_at) VALUES (?, ?, 1, ?, '2026-07-08T00:00:00.000Z')",
                    params![session_id, panel_id, state.to_string()],
                )
                .expect("state");
    }
    connection
            .execute(
                "INSERT INTO panel_selections (session_id, panel_id, asset_ref, selected_shape_ids_json, selection_json, updated_at) VALUES (?, ?, NULL, ?, ?, '2026-07-08T00:00:00.000Z')",
                params![
                    session_id,
                    panel_id,
                    selection["selectedShapeIds"].to_string(),
                    selection.to_string()
                ],
            )
            .expect("selection");
}

fn tiny_png() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==")
            .expect("tiny png")
}
