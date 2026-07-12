use crate::paths::MyOpenPanelsPaths;
use crate::studio::{process_exists, StudioSession};
use serde_json::Value;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const CONTEXT_RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const TRANSIENT_FILE_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const MAX_STUDIO_LOG_BYTES: u64 = 1024 * 1024;
const RETAINED_STUDIO_LOG_BYTES: u64 = 512 * 1024;
const TRANSIENT_DIRS: &[&str] = &["bridge-runs"];

/// Performs best-effort housekeeping. Cleanup must never prevent Studio from starting.
pub fn cleanup_context_storage(paths: &MyOpenPanelsPaths) {
    let _ = cleanup_context_storage_at(paths, SystemTime::now());
    crate::operations::cleanup_operation_artifacts(paths);
    crate::selection::cleanup_materializations(paths);
}

/// Removes per-context pointers to a Project that has already been deleted.
pub fn clear_deleted_project_references(paths: &MyOpenPanelsPaths, project_id: &str) {
    let contexts_dir = paths.storage_dir.join("contexts");
    let Ok(entries) = fs::read_dir(contexts_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        for file_name in ["active-project.json", "active-panel.json"] {
            let path = entry.path().join(file_name);
            if json_project_id(&path).as_deref() == Some(project_id) {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn cleanup_context_storage_at(paths: &MyOpenPanelsPaths, now: SystemTime) -> std::io::Result<()> {
    let contexts_dir = paths.storage_dir.join("contexts");
    if !contexts_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(&contexts_dir)?.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let context_dir = entry.path();
        let is_current = context_dir == paths.context_dir;
        let has_live_studio = live_studio_session(&context_dir);
        let is_expired = latest_modified(&context_dir).is_some_and(|last_modified| {
            now.duration_since(last_modified)
                .is_ok_and(|age| age > CONTEXT_RETENTION)
        });

        if !is_current && !has_live_studio && is_expired {
            let _ = fs::remove_dir_all(&context_dir);
            continue;
        }

        prune_transient_files(&context_dir, now);
        remove_legacy_agent_files(&context_dir);
        if !has_live_studio {
            compact_studio_log(&context_dir.join("studio.log"));
        }
    }
    Ok(())
}

fn remove_legacy_agent_files(context_dir: &Path) {
    for name in ["agent-runs", "wakeups"] {
        let _ = fs::remove_dir_all(context_dir.join(name));
    }
    let _ = fs::remove_file(context_dir.join("agent-targets.json"));
}

fn live_studio_session(context_dir: &Path) -> bool {
    let Ok(raw) = fs::read_to_string(context_dir.join("studio-session.json")) else {
        return false;
    };
    serde_json::from_str::<StudioSession>(&raw).is_ok_and(|session| {
        PathBuf::from(session.context_dir) == context_dir && process_exists(session.pid)
    })
}

fn latest_modified(path: &Path) -> Option<SystemTime> {
    let mut latest = fs::symlink_metadata(path).ok()?.modified().ok()?;
    let entries = fs::read_dir(path).ok()?;
    for entry in entries.flatten() {
        let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if let Ok(modified) = metadata.modified() {
            latest = latest.max(modified);
        }
        if metadata.is_dir() {
            if let Some(modified) = latest_modified(&entry.path()) {
                latest = latest.max(modified);
            }
        }
    }
    Some(latest)
}

fn prune_transient_files(context_dir: &Path, now: SystemTime) {
    for name in TRANSIENT_DIRS {
        let dir = context_dir.join(name);
        prune_expired_files(&dir, now);
        remove_empty_dirs(&dir);
    }
}

fn prune_expired_files(dir: &Path, now: SystemTime) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            prune_expired_files(&path, now);
            continue;
        }
        let expired = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > TRANSIENT_FILE_RETENTION);
        if expired {
            let _ = fs::remove_file(path);
        }
    }
}

fn remove_empty_dirs(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let children = entries
        .flatten()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    for child in children {
        if child.is_dir() {
            remove_empty_dirs(&child);
        }
    }
    if fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_none()) {
        let _ = fs::remove_dir(dir);
    }
}

fn compact_studio_log(path: &Path) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.len() <= MAX_STUDIO_LOG_BYTES {
        return;
    }
    let retained_len = RETAINED_STUDIO_LOG_BYTES.min(metadata.len());
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return,
    };
    if file
        .seek(SeekFrom::Start(metadata.len() - retained_len))
        .is_err()
    {
        return;
    }
    let mut tail = Vec::with_capacity(retained_len as usize);
    if file.read_to_end(&mut tail).is_err() {
        return;
    }
    drop(file);
    let _ = fs::write(path, tail);
}

