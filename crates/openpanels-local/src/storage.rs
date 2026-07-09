use crate::error::CliError;
use crate::paths::{sanitize_path_part, OpenPanelsPaths};
use crate::types::{Panel, Session};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

pub const DATABASE_FILE_NAME: &str = "main.sqlite3";

pub struct Storage {
    connection: Connection,
    root_dir: PathBuf,
}

impl Storage {
    pub fn open(paths: &OpenPanelsPaths) -> Result<Self, CliError> {
        fs::create_dir_all(&paths.storage_dir).map_err(to_cli_error)?;
        let connection =
            Connection::open(paths.storage_dir.join(DATABASE_FILE_NAME)).map_err(to_cli_error)?;
        connection
            .execute_batch(
                r#"
                PRAGMA journal_mode = WAL;
                PRAGMA foreign_keys = ON;
                PRAGMA busy_timeout = 5000;
                "#,
            )
            .map_err(to_cli_error)?;
        migrate(&connection)?;
        Ok(Self {
            connection,
            root_dir: paths.storage_dir.clone(),
        })
    }

    pub fn list_sessions(&self) -> Result<Vec<Session>, CliError> {
        let mut statement = self
            .connection
            .prepare("SELECT session_json FROM sessions ORDER BY updated_at DESC, id ASC")
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(to_cli_error)?;
        rows.map(|row| {
            let raw = row.map_err(to_cli_error)?;
            serde_json::from_str::<Session>(&raw).map_err(to_cli_error)
        })
        .collect()
    }

