use crate::error::CliError;
use std::env;
use std::path::{Path, PathBuf};

const CONTEXT_ENV_VARS: &[&str] = &[
    "CODEX_THREAD_ID",
    "HERMES_THREAD_ID",
    "HERMES_CONVERSATION_ID",
    "HERMES_SESSION_ID",
];

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MyOpenPanelsPaths {
    pub context_dir: PathBuf,
    pub context_id: String,
    pub context_id_source: String,
    pub focus_dir: PathBuf,
    pub project_dir: PathBuf,
    pub studio_dir: PathBuf,
    pub storage_dir: PathBuf,
}

pub fn resolve_myopenpanels_paths(
    project_dir: Option<&str>,
    storage_dir: Option<&str>,
    context_id: Option<&str>,
) -> Result<MyOpenPanelsPaths, CliError> {
    let project_dir = match project_dir {
        Some(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => match env::var("MYOPENPANELS_PROJECT_DIR") {
            Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
            _ => env::current_dir().map_err(to_cli_error)?,
        },
    };
    let project_dir = absolutize(&project_dir)?;
    if !project_dir.is_dir() {
        return Err(CliError::with_recovery(
            "project_directory_not_found",
            format!(
                "MyOpenPanels project directory does not exist: {}",
                project_dir.display()
            ),
            false,
            "Pass an existing filesystem directory with `--project-dir <dir>`.",
        ));
    }
    let storage_dir = match storage_dir {
        Some(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => default_storage_dir()?,
    };
    let storage_dir = absolutize(&storage_dir)?;
    let (context_id, context_id_source) = resolve_context_id(context_id);
    let context_dir = storage_dir.join("contexts").join(&context_id);
    let studio_dir = storage_dir.join("studio");
    let focus_dir = studio_dir.join("focus");
    assert_inside(&storage_dir, &context_dir)?;
    assert_inside(&storage_dir, &studio_dir)?;
    assert_inside(&storage_dir, &focus_dir)?;

    Ok(MyOpenPanelsPaths {
        context_dir,
        context_id,
        context_id_source,
        focus_dir,
        project_dir,
        studio_dir,
        storage_dir,
    })
}

pub fn resolve_studio_service_paths(
    paths: &MyOpenPanelsPaths,
) -> Result<MyOpenPanelsPaths, CliError> {
    resolve_myopenpanels_paths(
        Some(paths.project_dir.to_string_lossy().as_ref()),
        Some(paths.storage_dir.to_string_lossy().as_ref()),
        Some("studio"),
    )
}

pub fn sanitize_path_part(value: &str) -> String {
    let trimmed = value.trim();
    let sanitized = trimmed
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric()
                || matches!(ch, '-' | '_' | '.' | '@' | '+' | '=')
                || (!cfg!(target_os = "windows") && ch == ':')
            {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches('.').to_owned();
    if sanitized.is_empty() {
        "default".to_owned()
    } else {
        sanitized
    }
}

fn resolve_context_id(explicit_context_id: Option<&str>) -> (String, String) {
    if let Some(value) = explicit_context_id {
        if !value.trim().is_empty() {
            return (sanitize_path_part(value), "explicit".to_owned());
        }
    }

    for env_name in CONTEXT_ENV_VARS {
        if let Ok(value) = env::var(env_name) {
            if !value.trim().is_empty() {
                return (sanitize_path_part(&value), (*env_name).to_owned());
            }
        }
    }

    ("default".to_owned(), "default".to_owned())
}

fn default_storage_dir() -> Result<PathBuf, CliError> {
    if let Ok(value) = env::var("MYOPENPANELS_STORAGE_DIR") {
        if !value.trim().is_empty() {
            return Ok(PathBuf::from(value));
        }
    }

    // Keep release data in a stable, visible per-user directory. This is
    // intentionally a new location; older platform-specific data is not
    // migrated and remains untouched.
    Ok(home_dir()?.join(".myopenpanels"))
}

fn home_dir() -> Result<PathBuf, CliError> {
    if let Ok(home) = env::var("HOME") {
        if !home.trim().is_empty() {
            return Ok(PathBuf::from(home));
        }
    }
    if cfg!(target_os = "windows") {
        if let Ok(userprofile) = env::var("USERPROFILE") {
            if !userprofile.trim().is_empty() {
                return Ok(PathBuf::from(userprofile));
            }
        }
    }
    Err(CliError::new(
        "Unable to resolve a default MyOpenPanels storage directory.",
    ))
}

fn absolutize(path: &Path) -> Result<PathBuf, CliError> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(env::current_dir().map_err(to_cli_error)?.join(path))
}

fn assert_inside(root: &Path, candidate: &Path) -> Result<(), CliError> {
    if candidate.starts_with(root) {
        return Ok(());
    }
    Err(CliError::new(format!(
        "Resolved path escapes storage directory: {}",
        candidate.display()
    )))
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn studio_and_focus_paths_are_shared_while_agent_contexts_remain_distinct() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_a = temp.path().join("project-a");
        let project_b = temp.path().join("project-b");
        let storage = temp.path().join("storage");
        fs::create_dir_all(&project_a).expect("project a");
        fs::create_dir_all(&project_b).expect("project b");
        let a = resolve_myopenpanels_paths(
            Some(project_a.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("agent-a"),
        )
        .expect("paths a");
        let b = resolve_myopenpanels_paths(
            Some(project_b.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("agent-b"),
        )
        .expect("paths b");

        assert_eq!(a.studio_dir, b.studio_dir);
        assert_eq!(a.focus_dir, b.focus_dir);
        assert_ne!(a.context_id, b.context_id);
        assert_ne!(a.context_dir, b.context_dir);
    }
}
