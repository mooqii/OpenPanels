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
    pub project_dir: PathBuf,
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
    let storage_dir = match storage_dir {
        Some(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => default_storage_dir()?,
    };
    let storage_dir = absolutize(&storage_dir)?;
    let (context_id, context_id_source) = resolve_context_id(context_id);
    let context_dir = storage_dir.join("contexts").join(&context_id);
    assert_inside(&storage_dir, &context_dir)?;

    Ok(MyOpenPanelsPaths {
        context_dir,
        context_id,
        context_id_source,
        project_dir,
        storage_dir,
    })
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

    let home = home_dir()?;
    if cfg!(target_os = "macos") {
        return Ok(home
            .join("Library")
            .join("Application Support")
            .join("MyOpenPanels")
            .join(".myopenpanels"));
    }
    if cfg!(target_os = "windows") {
        if let Ok(appdata) = env::var("APPDATA") {
            if !appdata.trim().is_empty() {
                return Ok(PathBuf::from(appdata)
                    .join("MyOpenPanels")
                    .join(".myopenpanels"));
            }
        }
        return Ok(home
            .join("AppData")
            .join("Roaming")
            .join("MyOpenPanels")
            .join(".myopenpanels"));
    }
    if let Ok(xdg_data_home) = env::var("XDG_DATA_HOME") {
        if !xdg_data_home.trim().is_empty() {
            return Ok(PathBuf::from(xdg_data_home)
                .join("myopenpanels")
                .join(".myopenpanels"));
        }
    }
    Ok(home
        .join(".local")
        .join("share")
        .join("myopenpanels")
        .join(".myopenpanels"))
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