fn json_project_id(path: &PathBuf) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<Value>(&raw)
        .ok()?
        .get("projectId")?
        .as_str()
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::resolve_myopenpanels_paths;
    use std::io::Write;

    fn test_paths(temp: &tempfile::TempDir, context_id: &str) -> MyOpenPanelsPaths {
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        fs::create_dir_all(&project_dir).expect("project dir");
        resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some(context_id),
        )
        .expect("paths")
    }

    #[test]
    fn removes_expired_contexts_but_preserves_the_calling_context() {
        let temp = tempfile::tempdir().expect("temp dir");
        let paths = test_paths(&temp, "current");
        let old = paths.storage_dir.join("contexts/old");
        fs::create_dir_all(&paths.context_dir).expect("current context");
        fs::create_dir_all(&old).expect("old context");
        fs::write(old.join("active-project.json"), "{}\n").expect("old state");
        let latest = latest_modified(&old).expect("modified time");
        let future = latest + CONTEXT_RETENTION + Duration::from_secs(1);

        cleanup_context_storage_at(&paths, future).expect("cleanup");

        assert!(!old.exists());
        assert!(paths.context_dir.exists());
    }

    #[test]
    fn preserves_a_live_studio_owner_but_not_an_expired_borrower_binding() {
        let temp = tempfile::tempdir().expect("temp dir");
        let paths = test_paths(&temp, "current");
        let owner = paths.storage_dir.join("contexts/owner");
        let borrower = paths.storage_dir.join("contexts/borrower");
        for dir in [&owner, &borrower] {
            fs::create_dir_all(dir).expect("context");
            let session = StudioSession {
                system_browser_url: None,
                context_dir: owner.display().to_string(),
                context_id: "owner".to_owned(),
                context_id_source: "test".to_owned(),
                host: None,
                lan_server_urls: None,
                local_server_url: None,
                log_path: owner.join("studio.log").display().to_string(),
                pid: std::process::id(),
                port: 1234,
                project_dir: paths.project_dir.display().to_string(),
                server_url: "http://127.0.0.1:1234".to_owned(),
                started_at: "2026-01-01T00:00:00Z".to_owned(),
                storage_dir: paths.storage_dir.display().to_string(),
            };
            fs::write(
                dir.join("studio-session.json"),
                serde_json::to_vec(&session).expect("session json"),
            )
            .expect("session binding");
        }
        let latest = latest_modified(&borrower).expect("modified time");
        let future = latest + CONTEXT_RETENTION + Duration::from_secs(1);

        cleanup_context_storage_at(&paths, future).expect("cleanup");

        assert!(owner.exists());
        assert!(!borrower.exists());
    }

    #[test]
    fn prunes_old_run_files_and_bounds_inactive_logs() {
        let temp = tempfile::tempdir().expect("temp dir");
        let paths = test_paths(&temp, "current");
        let runs = paths.context_dir.join("bridge-runs");
        fs::create_dir_all(&runs).expect("runs dir");
        let run = runs.join("old.log");
        fs::write(&run, "old").expect("run log");
        let log = paths.context_dir.join("studio.log");
        let mut file = fs::File::create(&log).expect("studio log");
        file.write_all(&vec![b'x'; (MAX_STUDIO_LOG_BYTES + 1) as usize])
            .expect("large log");
        drop(file);
        let modified = fs::metadata(&run).unwrap().modified().unwrap();
        let future = modified + TRANSIENT_FILE_RETENTION + Duration::from_secs(1);

        cleanup_context_storage_at(&paths, future).expect("cleanup");

        assert!(!run.exists());
        assert!(fs::metadata(log).unwrap().len() <= RETAINED_STUDIO_LOG_BYTES);
    }

    #[test]
    fn clears_deleted_project_pointers_in_every_context() {
        let temp = tempfile::tempdir().expect("temp dir");
        let paths = test_paths(&temp, "current");
        let sibling = paths.storage_dir.join("contexts/sibling");
        for dir in [&paths.context_dir, &sibling] {
            fs::create_dir_all(dir).expect("context");
            fs::write(
                dir.join("active-project.json"),
                r#"{"projectId":"session:deleted"}"#,
            )
            .expect("active session");
            fs::write(
                dir.join("active-panel.json"),
                r#"{"projectId":"session:deleted","panelId":"panel:old"}"#,
            )
            .expect("active panel");
        }

        clear_deleted_project_references(&paths, "session:deleted");

        for dir in [&paths.context_dir, &sibling] {
            assert!(!dir.join("active-project.json").exists());
            assert!(!dir.join("active-panel.json").exists());
        }
    }
}
