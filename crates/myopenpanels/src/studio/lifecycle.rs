use crate::error::CliError;
use crate::paths::{resolve_studio_service_paths, MyOpenPanelsPaths};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

const STUDIO_HEALTH_REQUEST_TIMEOUT_MS: u64 = 700;
const STUDIO_TRANSITION_LOCK_TIMEOUT: Duration = Duration::from_secs(10);
const STUDIO_TRANSITION_LOCK_STALE_AFTER: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioSession {
    pub system_browser_url: Option<String>,
    pub host: Option<String>,
    pub lan_server_urls: Option<Vec<String>>,
    pub local_server_url: Option<String>,
    pub log_path: String,
    pub pid: u32,
    pub port: u16,
    pub server_url: String,
    pub started_at: String,
    pub storage_dir: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioStatusPayload {
    pub ok: bool,
    pub log_path: String,
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
    pub static_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioStartResult {
    pub session: StudioSession,
    pub reused_existing: bool,
    pub server_version: String,
    pub lifecycle: StudioLifecycle,
    pub previous_version: Option<String>,
    pub browser_refresh_required: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StudioLifecycle {
    Reused,
    Started,
    VersionRestarted,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StudioHealth {
    ok: bool,
    version: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum StudioVersionRelation {
    Current,
    Older,
    Newer,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioStopResult {
    pub stopped: bool,
}

pub(crate) struct StudioServeProcess {
    pub log_path: PathBuf,
    pub pid: u32,
}

pub(crate) struct StudioTransitionLock {
    path: PathBuf,
}

impl Drop for StudioTransitionLock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.path);
    }
}

pub fn start_studio(
    paths: &MyOpenPanelsPaths,
    options: StudioStartOptions,
) -> Result<StudioStartResult, CliError> {
    let _transition_lock = acquire_studio_transition_lock(paths)?;
    crate::context_cleanup::cleanup_context_storage(paths);
    if let Some(session) = reuse_existing_studio(paths)? {
        let previous_version = studio_version(&session)?;
        match compare_studio_version(previous_version.as_deref())? {
            StudioVersionRelation::Current => {
                return Ok(StudioStartResult {
                    session,
                    reused_existing: true,
                    server_version: env!("CARGO_PKG_VERSION").to_owned(),
                    lifecycle: StudioLifecycle::Reused,
                    previous_version: None,
                    browser_refresh_required: false,
                });
            }
            StudioVersionRelation::Newer => {
                let version = previous_version.as_deref().unwrap_or("unknown");
                return Err(CliError::with_code(
                    "studio_version_mismatch",
                    format!(
                        "Running MyOpenPanels Studio {version} is newer than CLI {}. Update the CLI before starting Studio.",
                        env!("CARGO_PKG_VERSION")
                    ),
                ));
            }
            StudioVersionRelation::Older => {
                let host = session.host.clone().unwrap_or_else(|| options.host.clone());
                let port = session.port;
                terminate_process(session.pid);
                remove_file_if_exists(&studio_session_path(paths))?;
                return launch_studio(
                    paths,
                    &host,
                    port,
                    options.static_dir.as_ref(),
                    StudioLifecycle::VersionRestarted,
                    previous_version,
                );
            }
        }
    }

    let port = find_open_port(&options.host)?;
    launch_studio(
        paths,
        &options.host,
        port,
        options.static_dir.as_ref(),
        StudioLifecycle::Started,
        None,
    )
}

fn launch_studio(
    paths: &MyOpenPanelsPaths,
    host: &str,
    port: u16,
    static_dir: Option<&PathBuf>,
    lifecycle: StudioLifecycle,
    previous_version: Option<String>,
) -> Result<StudioStartResult, CliError> {
    let service_paths = resolve_studio_service_paths(paths)?;
    fs::create_dir_all(&service_paths.context_dir).map_err(to_cli_error)?;
    let local_server_url = format!("http://127.0.0.1:{port}");
    let server_url = local_server_url.clone();
    let system_browser_url = server_url.clone();
    let process = spawn_studio_server_process(&service_paths, port, host, static_dir, None)?;

    let session = StudioSession {
        system_browser_url: Some(system_browser_url.clone()),
        host: Some(host.to_owned()),
        lan_server_urls: Some(Vec::new()),
        local_server_url: Some(local_server_url.clone()),
        log_path: process.log_path.display().to_string(),
        pid: process.pid,
        port,
        server_url,
        started_at: crate::control::now_iso(),
        storage_dir: service_paths.storage_dir.display().to_string(),
    };
    write_studio_session(paths, &session)?;
    if let Err(error) = wait_for_studio_version(
        &local_server_url,
        env!("CARGO_PKG_VERSION"),
        Duration::from_secs(10),
    ) {
        terminate_process(process.pid);
        let _ = remove_file_if_exists(&studio_session_path(paths));
        return Err(error);
    }
    Ok(StudioStartResult {
        session,
        reused_existing: false,
        server_version: env!("CARGO_PKG_VERSION").to_owned(),
        lifecycle,
        previous_version,
        browser_refresh_required: lifecycle == StudioLifecycle::VersionRestarted,
    })
}

pub fn reuse_existing_studio(paths: &MyOpenPanelsPaths) -> Result<Option<StudioSession>, CliError> {
    let current = studio_status(paths)?;
    if current.server == StudioServerStatus::Running {
        if let Some(session) = current.session {
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

    Ok(None)
}

pub(crate) fn spawn_studio_server_process(
    paths: &MyOpenPanelsPaths,
    port: u16,
    host: &str,
    static_dir: Option<&PathBuf>,
    restart_delay_ms: Option<u64>,
) -> Result<StudioServeProcess, CliError> {
    fs::create_dir_all(&paths.studio_dir).map_err(to_cli_error)?;
    let log_path = paths.studio_dir.join("studio.log");
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

pub fn studio_status(paths: &MyOpenPanelsPaths) -> Result<StudioStatusPayload, CliError> {
    let log_path = paths.studio_dir.join("studio.log");
    let session = read_studio_session(paths)?;
    let server = match session.as_ref() {
        None => StudioServerStatus::Missing,
        Some(session) if !process_exists(session.pid) => StudioServerStatus::Stale,
        Some(session) if is_studio_healthy(session) => StudioServerStatus::Running,
        Some(_) => StudioServerStatus::Unavailable,
    };

    Ok(StudioStatusPayload {
        ok: true,
        log_path: log_path.display().to_string(),
        server,
        session,
        storage_dir: paths.storage_dir.display().to_string(),
    })
}

pub fn resolve_current_studio_session(
    paths: &MyOpenPanelsPaths,
) -> Result<Option<StudioSession>, CliError> {
    if let Some(session) = read_studio_session(paths)? {
        if process_exists(session.pid) && is_studio_healthy(&session) {
            return Ok(Some(session));
        }
    }
    Ok(None)
}

pub fn wait_for_existing_studio(
    paths: &MyOpenPanelsPaths,
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

pub fn stop_studio_session(paths: &MyOpenPanelsPaths) -> Result<StudioStopResult, CliError> {
    let _transition_lock = acquire_studio_transition_lock(paths)?;
    let Some(session) = read_studio_session(paths)? else {
        return Ok(StudioStopResult { stopped: false });
    };
    let path = studio_session_path(paths);
    terminate_process(session.pid);
    if path.exists() {
        fs::remove_file(path).map_err(to_cli_error)?;
    }
    Ok(StudioStopResult { stopped: true })
}

pub(crate) fn acquire_studio_transition_lock(
    paths: &MyOpenPanelsPaths,
) -> Result<StudioTransitionLock, CliError> {
    fs::create_dir_all(&paths.studio_dir).map_err(to_cli_error)?;
    let path = paths.studio_dir.join("transition.lock");
    let started = Instant::now();
    loop {
        match fs::create_dir(&path) {
            Ok(()) => return Ok(StudioTransitionLock { path }),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let stale = fs::metadata(&path)
                    .and_then(|metadata| metadata.modified())
                    .ok()
                    .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                    .is_some_and(|age| age >= STUDIO_TRANSITION_LOCK_STALE_AFTER);
                if stale {
                    let _ = fs::remove_dir_all(&path);
                    continue;
                }
                if started.elapsed() >= STUDIO_TRANSITION_LOCK_TIMEOUT {
                    return Err(CliError::with_code(
                        "studio_transition_busy",
                        "Another MyOpenPanels process is starting, stopping, or restarting Studio. Retry shortly.",
                    ));
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(to_cli_error(error)),
        }
    }
}

fn read_studio_session(paths: &MyOpenPanelsPaths) -> Result<Option<StudioSession>, CliError> {
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
    paths: &MyOpenPanelsPaths,
    session: &StudioSession,
) -> Result<(), CliError> {
    fs::create_dir_all(&paths.studio_dir).map_err(to_cli_error)?;
    let mut file = tempfile::NamedTempFile::new_in(&paths.studio_dir).map_err(to_cli_error)?;
    file.write_all(
        format!(
            "{}\n",
            serde_json::to_string_pretty(session).map_err(to_cli_error)?
        )
        .as_bytes(),
    )
    .map_err(to_cli_error)?;
    file.persist(studio_session_path(paths))
        .map(|_| ())
        .map_err(to_cli_error)
}

fn studio_session_path(paths: &MyOpenPanelsPaths) -> PathBuf {
    paths.studio_dir.join("instance.json")
}

fn cleanup_current_session(
    paths: &MyOpenPanelsPaths,
    session: &StudioSession,
) -> Result<(), CliError> {
    terminate_process(session.pid);
    remove_file_if_exists(&studio_session_path(paths))
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
    is_studio_url_healthy(server_url)
}

fn is_studio_url_healthy(server_url: &str) -> bool {
    match studio_get(server_url, "/api/health") {
        Ok(response) => (200..300).contains(&response.status()),
        Err(ureq::Error::Status(404, _)) => match studio_get(server_url, "/api/bootstrap") {
            Ok(response) => (200..300).contains(&response.status()),
            Err(_) => false,
        },
        Err(_) => false,
    }
}

pub(crate) fn studio_version(session: &StudioSession) -> Result<Option<String>, CliError> {
    let server_url = session
        .local_server_url
        .as_deref()
        .unwrap_or(&session.server_url);
    let response = studio_get(server_url, "/api/health").map_err(to_cli_error)?;
    if !(200..300).contains(&response.status()) {
        return Err(CliError::new(format!(
            "MyOpenPanels Studio health check returned {}.",
            response.status()
        )));
    }
    let health = response.into_json::<StudioHealth>().map_err(to_cli_error)?;
    if !health.ok {
        return Err(CliError::new(
            "MyOpenPanels Studio reported an unhealthy state.",
        ));
    }
    Ok(health.version)
}

fn compare_studio_version(server_version: Option<&str>) -> Result<StudioVersionRelation, CliError> {
    let Some(server_version) = server_version else {
        return Ok(StudioVersionRelation::Older);
    };
    let server = Version::parse(server_version.trim_start_matches('v')).map_err(|error| {
        CliError::with_code(
            "studio_version_mismatch",
            format!("Running Studio returned invalid version `{server_version}`: {error}"),
        )
    })?;
    let cli = Version::parse(env!("CARGO_PKG_VERSION")).map_err(to_cli_error)?;
    Ok(match server.cmp(&cli) {
        std::cmp::Ordering::Less => StudioVersionRelation::Older,
        std::cmp::Ordering::Greater => StudioVersionRelation::Newer,
        std::cmp::Ordering::Equal => StudioVersionRelation::Current,
    })
}

#[allow(clippy::result_large_err)]
fn studio_get(server_url: &str, path: &str) -> Result<ureq::Response, ureq::Error> {
    let url = format!("{}{}", server_url.trim_end_matches('/'), path);
    ureq::AgentBuilder::new()
        .timeout(Duration::from_millis(STUDIO_HEALTH_REQUEST_TIMEOUT_MS))
        .build()
        .get(&url)
        .call()
}

fn wait_for_studio(server_url: &str, timeout: Duration) -> Result<(), CliError> {
    let started = Instant::now();
    let mut last_error = "not ready".to_owned();
    while started.elapsed() < timeout {
        match studio_get(server_url, "/api/health") {
            Ok(response) if (200..300).contains(&response.status()) => return Ok(()),
            Ok(response) => last_error = format!("Studio responded with {}", response.status()),
            Err(ureq::Error::Status(404, _)) => match studio_get(server_url, "/api/bootstrap") {
                Ok(response) if (200..300).contains(&response.status()) => return Ok(()),
                Ok(response) => {
                    last_error = format!("Studio responded with {}", response.status());
                }
                Err(error) => last_error = error.to_string(),
            },
            Err(error) => last_error = error.to_string(),
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(CliError::new(format!(
        "MyOpenPanels studio did not become ready at {server_url}: {last_error}"
    )))
}

fn wait_for_studio_version(
    server_url: &str,
    expected_version: &str,
    timeout: Duration,
) -> Result<(), CliError> {
    let started = Instant::now();
    let mut last_error = "not ready".to_owned();
    while started.elapsed() < timeout {
        match studio_get(server_url, "/api/health") {
            Ok(response) if (200..300).contains(&response.status()) => {
                match response.into_json::<StudioHealth>() {
                    Ok(health)
                        if health.ok && health.version.as_deref() == Some(expected_version) =>
                    {
                        return Ok(());
                    }
                    Ok(health) => {
                        last_error = format!(
                            "expected version {expected_version}, got {}",
                            health.version.as_deref().unwrap_or("unknown")
                        );
                    }
                    Err(error) => last_error = error.to_string(),
                }
            }
            Ok(response) => last_error = format!("Studio responded with {}", response.status()),
            Err(error) => last_error = error.to_string(),
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(CliError::with_recovery(
        "studio_version_mismatch",
        format!(
            "MyOpenPanels Studio did not start version {expected_version} at {server_url}: {last_error}"
        ),
        true,
        "Retry `myopenpanels studio start --project-dir <project> --format json` after checking studio.log.",
    ))
}

pub(crate) fn process_exists(pid: u32) -> bool {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BrowserPlatform {
    Macos,
    Windows,
    Other,
}

pub(crate) fn open_browser(url: &str) -> Result<(), CliError> {
    open_browser_with(url, current_browser_platform(), |program, args| {
        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
    })
}

fn current_browser_platform() -> BrowserPlatform {
    if cfg!(target_os = "macos") {
        BrowserPlatform::Macos
    } else if cfg!(target_os = "windows") {
        BrowserPlatform::Windows
    } else {
        BrowserPlatform::Other
    }
}

fn browser_command(platform: BrowserPlatform, url: &str) -> (&'static str, Vec<String>) {
    match platform {
        BrowserPlatform::Macos => ("open", vec![url.to_owned()]),
        BrowserPlatform::Windows => (
            "cmd",
            vec![
                "/c".to_owned(),
                "start".to_owned(),
                String::new(),
                url.to_owned(),
            ],
        ),
        BrowserPlatform::Other => ("xdg-open", vec![url.to_owned()]),
    }
}

fn open_browser_with(
    url: &str,
    platform: BrowserPlatform,
    launch: impl FnOnce(&str, &[String]) -> std::io::Result<bool>,
) -> Result<(), CliError> {
    let (program, args) = browser_command(platform, url);
    match launch(program, &args) {
        Ok(true) => Ok(()),
        Ok(false) => Err(browser_open_error(
            url,
            format!("{program} exited with a non-zero status"),
        )),
        Err(error) => Err(browser_open_error(
            url,
            format!("failed to launch {program}: {error}"),
        )),
    }
}

fn browser_open_error(url: &str, detail: String) -> CliError {
    CliError::with_recovery(
        "browser_open_failed",
        format!("Failed to open the MyOpenPanels Studio URL {url}: {detail}"),
        true,
        format!("Open {url} manually in a browser, or fix the system browser launcher and retry."),
    )
}

fn studio_server_args(
    paths: &MyOpenPanelsPaths,
    port: u16,
    host: &str,
    static_dir: Option<&PathBuf>,
) -> Vec<String> {
    let mut args = vec![
        "__serve-studio".to_owned(),
        "--project-dir".to_owned(),
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
