use crate::error::CliError;
use crate::paths::{sanitize_path_part, OpenPanelsPaths};
use crate::types::{Panel, PanelKind, Session};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

pub const DATABASE_FILE_NAME: &str = "main.sqlite3";

#[derive(Debug)]
pub struct Storage {
    connection: Connection,
    root_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PanelStateWriteConflict {
    pub base_revision: i64,
    pub current_revision: i64,
}

impl Storage {
    pub fn open(paths: &OpenPanelsPaths) -> Result<Self, CliError> {
        fs::create_dir_all(&paths.storage_dir).map_err(to_cli_error)?;
        let mut connection =
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
        migrate(&mut connection)?;
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
        self.record_change("session", Some(&session.id), None)?;
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
        self.record_change("session", Some(session_id), None)?;
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

    fn read_panel_kind(
        &self,
        session_id: &str,
        panel_id: &str,
    ) -> Result<Option<PanelKind>, CliError> {
        self.connection
            .query_row(
                "SELECT kind FROM panels WHERE session_id = ? AND id = ?",
                params![session_id, panel_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(to_cli_error)?
            .map(|kind| {
                PanelKind::parse(&kind)
                    .ok_or_else(|| CliError::new(format!("Unknown panel kind in storage: {kind}")))
            })
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
        self.record_change("panel", Some(&panel.session_id), Some(&panel.id))?;
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
    ) -> Result<i64, CliError> {
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
                    self.read_panel_kind(session_id, panel_id)?
                        .and_then(|kind| extract_panel_state_schema_version(kind, state)),
                    serde_json::to_string(state).map_err(to_cli_error)?,
                    crate::control::now_iso(),
                ],
            )
            .map_err(to_cli_error)?;
        self.record_change("panel_state", Some(session_id), Some(panel_id))
    }

    pub fn write_panel_state_if_current(
        &self,
        session_id: &str,
        panel_id: &str,
        state: &Value,
        base_revision: Option<i64>,
    ) -> Result<Result<i64, PanelStateWriteConflict>, CliError> {
        if let Some(base_revision) = base_revision {
            let current_revision = self.read_panel_state_revision(session_id, panel_id)?;
            if base_revision < current_revision {
                return Ok(Err(PanelStateWriteConflict {
                    base_revision,
                    current_revision,
                }));
            }
        }
        self.write_panel_state(session_id, panel_id, state).map(Ok)
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

    pub fn sync_project_tasks_from_panel(
        &self,
        session_id: &str,
        panel_id: &str,
        panel_kind: &str,
        queue: &str,
        state: &Value,
    ) -> Result<(), CliError> {
        let existing_task_runtime = self.read_project_task_runtime(session_id, panel_id, queue)?;
        self.connection
            .execute(
                "DELETE FROM project_tasks WHERE session_id = ? AND panel_id = ? AND queue = ?",
                params![session_id, panel_id, queue],
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
                .ok_or_else(|| CliError::new("Project task id is required."))?;
            let existing_runtime = existing_task_runtime.get(id);
            let created_at = task
                .get("createdAt")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| existing_runtime.map(|runtime| runtime.created_at.clone()))
                .unwrap_or_else(crate::control::now_iso);
            let updated_at = task
                .get("updatedAt")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| existing_runtime.map(|runtime| runtime.updated_at.clone()))
                .unwrap_or_else(|| created_at.clone());
            let attempts = task
                .get("attempt")
                .and_then(Value::as_i64)
                .or_else(|| existing_runtime.map(|runtime| runtime.attempts))
                .unwrap_or(0);
            let max_attempts = task
                .get("maxAttempts")
                .and_then(Value::as_i64)
                .or_else(|| existing_runtime.map(|runtime| runtime.max_attempts))
                .unwrap_or(3);
            let lease_owner = task
                .get("leaseOwner")
                .map(|value| value.as_str().map(str::to_owned))
                .unwrap_or_else(|| {
                    existing_runtime.and_then(|runtime| runtime.lease_owner.clone())
                });
            let lease_expires_at = task
                .get("leaseExpiresAt")
                .map(|value| value.as_str().map(str::to_owned))
                .unwrap_or_else(|| {
                    existing_runtime.and_then(|runtime| runtime.lease_expires_at.clone())
                });
            let last_heartbeat_at = task
                .get("lastHeartbeatAt")
                .map(|value| value.as_str().map(str::to_owned))
                .unwrap_or_else(|| {
                    existing_runtime.and_then(|runtime| runtime.last_heartbeat_at.clone())
                });
            let retry_after = task
                .get("retryAfter")
                .map(|value| value.as_str().map(str::to_owned))
                .unwrap_or_else(|| {
                    existing_runtime.and_then(|runtime| runtime.retry_after.clone())
                });
            self.connection
                .execute(
                    r#"
                    INSERT INTO project_tasks (
                      id, queue, session_id, panel_id, panel_kind, type, status,
                      target_id, created_at, updated_at, attempts, max_attempts,
                      lease_owner, lease_expires_at, last_heartbeat_at, retry_after,
                      task_json
                    )
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    ON CONFLICT(id) DO UPDATE SET
                      queue = excluded.queue,
                      session_id = excluded.session_id,
                      panel_id = excluded.panel_id,
                      panel_kind = excluded.panel_kind,
                      type = excluded.type,
                      status = excluded.status,
                      target_id = excluded.target_id,
                      created_at = excluded.created_at,
                      updated_at = excluded.updated_at,
                      attempts = excluded.attempts,
                      max_attempts = excluded.max_attempts,
                      lease_owner = excluded.lease_owner,
                      lease_expires_at = excluded.lease_expires_at,
                      last_heartbeat_at = excluded.last_heartbeat_at,
                      retry_after = excluded.retry_after,
                      task_json = excluded.task_json
                    "#,
                    params![
                        id,
                        queue,
                        session_id,
                        panel_id,
                        panel_kind,
                        task.get("type")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown"),
                        task.get("status")
                            .and_then(Value::as_str)
                            .unwrap_or("queued"),
                        task.get("targetId").and_then(Value::as_str).unwrap_or(""),
                        created_at,
                        updated_at,
                        attempts,
                        max_attempts,
                        lease_owner,
                        lease_expires_at,
                        last_heartbeat_at,
                        retry_after,
                        serde_json::to_string(task).map_err(to_cli_error)?,
                    ],
                )
                .map_err(to_cli_error)?;
        }
        Ok(())
    }

