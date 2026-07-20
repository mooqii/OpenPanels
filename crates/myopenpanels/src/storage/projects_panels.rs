use crate::error::CliError;
use crate::paths::{sanitize_path_part, MyOpenPanelsPaths};
use crate::tasks::model::{TaskLease, TaskRecord, TaskStatus};
use crate::types::{Panel, PanelKind, Project};
use rusqlite::{
    params, Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior,
};
use serde::Serialize;
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

#[derive(Debug, Clone)]
pub struct TaskInsert {
    pub id: String,
    pub queue: String,
    pub task_type: String,
    pub capability: String,
    pub target_ref: String,
    pub input: Value,
    pub source: Value,
    pub max_attempts: i64,
    pub dispatch_mode: String,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeScope {
    pub kind: String,
    pub project_id: Option<String>,
    pub panel_id: Option<String>,
    pub revision: i64,
}

impl Storage {
    pub fn open(paths: &MyOpenPanelsPaths) -> Result<Self, CliError> {
        fs::create_dir_all(&paths.storage_dir).map_err(to_cli_error)?;
        let database_path = paths.storage_dir.join(DATABASE_FILE_NAME);
        if database_path.exists() {
            preflight_existing_database(&database_path)?;
        }
        let mut connection = Connection::open(database_path).map_err(to_cli_error)?;
        connection
            .execute_batch(
                r#"
                PRAGMA busy_timeout = 5000;
                PRAGMA journal_mode = WAL;
                PRAGMA foreign_keys = ON;
                "#,
            )
            .map_err(to_cli_error)?;
        migrate(&mut connection)?;
        Ok(Self {
            connection,
            root_dir: paths.storage_dir.clone(),
        })
    }

    pub(crate) fn connection(&self) -> &Connection {
        &self.connection
    }

    pub(crate) fn connection_mut(&mut self) -> &mut Connection {
        &mut self.connection
    }

    pub fn list_projects(&self) -> Result<Vec<Project>, CliError> {
        let mut statement = self
            .connection
            .prepare("SELECT id, title, created_at, updated_at FROM projects ORDER BY updated_at DESC, id ASC")
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map([], project_from_row)
            .map_err(to_cli_error)?;
        rows.map(|row| {
            let mut project = row.map_err(to_cli_error)?;
            project.panel_ids = self.panel_ids(&project.id)?;
            Ok(project)
        })
        .collect()
    }

    pub fn read_project(&self, project_id: &str) -> Result<Option<Project>, CliError> {
        self.connection
            .query_row(
                "SELECT id, title, created_at, updated_at FROM projects WHERE id = ?",
                params![project_id],
                project_from_row,
            )
            .optional()
            .map_err(to_cli_error)?
            .map(|mut project| {
                project.panel_ids = self.panel_ids(&project.id)?;
                Ok(project)
            })
            .transpose()
    }

