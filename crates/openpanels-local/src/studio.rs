use crate::error::CliError;
use crate::paths::OpenPanelsPaths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioSession {
    pub browser_url: Option<String>,
    pub context_dir: String,
    pub context_id: String,
    pub context_id_source: String,
    pub host: Option<String>,
    pub lan_server_urls: Option<Vec<String>>,
    pub local_server_url: Option<String>,
    pub log_path: String,
    pub pid: u32,
    pub port: u16,
    pub project_dir: String,
    pub server_url: String,
    pub started_at: String,
    pub storage_dir: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioStatusPayload {
    pub ok: bool,
    pub context_dir: String,
    pub context_id: String,
    pub context_id_source: String,
    pub log_path: String,
    pub project_dir: String,
    pub server: StudioServerStatus,
    pub session: Option<StudioSession>,
    pub storage_dir: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StudioServerStatus {
    Missing,
    Running,
    Stale,
    Unavailable,
}

pub struct StudioStartOptions {
    pub host: String,
    pub open_browser: bool,
    pub static_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioStartResult {
    pub session: StudioSession,
    pub reused_existing: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioStopResult {
    pub stopped: bool,
    pub unbound: bool,
}

pub(crate) struct StudioServeProcess {
    pub log_path: PathBuf,
    pub pid: u32,
}

pub fn start_studio(
    paths: &OpenPanelsPaths,
    options: StudioStartOptions,
) -> Result<StudioStartResult, CliError> {
    if let Some(session) = reuse_existing_studio(paths, options.open_browser)? {
        return Ok(StudioStartResult {
            session,
            reused_existing: true,
        });
    }

    fs::create_dir_all(&paths.context_dir).map_err(to_cli_error)?;
    let port = find_open_port(&options.host)?;
    let local_server_url = format!("http://127.0.0.1:{port}");
    let server_url = local_server_url.clone();
    let browser_url = server_url.clone();
    let process = spawn_studio_server_process(
        paths,
        port,
        &options.host,
        options.static_dir.as_ref(),
        None,
    )?;

    let session = StudioSession {
        browser_url: Some(browser_url.clone()),
        context_dir: paths.context_dir.display().to_string(),
        context_id: paths.context_id.clone(),
        context_id_source: paths.context_id_source.clone(),
        host: Some(options.host),
        lan_server_urls: Some(Vec::new()),
        local_server_url: Some(local_server_url.clone()),
        log_path: process.log_path.display().to_string(),
        pid: process.pid,
        port,
        project_dir: paths.project_dir.display().to_string(),
        server_url,
        started_at: crate::control::now_iso(),
        storage_dir: paths.storage_dir.display().to_string(),
    };
    write_studio_session(paths, &session)?;
    wait_for_studio(&local_server_url, Duration::from_secs(10))?;
    if options.open_browser {
        open_browser(&browser_url);
    }
    Ok(StudioStartResult {
        session,
        reused_existing: false,
    })
}

pub fn reuse_existing_studio(
    paths: &OpenPanelsPaths,
    open_browser_requested: bool,
) -> Result<Option<StudioSession>, CliError> {
    let current = studio_status(paths)?;
    if current.server == StudioServerStatus::Running {
        if let Some(session) = current.session {
            if open_browser_requested {
                open_browser(
                    session
                        .browser_url
                        .as_deref()
                        .unwrap_or(&session.server_url),
                );
            }
            return Ok(Some(session));
        }
    }

    if let Some(session) = current.session.as_ref() {
        if matches!(
            current.server,
            StudioServerStatus::Unavailable | StudioServerStatus::Stale
        ) {
            cleanup_current_session(paths, session)?;
        }
    }

    let Some(session) = find_running_project_studio(paths)? else {
        return Ok(None);
    };
    write_studio_session(paths, &session)?;
    if open_browser_requested {
        open_browser(
            session
                .browser_url
                .as_deref()
                .unwrap_or(&session.server_url),
        );
    }
    Ok(Some(session))
}

pub(crate) fn spawn_studio_server_process(
    paths: &OpenPanelsPaths,
    port: u16,
    host: &str,
    static_dir: Option<&PathBuf>,
    restart_delay_ms: Option<u64>,
) -> Result<StudioServeProcess, CliError> {
    fs::create_dir_all(&paths.context_dir).map_err(to_cli_error)?;
    let log_path = paths.context_dir.join("studio.log");
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(to_cli_error)?;
    let log_for_stderr = log.try_clone().map_err(to_cli_error)?;
    let mut args = studio_server_args(paths, port, host, static_dir);
    if let Some(delay_ms) = restart_delay_ms {
        args.push("--restart-delay-ms".to_owned());
        args.push(delay_ms.to_string());
    }

    let child = spawn_detached_current_exe(args, log, log_for_stderr)?;
    Ok(StudioServeProcess {
        log_path,
        pid: child.id(),
    })
}

pub(crate) fn find_open_port(host: &str) -> Result<u16, CliError> {
    let listener = TcpListener::bind((host, 0)).map_err(to_cli_error)?;
    let port = listener.local_addr().map_err(to_cli_error)?.port();
    drop(listener);
    Ok(port)
}

pub fn studio_status(paths: &OpenPanelsPaths) -> Result<StudioStatusPayload, CliError> {
    let log_path = paths.context_dir.join("studio.log");
    let session = read_studio_session(paths)?;
    let server = match session.as_ref() {
        None => StudioServerStatus::Missing,
        Some(session) if !process_exists(session.pid) => StudioServerStatus::Stale,
        Some(session) if is_studio_healthy(session) => StudioServerStatus::Running,
        Some(_) => StudioServerStatus::Unavailable,
    };

    Ok(StudioStatusPayload {
        ok: true,
        context_dir: paths.context_dir.display().to_string(),
        context_id: paths.context_id.clone(),
        context_id_source: paths.context_id_source.clone(),
        log_path: log_path.display().to_string(),
        project_dir: paths.project_dir.display().to_string(),
        server,
        session,
        storage_dir: paths.storage_dir.display().to_string(),
    })
}

pub fn wait_for_existing_studio(
    paths: &OpenPanelsPaths,
    timeout: Duration,
) -> Result<StudioSession, CliError> {
    let status = studio_status(paths)?;
    let Some(session) = status.session else {
        return Err(CliError::new("MyOpenPanels studio is not running."));
    };
    let url = session
        .local_server_url
        .as_deref()
        .unwrap_or(&session.server_url)
        .to_owned();
    wait_for_studio(&url, timeout)?;
    Ok(session)
}

pub fn stop_studio_session(paths: &OpenPanelsPaths) -> Result<StudioStopResult, CliError> {
    let Some(session) = read_studio_session(paths)? else {
        return Ok(StudioStopResult {
            stopped: false,
            unbound: false,
        });
    };
    let borrowed = !is_session_owner(paths, &session);
    let path = studio_session_path(paths);
    if borrowed {
        if path.exists() {
            fs::remove_file(path).map_err(to_cli_error)?;
        }
        return Ok(StudioStopResult {
            stopped: false,
            unbound: true,
        });
    }

    terminate_process(session.pid);
    if path.exists() {
        fs::remove_file(path).map_err(to_cli_error)?;
    }
    Ok(StudioStopResult {
        stopped: true,
        unbound: false,
    })
}

fn read_studio_session(paths: &OpenPanelsPaths) -> Result<Option<StudioSession>, CliError> {
    let path = studio_session_path(paths);
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path).map_err(to_cli_error)?;
    serde_json::from_str::<StudioSession>(&content)
        .map(Some)
        .map_err(to_cli_error)
}

pub fn write_studio_session(
    paths: &OpenPanelsPaths,
    session: &StudioSession,
) -> Result<(), CliError> {
    fs::create_dir_all(&paths.context_dir).map_err(to_cli_error)?;
    fs::write(
        studio_session_path(paths),
        format!(
            "{}\n",
            serde_json::to_string_pretty(session).map_err(to_cli_error)?
        ),
    )
    .map_err(to_cli_error)
}

fn studio_session_path(paths: &OpenPanelsPaths) -> PathBuf {
    paths.context_dir.join("studio-session.json")
}

fn find_running_project_studio(paths: &OpenPanelsPaths) -> Result<Option<StudioSession>, CliError> {
    let mut candidates = Vec::new();
    for path in studio_session_paths(paths)? {
        if path == studio_session_path(paths) {
            continue;
        }
        let Some(session) = read_studio_session_file(&path)? else {
            continue;
        };
        if !same_project(paths, &session) {
            continue;
        }
        if !process_exists(session.pid) {
            remove_file_if_exists(&path)?;
            continue;
        }
        if is_studio_healthy(&session) {
            candidates.push(session);
        }
    }
    candidates.sort_by(|left, right| right.started_at.cmp(&left.started_at));
    Ok(candidates.into_iter().next())
}

fn studio_session_paths(paths: &OpenPanelsPaths) -> Result<Vec<PathBuf>, CliError> {
    let contexts_dir = paths.storage_dir.join("contexts");
    if !contexts_dir.exists() {
        return Ok(Vec::new());
    }
    let mut session_paths = Vec::new();
    for entry in fs::read_dir(contexts_dir).map_err(to_cli_error)? {
        let entry = entry.map_err(to_cli_error)?;
        let path = entry.path().join("studio-session.json");
        if path.exists() {
            session_paths.push(path);
        }
    }
    session_paths.sort();
    Ok(session_paths)
}

fn read_studio_session_file(path: &Path) -> Result<Option<StudioSession>, CliError> {
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path).map_err(to_cli_error)?;
    match serde_json::from_str::<StudioSession>(&content) {
        Ok(session) => Ok(Some(session)),
        Err(_) => Ok(None),
    }
}

fn cleanup_current_session(
    paths: &OpenPanelsPaths,
    session: &StudioSession,
) -> Result<(), CliError> {
    if is_session_owner(paths, session) {
        terminate_process(session.pid);
    }
    remove_file_if_exists(&studio_session_path(paths))
}

fn same_project(paths: &OpenPanelsPaths, session: &StudioSession) -> bool {
    PathBuf::from(&session.project_dir) == paths.project_dir
}

fn is_session_owner(paths: &OpenPanelsPaths, session: &StudioSession) -> bool {
    session.context_id == paths.context_id
}

fn remove_file_if_exists(path: &Path) -> Result<(), CliError> {
    if path.exists() {
        fs::remove_file(path).map_err(to_cli_error)?;
    }
    Ok(())
}

fn is_studio_healthy(session: &StudioSession) -> bool {
    let server_url = session
        .local_server_url
        .as_deref()
        .unwrap_or(&session.server_url);
    let url = format!("{}/api/bootstrap", server_url.trim_end_matches('/'));
    ureq::get(&url)
        .call()
        .map(|response| (200..300).contains(&response.status()))
        .unwrap_or(false)
}

fn wait_for_studio(server_url: &str, timeout: Duration) -> Result<(), CliError> {
    let started = Instant::now();
    let mut last_error = "not ready".to_owned();
    while started.elapsed() < timeout {
        match ureq::get(&format!(
            "{}/api/bootstrap",
            server_url.trim_end_matches('/')
        ))
        .call()
        {
            Ok(response) if (200..300).contains(&response.status()) => return Ok(()),
            Ok(response) => last_error = format!("Studio responded with {}", response.status()),
            Err(error) => last_error = error.to_string(),
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(CliError::new(format!(
        "MyOpenPanels studio did not become ready at {server_url}: {last_error}"
    )))
}

fn process_exists(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        let pid_text = pid.to_string();
        let filter = format!("PID eq {pid}");
        let output = Command::new("tasklist")
            .args(["/FI", &filter, "/NH"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        let Ok(output) = output else {
            return false;
        };
        if !output.status.success() {
            return false;
        }
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.split_whitespace().any(|part| part == pid_text))
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

fn terminate_process(pid: u32) {
    if pid == 0 {
        return;
    }
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        for _ in 0..30 {
            if !process_exists(pid) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
        let _ = Command::new("kill")
            .arg("-KILL")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(windows)]
    {
        let pid_text = pid.to_string();
        let _ = Command::new("taskkill")
            .args(["/PID", &pid_text, "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn open_browser(url: &str) {
    let (command, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(target_os = "windows") {
        ("cmd", vec!["/c", "start", "", url])
    } else {
        ("xdg-open", vec![url])
    };
    let _ = Command::new(command)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn studio_server_args(
    paths: &OpenPanelsPaths,
    port: u16,
    host: &str,
    static_dir: Option<&PathBuf>,
) -> Vec<String> {
    let mut args = vec![
        "__serve-studio".to_owned(),
        "--project".to_owned(),
        paths.project_dir.display().to_string(),
        "--storage-dir".to_owned(),
        paths.storage_dir.display().to_string(),
        "--context-id".to_owned(),
        paths.context_id.clone(),
        "--port".to_owned(),
        port.to_string(),
        "--host".to_owned(),
        host.to_owned(),
    ];
    if let Some(static_dir) = static_dir {
        args.push("--static-dir".to_owned());
        args.push(static_dir.display().to_string());
    }
    args
}

fn spawn_detached_current_exe(
    args: Vec<String>,
    stdout: fs::File,
    stderr: fs::File,
) -> Result<Child, CliError> {
    let mut command = Command::new(std::env::current_exe().map_err(to_cli_error)?);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    configure_detached_child(&mut command);
    command.spawn().map_err(to_cli_error)
}

#[cfg(unix)]
fn configure_detached_child(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(windows)]
fn configure_detached_child(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    command.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
}

#[cfg(not(any(unix, windows)))]
fn configure_detached_child(_command: &mut Command) {}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::resolve_openpanels_paths;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn paths_for(project_dir: &Path, storage_dir: &Path, context_id: &str) -> OpenPanelsPaths {
        resolve_openpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some(context_id),
        )
        .expect("paths")
    }

    fn fake_studio_server(request_count: usize) -> (u16, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        let port = listener.local_addr().expect("local addr").port();
        let handle = thread::spawn(move || {
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
        (port, handle)
    }

    fn studio_session(paths: &OpenPanelsPaths, port: u16) -> StudioSession {
        let server_url = format!("http://127.0.0.1:{port}");
        StudioSession {
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
            server_url,
            started_at: "2026-07-09T00:00:00.000Z".to_owned(),
            storage_dir: paths.storage_dir.display().to_string(),
        }
    }

    #[test]
    fn start_reuses_running_studio_from_same_project_context() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let owner_paths = paths_for(&project_dir, &storage_dir, "owner");
        let borrower_paths = paths_for(&project_dir, &storage_dir, "borrower");
        let (port, server) = fake_studio_server(1);
        let owner_session = studio_session(&owner_paths, port);
        write_studio_session(&owner_paths, &owner_session).expect("owner session");

        let result = start_studio(
            &borrower_paths,
            StudioStartOptions {
                host: "127.0.0.1".to_owned(),
                open_browser: false,
                static_dir: None,
            },
        )
        .expect("start");

        assert!(result.reused_existing);
        assert_eq!(result.session.server_url, owner_session.server_url);
        assert_eq!(result.session.context_id, "owner");
        assert!(studio_session_path(&borrower_paths).exists());
        server.join().expect("server thread");
    }

    #[test]
    fn reuse_ignores_running_studio_for_different_project() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let other_project_dir = temp.path().join("other-project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        fs::create_dir_all(&other_project_dir).expect("other project dir");
        let owner_paths = paths_for(&other_project_dir, &storage_dir, "owner");
        let borrower_paths = paths_for(&project_dir, &storage_dir, "borrower");
        let owner_session = studio_session(&owner_paths, 65_000);
        write_studio_session(&owner_paths, &owner_session).expect("owner session");

        let result = reuse_existing_studio(&borrower_paths, false).expect("reuse");

        assert!(result.is_none());
        assert!(!studio_session_path(&borrower_paths).exists());
    }

    #[test]
    fn reuse_removes_stale_sibling_session() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let owner_paths = paths_for(&project_dir, &storage_dir, "owner");
        let borrower_paths = paths_for(&project_dir, &storage_dir, "borrower");
        let mut owner_session = studio_session(&owner_paths, 65_000);
        owner_session.pid = 0;
        write_studio_session(&owner_paths, &owner_session).expect("owner session");

        let result = reuse_existing_studio(&borrower_paths, false).expect("reuse");

        assert!(result.is_none());
        assert!(!studio_session_path(&owner_paths).exists());
    }

    #[test]
    fn stop_unbinds_borrowed_session_without_stopping_owner() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let owner_paths = paths_for(&project_dir, &storage_dir, "owner");
        let borrower_paths = paths_for(&project_dir, &storage_dir, "borrower");
        let owner_session = studio_session(&owner_paths, 65_000);
        write_studio_session(&borrower_paths, &owner_session).expect("borrowed session");

        let result = stop_studio_session(&borrower_paths).expect("stop");

        assert!(!result.stopped);
        assert!(result.unbound);
        assert!(!studio_session_path(&borrower_paths).exists());
    }

    #[test]
    fn stop_removes_owner_session() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&project_dir).expect("project dir");
        let owner_paths = paths_for(&project_dir, &storage_dir, "owner");
        let mut owner_session = studio_session(&owner_paths, 65_000);
        owner_session.pid = 0;
        write_studio_session(&owner_paths, &owner_session).expect("owner session");

        let result = stop_studio_session(&owner_paths).expect("stop");

        assert!(result.stopped);
        assert!(!result.unbound);
        assert!(!studio_session_path(&owner_paths).exists());
    }
}