    fn read_project_task_runtime(
        &self,
        session_id: &str,
        panel_id: &str,
        queue: &str,
    ) -> Result<HashMap<String, ProjectTaskRuntime>, CliError> {
        let mut statement = self
            .connection
            .prepare(
                r#"
                SELECT
                  id, created_at, updated_at, attempts, max_attempts,
                  lease_owner, lease_expires_at, last_heartbeat_at, retry_after
                FROM project_tasks
                WHERE session_id = ? AND panel_id = ? AND queue = ?
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![session_id, panel_id, queue], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    ProjectTaskRuntime {
                        created_at: row.get::<_, String>(1)?,
                        updated_at: row.get::<_, String>(2)?,
                        attempts: row.get::<_, i64>(3)?,
                        max_attempts: row.get::<_, i64>(4)?,
                        lease_owner: row.get::<_, Option<String>>(5)?,
                        lease_expires_at: row.get::<_, Option<String>>(6)?,
                        last_heartbeat_at: row.get::<_, Option<String>>(7)?,
                        retry_after: row.get::<_, Option<String>>(8)?,
                    },
                ))
            })
            .map_err(to_cli_error)?;
        rows.map(|row| row.map_err(to_cli_error)).collect()
    }

    pub fn list_project_tasks(&self, session_id: &str) -> Result<Vec<Value>, CliError> {
        let mut statement = self
            .connection
            .prepare(
                r#"
                SELECT
                  id, queue, session_id, panel_id, panel_kind, type, status,
                  target_id, created_at, updated_at, attempts, max_attempts,
                  lease_owner, lease_expires_at, last_heartbeat_at, retry_after,
                  task_json
                FROM project_tasks
                WHERE session_id = ?
                ORDER BY updated_at DESC, id ASC
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![session_id], |row| {
                let task_json: String = row.get(16)?;
                let task = serde_json::from_str::<Value>(&task_json).unwrap_or_else(|_| json!({}));
                let queue = row.get::<_, String>(1)?;
                let panel_kind = row.get::<_, String>(4)?;
                let task_type = row.get::<_, String>(5)?;
                let status = row.get::<_, String>(6)?;
                let target_id = row.get::<_, String>(7)?;
                let attempts = row.get::<_, i64>(10)?;
                let max_attempts = row.get::<_, i64>(11)?;
                let lease_owner = row.get::<_, Option<String>>(12)?;
                let lease_expires_at = row.get::<_, Option<String>>(13)?;
                let last_heartbeat_at = row.get::<_, Option<String>>(14)?;
                let retry_after = row.get::<_, Option<String>>(15)?;
                Ok(json!({
                    "id": row.get::<_, String>(0)?,
                    "queue": queue,
                    "sessionId": row.get::<_, String>(2)?,
                    "panelId": row.get::<_, String>(3)?,
                    "panelKind": panel_kind,
                    "type": task_type,
                    "status": status,
                    "targetId": target_id,
                    "createdAt": row.get::<_, String>(8)?,
                    "updatedAt": row.get::<_, String>(9)?,
                    "attempt": attempts,
                    "maxAttempts": max_attempts,
                    "lease": {
                        "owner": lease_owner,
                        "expiresAt": lease_expires_at,
                        "heartbeatAt": last_heartbeat_at,
                    },
                    "retryAfter": retry_after,
                    "capability": project_task_capability(&queue, &task_type),
                    "input": project_task_input(&task),
                    "source": project_task_source(&task),
                    "result": task.get("result").cloned().unwrap_or(Value::Null),
                    "error": task.get("error").cloned().unwrap_or(Value::Null),
                    "task": task,
                }))
            })
            .map_err(to_cli_error)?;
        rows.map(|row| row.map_err(to_cli_error)).collect()
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
        self.record_change("artifact", Some(session_id), None)?;
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
        self.record_change("panel_selection", Some(session_id), Some(panel_id))?;
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

    pub fn read_change_seq(&self) -> Result<i64, CliError> {
        self.connection
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) FROM storage_changes",
                [],
                |row| row.get(0),
            )
            .map_err(to_cli_error)
    }

    pub fn read_panel_state_revision(
        &self,
        session_id: &str,
        panel_id: &str,
    ) -> Result<i64, CliError> {
        self.connection
            .query_row(
                r#"
                SELECT COALESCE(MAX(seq), 0)
                FROM storage_changes
                WHERE kind = 'panel_state' AND session_id = ? AND panel_id = ?
                "#,
                params![session_id, panel_id],
                |row| row.get(0),
            )
            .map_err(to_cli_error)
    }

    fn record_change(
        &self,
        kind: &str,
        session_id: Option<&str>,
        panel_id: Option<&str>,
    ) -> Result<i64, CliError> {
        self.connection
            .execute(
                r#"
                INSERT INTO storage_changes (kind, session_id, panel_id, created_at)
                VALUES (?, ?, ?, ?)
                "#,
                params![kind, session_id, panel_id, crate::control::now_iso()],
            )
            .map_err(to_cli_error)?;
        Ok(self.connection.last_insert_rowid())
    }
}

