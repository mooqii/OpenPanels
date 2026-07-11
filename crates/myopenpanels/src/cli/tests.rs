use super::*;
use base64::Engine;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::fs;
use std::io::Read;
use std::net::TcpListener;
use std::path::Path;
use std::sync::Mutex;
use std::thread;

static TRACE_ENV_LOCK: Mutex<()> = Mutex::new(());

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

fn print_myopenpanels_task_id_command() -> &'static str {
    if cfg!(windows) {
        "<nul set /p dummy=%MYOPENPANELS_TASK_ID%"
    } else {
        "printf \"$MYOPENPANELS_TASK_ID\""
    }
}

fn update_task_in_panel_state(storage_dir: &Path, task_id: &str, fields: &[(&str, Value)]) {
    let connection = Connection::open(storage_dir.join("main.sqlite3")).expect("db");
    let raw: String = connection
        .query_row(
            "SELECT state_json FROM panel_states WHERE state_json LIKE '%\"tasks\"%' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("panel state");
    let mut state = serde_json::from_str::<Value>(&raw).expect("state json");
    let task = state
        .get_mut("tasks")
        .and_then(Value::as_array_mut)
        .and_then(|tasks| {
            tasks
                .iter_mut()
                .find(|task| task.get("id").and_then(Value::as_str) == Some(task_id))
        })
        .expect("task");
    let object = task.as_object_mut().expect("task object");
    for (key, value) in fields {
        object.insert((*key).to_owned(), value.clone());
    }
    connection
        .execute(
            "UPDATE panel_states SET state_json = ? WHERE state_json LIKE '%\"tasks\"%'",
            [serde_json::to_string(&state).expect("state string")],
        )
        .expect("update panel state");
}

fn update_wiki_state_field(storage_dir: &Path, key: &str, value: Value) {
    let connection = Connection::open(storage_dir.join("main.sqlite3")).expect("db");
    let raw: String = connection
        .query_row(
            "SELECT state_json FROM panel_states WHERE state_json LIKE '%\"wikiSpaces\"%' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("wiki panel state");
    let mut state = serde_json::from_str::<Value>(&raw).expect("state json");
    state
        .as_object_mut()
        .expect("wiki state object")
        .insert(key.to_owned(), value);
    connection
        .execute(
            "UPDATE panel_states SET state_json = ? WHERE state_json LIKE '%\"wikiSpaces\"%'",
            [serde_json::to_string(&state).expect("state string")],
        )
        .expect("update wiki panel state");
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
    let _lock = TRACE_ENV_LOCK.lock().expect("trace env lock");
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    let paths = resolve_myopenpanels_paths(
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
    let owner_paths = resolve_myopenpanels_paths(
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
    let owner_paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("owner"),
    )
    .expect("owner paths");
    let borrower_paths = resolve_myopenpanels_paths(
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
    let owner_paths = resolve_myopenpanels_paths(
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

    let (code, stdout, stderr) = run(&[
        "agent",
        "bootstrap",
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
    let bootstrap = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(bootstrap["activePanelSkill"]["id"], "canvas-panel");
    assert_eq!(
        bootstrap["nextRequiredAction"]["command"],
        "myopenpanels agent skill canvas-panel --format json"
    );
}

#[test]
fn agent_bootstrap_emits_focus_guides_and_capabilities() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);

    let (code, stdout, stderr) = run(&[
        "agent",
        "bootstrap",
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
    assert_eq!(payload["protocolVersion"], 2);
    assert_eq!(payload["cliVersion"], VERSION);
    assert_eq!(payload["entrySkill"]["id"], "myopenpanels");
    assert_eq!(payload["entrySkill"]["requiredVersion"], "1.1");
    assert!(payload["entrySkill"]["instruction"]
        .as_str()
        .expect("entry skill instruction")
        .contains("missing or lower"));
    assert_eq!(payload["activePanel"]["kind"], "wiki");
    assert_eq!(
        payload["state"]["wiki"]["agentSkillId"],
        "karpathy-llm-wiki"
    );
    assert!(payload["capabilities"]
        .as_array()
        .expect("capabilities")
        .iter()
        .any(
            |capability| capability["intent"] == "canvas.generation.begin"
                && capability["relatedSkills"][0] == "canvas-panel"
        ));
    assert!(payload["capabilities"]
        .as_array()
        .expect("capabilities")
        .iter()
        .any(|capability| capability["intent"] == "agent.skill.read"));
    assert!(payload["capabilities"]
        .as_array()
        .expect("capabilities")
        .iter()
        .any(|capability| capability["intent"] == "wiki.generation.begin"));
    assert!(payload["availableGuides"]
        .as_array()
        .expect("available guides")
        .iter()
        .all(|guide| guide["id"] != "canvas.image-generation"
            && guide["id"] != "wiki.knowledge-context"
            && guide["id"] != "wiki.generated-documents"));
    assert_eq!(payload["knowledgeContext"]["panelSkillId"], "wiki-panel");
    assert!(payload["knowledgeContext"].get("policy").is_none());
    assert!(payload["suggestedCommands"]
        .as_array()
        .expect("suggested commands")
        .iter()
        .any(|command| {
            command["intent"] == "agent.panelSkill.read"
                && command["command"]
                    .as_str()
                    .unwrap_or("")
                    .contains("agent skill wiki-panel")
        }));
    assert_eq!(payload["activePanelSkill"]["id"], "wiki-panel");
    assert_eq!(payload["activePanelSkill"]["required"], true);
    assert_eq!(
        payload["nextRequiredAction"]["intent"],
        "load-active-panel-skill"
    );
    assert!(payload["availableSkills"]
        .as_array()
        .expect("available skills")
        .iter()
        .any(|skill| skill["skill"]["id"] == "karpathy-llm-wiki"
            && skill["source"] == "builtin"
            && Path::new(skill["localPath"].as_str().unwrap_or("")).ends_with(
                Path::new(".myopenpanels")
                    .join("skills")
                    .join("karpathy-llm-wiki")
                    .join("SKILL.md")
            )));
    assert!(payload["availableSkills"]
        .as_array()
        .expect("available skills")
        .iter()
        .any(
            |skill| skill["skill"]["id"] == "karpathy-llm-wiki-zh" && skill["source"] == "builtin"
        ));
    assert!(storage_dir
        .join("skills")
        .join("karpathy-llm-wiki")
        .join("SKILL.md")
        .exists());

    let (code, stdout, stderr) = run(&[
        "agent",
        "bootstrap",
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
    let bootstrap = serde_json::from_str::<Value>(&stdout).expect("bootstrap json");
    assert_eq!(bootstrap["protocolVersion"], 2);
    assert_eq!(bootstrap["focus"]["panelKind"], "wiki");
    assert!(bootstrap["focus"]["focusRevision"].as_u64().is_some());
    assert!(bootstrap["applicableGuides"]
        .as_array()
        .unwrap()
        .iter()
        .all(|guide| guide["id"] != "wiki.generated-documents"));
    assert_eq!(bootstrap["activeOperations"], json!([]));
    assert!(storage_dir
        .join("skills")
        .join("karpathy-llm-wiki")
        .join("references")
        .join("ingest-markdown-into-wiki.md")
        .exists());
    assert!(storage_dir
        .join("skills")
        .join("wiki-panel")
        .join("references")
        .join("authoring-skill-routing.md")
        .exists());
    assert!(storage_dir
        .join("skills")
        .join("canvas-panel")
        .join("references")
        .join("image-generation.md")
        .exists());

    let (code, stdout, stderr) = run(&[
        "agent",
        "bootstrap",
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
    let bootstrap = serde_json::from_str::<Value>(&stdout).expect("bootstrap json");
    assert_eq!(bootstrap["protocolVersion"], 2);
    assert!(bootstrap["capabilities"].as_array().is_some());

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
    assert!(stdout.contains("tasks.queue"));
    assert!(!stdout.contains("canvas.image-generation"));
    assert!(!stdout.contains("wiki.knowledge-context"));
    assert!(!stdout.contains("wiki.generated-documents"));

    let (code, stdout, stderr) = run(&[
        "agent",
        "skills",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("karpathy-llm-wiki"));
    assert!(stdout.contains(".myopenpanels"));
    assert!(stdout.contains("SKILL.md"));

    let (code, stdout, stderr) = run(&[
        "agent",
        "skills",
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
    let skills_payload = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(skills_payload["skills"].as_array().unwrap().len(), 4);
    assert_eq!(skills_payload["skills"][0]["skill"]["id"], "canvas-panel");
    assert_eq!(skills_payload["skills"][0]["source"], "builtin");
    assert_eq!(
        skills_payload["skills"][1]["skill"]["id"],
        "karpathy-llm-wiki"
    );
    assert_eq!(
        skills_payload["skills"][2]["skill"]["id"],
        "karpathy-llm-wiki-zh"
    );
    assert_eq!(skills_payload["skills"][3]["skill"]["id"], "wiki-panel");
    assert!(skills_payload["skills"][0]["skill"]["taskTypes"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(skills_payload["skills"][3]["skill"]["taskTypes"]
        .as_array()
        .unwrap()
        .is_empty());

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "wiki-panel",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("# Skill: wiki-panel"));
    assert!(stdout.contains("Read `SKILL.md` directly"));
    assert!(stdout.contains("myopenpanels wiki selection read"));

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "canvas-panel",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("# Skill: canvas-panel"));
    assert!(stdout.contains("myopenpanels canvas generation begin"));

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
        serde_json::from_str::<Value>(&stdout).expect("json")["panel"]["kind"],
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
    assert_eq!(next["task"]["agentSkillId"], "karpathy-llm-wiki");
    let task_id = next["task"]["id"].as_str().unwrap();

    let (code, stdout, stderr) = run(&[
        "agent",
        "bootstrap",
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
    let context = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(context["suggestedCommands"]
        .as_array()
        .expect("suggested commands")
        .iter()
        .any(|command| command["intent"] == "agent.skill.read"
            && command["command"].as_str().unwrap_or("").contains(&format!(
                "myopenpanels agent skill karpathy-llm-wiki --task-id {task_id}"
            ))));

    update_wiki_state_field(
        &storage_dir,
        "wikiAgentSkillId",
        json!("karpathy-llm-wiki-zh"),
    );
    let (code, stdout, stderr) = run(&[
        "agent",
        "bootstrap",
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
    let context = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(
        context["state"]["wiki"]["agentSkillId"],
        "karpathy-llm-wiki-zh"
    );
    assert!(context["suggestedCommands"]
        .as_array()
        .expect("suggested commands")
        .iter()
        .any(|command| command["intent"] == "agent.skill.read"
            && command["command"].as_str().unwrap_or("").contains(&format!(
                "myopenpanels agent skill karpathy-llm-wiki --task-id {task_id}"
            ))));
    assert_eq!(
        context["state"]["wiki"]["nextTaskAgentSkillId"],
        "karpathy-llm-wiki"
    );

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "wiki-panel",
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
    let panel_skill = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(panel_skill["markdown"]
        .as_str()
        .unwrap_or("")
        .contains(&format!(
            "agent skill karpathy-llm-wiki --task-id {task_id}"
        )));

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
    assert_eq!(project_tasks["tasks"][0]["maxAttempts"], 3);
    assert!(project_tasks["tasks"][0]["lease"]["owner"].is_null());
    assert!(project_tasks["tasks"][0]["retryAfter"].is_null());
    let (code, stdout, stderr) = run(&[
        "tasks",
        "list",
        "--project",
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
    let leased_next = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(leased_next["task"].is_null());
    let (code, stdout, stderr) = run(&[
        "tasks",
        "list",
        "--project",
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
        "--project",
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
        "tasks",
        "release",
        "--project",
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
        "--project",
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
        "tasks",
        "release",
        "--project",
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
        "tasks",
        "retry",
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

    let (code, stdout, stderr) = run(&[
        "agent",
        "bridge",
        "--project",
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
        "tasks",
        "inspect",
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
    let inspected_task = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(inspected_task["task"]["id"], task_id);
    assert_eq!(inspected_task["task"]["task"]["id"], task_id);

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
    let (code, _, stderr) = run(&[
        "tasks",
        "release",
        "--project",
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
    assert_eq!(code, 0, "{stderr}");

    let (code, _, stderr) = run(&[
        "tasks",
        "retry",
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

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "karpathy-llm-wiki",
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
    assert!(stdout.contains("myopenpanels tasks claim"));
    assert!(stdout.contains("Read `SKILL.md` directly from the local path above"));
    assert!(stdout.contains("# Skill: karpathy-llm-wiki"));

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "karpathy-llm-wiki",
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
    assert_eq!(claim["task"]["attempt"], 1);
    assert_eq!(claim["task"]["leaseOwner"], claim["process"]["id"]);
    assert!(claim["task"]["leaseExpiresAt"].is_string());
    assert!(claim["task"]["lastHeartbeatAt"].is_string());
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
fn wiki_selection_and_query_context_are_agent_facing_without_panel_state_churn() {
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
        "Product brief",
        "--content",
        "# Product brief\n\nMyOpenPanels keeps project knowledge local.",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let document = serde_json::from_str::<Value>(&stdout).expect("json")["document"].clone();

    let page_file = project_dir.join("product.md");
    fs::write(
        &page_file,
        "# MyOpenPanels\n\nMyOpenPanels provides a local indexed Wiki for project knowledge.\n",
    )
    .expect("page file");
    let (code, _stdout, stderr) = run(&[
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
        "concepts/myopenpanels.md",
        "--file",
        page_file.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");

    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let storage = Storage::open(&paths).expect("storage");
    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let wiki_panel = bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");
    let revision_before = storage
        .read_panel_state_revision(&bootstrap.session.id, &wiki_panel.panel.id)
        .expect("panel revision");
    wiki::write_agent_selection(
        &paths,
        true,
        &[document["id"].as_str().unwrap().to_owned()],
        &[],
    )
    .expect("selection");
    let revision_after = Storage::open(&paths)
        .expect("storage")
        .read_panel_state_revision(&bootstrap.session.id, &wiki_panel.panel.id)
        .expect("panel revision");
    assert_eq!(revision_after, revision_before);

    let (code, stdout, stderr) = run(&[
        "wiki",
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
    let selection = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(selection["selection"]["isWikiSelected"], true);
    assert_eq!(selection["selectedRawDocuments"][0]["id"], document["id"]);
    assert!(selection["selectedRawDocuments"][0]["originalFilePath"]
        .as_str()
        .is_some_and(|path| Path::new(path).is_file()));

    let (code, stdout, stderr) = run(&[
        "agent",
        "bootstrap",
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
    let context = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(context["knowledgeContext"]["wiki"]["selected"], true);
    assert_eq!(context["knowledgeContext"]["panelSkillId"], "wiki-panel");
    assert!(context["knowledgeContext"].get("policy").is_none());
    assert_eq!(
        context["knowledgeContext"]["wiki"]["querySkillId"],
        "wiki-panel"
    );
    assert_eq!(
        context["knowledgeContext"]["rawDocuments"]["selected"][0]["documentId"],
        document["id"]
    );

    let (code, stdout, stderr) = run(&[
        "agent",
        "skill",
        "wiki-panel",
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
    let skill = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(skill["markdown"]
        .as_str()
        .unwrap_or("")
        .contains("wiki pages search --wiki-space-id wiki:default"));

    let (code, stdout, stderr) = run(&[
        "wiki",
        "pages",
        "search",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--wiki-space-id",
        "wiki:default",
        "--query",
        "local indexed Wiki",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let search = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(search["matches"][0]["path"], "concepts/myopenpanels.md");
}

#[test]
fn generated_documents_support_versions_selection_publication_and_deletion() {
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

    let created = wiki::create_generated_document(
        &paths,
        "report.md",
        Some("Report"),
        Some("text/markdown"),
        Some("task:1"),
        Some("thread:1"),
        b"# Report\n\nVersion one.",
    )
    .expect("create generated document");
    let document_id = created["document"]["id"]
        .as_str()
        .expect("document id")
        .to_owned();
    assert_eq!(created["document"]["contentVersion"], 1);
    assert!(wiki::create_generated_document(
        &paths,
        "report.pdf",
        None,
        Some("application/pdf"),
        None,
        None,
        b"not a pdf",
    )
    .is_err());

    let bootstrap = read_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let wiki_panel = bootstrap
        .panels
        .iter()
        .find(|snapshot| snapshot.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");
    let revision_before = Storage::open(&paths)
        .expect("storage")
        .read_panel_state_revision(&bootstrap.session.id, &wiki_panel.panel.id)
        .expect("revision");
    let selection =
        wiki::write_agent_selection(&paths, false, &[], &[document_id.clone()]).expect("selection");
    assert_eq!(
        selection["selectedGeneratedDocuments"][0]["id"],
        document_id
    );
    let revision_after = Storage::open(&paths)
        .expect("storage")
        .read_panel_state_revision(&bootstrap.session.id, &wiki_panel.panel.id)
        .expect("revision");
    assert_eq!(revision_after, revision_before);

    let read = wiki::read_generated_document(&paths, &document_id).expect("read");
    assert_eq!(read["content"], "# Report\n\nVersion one.");
    let first_publish =
        wiki::publish_generated_document(&paths, &document_id, None).expect("first publish");
    assert_eq!(first_publish["rawDocument"]["source"], "agent");
    assert!(wiki::publish_generated_document(&paths, &document_id, None).is_err());

    let updated = wiki::write_generated_document(
        &paths,
        &document_id,
        "report.md",
        Some("text/markdown"),
        b"# Report\n\nVersion two.",
    )
    .expect("update");
    assert_eq!(updated["document"]["contentVersion"], 2);
    let second_publish =
        wiki::publish_generated_document(&paths, &document_id, None).expect("second publish");
    assert_eq!(
        second_publish["document"]["publishHistory"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );

    wiki::delete_generated_document(&paths, &document_id).expect("delete");
    let context = wiki::wiki_context(&paths).expect("context");
    assert_eq!(
        context["state"]["rawDocuments"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        wiki::read_agent_selection(&paths).expect("selection")["selectedGeneratedDocuments"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
}

#[test]
fn wiki_mdx_upload_skips_conversion_and_queues_ingest() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    update_wiki_state_field(
        &storage_dir,
        "wikiAgentSkillId",
        json!("karpathy-llm-wiki-zh"),
    );
    let mdx_path = project_dir.join("component.mdx");
    fs::write(
        &mdx_path,
        "# Component\n\n<ComponentPreview name=\"Button\" />\n",
    )
    .expect("mdx file");

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
        mdx_path.to_str().unwrap(),
        "--mime-type",
        "application/octet-stream",
        "--format",
        "json",
    ]);

    assert_eq!(code, 0, "{stderr}");
    let result = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(result["document"]["conversion"]["status"], "not_required");
    assert_eq!(result["document"]["markdownVersion"], 1);
    assert_eq!(
        result["document"]["ingestionByWikiSpace"]["wiki:default"]["status"],
        "queued"
    );
    assert_eq!(
        result["state"]["tasks"][0]["type"],
        "ingest_markdown_into_wiki"
    );
    assert_eq!(
        result["state"]["tasks"][0]["agentSkillId"],
        "karpathy-llm-wiki-zh"
    );
}

#[test]
fn agent_bridge_without_command_does_not_process_wiki_tasks() {
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
        "Bridge Source",
        "--file-name",
        "bridge-source.md",
        "--content",
        "# Bridge Source\n\nContent imported by the built-in worker.",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let created = serde_json::from_str::<Value>(&stdout).expect("json");
    let _task_id = created["document"]["ingestionByWikiSpace"]["wiki:default"]["taskId"]
        .as_str()
        .unwrap();

    let (code, stdout, _stderr) = run(&[
        "agent",
        "bridge",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--once",
        "--format",
        "json",
    ]);
    assert_eq!(code, 1);
    let bridge = serde_json::from_str::<Value>(&stdout).expect("json");
    assert!(bridge["error"]
        .as_str()
        .is_some_and(|message| message.contains("requires --command")));

    let (code, stdout, stderr) = run(&[
        "tasks",
        "list",
        "--project",
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
    let pending = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(pending["pendingCount"], 1);
    assert_eq!(pending["tasks"][0]["status"], "queued");

    let (code, stdout, stderr) = run(&[
        "agent",
        "bridge",
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
    let status = serde_json::from_str::<Value>(&stdout).expect("json");
    assert_eq!(status["status"], "noTarget");
    assert_eq!(status["dispatcher"]["unhandledCount"], 1);
}

#[test]
fn generic_targets_claim_and_complete_project_tasks() {
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
        "Generic Task",
        "--content",
        "# Generic Task\n\nQueue protocol coverage.",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let created = serde_json::from_str::<Value>(&stdout).expect("created json");
    let task_id = created["document"]["ingestionByWikiSpace"]["wiki:default"]["taskId"]
        .as_str()
        .unwrap();

    let (code, stdout, stderr) = run(&[
        "tasks",
        "list",
        "--project",
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
    let unhandled = serde_json::from_str::<Value>(&stdout).expect("tasks json");
    assert_eq!(unhandled["unhandledCount"], 1);
    assert_eq!(unhandled["tasks"][0]["dispatchState"], "noTarget");

    let (code, stdout, stderr) = run(&[
        "agent",
        "targets",
        "register",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--name",
        "test-poller",
        "--transport",
        "poll",
        "--capability",
        "wiki.ingestMarkdown",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let registered = serde_json::from_str::<Value>(&stdout).expect("target json");
    let target_id = registered["target"]["id"].as_str().unwrap();
    assert!(registered["token"].is_string());

    let (code, stdout, stderr) = run(&[
        "tasks",
        "claim-next",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--target-id",
        target_id,
        "--wait-ms",
        "0",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let claimed = serde_json::from_str::<Value>(&stdout).expect("claim json");
    assert_eq!(claimed["task"]["id"], task_id);
    assert_eq!(claimed["task"]["attempt"], 1);
    let lease_token = claimed["leaseToken"].as_str().unwrap();

    let (code, _, _) = run(&[
        "tasks",
        "heartbeat",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--task-id",
        task_id,
        "--lease-token",
        "wrong-token",
        "--format",
        "json",
    ]);
    assert_eq!(code, 1);

    let result_file = temp.path().join("result.json");
    fs::write(&result_file, r#"{"executor":"test-poller"}"#).expect("result file");
    let (code, stdout, stderr) = run(&[
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
        "--lease-token",
        lease_token,
        "--result-file",
        result_file.to_str().unwrap(),
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let completed = serde_json::from_str::<Value>(&stdout).expect("complete json");
    assert_eq!(completed["task"]["status"], "succeeded");
    assert_eq!(completed["task"]["result"]["executor"], "test-poller");

    let (code, stdout, stderr) = run(&[
        "tasks",
        "list",
        "--project",
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
    let pending = serde_json::from_str::<Value>(&stdout).expect("pending json");
    assert_eq!(pending["pendingCount"], 0);
}

#[test]
fn claim_next_respects_target_priority() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let common = [
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--format",
        "json",
    ];
    let (code, _, stderr) = run(&[
        "wiki",
        "documents",
        "create-markdown",
        "--title",
        "Priority",
        "--content",
        "# Priority",
        common[0],
        common[1],
        common[2],
        common[3],
        common[4],
        common[5],
        common[6],
        common[7],
    ]);
    assert_eq!(code, 0, "{stderr}");

    let register = |name: &str, priority: &str| {
        run(&[
            "agent",
            "targets",
            "register",
            "--name",
            name,
            "--transport",
            "poll",
            "--capability",
            "wiki.ingestMarkdown",
            "--priority",
            priority,
            common[0],
            common[1],
            common[2],
            common[3],
            common[4],
            common[5],
            common[6],
            common[7],
        ])
    };
    let (code, stdout, stderr) = register("fallback", "-100");
    assert_eq!(code, 0, "{stderr}");
    let fallback = serde_json::from_str::<Value>(&stdout).expect("fallback target");
    let fallback_id = fallback["target"]["id"].as_str().unwrap();
    let (code, stdout, stderr) = register("preferred", "10");
    assert_eq!(code, 0, "{stderr}");
    let preferred = serde_json::from_str::<Value>(&stdout).expect("preferred target");
    let preferred_id = preferred["target"]["id"].as_str().unwrap();

    let claim = |target_id: &str| {
        run(&[
            "tasks",
            "claim-next",
            "--target-id",
            target_id,
            "--wait-ms",
            "0",
            common[0],
            common[1],
            common[2],
            common[3],
            common[4],
            common[5],
            common[6],
            common[7],
        ])
    };
    let (code, stdout, stderr) = claim(fallback_id);
    assert_eq!(code, 0, "{stderr}");
    let fallback_claim = serde_json::from_str::<Value>(&stdout).expect("fallback claim");
    assert!(fallback_claim["task"].is_null());

    let (code, stdout, stderr) = claim(preferred_id);
    assert_eq!(code, 0, "{stderr}");
    let preferred_claim = serde_json::from_str::<Value>(&stdout).expect("preferred claim");
    assert_eq!(preferred_claim["task"]["capability"], "wiki.ingestMarkdown");
}

#[test]
fn command_bridge_owns_task_lifecycle_by_default() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let (code, _, stderr) = run(&[
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
        "Bridge Lifecycle",
        "--content",
        "# Bridge Lifecycle",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");

    let (code, stdout, stderr) = run(&[
        "agent",
        "bridge",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--name",
        "lifecycle-bridge",
        "--capability",
        "wiki.ingestMarkdown",
        "--once",
        "--command",
        "printf '{\"result\":{\"executor\":\"command-test\"}}'",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let bridge = serde_json::from_str::<Value>(&stdout).expect("bridge json");
    assert_eq!(bridge["success"], true);
    assert_eq!(bridge["lifecycle"]["task"]["status"], "succeeded");
    assert_eq!(
        bridge["lifecycle"]["task"]["result"]["executor"],
        "command-test"
    );
}

#[test]
fn webhook_delivery_is_signed_and_does_not_claim_the_task() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let (code, _, stderr) = run(&[
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
        "Webhook Task",
        "--content",
        "# Webhook Task",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");

    let listener = TcpListener::bind("127.0.0.1:0").expect("webhook listener");
    let port = listener.local_addr().expect("webhook addr").port();
    let (sender, receiver) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("webhook accept");
        let mut request = Vec::new();
        loop {
            let mut buffer = [0_u8; 2048];
            let read = stream.read(&mut buffer).expect("webhook request");
            request.extend_from_slice(&buffer[..read]);
            let Some(header_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
                continue;
            };
            let header_text = String::from_utf8_lossy(&request[..header_end]);
            let content_length = header_text
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            if request.len() >= header_end + 4 + content_length {
                break;
            }
        }
        sender
            .send(String::from_utf8_lossy(&request).to_string())
            .expect("captured request");
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\ncontent-length: 0\r\n\r\n")
            .expect("webhook response");
    });
    let endpoint = format!("http://127.0.0.1:{port}/wake");
    let (code, _, stderr) = run(&[
        "agent",
        "targets",
        "register",
        "--project",
        project_dir.to_str().unwrap(),
        "--storage-dir",
        storage_dir.to_str().unwrap(),
        "--context-id",
        "ctx",
        "--name",
        "webhook-test",
        "--transport",
        "webhook",
        "--endpoint",
        &endpoint,
        "--capability",
        "wiki.ingestMarkdown",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let dispatched =
        crate::tasks::dispatch_webhooks_once(&paths, "http://127.0.0.1:9999").expect("dispatch");
    assert_eq!(dispatched["delivered"], 1);
    let request = receiver.recv().expect("webhook request capture");
    assert!(request
        .to_ascii_lowercase()
        .contains("x-myopenpanels-signature: sha256="));
    assert!(request.contains("wiki.ingestMarkdown"));
    server.join().expect("webhook server");

    let (code, stdout, stderr) = run(&[
        "tasks",
        "list",
        "--project",
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
    let tasks = serde_json::from_str::<Value>(&stdout).expect("tasks json");
    assert_eq!(tasks["tasks"][0]["attempt"], 0);
    assert_eq!(tasks["tasks"][0]["lastDelivery"]["status"], "sent");
}

#[test]
fn concurrent_claim_next_assigns_a_task_once() {
    let temp = tempfile::tempdir().expect("temp dir");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join(".myopenpanels");
    fs::create_dir_all(&project_dir).expect("project dir");
    create_cli_project(&project_dir, &storage_dir);
    let (code, _, stderr) = run(&[
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
        "Concurrent Claim",
        "--content",
        "# Concurrent Claim",
        "--format",
        "json",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let paths = resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("ctx"),
    )
    .expect("paths");
    let registered = crate::tasks::register_target(
        &paths,
        crate::tasks::TargetRegistration {
            name: "concurrent-poller",
            host: Some("test"),
            transport: "poll",
            endpoint: None,
            capabilities: vec!["wiki.ingestMarkdown".to_owned()],
            priority: 0,
        },
    )
    .expect("target");
    let target_id = registered["target"]["id"].as_str().unwrap().to_owned();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let paths = paths.clone();
        let target_id = target_id.clone();
        let barrier = barrier.clone();
        workers.push(thread::spawn(move || {
            barrier.wait();
            crate::tasks::claim_next(&paths, &target_id, None, Some(0)).expect("claim")
        }));
    }
    barrier.wait();
    let results = workers
        .into_iter()
        .map(|worker| worker.join().expect("worker"))
        .collect::<Vec<_>>();
    assert_eq!(
        results
            .iter()
            .filter(|payload| !payload["task"].is_null())
            .count(),
        1
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
        .join(crate::paths::sanitize_path_part("session:1"))
        .join("panels")
        .join(crate::paths::sanitize_path_part("panel:canvas"))
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
fn version_ignores_inherited_trace_url() {
    let _lock = TRACE_ENV_LOCK.lock().expect("trace env lock");
    let previous = std::env::var_os("MYOPENPANELS_TRACE_URL");
    std::env::set_var(
        "MYOPENPANELS_TRACE_URL",
        "http://127.0.0.1:9/api/trace/events",
    );
    let argv = vec!["--version".to_owned()];

    assert_eq!(trace_url_for_cli(&argv), None);

    if let Some(previous) = previous {
        std::env::set_var("MYOPENPANELS_TRACE_URL", previous);
    } else {
        std::env::remove_var("MYOPENPANELS_TRACE_URL");
    }
}

#[test]
fn help_prints_current_command_map() {
    let (code, stdout, stderr) = run(&[]);

    assert_eq!(code, 0);
    assert!(stdout.contains("myopenpanels <command> [options]"));
    assert!(stdout.contains("studio start"));
    assert!(stdout.contains("canvas image insert"));
    assert!(stdout.contains("agent bootstrap"));
    assert!(stdout.contains("update check"));
    assert!(!stdout.contains("agent-context"));
    assert!(!stdout.contains("agent context"));
    assert!(!stdout.contains("active-panel"));
    assert!(!stdout.contains("insert-image"));
    assert_eq!(stderr, "");
}

#[test]
fn legacy_and_implicit_aliases_are_rejected() {
    let commands: &[&[&str]] = &[
        &["agent-context"],
        &["agent", "context"],
        &["panels"],
        &["active-panel"],
        &["panel-state"],
        &["canvas-state"],
        &["selection"],
        &["read-selection-asset"],
        &["insert-placeholder"],
        &["insert-image"],
        &["agent"],
        &["agent", "targets"],
        &["project"],
        &["panel"],
        &["canvas"],
        &["canvas", "selection"],
        &["wiki"],
        &["wiki", "selection"],
        &["wiki", "documents"],
        &["wiki", "tasks"],
        &["wiki", "spaces"],
        &["wiki", "pages"],
        &["tasks"],
    ];

    for command in commands {
        let mut args = command.to_vec();
        args.push("--format=json");
        let (code, stdout, stderr) = run(&args);
        assert_eq!(code, 1, "command unexpectedly succeeded: {command:?}");
        assert_eq!(stderr, "", "unexpected stderr for {command:?}");
        assert!(
            stdout.contains("\"ok\": false"),
            "missing JSON error for {command:?}: {stdout}"
        );
    }
}

#[test]
fn update_help_prints_manifest_controls() {
    let (code, stdout, stderr) = run(&["update", "help"]);

    assert_eq!(code, 0);
    assert!(stdout.contains("myopenpanels update"));
    assert!(stdout.contains("MYOPENPANELS_UPDATE_MANIFEST_URL"));
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