    pub fn write_project(&self, project: &Project) -> Result<(), CliError> {
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        tx.execute(
            r#"
                INSERT INTO projects (id, title, created_at, updated_at)
                VALUES (?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                  title = excluded.title,
                  created_at = excluded.created_at,
                  updated_at = excluded.updated_at
                "#,
            params![
                project.id,
                project.title,
                project.created_at,
                project.updated_at,
            ],
        )
        .map_err(to_cli_error)?;
        let revision = record_scope(&tx, "project", Some(&project.id), None)?;
        upsert_scope(&tx, revision, "catalog", None, None)?;
        tx.commit().map_err(to_cli_error)
    }

    pub fn delete_project(&self, project_id: &str) -> Result<(), CliError> {
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        tx.execute("DELETE FROM projects WHERE id = ?", params![project_id])
            .map_err(to_cli_error)?;
        record_scope(&tx, "catalog", None, None)?;
        tx.commit().map_err(to_cli_error)?;
        let project_dir = self
            .root_dir
            .join("projects")
            .join(sanitize_path_part(project_id));
        fs::remove_dir_all(project_dir)
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

    pub fn read_panel(&self, project_id: &str, panel_id: &str) -> Result<Option<Panel>, CliError> {
        self.connection
            .query_row(
                "SELECT project_id, id, kind, title, created_at, updated_at FROM panels WHERE project_id = ? AND id = ?",
                params![project_id, panel_id],
                panel_from_row,
            )
            .optional()
            .map_err(to_cli_error)
    }

    fn read_panel_kind(
        &self,
        project_id: &str,
        panel_id: &str,
    ) -> Result<Option<PanelKind>, CliError> {
        self.connection
            .query_row(
                "SELECT kind FROM panels WHERE project_id = ? AND id = ?",
                params![project_id, panel_id],
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
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        tx.execute(
            r#"
                INSERT INTO panels (project_id, id, kind, title, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?)
                ON CONFLICT(project_id, id) DO UPDATE SET
                  kind = excluded.kind,
                  title = excluded.title,
                  created_at = excluded.created_at,
                  updated_at = excluded.updated_at
                "#,
            params![
                panel.project_id,
                panel.id,
                panel.kind.as_str(),
                panel.title,
                panel.created_at,
                panel.updated_at,
            ],
        )
        .map_err(to_cli_error)?;
        record_scope(&tx, "project", Some(&panel.project_id), Some(&panel.id))?;
        tx.commit().map_err(to_cli_error)
    }

    pub fn read_panel_state(
        &self,
        project_id: &str,
        panel_id: &str,
    ) -> Result<Option<Value>, CliError> {
        self.connection
            .query_row(
                "SELECT state_json FROM panel_states WHERE project_id = ? AND panel_id = ?",
                params![project_id, panel_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(to_cli_error)?
            .map(|raw| serde_json::from_str::<Value>(&raw).map_err(to_cli_error))
            .transpose()
    }

    pub fn write_panel_state(
        &self,
        project_id: &str,
        panel_id: &str,
        state: &Value,
    ) -> Result<i64, CliError> {
        match self.write_panel_state_if_current(project_id, panel_id, state, None)? {
            Ok(revision) => Ok(revision),
            Err(_) => unreachable!("unconditional state writes cannot conflict"),
        }
    }

    pub fn write_panel_state_if_current(
        &self,
        project_id: &str,
        panel_id: &str,
        state: &Value,
        base_revision: Option<i64>,
    ) -> Result<Result<i64, PanelStateWriteConflict>, CliError> {
        let state_json = serde_json::to_string(state).map_err(to_cli_error)?;
        let content_hash = hash_text(&state_json);
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        let current = tx
            .query_row(
                "SELECT revision, content_hash FROM panel_states WHERE project_id = ? AND panel_id = ?",
                params![project_id, panel_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(to_cli_error)?;
        let current_revision = current.as_ref().map(|value| value.0).unwrap_or(0);
        if base_revision.is_some_and(|base| base != current_revision) {
            return Ok(Err(PanelStateWriteConflict {
                base_revision: base_revision.unwrap_or(0),
                current_revision,
            }));
        }
        if current
            .as_ref()
            .is_some_and(|value| value.1 == content_hash)
        {
            tx.commit().map_err(to_cli_error)?;
            return Ok(Ok(current_revision));
        }
        let revision = record_scope(&tx, "panel_state", Some(project_id), Some(panel_id))?;
        tx.execute(
            r#"
            INSERT INTO panel_states (
              project_id, panel_id, schema_version, revision, content_hash, state_json, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(project_id, panel_id) DO UPDATE SET
              schema_version = excluded.schema_version,
              revision = excluded.revision,
              content_hash = excluded.content_hash,
              state_json = excluded.state_json,
              updated_at = excluded.updated_at
            "#,
            params![
                project_id,
                panel_id,
                self.read_panel_kind(project_id, panel_id)?
                    .and_then(|kind| extract_panel_state_schema_version(kind, state)),
                revision,
                content_hash,
                state_json,
                crate::control::now_iso(),
            ],
        )
        .map_err(to_cli_error)?;
        tx.commit().map_err(to_cli_error)?;
        Ok(Ok(revision))
    }

    pub(crate) fn write_panel_state_in_transaction(
        tx: &Transaction<'_>,
        project_id: &str,
        panel_id: &str,
        state: &Value,
    ) -> Result<i64, CliError> {
        let state_json = serde_json::to_string(state).map_err(to_cli_error)?;
        let content_hash = hash_text(&state_json);
        let current = tx
            .query_row(
                "SELECT revision, content_hash FROM panel_states WHERE project_id = ? AND panel_id = ?",
                params![project_id, panel_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(to_cli_error)?;
        if current
            .as_ref()
            .is_some_and(|value| value.1 == content_hash)
        {
            return Ok(current.map(|value| value.0).unwrap_or(0));
        }
        let panel_kind = tx
            .query_row(
                "SELECT kind FROM panels WHERE project_id = ? AND id = ?",
                params![project_id, panel_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(to_cli_error)?;
        let revision = record_scope(tx, "panel_state", Some(project_id), Some(panel_id))?;
        tx.execute(
            r#"
            INSERT INTO panel_states (
              project_id, panel_id, schema_version, revision, content_hash, state_json, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(project_id, panel_id) DO UPDATE SET
              schema_version = excluded.schema_version,
              revision = excluded.revision,
              content_hash = excluded.content_hash,
              state_json = excluded.state_json,
              updated_at = excluded.updated_at
            "#,
            params![
                project_id,
                panel_id,
                panel_kind
                    .as_deref()
                    .and_then(PanelKind::parse)
                    .and_then(|kind| extract_panel_state_schema_version(kind, state)),
                revision,
                content_hash,
                state_json,
                crate::control::now_iso(),
            ],
        )
        .map_err(to_cli_error)?;
        Ok(revision)
    }

    pub fn write_agent_operation(&self, operation: &Value) -> Result<(), CliError> {
        let required = |name: &str| {
            operation
                .get(name)
                .and_then(Value::as_str)
                .ok_or_else(|| CliError::new(format!("Agent operation is missing {name}")))
        };
        let project_id = operation
            .get("projectId")
            .and_then(Value::as_str)
            .ok_or_else(|| CliError::new("Agent operation is missing projectId"))?;
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        tx.execute(
            r#"
                INSERT INTO agent_operations (
                  id, owner_context_id, intent, status, project_id, panel_id,
                  guide_id, protocol_version, target_json, input_json,
                  result_json, error_json, created_at, updated_at, completed_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                  status = excluded.status,
                  target_json = excluded.target_json,
                  input_json = excluded.input_json,
                  result_json = excluded.result_json,
                  error_json = excluded.error_json,
                  updated_at = excluded.updated_at,
                  completed_at = excluded.completed_at
                "#,
            params![
                required("id")?,
                required("ownerContextId")?,
                required("intent")?,
                required("status")?,
                project_id,
                required("panelId")?,
                operation.get("guideId").and_then(Value::as_str),
                operation
                    .get("protocolVersion")
                    .and_then(Value::as_i64)
                    .unwrap_or(2),
                serde_json::to_string(operation.get("target").unwrap_or(&Value::Null))
                    .map_err(to_cli_error)?,
                serde_json::to_string(operation.get("input").unwrap_or(&Value::Null))
                    .map_err(to_cli_error)?,
                operation
                    .get("result")
                    .filter(|value| !value.is_null())
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(to_cli_error)?,
                operation
                    .get("error")
                    .filter(|value| !value.is_null())
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(to_cli_error)?,
                required("createdAt")?,
                required("updatedAt")?,
                operation.get("completedAt").and_then(Value::as_str),
            ],
        )
        .map_err(to_cli_error)?;
        record_scope(
            &tx,
            "agent_operations",
            Some(project_id),
            operation.get("panelId").and_then(Value::as_str),
        )?;
        tx.commit().map_err(to_cli_error)
    }

    pub fn read_agent_operation(&self, operation_id: &str) -> Result<Option<Value>, CliError> {
        self.connection
            .query_row(
                &operation_select_sql("WHERE o.id = ?"),
                params![operation_id],
                operation_from_row,
            )
            .optional()
            .map_err(to_cli_error)
    }

    pub fn list_agent_operations(
        &self,
        owner_context_id: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<Value>, CliError> {
        let (where_clause, values): (&str, Vec<&str>) = match (owner_context_id, status) {
            (Some(owner), Some(status)) => (
                "WHERE o.owner_context_id = ? AND o.status = ?",
                vec![owner, status],
            ),
            (Some(owner), None) => ("WHERE o.owner_context_id = ?", vec![owner]),
            (None, Some(status)) => ("WHERE o.status = ?", vec![status]),
            (None, None) => ("", Vec::new()),
        };
        let sql = format!(
            "{} {}",
            operation_select_sql(where_clause),
            "ORDER BY o.updated_at DESC, o.id ASC"
        );
        let mut statement = self.connection.prepare(&sql).map_err(to_cli_error)?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(values), operation_from_row)
            .map_err(to_cli_error)?;
        rows.map(|row| row.map_err(to_cli_error)).collect()
    }
    pub(crate) fn list_terminal_agent_operation_ids_before(
        &self,
        completed_before: &str,
    ) -> Result<Vec<String>, CliError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id FROM agent_operations
                 WHERE status IN ('completed', 'cancelled')
                   AND completed_at IS NOT NULL
                   AND completed_at <= ?
                 ORDER BY completed_at ASC, id ASC",
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![completed_before], |row| row.get::<_, String>(0))
            .map_err(to_cli_error)?;
        rows.map(|row| row.map_err(to_cli_error)).collect()
    }
}