pub struct WrittenAsset {
    pub asset_ref: String,
    pub file_name: String,
    pub file_path: PathBuf,
}

#[derive(Debug, Clone)]
struct ProjectTaskRuntime {
    created_at: String,
    updated_at: String,
    attempts: i64,
    max_attempts: i64,
    lease_owner: Option<String>,
    lease_expires_at: Option<String>,
    last_heartbeat_at: Option<String>,
    retry_after: Option<String>,
}

fn project_task_capability(queue: &str, task_type: &str) -> String {
    match (queue, task_type) {
        ("wiki", "convert_document_to_markdown") => "wiki.convertDocument".to_owned(),
        ("wiki", "ingest_markdown_into_wiki") => "wiki.ingestMarkdown".to_owned(),
        ("wiki", "rebuild_wiki_index") => "wiki.rebuildIndex".to_owned(),
        _ => format!("{}.{}", queue, task_type.replace('_', ".")),
    }
}

fn project_task_input(task: &Value) -> Value {
    if let Some(input) = task.get("input") {
        return input.clone();
    }
    let Some(object) = task.as_object() else {
        return Value::Null;
    };
    let mut input = serde_json::Map::new();
    for (key, value) in object {
        if matches!(
            key.as_str(),
            "id" | "type"
                | "status"
                | "createdAt"
                | "updatedAt"
                | "claimedByProcessId"
                | "result"
                | "error"
        ) {
            continue;
        }
        input.insert(key.clone(), value.clone());
    }
    Value::Object(input)
}

