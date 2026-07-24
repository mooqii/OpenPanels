use crate::error::CliError;
use crate::paths::{sanitize_path_part, MyOpenPanelsPaths};
use crate::types::{Panel, PanelKind, Project};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const DATABASE_FILE_NAME: &str = "main.sqlite3";

#[derive(Debug)]
pub struct Storage {
    connection: Connection,
    root_dir: PathBuf,
    project_root_dir: PathBuf,
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
    pub idempotency_key: Option<String>,
    pub exclusive_non_terminal: bool,
}

impl TaskInsert {
    pub fn for_capability(
        capability_key: &str,
        task_type: &str,
        id: String,
        target_ref: String,
        input: Value,
        source: Value,
    ) -> Result<Self, CliError> {
        let route = crate::capabilities::task_route_for_capability(capability_key, task_type)?;
        crate::capabilities::validate_task_local_skill(
            &route.queue,
            &route.task_type,
            &route.capability,
            &input,
            &source,
        )?;
        Ok(Self {
            id,
            queue: route.queue.clone(),
            task_type: route.task_type.clone(),
            capability: route.capability.clone(),
            target_ref,
            input,
            source,
            idempotency_key: None,
            exclusive_non_terminal: false,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeScope {
    pub kind: String,
    pub project_id: Option<String>,
    pub panel_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    pub revision: i64,
}

impl Storage {
    pub fn open(paths: &MyOpenPanelsPaths) -> Result<Self, CliError> {
        fs::create_dir_all(&paths.storage_dir).map_err(to_cli_error)?;
        let database_path = paths.storage_dir.join(DATABASE_FILE_NAME);
        archive_legacy_storage_if_needed(paths, &database_path)?;
        let mut connection = Connection::open(&database_path).map_err(to_cli_error)?;
        connection
            .execute_batch(
                r#"
                PRAGMA busy_timeout = 5000;
                PRAGMA journal_mode = WAL;
                PRAGMA foreign_keys = ON;
                "#,
        )
        .map_err(to_cli_error)?;
        initialize_storage_schema(&mut connection)?;
        Ok(Self {
            connection,
            root_dir: paths.storage_dir.clone(),
            project_root_dir: paths.project_dir.clone(),
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
                INSERT INTO projects (id, title, root_path, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                  title = excluded.title,
                  root_path = excluded.root_path,
                  created_at = excluded.created_at,
                  updated_at = excluded.updated_at
                "#,
            params![
                project.id,
                project.title,
                self.project_root_dir.to_string_lossy(),
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
                "SELECT project_id, id, kind, created_at, updated_at FROM panels WHERE project_id = ? AND id = ?",
                params![project_id, panel_id],
                panel_from_row,
            )
            .optional()
            .map_err(to_cli_error)
    }

    pub fn write_panel(&self, panel: &Panel) -> Result<(), CliError> {
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        tx.execute(
            r#"
                INSERT INTO panels (project_id, id, kind, position, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?)
                ON CONFLICT(project_id, id) DO UPDATE SET
                  kind = excluded.kind,
                  position = excluded.position,
                  created_at = excluded.created_at,
                  updated_at = excluded.updated_at
                "#,
            params![
                panel.project_id,
                panel.id,
                panel.kind.as_str(),
                panel_position(panel.kind),
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
        read_composed_panel_state(&self.connection, project_id, panel_id)
    }

    pub(crate) fn read_panel_state_snapshot(
        &self,
        project_id: &str,
        panel_id: &str,
    ) -> Result<Option<(Value, i64)>, CliError> {
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Deferred)
            .map_err(to_cli_error)?;
        let state = read_composed_panel_state(&tx, project_id, panel_id)?;
        let revision = state
            .as_ref()
            .map(|_| panel_state_revision(&tx, project_id, panel_id))
            .transpose()?;
        tx.commit().map_err(to_cli_error)?;
        Ok(state.zip(revision))
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
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        let current_revision = panel_state_revision(&tx, project_id, panel_id)?;
        if base_revision.is_some_and(|base| base != current_revision) {
            return Ok(Err(PanelStateWriteConflict {
                base_revision: base_revision.unwrap_or(0),
                current_revision,
            }));
        }
        if read_composed_panel_state(&tx, project_id, panel_id)?.as_ref() == Some(state) {
            tx.commit().map_err(to_cli_error)?;
            return Ok(Ok(current_revision));
        }
        let revision = write_decomposed_panel_state(&tx, project_id, panel_id, state)?;
        tx.commit().map_err(to_cli_error)?;
        Ok(Ok(revision))
    }

    pub(crate) fn write_panel_state_in_transaction(
        tx: &Transaction<'_>,
        project_id: &str,
        panel_id: &str,
        state: &Value,
    ) -> Result<i64, CliError> {
        if read_composed_panel_state(tx, project_id, panel_id)?.as_ref() == Some(state) {
            return panel_state_revision(tx, project_id, panel_id);
        }
        write_decomposed_panel_state(tx, project_id, panel_id, state)
    }

    pub(crate) fn write_panel_state_if_revision_in_transaction(
        tx: &Transaction<'_>,
        project_id: &str,
        panel_id: &str,
        base_revision: i64,
        state: &Value,
    ) -> Result<i64, CliError> {
        let current_revision = panel_state_revision(tx, project_id, panel_id)?;
        if current_revision != base_revision {
            return Err(CliError::with_code(
                "content_conflict",
                format!(
                    "Panel state changed from revision {base_revision} to {current_revision} before Task finalization."
                ),
            ));
        }
        Self::write_panel_state_in_transaction(tx, project_id, panel_id, state)
    }

}

fn panel_position(kind: PanelKind) -> i64 {
    match kind {
        PanelKind::Wiki => 0,
        PanelKind::Writing => 1,
        PanelKind::Canvas => 2,
        PanelKind::Typesetting => 3,
        PanelKind::Publishing => 4,
    }
}
