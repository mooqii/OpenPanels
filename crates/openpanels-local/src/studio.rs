use crate::error::CliError;
use crate::paths::OpenPanelsPaths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
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

pub fn start_studio(
    paths: &OpenPanelsPaths,
    options: StudioStartOptions,
) -> Result<StudioSession, CliError> {
    let current = studio_status(paths)?;
    if current.server == StudioServerStatus::Running {
        if let Some(session) = current.session {
            return Ok(session);
        }
    }
    if let Some(session) = current.session {
        if matches!(
            current.server,
            StudioServerStatus::Unavailable | StudioServerStatus::Stale
        ) {
            terminate_process(session.pid);
        }
    }

    fs::create_dir_all(&paths.context_dir).map_err(to_cli_error)?;
    let port = find_open_port(&options.host)?;
    let local_server_url = format!("http://127.0.0.1:{port}");
    let server_url = local_server_url.clone();
    let browser_url = server_url.clone();
    let log_path = paths.context_dir.join("studio.log");
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(to_cli_error)?;
    let log_for_stderr = log.try_clone().map_err(to_cli_error)?;
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
        options.host.clone(),
    ];
    if let Some(static_dir) = &options.static_dir {
        args.push("--static-dir".to_owned());
        args.push(static_dir.display().to_string());
    }

    let child = Command::new(std::env::current_exe().map_err(to_cli_error)?)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_for_stderr))
        .spawn()
        .map_err(to_cli_error)?;

    let session = StudioSession {
        browser_url: Some(browser_url.clone()),
        context_dir: paths.context_dir.display().to_string(),
        context_id: paths.context_id.clone(),
        context_id_source: paths.context_id_source.clone(),
        host: Some(options.host),
        lan_server_urls: Some(Vec::new()),
        local_server_url: Some(local_server_url.clone()),
        log_path: log_path.display().to_string(),
        pid: child.id(),
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
    Ok(session)
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

pub fn stop_studio(paths: &OpenPanelsPaths) -> Result<(), CliError> {
    if let Some(session) = read_studio_session(paths)? {
        terminate_process(session.pid);
    }
    let path = studio_session_path(paths);
    if path.exists() {
        fs::remove_file(path).map_err(to_cli_error)?;
    }
    Ok(())
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

fn write_studio_session(paths: &OpenPanelsPaths, session: &StudioSession) -> Result<(), CliError> {
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

fn find_open_port(host: &str) -> Result<u16, CliError> {
    let listener = TcpListener::bind((host, 0)).map_err(to_cli_error)?;
    let port = listener.local_addr().map_err(to_cli_error)?.port();
    drop(listener);
    Ok(port)
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
    #[cfg(not(unix))]
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

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