fn project_task_source(task: &Value) -> Value {
    if let Some(source) = task.get("source") {
        return source.clone();
    }
    let mut source = serde_json::Map::new();
    for key in ["documentId", "targetId", "wikiSpaceId"] {
        if let Some(value) = task.get(key) {
            source.insert(key.to_owned(), value.clone());
        }
    }
    Value::Object(source)
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

fn extract_panel_state_schema_version(kind: PanelKind, state: &Value) -> Option<i64> {
    match kind {
        PanelKind::Canvas => state
            .get("schema")
            .and_then(|schema| schema.get("schemaVersion"))
            .and_then(Value::as_i64),
        PanelKind::Wiki => state.get("schemaVersion").and_then(Value::as_i64),
        _ => state.get("schemaVersion").and_then(Value::as_i64),
    }
}

const SCHEMA_MIGRATIONS_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS schema_migrations (
  id TEXT PRIMARY KEY NOT NULL,
  description TEXT NOT NULL,
  checksum TEXT NOT NULL,
  applied_at TEXT NOT NULL
);
"#;

const MIGRATION_0001_SQL: &str = r#"
CREATE TABLE sessions (
  id TEXT PRIMARY KEY NOT NULL,
  title TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  panel_ids_json TEXT NOT NULL DEFAULT '[]',
  session_json TEXT NOT NULL
);

CREATE INDEX sessions_updated_at_idx
  ON sessions(updated_at DESC, id ASC);

CREATE TABLE panels (
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

CREATE INDEX panels_session_kind_idx
  ON panels(session_id, kind, updated_at DESC);

CREATE TABLE panel_states (
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

CREATE TABLE artifacts (
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

CREATE INDEX artifacts_session_panel_idx
  ON artifacts(session_id, panel_id, created_at DESC);

CREATE TABLE panel_selections (
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

CREATE TABLE wiki_tasks (
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

CREATE INDEX wiki_tasks_status_idx
  ON wiki_tasks(status, updated_at ASC);

CREATE INDEX wiki_tasks_panel_status_idx
  ON wiki_tasks(session_id, panel_id, status, updated_at ASC);

CREATE TABLE project_tasks (
  id TEXT PRIMARY KEY NOT NULL,
  queue TEXT NOT NULL,
  session_id TEXT NOT NULL,
  panel_id TEXT NOT NULL,
  panel_kind TEXT NOT NULL,
  type TEXT NOT NULL,
  status TEXT NOT NULL,
  target_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  max_attempts INTEGER NOT NULL DEFAULT 3,
  lease_owner TEXT,
  lease_expires_at TEXT,
  last_heartbeat_at TEXT,
  retry_after TEXT,
  task_json TEXT NOT NULL,
  FOREIGN KEY (session_id, panel_id)
    REFERENCES panels(session_id, id)
    ON DELETE CASCADE
);

CREATE INDEX project_tasks_session_updated_idx
  ON project_tasks(session_id, updated_at DESC);

CREATE INDEX project_tasks_session_status_idx
  ON project_tasks(session_id, status, updated_at DESC);

CREATE TABLE key_values (
  namespace TEXT NOT NULL,
  key TEXT NOT NULL,
  value_json TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (namespace, key)
);

CREATE TABLE storage_changes (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  kind TEXT NOT NULL,
  session_id TEXT,
  panel_id TEXT,
  created_at TEXT NOT NULL
);

CREATE INDEX storage_changes_seq_idx
  ON storage_changes(seq);
"#;

struct Migration {
    id: &'static str,
    description: &'static str,
    checksum_material: &'static str,
    up: fn(&Transaction<'_>) -> Result<(), CliError>,
}

#[derive(Debug, Clone)]
struct AppliedMigration {
    checksum: String,
}

fn migrations() -> &'static [Migration] {
    &[Migration {
        id: "0001_initial",
        description: "Create initial OpenPanels SQLite storage schema",
        checksum_material: MIGRATION_0001_SQL,
        up: migration_0001,
    }]
}

fn migrate(connection: &mut Connection) -> Result<(), CliError> {
    connection
        .execute_batch(SCHEMA_MIGRATIONS_SQL)
        .map_err(to_cli_error)?;
    run_migrations(connection, migrations())
}

fn run_migrations(connection: &mut Connection, migrations: &[Migration]) -> Result<(), CliError> {
    validate_registry(migrations)?;
    let applied = read_applied_migrations(connection)?;
    let registry_ids = migrations
        .iter()
        .map(|migration| migration.id)
        .collect::<HashSet<_>>();
    for id in applied.keys() {
        if !registry_ids.contains(id.as_str()) {
            return Err(CliError::new(format!(
                "unknown future migration found in database: {id}"
            )));
        }
    }

    let mut saw_missing = false;
    for migration in migrations {
        if applied.contains_key(migration.id) {
            if saw_missing {
                return Err(CliError::new(format!(
                    "non-contiguous migration history: {} is applied after a missing earlier migration",
                    migration.id
                )));
            }
        } else {
            saw_missing = true;
            continue;
        }
    }

    for migration in migrations {
        let Some(applied_migration) = applied.get(migration.id) else {
            continue;
        };
        let expected_checksum = migration_checksum(migration);
        if applied_migration.checksum == expected_checksum {
            continue;
        }
        return Err(CliError::new(format!(
            "migration checksum mismatch for {}: expected {}, got {}",
            migration.id, expected_checksum, applied_migration.checksum
        )));
    }

    for migration in migrations {
        if applied.contains_key(migration.id) {
            continue;
        }
        apply_migration(connection, migration)?;
    }
    Ok(())
}

fn validate_registry(migrations: &[Migration]) -> Result<(), CliError> {
    let mut ids = HashSet::new();
    for migration in migrations {
        if !ids.insert(migration.id) {
            return Err(CliError::new(format!(
                "duplicate migration id in registry: {}",
                migration.id
            )));
        }
    }
    Ok(())
}

fn read_applied_migrations(
    connection: &Connection,
) -> Result<HashMap<String, AppliedMigration>, CliError> {
    let mut statement = connection
        .prepare("SELECT id, checksum FROM schema_migrations ORDER BY id ASC")
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                AppliedMigration {
                    checksum: row.get::<_, String>(1)?,
                },
            ))
        })
        .map_err(to_cli_error)?;
    rows.map(|row| row.map_err(to_cli_error)).collect()
}

fn apply_migration(connection: &mut Connection, migration: &Migration) -> Result<(), CliError> {
    let checksum = migration_checksum(migration);
    let tx = connection.transaction().map_err(to_cli_error)?;
    let result = (|| -> Result<(), CliError> {
        (migration.up)(&tx)?;
        tx.execute(
            r#"
            INSERT INTO schema_migrations (id, description, checksum, applied_at)
            VALUES (?, ?, ?, ?)
            "#,
            params![
                migration.id,
                migration.description,
                checksum,
                crate::control::now_iso()
            ],
        )
        .map_err(to_cli_error)?;
        Ok(())
    })();
    match result {
        Ok(()) => tx.commit().map_err(to_cli_error),
        Err(error) => Err(CliError::new(format!(
            "migration failed and rolled back for {}: {}",
            migration.id, error
        ))),
    }
}

fn migration_checksum(migration: &Migration) -> String {
    format!(
        "{:x}",
        Sha256::digest(migration.checksum_material.as_bytes())
    )
}

fn migration_0001(tx: &Transaction<'_>) -> Result<(), CliError> {
    tx.execute_batch(MIGRATION_0001_SQL).map_err(to_cli_error)
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::OpenPanelsPaths;
    use crate::types::{Panel, PanelKind, Session};
    use serde_json::json;
    use tempfile::tempdir;

    fn paths_for(storage_dir: PathBuf) -> OpenPanelsPaths {
        OpenPanelsPaths {
            context_dir: storage_dir.join("contexts").join("test"),
            context_id: "test".to_owned(),
            context_id_source: "test".to_owned(),
            project_dir: storage_dir.join("project"),
            storage_dir,
        }
    }

    #[test]
    fn storage_writes_advance_change_seq() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");

        assert_eq!(storage.read_change_seq().expect("initial seq"), 0);

        let session = Session {
            id: "session:test".to_owned(),
            title: "Test".to_owned(),
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            updated_at: "2026-01-01T00:00:00.000Z".to_owned(),
            panel_ids: vec!["panel:canvas".to_owned()],
        };
        storage.write_session(&session).expect("write session");
        let after_session = storage.read_change_seq().expect("session seq");
        assert!(after_session > 0);

        let panel = Panel {
            id: "panel:canvas".to_owned(),
            session_id: session.id.clone(),
            kind: PanelKind::Canvas,
            title: "Canvas".to_owned(),
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            updated_at: "2026-01-01T00:00:00.000Z".to_owned(),
            state_ref: None,
        };
        storage.write_panel(&panel).expect("write panel");
        let after_panel = storage.read_change_seq().expect("panel seq");
        assert!(after_panel > after_session);

        storage
            .write_panel_state(
                &session.id,
                &panel.id,
                &json!({ "schema": { "schemaVersion": 1 }, "store": {} }),
            )
            .expect("write state");
        let after_state = storage.read_change_seq().expect("state seq");
        assert!(after_state > after_panel);
        assert_eq!(
            storage
                .read_panel_state_revision(&session.id, &panel.id)
                .expect("state revision"),
            after_state
        );

        let stale_write = storage
            .write_panel_state_if_current(
                &session.id,
                &panel.id,
                &json!({ "schema": { "schemaVersion": 1 }, "store": { "stale": true } }),
                Some(after_panel),
            )
            .expect("stale write");
        assert_eq!(
            stale_write,
            Err(PanelStateWriteConflict {
                base_revision: after_panel,
                current_revision: after_state,
            })
        );

        storage
            .write_panel_selection(&session.id, &panel.id, &json!({ "selectedShapeIds": [] }))
            .expect("write selection");
        let after_selection = storage.read_change_seq().expect("selection seq");
        assert!(after_selection > after_state);

        let schema_version: i64 = storage
            .connection
            .query_row(
                "SELECT schema_version FROM panel_states WHERE session_id = ? AND panel_id = ?",
                params![session.id, panel.id],
                |row| row.get(0),
            )
            .expect("schema version");
        assert_eq!(schema_version, 1);
    }

    #[test]
    fn storage_records_initial_migration() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");

        let migration_count: i64 = storage
            .connection
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE id = '0001_initial'",
                [],
                |row| row.get(0),
            )
            .expect("migration count");
        assert_eq!(migration_count, 1);

        let table_count: i64 = storage
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'project_tasks'",
                [],
                |row| row.get(0),
            )
            .expect("project_tasks table");
        assert_eq!(table_count, 1);

        let runtime_column_count: i64 = storage
            .connection
            .query_row(
                r#"
                SELECT COUNT(*)
                FROM pragma_table_info('project_tasks')
                WHERE name IN (
                  'attempts', 'max_attempts', 'lease_owner', 'lease_expires_at',
                  'last_heartbeat_at', 'retry_after'
                )
                "#,
                [],
                |row| row.get(0),
            )
            .expect("project_tasks runtime columns");
        assert_eq!(runtime_column_count, 6);
    }

    #[test]
    fn migration_open_is_idempotent() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");
        let applied_before: Vec<(String, String)> = storage
            .connection
            .prepare("SELECT id, applied_at FROM schema_migrations ORDER BY id")
            .expect("prepare")
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("query")
            .map(|row| row.expect("row"))
            .collect();
        drop(storage);

        let reopened = Storage::open(&paths).expect("reopen");
        let applied_after: Vec<(String, String)> = reopened
            .connection
            .prepare("SELECT id, applied_at FROM schema_migrations ORDER BY id")
            .expect("prepare")
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("query")
            .map(|row| row.expect("row"))
            .collect();
        assert_eq!(applied_after, applied_before);
    }

    #[test]
    fn migration_rejects_checksum_mismatch() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");
        storage
            .connection
            .execute(
                "UPDATE schema_migrations SET checksum = 'not-the-checksum' WHERE id = '0001_initial'",
                [],
            )
            .expect("bad checksum");
        drop(storage);

        let error = match Storage::open(&paths) {
            Ok(_) => panic!("checksum mismatch"),
            Err(error) => error,
        };
        assert!(error.message().contains("migration checksum mismatch"));
    }

    #[test]
    fn migration_rejects_unknown_future_migration() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");
        storage
            .connection
            .execute(
                r#"
                INSERT INTO schema_migrations (id, description, checksum, applied_at)
                VALUES ('9999_future', 'Future migration', 'future', '2026-01-01T00:00:00.000Z')
                "#,
                [],
            )
            .expect("future migration");
        drop(storage);

        let error = match Storage::open(&paths) {
            Ok(_) => panic!("future migration"),
            Err(error) => error,
        };
        assert!(error.message().contains("unknown future migration"));
    }

    #[test]
    fn migration_failure_rolls_back() {
        fn bad_migration(tx: &Transaction<'_>) -> Result<(), CliError> {
            tx.execute_batch(
                r#"
                CREATE TABLE rollback_probe (id TEXT PRIMARY KEY NOT NULL);
                INSERT INTO missing_table (id) VALUES ('boom');
                "#,
            )
            .map_err(to_cli_error)
        }

        let temp = tempdir().expect("tempdir");
        let mut connection = Connection::open(temp.path().join("test.sqlite3")).expect("db");
        connection
            .execute_batch(SCHEMA_MIGRATIONS_SQL)
            .expect("schema migrations");
        let bad = Migration {
            id: "0001_bad",
            description: "Bad migration",
            checksum_material: "bad",
            up: bad_migration,
        };

        let error = run_migrations(&mut connection, &[bad]).expect_err("migration failure");
        assert!(error.message().contains("migration failed and rolled back"));
        let table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'rollback_probe'",
                [],
                |row| row.get(0),
            )
            .expect("table count");
        assert_eq!(table_count, 0);
    }

    #[test]
    fn project_task_sync_preserves_existing_times_when_task_omits_them() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");
        let session = Session {
            id: "session:test".to_owned(),
            title: "Test".to_owned(),
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            updated_at: "2026-01-01T00:00:00.000Z".to_owned(),
            panel_ids: vec!["panel:wiki".to_owned()],
        };
        storage.write_session(&session).expect("write session");
        let panel = Panel {
            id: "panel:wiki".to_owned(),
            session_id: session.id.clone(),
            kind: PanelKind::Wiki,
            title: "Wiki".to_owned(),
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            updated_at: "2026-01-01T00:00:00.000Z".to_owned(),
            state_ref: None,
        };
        storage.write_panel(&panel).expect("write panel");
        let state = json!({
            "tasks": [{
                "id": "task:missing-times",
                "type": "demo",
                "status": "queued",
                "targetId": "target",
            }],
        });

        storage
            .sync_project_tasks_from_panel(&session.id, &panel.id, "wiki", "wiki", &state)
            .expect("initial sync");
        storage
            .connection
            .execute(
                r#"
                UPDATE project_tasks
                SET
                  created_at = 'created:stable',
                  updated_at = 'updated:stable',
                  attempts = 2,
                  max_attempts = 5,
                  lease_owner = 'agent:test',
                  lease_expires_at = 'expires:stable',
                  last_heartbeat_at = 'heartbeat:stable',
                  retry_after = 'retry:stable'
                WHERE id = 'task:missing-times'
                "#,
                [],
            )
            .expect("seed stable times");
        storage
            .sync_project_tasks_from_panel(&session.id, &panel.id, "wiki", "wiki", &state)
            .expect("repeat sync");

        let tasks = storage
            .list_project_tasks(&session.id)
            .expect("project tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["createdAt"], json!("created:stable"));
        assert_eq!(tasks[0]["updatedAt"], json!("updated:stable"));
        assert_eq!(tasks[0]["attempt"], json!(2));
        assert_eq!(tasks[0]["maxAttempts"], json!(5));
        assert_eq!(tasks[0]["lease"]["owner"], json!("agent:test"));
        assert_eq!(tasks[0]["lease"]["expiresAt"], json!("expires:stable"));
        assert_eq!(tasks[0]["lease"]["heartbeatAt"], json!("heartbeat:stable"));
        assert_eq!(tasks[0]["retryAfter"], json!("retry:stable"));
    }
}
