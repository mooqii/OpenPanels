use crate::error::CliError;
use flate2::read::GzDecoder;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const DEFAULT_MANIFEST_URL: &str =
    "https://github.com/mooqii/OpenPanels/releases/latest/download/myopenpanels-manifest.json";
pub const UPDATE_CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;
const UPDATE_HTTP_TIMEOUT_SECS: u64 = 60;
const VERSION_CHECK_TIMEOUT_SECS: u64 = 5;

const BINARY_NAME: &str = if cfg!(windows) {
    "myopenpanels.exe"
} else {
    "myopenpanels"
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateManifest {
    pub schema_version: u32,
    pub name: String,
    pub version: String,
    pub channel: Option<String>,
    pub entry_skill: EntrySkillRelease,
    pub assets: BTreeMap<String, ReleaseAsset>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntrySkillRelease {
    pub id: String,
    pub version: String,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    pub url: String,
    pub sha256: String,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckPayload {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub channel: Option<String>,
    pub target: String,
    pub asset_available: bool,
    pub manifest_url: String,
    pub checked_at_unix: u64,
    pub cached: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInstallPayload {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub updated: bool,
    pub studio_restart_required: bool,
    pub target: String,
    pub manifest_url: String,
    pub installed_path: Option<String>,
    pub entry_skill_update_reminder: EntrySkillUpdateReminder,
    pub next_actions: Vec<UpdateInstallAction>,
    pub next_required_action: UpdateInstallNextRequiredAction,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntrySkillUpdateReminder {
    pub comparison_required: bool,
    pub id: String,
    pub version: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInstallAction {
    pub intent: String,
    pub executor: String,
    pub load_when: String,
    pub instruction: String,
    pub skill: EntrySkillRelease,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInstallNextRequiredAction {
    pub intent: String,
    pub instruction: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDownloadPayload {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub downloaded: bool,
    pub target: String,
    pub manifest_url: String,
    pub archive_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedAsset {
    version: String,
    target: String,
    url: String,
    sha256: String,
    size: Option<u64>,
    archive_path: String,
    downloaded_at_unix: u64,
}

pub fn check_for_update(
    current_version: &str,
    use_cache: bool,
) -> Result<UpdateCheckPayload, CliError> {
    let manifest_url = manifest_url();
    let target = current_target();

    if use_cache {
        if let Some(cached) = read_cached_update_check(&manifest_url, &target, current_version)? {
            if !is_cache_stale(cached.checked_at_unix) {
                return Ok(UpdateCheckPayload {
                    cached: true,
                    ..cached
                });
            }
        }
    }

    let manifest = fetch_manifest(&manifest_url)?;
    let checked = check_from_manifest(current_version, manifest, manifest_url, target)?;
    write_cached_update_check(&checked)?;
    Ok(checked)
}

pub fn install_update(current_version: &str) -> Result<UpdateInstallPayload, CliError> {
    let manifest_url = manifest_url();
    let target = current_target();
    let manifest = fetch_manifest(&manifest_url)?;
    let checked = check_from_manifest(
        current_version,
        manifest.clone(),
        manifest_url.clone(),
        target.clone(),
    )?;
    write_cached_update_check(&checked)?;

    if !checked.update_available {
        return Ok(update_install_payload(
            current_version,
            checked.latest_version,
            false,
            false,
            target,
            manifest_url,
            None,
            manifest.entry_skill,
        ));
    }

    let asset = manifest.assets.get(&target).ok_or_else(|| {
        CliError::new(format!(
            "No release asset is available for target `{target}` in manifest {manifest_url}."
        ))
    })?;

    ensure_self_update_allowed()?;

    let archive_bytes = read_or_download_asset(asset, checked.latest_version.as_deref(), &target)?;

    let temp_dir = tempfile::tempdir().map_err(to_cli_error)?;
    let candidate = extract_binary(&archive_bytes, &asset.url, temp_dir.path())?;
    verify_candidate_version(&candidate, checked.latest_version.as_deref())?;
    self_replace::self_replace(&candidate).map_err(to_cli_error)?;

    Ok(update_install_payload(
        current_version,
        checked.latest_version,
        true,
        true,
        target,
        manifest_url,
        Some(current_exe_display()?),
        manifest.entry_skill,
    ))
}

#[allow(clippy::too_many_arguments)]
fn update_install_payload(
    current_version: &str,
    latest_version: Option<String>,
    updated: bool,
    studio_restart_required: bool,
    target: String,
    manifest_url: String,
    installed_path: Option<String>,
    entry_skill: EntrySkillRelease,
) -> UpdateInstallPayload {
    let reminder = EntrySkillUpdateReminder {
        comparison_required: true,
        id: entry_skill.id.clone(),
        version: entry_skill.version.clone(),
        source: entry_skill.source.clone(),
    };
    let action = UpdateInstallAction {
        intent: "agent-host.skill.update-recommended".to_owned(),
        executor: "agent-host".to_owned(),
        load_when: format!(
            "The currently loaded {} Skill version is missing or lower than {}.",
            entry_skill.id, entry_skill.version
        ),
        instruction: "Compare the loaded Skill metadata version with the release version. If it is lower, consider updating the Skill with the Agent host's Skill installer. This reminder is advisory and does not block CLI update completion.".to_owned(),
        skill: entry_skill,
    };
    UpdateInstallPayload {
        current_version: current_version.to_owned(),
        latest_version,
        updated,
        studio_restart_required,
        target,
        manifest_url,
        installed_path,
        entry_skill_update_reminder: reminder,
        next_actions: vec![action],
        next_required_action: UpdateInstallNextRequiredAction {
            intent: "review-update-reminders".to_owned(),
            instruction: "Review the advisory entries in nextActions. Restart Studio when studioRestartRequired is true.".to_owned(),
        },
    }
}

pub fn download_update(current_version: &str) -> Result<UpdateDownloadPayload, CliError> {
    let manifest_url = manifest_url();
    let target = current_target();
    let manifest = fetch_manifest(&manifest_url)?;
    let checked = check_from_manifest(
        current_version,
        manifest.clone(),
        manifest_url.clone(),
        target.clone(),
    )?;
    write_cached_update_check(&checked)?;

    if !checked.update_available {
        return Ok(UpdateDownloadPayload {
            current_version: current_version.to_owned(),
            latest_version: checked.latest_version,
            update_available: false,
            downloaded: false,
            target,
            manifest_url,
            archive_path: None,
        });
    }

    let asset = manifest.assets.get(&target).ok_or_else(|| {
        CliError::new(format!(
            "No release asset is available for target `{target}` in manifest {manifest_url}."
        ))
    })?;
    let archive_path = write_downloaded_asset(asset, checked.latest_version.as_deref(), &target)?;

    Ok(UpdateDownloadPayload {
        current_version: current_version.to_owned(),
        latest_version: checked.latest_version,
        update_available: true,
        downloaded: true,
        target,
        manifest_url,
        archive_path: Some(archive_path.display().to_string()),
    })
}

pub fn maybe_notify_update(current_version: &str, stderr: &mut impl Write) {
    if env_flag("MYOPENPANELS_DISABLE_UPDATE_CHECK") {
        return;
    }

    let Ok(cached) = read_cached_update_check(&manifest_url(), &current_target(), current_version)
    else {
        return;
    };

    if cached
        .as_ref()
        .is_some_and(|state| !is_cache_stale(state.checked_at_unix))
    {
        return;
    }

    let Ok(checked) = check_for_update(current_version, false) else {
        return;
    };

    if checked.update_available {
        let latest = checked.latest_version.as_deref().unwrap_or("unknown");
        let _ = writeln!(
            stderr,
            "Update available: myopenpanels {current_version} -> {latest}. Run `myopenpanels update install` to install."
        );
    }
}

pub fn current_target() -> String {
    format!(
        "{}-{}-{}",
        env::consts::ARCH,
        env::consts::OS,
        env::consts::FAMILY
    )
    .replace("aarch64-macos-unix", "aarch64-apple-darwin")
    .replace("x86_64-macos-unix", "x86_64-apple-darwin")
    .replace("x86_64-linux-unix", "x86_64-unknown-linux-gnu")
    .replace("aarch64-linux-unix", "aarch64-unknown-linux-gnu")
    .replace("x86_64-windows-windows", "x86_64-pc-windows-msvc")
}

fn check_from_manifest(
    current_version: &str,
    manifest: UpdateManifest,
    manifest_url: String,
    target: String,
) -> Result<UpdateCheckPayload, CliError> {
    if manifest.schema_version != 1 {
        return Err(CliError::new(format!(
            "Unsupported update manifest schemaVersion {}.",
            manifest.schema_version
        )));
    }
    if manifest.name != "myopenpanels" {
        return Err(CliError::new(format!(
            "Unexpected update manifest name `{}`.",
            manifest.name
        )));
    }

    let current = parse_version(current_version)?;
    let latest_version = manifest.version.trim_start_matches('v').to_owned();
    let latest = parse_version(&latest_version)?;
    let asset_available = manifest.assets.contains_key(&target);

    Ok(UpdateCheckPayload {
        current_version: current_version.to_owned(),
        latest_version: Some(latest_version),
        update_available: latest > current,
        channel: manifest.channel,
        target,
        asset_available,
        manifest_url,
        checked_at_unix: now_unix_secs(),
        cached: false,
    })
}

fn fetch_manifest(manifest_url: &str) -> Result<UpdateManifest, CliError> {
    update_http_agent()
        .get(manifest_url)
        .set(
            "User-Agent",
            concat!("myopenpanels/", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .map_err(|error| CliError::new(format!("Failed to fetch update manifest: {error}")))?
        .into_json()
        .map_err(|error| CliError::new(format!("Failed to parse update manifest: {error}")))
}

fn download_bytes(url: &str) -> Result<Vec<u8>, CliError> {
    let response = update_http_agent()
        .get(url)
        .set(
            "User-Agent",
            concat!("myopenpanels/", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .map_err(|error| CliError::new(format!("Failed to download update asset: {error}")))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(to_cli_error)?;
    Ok(bytes)
}

fn read_or_download_asset(
    asset: &ReleaseAsset,
    latest_version: Option<&str>,
    target: &str,
) -> Result<Vec<u8>, CliError> {
    let Some(version) = latest_version else {
        return Err(CliError::new("Update manifest does not include a version."));
    };
    if let Some(cached) = read_cached_asset()? {
        if cached.version == version
            && cached.target == target
            && cached.url == asset.url
            && cached.sha256.eq_ignore_ascii_case(&asset.sha256)
        {
            let bytes = fs::read(Path::new(&cached.archive_path)).map_err(to_cli_error)?;
            verify_asset_bytes(&bytes, asset)?;
            return Ok(bytes);
        }
    }

    let archive_path = write_downloaded_asset(asset, latest_version, target)?;
    fs::read(archive_path).map_err(to_cli_error)
}

fn write_downloaded_asset(
    asset: &ReleaseAsset,
    latest_version: Option<&str>,
    target: &str,
) -> Result<PathBuf, CliError> {
    let Some(version) = latest_version else {
        return Err(CliError::new("Update manifest does not include a version."));
    };
    let bytes = download_bytes(&asset.url)?;
    verify_asset_bytes(&bytes, asset)?;

    let archive_dir = update_archive_dir()?;
    fs::create_dir_all(&archive_dir).map_err(to_cli_error)?;
    let extension = if asset.url.ends_with(".zip") {
        "zip"
    } else {
        "tar.gz"
    };
    let archive_path = archive_dir.join(format!("myopenpanels-{version}-{target}.{extension}"));
    fs::write(&archive_path, &bytes).map_err(to_cli_error)?;
    write_cached_asset(&CachedAsset {
        version: version.to_owned(),
        target: target.to_owned(),
        url: asset.url.clone(),
        sha256: asset.sha256.clone(),
        size: asset.size,
        archive_path: archive_path.display().to_string(),
        downloaded_at_unix: now_unix_secs(),
    })?;
    Ok(archive_path)
}

fn verify_asset_bytes(bytes: &[u8], asset: &ReleaseAsset) -> Result<(), CliError> {
    verify_sha256(bytes, &asset.sha256)?;
    if let Some(expected_size) = asset.size {
        let actual_size = bytes.len() as u64;
        if expected_size != actual_size {
            return Err(CliError::new(format!(
                "Downloaded asset size mismatch: expected {expected_size} bytes, got {actual_size} bytes."
            )));
        }
    }
    Ok(())
}

fn extract_binary(archive_bytes: &[u8], url: &str, temp_dir: &Path) -> Result<PathBuf, CliError> {
    let candidate_path = temp_dir.join(BINARY_NAME);
    if url.ends_with(".zip") {
        extract_binary_from_zip(archive_bytes, &candidate_path)?;
    } else {
        extract_binary_from_tar_gz(archive_bytes, &candidate_path)?;
    }

    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&candidate_path)
            .map_err(to_cli_error)?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&candidate_path, permissions).map_err(to_cli_error)?;
    }

    Ok(candidate_path)
}

fn extract_binary_from_tar_gz(archive_bytes: &[u8], candidate_path: &Path) -> Result<(), CliError> {
    let decoder = GzDecoder::new(Cursor::new(archive_bytes));
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries().map_err(to_cli_error)? {
        let mut entry = entry.map_err(to_cli_error)?;
        let path = entry.path().map_err(to_cli_error)?.into_owned();
        if path
            .file_name()
            .is_some_and(|name| name == OsStr::new(BINARY_NAME))
        {
            let mut output = File::create(candidate_path).map_err(to_cli_error)?;
            std::io::copy(&mut entry, &mut output).map_err(to_cli_error)?;
            return Ok(());
        }
    }

    Err(CliError::new(format!(
        "Release archive does not contain `{BINARY_NAME}`."
    )))
}

fn extract_binary_from_zip(archive_bytes: &[u8], candidate_path: &Path) -> Result<(), CliError> {
    let reader = Cursor::new(archive_bytes);
    let mut archive = zip::ZipArchive::new(reader).map_err(to_cli_error)?;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(to_cli_error)?;
        let Some(enclosed_name) = file.enclosed_name() else {
            continue;
        };
        if enclosed_name
            .file_name()
            .is_some_and(|name| name == OsStr::new(BINARY_NAME))
        {
            let mut output = File::create(candidate_path).map_err(to_cli_error)?;
            std::io::copy(&mut file, &mut output).map_err(to_cli_error)?;
            return Ok(());
        }
    }

    Err(CliError::new(format!(
        "Release archive does not contain `{BINARY_NAME}`."
    )))
}

fn verify_candidate_version(
    candidate: &Path,
    latest_version: Option<&str>,
) -> Result<(), CliError> {
    let Some(latest_version) = latest_version else {
        return Err(CliError::new("Update manifest does not include a version."));
    };
    let mut child = candidate_version_command(candidate)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(to_cli_error)?;
    let started = Instant::now();
    loop {
        if child.try_wait().map_err(to_cli_error)?.is_some() {
            break;
        }
        if started.elapsed() >= Duration::from_secs(VERSION_CHECK_TIMEOUT_SECS) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CliError::new(
                "Downloaded update binary timed out while running `--version`.",
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
    let output = child.wait_with_output().map_err(to_cli_error)?;
    if !output.status.success() {
        return Err(CliError::new(
            "Downloaded update binary failed `--version`.",
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(to_cli_error)?;
    let candidate_version = stdout.trim();
    if candidate_version != latest_version {
        return Err(CliError::new(format!(
            "Downloaded update binary version mismatch: expected {latest_version}, got {candidate_version}."
        )));
    }
    Ok(())
}

fn candidate_version_command(candidate: &Path) -> Command {
    let mut command = Command::new(candidate);
    command
        .arg("--version")
        .env_remove("MYOPENPANELS_TRACE_URL")
        .env_remove("MYOPENPANELS_TRACE_AUDIENCE")
        .env_remove("MYOPENPANELS_TRACE_RUN_ID")
        .env("MYOPENPANELS_DISABLE_UPDATE_CHECK", "1");
    command
}

fn verify_sha256(bytes: &[u8], expected_hex: &str) -> Result<(), CliError> {
    let digest = Sha256::digest(bytes);
    let actual_hex = digest.iter().fold(
        String::with_capacity(digest.len() * 2),
        |mut output, byte| {
            use std::fmt::Write as _;
            write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
            output
        },
    );
    if actual_hex.eq_ignore_ascii_case(expected_hex) {
        return Ok(());
    }

    Err(CliError::new(format!(
        "Downloaded update checksum mismatch: expected {expected_hex}, got {actual_hex}."
    )))
}

fn ensure_self_update_allowed() -> Result<(), CliError> {
    if env_flag("MYOPENPANELS_ALLOW_DEV_SELF_UPDATE") {
        return Ok(());
    }

    let current_exe = env::current_exe().map_err(to_cli_error)?;
    let path = current_exe.to_string_lossy();
    if path.contains("/target/debug/") || path.contains("/target/release/") {
        return Err(CliError::new(
            "Refusing to self-update a development build. Rebuild it with Cargo instead, or set MYOPENPANELS_ALLOW_DEV_SELF_UPDATE=1.",
        ));
    }
    if path.contains("/Cellar/") || path.contains("\\Cellar\\") {
        return Err(CliError::new(
            "This binary appears to be managed by Homebrew. Use `brew upgrade myopenpanels` instead.",
        ));
    }
    Ok(())
}

fn read_cached_update_check(
    manifest_url: &str,
    target: &str,
    current_version: &str,
) -> Result<Option<UpdateCheckPayload>, CliError> {
    let cache_path = update_state_path()?;
    if !cache_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(cache_path).map_err(to_cli_error)?;
    let cached = serde_json::from_str::<UpdateCheckPayload>(&content).map_err(to_cli_error)?;
    if cached_update_check_matches(&cached, manifest_url, target, current_version) {
        Ok(Some(cached))
    } else {
        Ok(None)
    }
}

fn cached_update_check_matches(
    cached: &UpdateCheckPayload,
    manifest_url: &str,
    target: &str,
    current_version: &str,
) -> bool {
    cached.manifest_url == manifest_url
        && cached.target == target
        && cached.current_version == current_version
}

fn write_cached_update_check(payload: &UpdateCheckPayload) -> Result<(), CliError> {
    let cache_path = update_state_path()?;
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    let json = serde_json::to_string_pretty(payload).map_err(to_cli_error)?;
    fs::write(cache_path, format!("{json}\n")).map_err(to_cli_error)
}

fn read_cached_asset() -> Result<Option<CachedAsset>, CliError> {
    let cache_path = downloaded_asset_state_path()?;
    if !cache_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(cache_path).map_err(to_cli_error)?;
    serde_json::from_str::<CachedAsset>(&content)
        .map(Some)
        .map_err(to_cli_error)
}

fn write_cached_asset(payload: &CachedAsset) -> Result<(), CliError> {
    let cache_path = downloaded_asset_state_path()?;
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    let json = serde_json::to_string_pretty(payload).map_err(to_cli_error)?;
    fs::write(cache_path, format!("{json}\n")).map_err(to_cli_error)
}

fn update_state_path() -> Result<PathBuf, CliError> {
    Ok(update_cache_dir()?.join("update-state.json"))
}

fn downloaded_asset_state_path() -> Result<PathBuf, CliError> {
    Ok(update_cache_dir()?.join("downloaded-asset.json"))
}

fn update_archive_dir() -> Result<PathBuf, CliError> {
    Ok(update_cache_dir()?.join("archives"))
}

fn update_cache_dir() -> Result<PathBuf, CliError> {
    if let Ok(dir) = env::var("MYOPENPANELS_UPDATE_CACHE_DIR") {
        return Ok(PathBuf::from(dir));
    }

    let base = if let Ok(dir) = env::var("XDG_CACHE_HOME") {
        PathBuf::from(dir)
    } else if cfg!(windows) {
        env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| CliError::new("LOCALAPPDATA is not set."))?
    } else if cfg!(target_os = "macos") {
        home_dir()?.join("Library").join("Caches")
    } else {
        home_dir()?.join(".cache")
    };

    Ok(base.join("myopenpanels"))
}

fn update_http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(UPDATE_HTTP_TIMEOUT_SECS))
        .build()
}

fn home_dir() -> Result<PathBuf, CliError> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| CliError::new("HOME is not set."))
}

fn manifest_url() -> String {
    env::var("MYOPENPANELS_UPDATE_MANIFEST_URL").unwrap_or_else(|_| DEFAULT_MANIFEST_URL.to_owned())
}

fn parse_version(version: &str) -> Result<Version, CliError> {
    Version::parse(version.trim_start_matches('v'))
        .map_err(|error| CliError::new(format!("Invalid semver version `{version}`: {error}")))
}

fn is_cache_stale(last_checked_at_unix: u64) -> bool {
    now_unix_secs().saturating_sub(last_checked_at_unix) >= UPDATE_CHECK_INTERVAL_SECS
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn current_exe_display() -> Result<String, CliError> {
    env::current_exe()
        .map(|path| path.display().to_string())
        .map_err(to_cli_error)
}

fn env_flag(name: &str) -> bool {
    matches!(env::var(name).as_deref(), Ok("1" | "true" | "yes" | "on"))
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_target_uses_release_asset_triples() {
        let target = current_target();
        assert!(target.contains('-'));
        assert!(!target.contains("macos"));
    }

    #[test]
    fn semver_comparison_ignores_tag_prefix() {
        let manifest = UpdateManifest {
            schema_version: 1,
            name: "myopenpanels".to_owned(),
            version: "v0.1.10".to_owned(),
            channel: Some("stable".to_owned()),
            entry_skill: test_entry_skill(),
            assets: BTreeMap::new(),
        };

        let checked = check_from_manifest(
            "0.1.9",
            manifest,
            DEFAULT_MANIFEST_URL.to_owned(),
            "aarch64-apple-darwin".to_owned(),
        )
        .expect("manifest should be valid");

        assert!(checked.update_available);
        assert_eq!(checked.latest_version.as_deref(), Some("0.1.10"));
    }

    #[test]
    fn cached_update_checks_are_scoped_to_current_version() {
        let cached = UpdateCheckPayload {
            current_version: "0.1.9".to_owned(),
            latest_version: Some("0.1.9".to_owned()),
            update_available: false,
            channel: Some("stable".to_owned()),
            target: "aarch64-apple-darwin".to_owned(),
            asset_available: true,
            manifest_url: DEFAULT_MANIFEST_URL.to_owned(),
            checked_at_unix: 1,
            cached: false,
        };

        assert!(cached_update_check_matches(
            &cached,
            DEFAULT_MANIFEST_URL,
            "aarch64-apple-darwin",
            "0.1.9"
        ));
        assert!(!cached_update_check_matches(
            &cached,
            DEFAULT_MANIFEST_URL,
            "aarch64-apple-darwin",
            "0.1.10"
        ));
    }

    #[test]
    fn candidate_version_command_removes_trace_environment() {
        let command = candidate_version_command(Path::new("myopenpanels"));
        let envs = command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().to_string(),
                    value.map(|value| value.to_string_lossy().to_string()),
                )
            })
            .collect::<BTreeMap<_, _>>();

        assert_eq!(envs.get("MYOPENPANELS_TRACE_URL"), Some(&None));
        assert_eq!(envs.get("MYOPENPANELS_TRACE_AUDIENCE"), Some(&None));
        assert_eq!(envs.get("MYOPENPANELS_TRACE_RUN_ID"), Some(&None));
        assert_eq!(
            envs.get("MYOPENPANELS_DISABLE_UPDATE_CHECK"),
            Some(&Some("1".to_owned()))
        );
    }

    #[test]
    fn installed_update_returns_an_advisory_entry_skill_reminder() {
        let payload = update_install_payload(
            "0.4.1",
            Some("0.4.2".to_owned()),
            true,
            true,
            "test-target".to_owned(),
            "https://example.invalid/manifest.json".to_owned(),
            Some("/tmp/myopenpanels".to_owned()),
            test_entry_skill(),
        );

        let json = serde_json::to_value(payload).expect("serialize install payload");
        assert_eq!(json["studioRestartRequired"], true);
        assert_eq!(json["entrySkillUpdateReminder"]["comparisonRequired"], true);
        assert_eq!(json["entrySkillUpdateReminder"]["version"], "3.1");
        assert_eq!(json["nextActions"][0]["executor"], "agent-host");
        assert_eq!(
            json["nextActions"][0]["intent"],
            "agent-host.skill.update-recommended"
        );
        assert!(json["nextActions"][0].get("argv").is_none());
        assert!(json["nextActions"][0]["instruction"]
            .as_str()
            .unwrap()
            .contains("advisory"));
    }

    #[test]
    fn no_op_install_still_returns_the_advisory_reminder() {
        let payload = update_install_payload(
            "0.4.2",
            Some("0.4.2".to_owned()),
            false,
            false,
            "test-target".to_owned(),
            "https://example.invalid/manifest.json".to_owned(),
            None,
            test_entry_skill(),
        );

        assert!(!payload.updated);
        assert!(!payload.studio_restart_required);
        assert!(payload.entry_skill_update_reminder.comparison_required);
        assert_eq!(payload.next_actions.len(), 1);
    }

    #[test]
    fn check_and_download_payloads_do_not_emit_skill_reminders() {
        let check = serde_json::to_value(UpdateCheckPayload {
            current_version: "0.4.2".to_owned(),
            latest_version: Some("0.4.2".to_owned()),
            update_available: false,
            channel: Some("stable".to_owned()),
            target: "test-target".to_owned(),
            asset_available: true,
            manifest_url: "https://example.invalid/manifest.json".to_owned(),
            checked_at_unix: 1,
            cached: false,
        })
        .expect("serialize check payload");
        let download = serde_json::to_value(UpdateDownloadPayload {
            current_version: "0.4.2".to_owned(),
            latest_version: Some("0.4.2".to_owned()),
            update_available: false,
            downloaded: false,
            target: "test-target".to_owned(),
            manifest_url: "https://example.invalid/manifest.json".to_owned(),
            archive_path: None,
        })
        .expect("serialize download payload");

        for payload in [check, download] {
            assert!(payload.get("entrySkillUpdateReminder").is_none());
            assert!(payload.get("nextActions").is_none());
        }
    }

    fn test_entry_skill() -> EntrySkillRelease {
        EntrySkillRelease {
            id: "myopenpanels".to_owned(),
            version: "3.1".to_owned(),
            source: "https://example.invalid/v0.4.2/skills/myopenpanels".to_owned(),
        }
    }
}
