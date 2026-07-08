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
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_MANIFEST_URL: &str =
    "https://github.com/mooqii/OpenPanels/releases/latest/download/openpanels-local-manifest.json";
pub const UPDATE_CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;

const BINARY_NAME: &str = if cfg!(windows) {
    "openpanels-local.exe"
} else {
    "openpanels-local"
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateManifest {
    pub schema_version: u32,
    pub name: String,
    pub version: String,
    pub channel: Option<String>,
    pub assets: BTreeMap<String, ReleaseAsset>,
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
    pub target: String,
    pub manifest_url: String,
    pub installed_path: Option<String>,
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
        if let Some(cached) = read_cached_update_check(&manifest_url, &target)? {
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
        return Ok(UpdateInstallPayload {
            current_version: current_version.to_owned(),
            latest_version: checked.latest_version,
            updated: false,
            target,
            manifest_url,
            installed_path: None,
        });
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

    Ok(UpdateInstallPayload {
        current_version: current_version.to_owned(),
        latest_version: checked.latest_version,
        updated: true,
        target,
        manifest_url,
        installed_path: Some(current_exe_display()?),
    })
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
    if env_flag("OPENPANELS_DISABLE_UPDATE_CHECK") {
        return;
    }

    let Ok(cached) = read_cached_update_check(&manifest_url(), &current_target()) else {
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
            "Update available: openpanels-local {current_version} -> {latest}. Run `openpanels-local update` to install."
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
    if manifest.name != "openpanels-local" {
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
    ureq::get(manifest_url)
        .set(
            "User-Agent",
            concat!("openpanels-local/", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .map_err(|error| CliError::new(format!("Failed to fetch update manifest: {error}")))?
        .into_json()
        .map_err(|error| CliError::new(format!("Failed to parse update manifest: {error}")))
}

fn download_bytes(url: &str) -> Result<Vec<u8>, CliError> {
    let response = ureq::get(url)
        .set(
            "User-Agent",
            concat!("openpanels-local/", env!("CARGO_PKG_VERSION")),
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
    let archive_path = archive_dir.join(format!("openpanels-local-{version}-{target}.{extension}"));
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
    let output = Command::new(candidate)
        .arg("--version")
        .output()
        .map_err(to_cli_error)?;
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

fn verify_sha256(bytes: &[u8], expected_hex: &str) -> Result<(), CliError> {
    let digest = Sha256::digest(bytes);
    let actual_hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if actual_hex.eq_ignore_ascii_case(expected_hex) {
        return Ok(());
    }

    Err(CliError::new(format!(
        "Downloaded update checksum mismatch: expected {expected_hex}, got {actual_hex}."
    )))
}

fn ensure_self_update_allowed() -> Result<(), CliError> {
    if env_flag("OPENPANELS_ALLOW_DEV_SELF_UPDATE") {
        return Ok(());
    }

    let current_exe = env::current_exe().map_err(to_cli_error)?;
    let path = current_exe.to_string_lossy();
    if path.contains("/target/debug/") || path.contains("/target/release/") {
        return Err(CliError::new(
            "Refusing to self-update a development build. Rebuild it with Cargo instead, or set OPENPANELS_ALLOW_DEV_SELF_UPDATE=1.",
        ));
    }
    if path.contains("/Cellar/") || path.contains("\\Cellar\\") {
        return Err(CliError::new(
            "This binary appears to be managed by Homebrew. Use `brew upgrade openpanels-local` instead.",
        ));
    }
    Ok(())
}

fn read_cached_update_check(
    manifest_url: &str,
    target: &str,
) -> Result<Option<UpdateCheckPayload>, CliError> {
    let cache_path = update_state_path()?;
    if !cache_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(cache_path).map_err(to_cli_error)?;
    let cached = serde_json::from_str::<UpdateCheckPayload>(&content).map_err(to_cli_error)?;
    if cached.manifest_url == manifest_url && cached.target == target {
        Ok(Some(cached))
    } else {
        Ok(None)
    }
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
    if let Ok(dir) = env::var("OPENPANELS_UPDATE_CACHE_DIR") {
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

    Ok(base.join("openpanels-local"))
}

fn home_dir() -> Result<PathBuf, CliError> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| CliError::new("HOME is not set."))
}

fn manifest_url() -> String {
    env::var("OPENPANELS_UPDATE_MANIFEST_URL").unwrap_or_else(|_| DEFAULT_MANIFEST_URL.to_owned())
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
            name: "openpanels-local".to_owned(),
            version: "v0.1.10".to_owned(),
            channel: Some("stable".to_owned()),
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
}