    pub fn read_session(&self, session_id: &str) -> Result<Option<Session>, CliError> {
        self.connection
            .query_row(
                "SELECT session_json FROM sessions WHERE id = ?",
                params![session_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(to_cli_error)?
            .map(|raw| serde_json::from_str::<Session>(&raw).map_err(to_cli_error))
            .transpose()
    }

    pub fn write_session(&self, session: &Session) -> Result<(), CliError> {
        self.connection
            .execute(
                r#"
                INSERT INTO sessions (
                  id, title, created_at, updated_at, panel_ids_json, session_json
                )
                VALUES (?, ?, ?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                  title = excluded.title,
                  created_at = excluded.created_at,
                  updated_at = excluded.updated_at,
                  panel_ids_json = excluded.panel_ids_json,
                  session_json = excluded.session_json
                "#,
                params![
                    session.id,
                    session.title,
                    session.created_at,
                    session.updated_at,
                    serde_json::to_string(&session.panel_ids).map_err(to_cli_error)?,
                    serde_json::to_string(session).map_err(to_cli_error)?,
                ],
            )
            .map_err(to_cli_error)?;
        Ok(())
    }

    pub fn delete_session(&self, session_id: &str) -> Result<(), CliError> {
        self.connection
            .execute("DELETE FROM sessions WHERE id = ?", params![session_id])
            .map_err(to_cli_error)?;
        let session_dir = self
            .root_dir
            .join("sessions")
            .join(sanitize_path_part(session_id));
        fs::remove_dir_all(session_dir)
            .or_else(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    Ok(())
                } else {
                    Err(error)
                }
            })
            .map_err(to_cli_error)?;
        Ok(())
    }

    pub fn read_panel(&self, session_id: &str, panel_id: &str) -> Result<Option<Panel>, CliError> {
        self.connection
            .query_row(
                "SELECT panel_json FROM panels WHERE session_id = ? AND id = ?",
                params![session_id, panel_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(to_cli_error)?
            .map(|raw| serde_json::from_str::<Panel>(&raw).map_err(to_cli_error))
            .transpose()
    }

    pub fn write_panel(&self, panel: &Panel) -> Result<(), CliError> {
        self.connection
            .execute(
                r#"
                INSERT INTO panels (
                  id, session_id, kind, title, created_at, updated_at, state_ref, panel_json
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(session_id, id) DO UPDATE SET
                  kind = excluded.kind,
                  title = excluded.title,
                  created_at = excluded.created_at,
                  updated_at = excluded.updated_at,
                  state_ref = excluded.state_ref,
                  panel_json = excluded.panel_json
                "#,
                params![
                    panel.id,
                    panel.session_id,
                    panel.kind.as_str(),
                    panel.title,
                    panel.created_at,
                    panel.updated_at,
                    panel.state_ref,
                    serde_json::to_string(panel).map_err(to_cli_error)?,
                ],
            )
            .map_err(to_cli_error)?;
        Ok(())
    }

    pub fn read_panel_state(
        &self,
        session_id: &str,
        panel_id: &str,
    ) -> Result<Option<Value>, CliError> {
        self.connection
            .query_row(
                "SELECT state_json FROM panel_states WHERE session_id = ? AND panel_id = ?",
                params![session_id, panel_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(to_cli_error)?
            .map(|raw| serde_json::from_str::<Value>(&raw).map_err(to_cli_error))
            .transpose()
    }

    pub fn write_panel_state(
        &self,
        session_id: &str,
        panel_id: &str,
        state: &Value,
    ) -> Result<(), CliError> {
        self.connection
            .execute(
                r#"
                INSERT INTO panel_states (
                  session_id, panel_id, schema_version, state_json, updated_at
                )
                VALUES (?, ?, ?, ?, ?)
                ON CONFLICT(session_id, panel_id) DO UPDATE SET
                  schema_version = excluded.schema_version,
                  state_json = excluded.state_json,
                  updated_at = excluded.updated_at
                "#,
                params![
                    session_id,
                    panel_id,
                    state.get("schemaVersion").and_then(Value::as_i64),
                    serde_json::to_string(state).map_err(to_cli_error)?,
                    crate::control::now_iso(),
                ],
            )
            .map_err(to_cli_error)?;
        Ok(())
    }

    pub fn sync_wiki_tasks(
        &self,
        session_id: &str,
        panel_id: &str,
        state: &Value,
    ) -> Result<(), CliError> {
        self.connection
            .execute(
                "DELETE FROM wiki_tasks WHERE session_id = ? AND panel_id = ?",
                params![session_id, panel_id],
            )
            .map_err(to_cli_error)?;
        for task in state
            .get("tasks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let id = task
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| CliError::new("Wiki task id is required."))?;
            self.connection
                .execute(
                    r#"
                    INSERT INTO wiki_tasks (
                      id, session_id, panel_id, type, status, target_id,
                      document_id, wiki_space_id, markdown_version,
                      claimed_by_process_id, created_at, updated_at, task_json
                    )
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    ON CONFLICT(id) DO UPDATE SET
                      session_id = excluded.session_id,
                      panel_id = excluded.panel_id,
                      type = excluded.type,
                      status = excluded.status,
                      target_id = excluded.target_id,
                      document_id = excluded.document_id,
                      wiki_space_id = excluded.wiki_space_id,
                      markdown_version = excluded.markdown_version,
                      claimed_by_process_id = excluded.claimed_by_process_id,
                      created_at = excluded.created_at,
                      updated_at = excluded.updated_at,
                      task_json = excluded.task_json
                    "#,
                    params![
                        id,
                        session_id,
                        panel_id,
                        task.get("type")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown"),
                        task.get("status")
                            .and_then(Value::as_str)
                            .unwrap_or("queued"),
                        task.get("targetId").and_then(Value::as_str).unwrap_or(""),
                        task.get("documentId").and_then(Value::as_str),
                        task.get("wikiSpaceId").and_then(Value::as_str),
                        task.get("markdownVersion").and_then(Value::as_i64),
                        task.get("claimedByProcessId").and_then(Value::as_str),
                        task.get("createdAt")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                            .unwrap_or_else(crate::control::now_iso),
                        task.get("updatedAt")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                            .unwrap_or_else(crate::control::now_iso),
                        serde_json::to_string(task).map_err(to_cli_error)?,
                    ],
                )
                .map_err(to_cli_error)?;
        }
        Ok(())
    }

    pub fn write_artifact(&self, session_id: &str, artifact: &Value) -> Result<(), CliError> {
        let id = artifact
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| CliError::new("Artifact id is required."))?;
        self.connection
            .execute(
                r#"
                INSERT INTO artifacts (
                  id, session_id, panel_id, kind, title, created_at, artifact_json
                )
                VALUES (?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(session_id, id) DO UPDATE SET
                  panel_id = excluded.panel_id,
                  kind = excluded.kind,
                  title = excluded.title,
                  created_at = excluded.created_at,
                  artifact_json = excluded.artifact_json
                "#,
                params![
                    id,
                    session_id,
                    artifact.get("panelId").and_then(Value::as_str),
                    artifact
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or("file"),
                    artifact.get("title").and_then(Value::as_str),
                    artifact
                        .get("createdAt")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_else(crate::control::now_iso),
                    serde_json::to_string(artifact).map_err(to_cli_error)?,
                ],
            )
            .map_err(to_cli_error)?;
        Ok(())
    }

    pub fn write_panel_selection(
        &self,
        session_id: &str,
        panel_id: &str,
        selection: &Value,
    ) -> Result<(), CliError> {
        let selected_shape_ids = selection
            .get("selectedShapeIds")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        self.connection
            .execute(
                r#"
                INSERT INTO panel_selections (
                  session_id, panel_id, asset_ref, selected_shape_ids_json,
                  selection_json, updated_at
                )
                VALUES (?, ?, ?, ?, ?, ?)
                ON CONFLICT(session_id, panel_id) DO UPDATE SET
                  asset_ref = excluded.asset_ref,
                  selected_shape_ids_json = excluded.selected_shape_ids_json,
                  selection_json = excluded.selection_json,
                  updated_at = excluded.updated_at
                "#,
                params![
                    session_id,
                    panel_id,
                    selection.get("assetRef").and_then(Value::as_str),
                    serde_json::to_string(&selected_shape_ids).map_err(to_cli_error)?,
                    serde_json::to_string(selection).map_err(to_cli_error)?,
                    selection
                        .get("updatedAt")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_else(crate::control::now_iso),
                ],
            )
            .map_err(to_cli_error)?;
        Ok(())
    }

    pub fn write_asset_from_buffer(
        &self,
        session_id: &str,
        panel_id: &str,
        requested_name: &str,
        bytes: &[u8],
        overwrite: bool,
    ) -> Result<WrittenAsset, CliError> {
        let assets_dir = self.panel_dir(session_id, panel_id).join("assets");
        fs::create_dir_all(&assets_dir).map_err(to_cli_error)?;
        let file_name = if overwrite {
            sanitize_asset_path(requested_name)
        } else {
            unique_file_name(&assets_dir, requested_name)
        };
        let file_path = assets_dir.join(&file_name);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(to_cli_error)?;
        }
        if !file_path.starts_with(&self.root_dir) {
            return Err(CliError::new(
                "Resolved asset path escapes storage directory.",
            ));
        }
        fs::write(&file_path, bytes).map_err(to_cli_error)?;
        let asset_ref = format!(
            "sessions/{}/panels/{}/assets/{}",
            sanitize_path_part(session_id),
            sanitize_path_part(panel_id),
            file_name
                .split('/')
                .map(sanitize_path_part)
                .collect::<Vec<_>>()
                .join("/")
        );
        Ok(WrittenAsset {
            asset_ref,
            file_name,
            file_path,
        })
    }

    pub fn read_asset(&self, asset_ref: &str) -> Result<Vec<u8>, CliError> {
        let path = self.asset_path(asset_ref)?;
        fs::read(path).map_err(to_cli_error)
    }

    pub fn asset_path(&self, asset_ref: &str) -> Result<PathBuf, CliError> {
        let mut path = self.root_dir.clone();
        for part in asset_ref.split('/') {
            path.push(sanitize_path_part(part));
        }
        if !path.starts_with(&self.root_dir) {
            return Err(CliError::new(
                "Resolved asset path escapes storage directory.",
            ));
        }
        Ok(path)
    }

    pub fn panel_dir(&self, session_id: &str, panel_id: &str) -> PathBuf {
        self.root_dir
            .join("sessions")
            .join(sanitize_path_part(session_id))
            .join("panels")
            .join(sanitize_path_part(panel_id))
    }
}

pub struct WrittenAsset {
    pub asset_ref: String,
    pub file_name: String,
    pub file_path: PathBuf,
}

fn sanitize_asset_path(value: &str) -> String {
    let parts = value
        .split('/')
        .map(sanitize_path_part)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        "asset.png".to_owned()
    } else {
        parts.join("/")
    }
}

fn unique_file_name(assets_dir: &std::path::Path, requested_name: &str) -> String {
    let requested = sanitize_asset_path(requested_name);
    if !assets_dir.join(&requested).exists() {
        return requested;
    }
    let path = std::path::Path::new(&requested);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("asset");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    for index in 1.. {
        let candidate = format!("{stem}-{index}{extension}");
        if !assets_dir.join(&candidate).exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn migrate(connection: &Connection) -> Result<(), CliError> {
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS schema_migrations (
              id TEXT PRIMARY KEY NOT NULL,
              description TEXT NOT NULL,
              checksum TEXT NOT NULL,
              applied_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sessions (
              id TEXT PRIMARY KEY NOT NULL,
              title TEXT NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              panel_ids_json TEXT NOT NULL DEFAULT '[]',
              session_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS sessions_updated_at_idx
              ON sessions(updated_at DESC, id ASC);

            CREATE TABLE IF NOT EXISTS panels (
              id TEXT NOT NULL,
              session_id TEXT NOT NULL,
              kind TEXT NOT NULL,
              title TEXT NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              state_ref TEXT,
              panel_json TEXT NOT NULL,
              PRIMARY KEY (session_id, id),
              FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS panels_session_kind_idx
              ON panels(session_id, kind, updated_at DESC);

            CREATE TABLE IF NOT EXISTS panel_states (
              session_id TEXT NOT NULL,
              panel_id TEXT NOT NULL,
              schema_version INTEGER,
              state_json TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              PRIMARY KEY (session_id, panel_id),
              FOREIGN KEY (session_id, panel_id)
                REFERENCES panels(session_id, id)
                ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS artifacts (
              id TEXT NOT NULL,
              session_id TEXT NOT NULL,
              panel_id TEXT,
              kind TEXT NOT NULL,
              title TEXT,
              created_at TEXT NOT NULL,
              artifact_json TEXT NOT NULL,
              PRIMARY KEY (session_id, id),
              FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS artifacts_session_panel_idx
              ON artifacts(session_id, panel_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS panel_selections (
              session_id TEXT NOT NULL,
              panel_id TEXT NOT NULL,
              asset_ref TEXT,
              selected_shape_ids_json TEXT NOT NULL DEFAULT '[]',
              selection_json TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              PRIMARY KEY (session_id, panel_id),
              FOREIGN KEY (session_id, panel_id)
                REFERENCES panels(session_id, id)
                ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS wiki_tasks (
              id TEXT PRIMARY KEY NOT NULL,
              session_id TEXT NOT NULL,
              panel_id TEXT NOT NULL,
              type TEXT NOT NULL,
              status TEXT NOT NULL,
              target_id TEXT NOT NULL,
              document_id TEXT,
              wiki_space_id TEXT,
              markdown_version INTEGER,
              claimed_by_process_id TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              task_json TEXT NOT NULL,
              FOREIGN KEY (session_id, panel_id)
                REFERENCES panels(session_id, id)
                ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS wiki_tasks_status_idx
              ON wiki_tasks(status, updated_at ASC);

            CREATE INDEX IF NOT EXISTS wiki_tasks_panel_status_idx
              ON wiki_tasks(session_id, panel_id, status, updated_at ASC);

            CREATE TABLE IF NOT EXISTS key_values (
              namespace TEXT NOT NULL,
              key TEXT NOT NULL,
              value_json TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              PRIMARY KEY (namespace, key)
            );
            "#,
        )
        .map_err(to_cli_error)?;

    connection
        .execute(
            r#"
            INSERT OR IGNORE INTO schema_migrations (id, description, checksum, applied_at)
            VALUES ('0001_initial', 'Create initial OpenPanels SQLite storage schema', 'rust-compatible', ?)
            "#,
            params![crate::control::now_iso()],
        )
        .map_err(to_cli_error)?;
    Ok(())
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
