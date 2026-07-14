use crate::error::CliError;
use crate::paths::{sanitize_path_part, MyOpenPanelsPaths};
use crate::tasks::model::{TaskLease, TaskRecord, TaskStatus};
use crate::types::{Panel, PanelKind, Project};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
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
    pub queue: String,
    pub task_type: String,
    pub capability: String,
    pub target_ref: String,
    pub input: Value,
    pub source: Value,
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
        let mut connection =
            Connection::open(paths.storage_dir.join(DATABASE_FILE_NAME)).map_err(to_cli_error)?;
        connection
            .execute_batch(
                r#"
                PRAGMA busy_timeout = 5000;
                PRAGMA journal_mode = WAL;
                PRAGMA foreign_keys = ON;
                "#,
            )
            .map_err(to_cli_error)?;
        let upgrading_from_v1 = table_exists(&connection, "sessions")?;
        if upgrading_from_v1 {
            backup_unsupported_panels(paths, &connection)?;
        }
        migrate(&mut connection)?;
        backfill_content_hashes(&connection)?;
        migrate_storage_layout(paths, upgrading_from_v1)?;
        if upgrading_from_v1 {
            connection
                .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM; PRAGMA optimize;")
                .map_err(to_cli_error)?;
        }
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

    pub fn upsert_tasks(
        &self,
        project_id: &str,
        panel_id: &str,
        queue: &str,
        tasks: &[Value],
    ) -> Result<(), CliError> {
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        let mut inserted = 0usize;
        for task in tasks {
            let id = task
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| CliError::new("Project task id is required."))?;
            let created_at = task
                .get("createdAt")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(crate::control::now_iso);
            let updated_at = task
                .get("updatedAt")
                .and_then(Value::as_str)
                .unwrap_or(&created_at);
            let task_type = task
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let input = json!({
                "documentId": task.get("documentId"),
                "markdownVersion": task.get("markdownVersion"),
            });
            let source = json!({
                "wikiSpaceId": task.get("wikiSpaceId"),
                "ruleSetId": task.get("ruleSetId"),
                "ruleSetVersion": task.get("ruleSetVersion"),
                "agentSkillId": task.get("agentSkillId"),
            });
            inserted += tx
                .execute(
                    r#"
                    INSERT INTO tasks (
                      id, project_id, panel_id, queue, type, capability, status, target_ref,
                      input_json, source_json, attempts, max_attempts, created_at, updated_at
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    ON CONFLICT(id) DO NOTHING
                    "#,
                    params![
                        id,
                        project_id,
                        panel_id,
                        queue,
                        task_type,
                        project_task_capability(queue, task_type),
                        task.get("status")
                            .and_then(Value::as_str)
                            .unwrap_or("queued"),
                        task.get("targetId").and_then(Value::as_str).unwrap_or(""),
                        serde_json::to_string(&input).map_err(to_cli_error)?,
                        serde_json::to_string(&source).map_err(to_cli_error)?,
                        task.get("attempt").and_then(Value::as_i64).unwrap_or(0),
                        task.get("maxAttempts").and_then(Value::as_i64).unwrap_or(3),
                        created_at,
                        updated_at,
                    ],
                )
                .map_err(to_cli_error)?;
        }
        if inserted > 0 {
            record_scope(&tx, "tasks", Some(project_id), None)?;
        }
        tx.commit().map_err(to_cli_error)
    }

    pub fn insert_task(
        &self,
        project_id: &str,
        panel_id: &str,
        queue: &str,
        task_type: &str,
        capability: &str,
        target_ref: &str,
        input: &Value,
        source: &Value,
    ) -> Result<Value, CliError> {
        let id = format!("task:{:032x}", rand::random::<u128>());
        let now = crate::control::now_iso();
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        tx.execute(
            r#"
            INSERT INTO tasks (
              id, project_id, panel_id, queue, type, capability, status, target_ref,
              input_json, source_json, attempts, max_attempts, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, 'queued', ?, ?, ?, 0, 3, ?, ?)
            "#,
            params![
                id,
                project_id,
                panel_id,
                queue,
                task_type,
                capability,
                target_ref,
                serde_json::to_string(input).map_err(to_cli_error)?,
                serde_json::to_string(source).map_err(to_cli_error)?,
                now,
                now,
            ],
        )
        .map_err(to_cli_error)?;
        record_scope(&tx, "tasks", Some(project_id), None)?;
        tx.commit().map_err(to_cli_error)?;
        self.list_tasks(project_id)?
            .into_iter()
            .find(|task| task.get("id").and_then(Value::as_str) == Some(id.as_str()))
            .ok_or_else(|| CliError::new(format!("Created task was not found: {id}")))
    }

    pub fn insert_tasks(
        &self,
        project_id: &str,
        panel_id: &str,
        tasks: &[TaskInsert],
    ) -> Result<Vec<Value>, CliError> {
        if tasks.is_empty() {
            return Ok(Vec::new());
        }
        let now = crate::control::now_iso();
        let task_ids = tasks
            .iter()
            .map(|_| format!("task:{:032x}", rand::random::<u128>()))
            .collect::<Vec<_>>();
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        for (task, id) in tasks.iter().zip(&task_ids) {
            tx.execute(
                r#"
                INSERT INTO tasks (
                  id, project_id, panel_id, queue, type, capability, status, target_ref,
                  input_json, source_json, attempts, max_attempts, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, 'queued', ?, ?, ?, 0, 3, ?, ?)
                "#,
                params![
                    id,
                    project_id,
                    panel_id,
                    &task.queue,
                    &task.task_type,
                    &task.capability,
                    &task.target_ref,
                    serde_json::to_string(&task.input).map_err(to_cli_error)?,
                    serde_json::to_string(&task.source).map_err(to_cli_error)?,
                    now,
                    now,
                ],
            )
            .map_err(to_cli_error)?;
        }
        record_scope(&tx, "tasks", Some(project_id), None)?;
        tx.commit().map_err(to_cli_error)?;

        let mut created = self
            .list_tasks(project_id)?
            .into_iter()
            .filter_map(|task| {
                let id = task.get("id").and_then(Value::as_str)?.to_owned();
                Some((id, task))
            })
            .collect::<HashMap<_, _>>();
        task_ids
            .into_iter()
            .map(|id| {
                created
                    .remove(&id)
                    .ok_or_else(|| CliError::new(format!("Created task was not found: {id}")))
            })
            .collect()
    }

    pub fn list_tasks(&self, project_id: &str) -> Result<Vec<Value>, CliError> {
        let mut statement = self
            .connection
            .prepare(
                r#"
                SELECT
                  t.id, t.queue, t.project_id, t.panel_id, p.kind, t.type, t.status,
                  t.target_ref, t.created_at, t.updated_at, t.attempts, t.max_attempts,
                  lease_owner, lease_expires_at, last_heartbeat_at, retry_after,
                  t.capability, t.assigned_agent_id, t.result_json, t.error_json, t.completed_at,
                  t.input_json, t.source_json
                FROM tasks t
                JOIN panels p ON p.project_id = t.project_id AND p.id = t.panel_id
                WHERE t.project_id = ?
                ORDER BY t.updated_at DESC, t.id ASC
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![project_id], |row| {
                let input_json: String = row.get(21)?;
                let source_json: String = row.get(22)?;
                let input =
                    serde_json::from_str::<Value>(&input_json).unwrap_or_else(|_| json!({}));
                let source =
                    serde_json::from_str::<Value>(&source_json).unwrap_or_else(|_| json!({}));
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
                let capability = row.get::<_, Option<String>>(16)?;
                let assigned_agent_id = row.get::<_, Option<String>>(17)?;
                let result_json = row.get::<_, Option<String>>(18)?;
                let error_json = row.get::<_, Option<String>>(19)?;
                let completed_at = row.get::<_, Option<String>>(20)?;
                let result = result_json
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                    .unwrap_or(Value::Null);
                let error = error_json
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                    .unwrap_or(Value::Null);
                Ok(TaskRecord {
                    id: row.get(0)?,
                    queue: queue.clone(),
                    project_id: row.get(2)?,
                    panel_id: row.get(3)?,
                    panel_kind,
                    task_type: task_type.clone(),
                    status: TaskStatus(status),
                    target_id,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                    attempt: attempts,
                    max_attempts,
                    lease: TaskLease {
                        owner: assigned_agent_id.clone().or(lease_owner),
                        expires_at: lease_expires_at,
                        heartbeat_at: last_heartbeat_at,
                    },
                    retry_after,
                    capability: capability
                        .unwrap_or_else(|| project_task_capability(&queue, &task_type)),
                    assigned_target_id: assigned_agent_id,
                    completed_at,
                    input,
                    source,
                    result,
                    error,
                })
            })
            .map_err(to_cli_error)?;
        rows.map(|row| {
            row.map_err(to_cli_error)
                .and_then(|task| serde_json::to_value(task).map_err(to_cli_error))
        })
        .collect()
    }

    pub fn task_panel_target(&self, task_id: &str) -> Result<Option<(String, String)>, CliError> {
        self.connection
            .query_row(
                "SELECT project_id, panel_id FROM tasks WHERE id = ? LIMIT 1",
                [task_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(to_cli_error)
    }

    pub fn read_setting(&self, namespace: &str, key: &str) -> Result<Option<String>, CliError> {
        self.connection
            .query_row(
                "SELECT value_json FROM settings WHERE namespace = ? AND key = ?",
                params![namespace, key],
                |row| row.get(0),
            )
            .optional()
            .map_err(to_cli_error)
    }

    pub fn write_setting(
        &self,
        namespace: &str,
        key: &str,
        value_json: &str,
    ) -> Result<(), CliError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(to_cli_error)?;
        tx.execute(
            r#"
            INSERT INTO settings (namespace, key, value_json, updated_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(namespace, key) DO UPDATE SET
              value_json = excluded.value_json,
              updated_at = excluded.updated_at
            "#,
            params![namespace, key, value_json, crate::control::now_iso()],
        )
        .map_err(to_cli_error)?;
        record_scope(&tx, "catalog", None, None)?;
        tx.commit().map_err(to_cli_error)
    }

    pub fn write_artifact(&self, project_id: &str, artifact: &Value) -> Result<(), CliError> {
        let id = artifact
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| CliError::new("Artifact id is required."))?;
        let now = artifact
            .get("updatedAt")
            .and_then(Value::as_str)
            .or_else(|| artifact.get("createdAt").and_then(Value::as_str))
            .map(str::to_owned)
            .unwrap_or_else(crate::control::now_iso);
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        tx.execute(
            r#"
                INSERT INTO artifacts (
                  id, project_id, panel_id, kind, title, payload_json, created_at, updated_at
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                  panel_id = excluded.panel_id,
                  kind = excluded.kind,
                  title = excluded.title,
                  payload_json = excluded.payload_json,
                  updated_at = excluded.updated_at
                "#,
            params![
                id,
                project_id,
                artifact.get("panelId").and_then(Value::as_str),
                artifact
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("file"),
                artifact.get("title").and_then(Value::as_str),
                serde_json::to_string(artifact).map_err(to_cli_error)?,
                artifact
                    .get("createdAt")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(crate::control::now_iso),
                now,
            ],
        )
        .map_err(to_cli_error)?;
        record_scope(&tx, "artifacts", Some(project_id), None)?;
        tx.commit().map_err(to_cli_error)
    }

    pub fn write_panel_selection(
        &self,
        project_id: &str,
        panel_id: &str,
        selection: &Value,
    ) -> Result<(), CliError> {
        let selection_json = serde_json::to_string(selection).map_err(to_cli_error)?;
        let content_hash = hash_text(&selection_json);
        let tx = Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
            .map_err(to_cli_error)?;
        let current = tx.query_row(
            "SELECT revision, content_hash FROM panel_selections WHERE project_id = ? AND panel_id = ?",
            params![project_id, panel_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        ).optional().map_err(to_cli_error)?;
        if current
            .as_ref()
            .is_some_and(|value| value.1 == content_hash)
        {
            tx.commit().map_err(to_cli_error)?;
            return Ok(());
        }
        let revision = record_scope(&tx, "panel_selection", Some(project_id), Some(panel_id))?;
        tx.execute(
            r#"
                INSERT INTO panel_selections (
                  project_id, panel_id, revision, content_hash, selection_json, updated_at
                )
                VALUES (?, ?, ?, ?, ?, ?)
                ON CONFLICT(project_id, panel_id) DO UPDATE SET
                  revision = excluded.revision,
                  content_hash = excluded.content_hash,
                  selection_json = excluded.selection_json,
                  updated_at = excluded.updated_at
                "#,
            params![
                project_id,
                panel_id,
                revision,
                content_hash,
                selection_json,
                selection
                    .get("updatedAt")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(crate::control::now_iso),
            ],
        )
        .map_err(to_cli_error)?;
        tx.commit().map_err(to_cli_error)
    }

    pub fn read_panel_selection(
        &self,
        project_id: &str,
        panel_id: &str,
    ) -> Result<Option<Value>, CliError> {
        let selection_json = self
            .connection
            .query_row(
                "SELECT selection_json FROM panel_selections WHERE project_id = ? AND panel_id = ?",
                params![project_id, panel_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(to_cli_error)?;
        selection_json
            .map(|raw| serde_json::from_str::<Value>(&raw).map_err(to_cli_error))
            .transpose()
    }

    pub fn write_asset_from_buffer(
        &self,
        project_id: &str,
        panel_id: &str,
        requested_name: &str,
        bytes: &[u8],
        overwrite: bool,
    ) -> Result<WrittenAsset, CliError> {
        let assets_dir = self.panel_dir(project_id, panel_id).join("assets");
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
            "projects/{}/panels/{}/assets/{}",
            project_id,
            panel_id,
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

    pub fn panel_dir(&self, project_id: &str, panel_id: &str) -> PathBuf {
        self.project_dir(project_id)
            .join("panels")
            .join(sanitize_path_part(panel_id))
    }

    pub fn project_dir(&self, project_id: &str) -> PathBuf {
        self.root_dir
            .join("projects")
            .join(sanitize_path_part(project_id))
    }

    pub fn read_change_seq(&self) -> Result<i64, CliError> {
        self.connection
            .query_row(
                "SELECT global_revision FROM storage_meta WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .map_err(to_cli_error)
    }

    pub fn read_changes_after(
        &self,
        revision: i64,
        project_id: Option<&str>,
    ) -> Result<(i64, Vec<ChangeScope>), CliError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(to_cli_error)?;
        let global_revision = tx
            .query_row(
                "SELECT global_revision FROM storage_meta WHERE id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(to_cli_error)?;
        let mut statement = tx
            .prepare(
                r#"
            SELECT kind, project_id, panel_id, revision
            FROM change_scopes
            WHERE revision > ? AND revision <= ?
              AND (project_id IS NULL OR project_id = ?)
            ORDER BY revision ASC, scope_key ASC
            "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![revision, global_revision, project_id], |row| {
                Ok(ChangeScope {
                    kind: row.get(0)?,
                    project_id: row.get(1)?,
                    panel_id: row.get(2)?,
                    revision: row.get(3)?,
                })
            })
            .map_err(to_cli_error)?;
        let changes = rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?;
        drop(statement);
        tx.commit().map_err(to_cli_error)?;
        Ok((global_revision, changes))
    }

    pub fn read_panel_state_revision(
        &self,
        project_id: &str,
        panel_id: &str,
    ) -> Result<i64, CliError> {
        self.connection
            .query_row(
                "SELECT COALESCE((SELECT revision FROM panel_states WHERE project_id = ? AND panel_id = ?), 0)",
                params![project_id, panel_id],
                |row| row.get(0),
            )
            .map_err(to_cli_error)
    }

    pub fn read_panel_selection_revision(
        &self,
        project_id: &str,
        panel_id: &str,
    ) -> Result<i64, CliError> {
        self.connection
            .query_row(
                "SELECT COALESCE((SELECT revision FROM panel_selections WHERE project_id = ? AND panel_id = ?), 0)",
                params![project_id, panel_id],
                |row| row.get(0),
            )
            .map_err(to_cli_error)
    }

    fn panel_ids(&self, project_id: &str) -> Result<Vec<String>, CliError> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM panels WHERE project_id = ? ORDER BY CASE kind WHEN 'wiki' THEN 0 WHEN 'writing' THEN 1 WHEN 'canvas' THEN 2 WHEN 'typesetting' THEN 3 WHEN 'publishing' THEN 4 ELSE 5 END, id ASC")
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map(params![project_id], |row| row.get::<_, String>(0))
            .map_err(to_cli_error)?;
        rows.map(|row| row.map_err(to_cli_error)).collect()
    }
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, CliError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?)",
            params![table],
            |row| row.get(0),
        )
        .map_err(to_cli_error)
}

fn backup_unsupported_panels(
    paths: &MyOpenPanelsPaths,
    connection: &Connection,
) -> Result<(), CliError> {
    let panels = {
        let mut statement = connection
            .prepare(
                r#"
                SELECT p.session_id, p.id, p.kind, p.title, p.created_at, p.updated_at,
                       ps.state_json
                FROM panels p
                LEFT JOIN panel_states ps
                  ON ps.session_id = p.session_id AND ps.panel_id = p.id
                WHERE p.kind NOT IN ('wiki', 'writing', 'canvas', 'typesetting', 'publishing')
                ORDER BY p.session_id, p.id
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            })
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    if panels.is_empty() {
        return Ok(());
    }

    let marker = paths.storage_dir.join("unsupported-panel-backup.json");
    let backup_root = if marker.exists() {
        let value: Value = serde_json::from_slice(&fs::read(&marker).map_err(to_cli_error)?)
            .map_err(to_cli_error)?;
        value
            .get("backupDir")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .ok_or_else(|| CliError::new("Unsupported Panel backup marker is invalid."))?
    } else {
        let timestamp = crate::control::now_iso()
            .replace([':', '.'], "-")
            .trim_end_matches('Z')
            .to_owned();
        let backup_root = paths
            .storage_dir
            .join("backups")
            .join(format!("unsupported-panels-{timestamp}"));
        fs::create_dir_all(&backup_root).map_err(to_cli_error)?;
        fs::write(
            &marker,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&json!({
                    "backupDir": backup_root,
                    "createdAt": crate::control::now_iso(),
                }))
                .map_err(to_cli_error)?
            ),
        )
        .map_err(to_cli_error)?;
        backup_root
    };

    for (project_id, panel_id, kind, title, created_at, updated_at, state_json) in panels {
        let panel_backup = backup_root
            .join(sanitize_path_part(&project_id))
            .join(sanitize_path_part(&panel_id));
        fs::create_dir_all(&panel_backup).map_err(to_cli_error)?;
        let state = state_json
            .as_deref()
            .map(serde_json::from_str::<Value>)
            .transpose()
            .map_err(to_cli_error)?;
        fs::write(
            panel_backup.join("panel.json"),
            format!(
                "{}\n",
                serde_json::to_string_pretty(&json!({
                    "id": panel_id,
                    "projectId": project_id,
                    "kind": kind,
                    "title": title,
                    "createdAt": created_at,
                    "updatedAt": updated_at,
                    "state": state,
                }))
                .map_err(to_cli_error)?
            ),
        )
        .map_err(to_cli_error)?;
        let source_dir = paths
            .storage_dir
            .join("sessions")
            .join(sanitize_path_part(&project_id))
            .join("panels")
            .join(sanitize_path_part(&panel_id));
        let target_dir = panel_backup.join("files");
        if source_dir.exists() && !target_dir.exists() {
            fs::rename(source_dir, target_dir).map_err(to_cli_error)?;
        }
    }
    Ok(())
}

fn backfill_content_hashes(connection: &Connection) -> Result<(), CliError> {
    for (table, json_column) in [
        ("panel_states", "state_json"),
        ("panel_selections", "selection_json"),
    ] {
        let sql = format!(
            "SELECT project_id, panel_id, {json_column} FROM {table} WHERE content_hash = ''"
        );
        let rows = {
            let mut statement = connection.prepare(&sql).map_err(to_cli_error)?;
            let mapped_rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(to_cli_error)?;
            mapped_rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(to_cli_error)?
        };
        let update = format!(
            "UPDATE {table} SET content_hash = ? WHERE project_id = ? AND panel_id = ? AND content_hash = ''"
        );
        for (project_id, panel_id, json) in rows {
            connection
                .execute(&update, params![hash_text(&json), project_id, panel_id])
                .map_err(to_cli_error)?;
        }
    }
    Ok(())
}

fn migrate_storage_layout(paths: &MyOpenPanelsPaths, force: bool) -> Result<(), CliError> {
    let legacy_root = paths.storage_dir.join("sessions");
    let projects_root = paths.storage_dir.join("projects");
    let marker = paths.storage_dir.join("storage-v2-migration.json");
    if !force && !legacy_root.exists() && !marker.exists() {
        return Ok(());
    }
    fs::write(
        &marker,
        format!(
            "{{\"version\":2,\"phase\":\"layout\",\"updatedAt\":\"{}\"}}\n",
            crate::control::now_iso()
        ),
    )
    .map_err(to_cli_error)?;
    if legacy_root.exists() {
        if projects_root.exists() {
            merge_directory(&legacy_root, &projects_root)?;
            fs::remove_dir_all(&legacy_root).map_err(to_cli_error)?;
        } else {
            fs::rename(&legacy_root, &projects_root).map_err(to_cli_error)?;
        }
    }
    let contexts = paths.storage_dir.join("contexts");
    if contexts.exists() {
        rewrite_context_json(&contexts)?;
    }
    fs::remove_file(marker).map_err(to_cli_error)?;
    Ok(())
}

fn merge_directory(source: &std::path::Path, target: &std::path::Path) -> Result<(), CliError> {
    fs::create_dir_all(target).map_err(to_cli_error)?;
    for entry in fs::read_dir(source).map_err(to_cli_error)? {
        let entry = entry.map_err(to_cli_error)?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let file_type = entry.file_type().map_err(to_cli_error)?;
        if file_type.is_symlink() {
            return Err(CliError::new(format!(
                "Storage layout migration refuses symlink: {}",
                source_path.display()
            )));
        }
        if file_type.is_dir() {
            merge_directory(&source_path, &target_path)?;
            fs::remove_dir_all(source_path).map_err(to_cli_error)?;
        } else if target_path.exists() {
            let source_bytes = fs::read(&source_path).map_err(to_cli_error)?;
            let target_bytes = fs::read(&target_path).map_err(to_cli_error)?;
            if source_bytes != target_bytes {
                return Err(CliError::new(format!(
                    "Storage layout migration found conflicting files: {}",
                    target_path.display()
                )));
            }
            fs::remove_file(source_path).map_err(to_cli_error)?;
        } else {
            fs::rename(source_path, target_path).map_err(to_cli_error)?;
        }
    }
    Ok(())
}

fn rewrite_context_json(root: &std::path::Path) -> Result<(), CliError> {
    for entry in fs::read_dir(root).map_err(to_cli_error)? {
        let entry = entry.map_err(to_cli_error)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(to_cli_error)?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            rewrite_context_json(&path)?;
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&path).map_err(to_cli_error)?;
        let rewritten = raw
            .replace("\"sessionId\"", "\"projectId\"")
            .replace("sessions/", "projects/");
        let target =
            if path.file_name().and_then(|value| value.to_str()) == Some("active-session.json") {
                path.with_file_name("active-project.json")
            } else {
                path.clone()
            };
        if rewritten != raw || target != path {
            fs::write(&target, rewritten).map_err(to_cli_error)?;
            if target != path {
                fs::remove_file(path).map_err(to_cli_error)?;
            }
        }
    }
    Ok(())
}

fn project_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        title: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        panel_ids: Vec::new(),
    })
}

fn panel_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Panel> {
    let kind = row.get::<_, String>(2)?;
    let project_id = row.get::<_, String>(0)?;
    let panel_id = row.get::<_, String>(1)?;
    Ok(Panel {
        id: panel_id.clone(),
        project_id: project_id.clone(),
        kind: PanelKind::parse(&kind).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                format!("unknown panel kind: {kind}").into(),
            )
        })?,
        title: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        state_ref: Some(format!("sqlite:panel-states/{project_id}/{panel_id}")),
    })
}

fn operation_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT o.id, o.owner_context_id, o.intent, o.status, o.project_id,
               o.panel_id, p.kind, o.guide_id, o.protocol_version,
               o.target_json, o.input_json, o.result_json, o.error_json,
               o.created_at, o.updated_at, o.completed_at
        FROM agent_operations o
        JOIN panels p ON p.project_id = o.project_id AND p.id = o.panel_id
        {where_clause}
        "#
    )
}

fn operation_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let parse = |index| -> rusqlite::Result<Value> {
        let raw = row.get::<_, String>(index)?;
        serde_json::from_str(&raw).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
    };
    let parse_optional = |index| -> rusqlite::Result<Value> {
        row.get::<_, Option<String>>(index)?
            .map(|raw| {
                serde_json::from_str(&raw).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        index,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
            })
            .transpose()
            .map(|value| value.unwrap_or(Value::Null))
    };
    Ok(json!({
        "id": row.get::<_, String>(0)?,
        "ownerContextId": row.get::<_, String>(1)?,
        "intent": row.get::<_, String>(2)?,
        "status": row.get::<_, String>(3)?,
        "projectId": row.get::<_, String>(4)?,
        "panelId": row.get::<_, String>(5)?,
        "panelKind": row.get::<_, String>(6)?,
        "guideId": row.get::<_, Option<String>>(7)?,
        "protocolVersion": row.get::<_, i64>(8)?,
        "target": parse(9)?,
        "input": parse(10)?,
        "result": parse_optional(11)?,
        "error": parse_optional(12)?,
        "createdAt": row.get::<_, String>(13)?,
        "updatedAt": row.get::<_, String>(14)?,
        "completedAt": row.get::<_, Option<String>>(15)?,
    }))
}

fn hash_text(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

pub(crate) fn record_scope(
    connection: &Connection,
    kind: &str,
    project_id: Option<&str>,
    panel_id: Option<&str>,
) -> Result<i64, CliError> {
    let revision = connection
        .query_row(
            "UPDATE storage_meta SET global_revision = global_revision + 1 WHERE id = 1 RETURNING global_revision",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(to_cli_error)?;
    upsert_scope(connection, revision, kind, project_id, panel_id)?;
    Ok(revision)
}

fn upsert_scope(
    connection: &Connection,
    revision: i64,
    kind: &str,
    project_id: Option<&str>,
    panel_id: Option<&str>,
) -> Result<(), CliError> {
    let scope_key = match (kind, project_id, panel_id) {
        ("catalog", _, _) => "catalog".to_owned(),
        (_, Some(project_id), Some(panel_id)) => format!("{kind}:{project_id}:{panel_id}"),
        (_, Some(project_id), None) => format!("{kind}:{project_id}"),
        _ => kind.to_owned(),
    };
    connection
        .execute(
            r#"
            INSERT INTO change_scopes (scope_key, kind, project_id, panel_id, revision, updated_at)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(scope_key) DO UPDATE SET
              kind = excluded.kind,
              project_id = excluded.project_id,
              panel_id = excluded.panel_id,
              revision = excluded.revision,
              updated_at = excluded.updated_at
            "#,
            params![
                scope_key,
                kind,
                project_id,
                panel_id,
                revision,
                crate::control::now_iso()
            ],
        )
        .map_err(to_cli_error)?;
    Ok(())
}

pub struct WrittenAsset {
    pub asset_ref: String,
    pub file_name: String,
    pub file_path: PathBuf,
}

fn project_task_capability(queue: &str, task_type: &str) -> String {
    match (queue, task_type) {
        ("wiki", "convert_document_to_markdown") => "wiki.convertDocument".to_owned(),
        ("wiki", "ingest_markdown_into_wiki") => "wiki.ingestMarkdown".to_owned(),
        ("wiki", "rebuild_wiki_index") => "wiki.rebuildIndex".to_owned(),
        ("writing", "refine_writing_skill") => "writing.refineSkill".to_owned(),
        _ => format!("{}.{}", queue, task_type.replace('_', ".")),
    }
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
        PanelKind::Writing => state.get("schemaVersion").and_then(Value::as_i64),
        PanelKind::Typesetting => state.get("schemaVersion").and_then(Value::as_i64),
        PanelKind::Publishing => state.get("schemaVersion").and_then(Value::as_i64),
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

const MIGRATION_0002_SQL: &str = r#"
ALTER TABLE project_tasks ADD COLUMN capability TEXT;
ALTER TABLE project_tasks ADD COLUMN assigned_target_id TEXT;
ALTER TABLE project_tasks ADD COLUMN lease_token_hash TEXT;
ALTER TABLE project_tasks ADD COLUMN result_json TEXT;
ALTER TABLE project_tasks ADD COLUMN error_json TEXT;
ALTER TABLE project_tasks ADD COLUMN completed_at TEXT;

UPDATE project_tasks
SET capability = CASE
  WHEN queue = 'wiki' AND type = 'convert_document_to_markdown' THEN 'wiki.convertDocument'
  WHEN queue = 'wiki' AND type = 'ingest_markdown_into_wiki' THEN 'wiki.ingestMarkdown'
  WHEN queue = 'wiki' AND type = 'rebuild_wiki_index' THEN 'wiki.rebuildIndex'
  ELSE queue || '.' || replace(type, '_', '.')
END
WHERE capability IS NULL OR capability = '';

CREATE INDEX project_tasks_session_capability_idx
  ON project_tasks(session_id, capability, status, updated_at ASC);
CREATE INDEX project_tasks_lease_idx
  ON project_tasks(session_id, lease_expires_at, status);

CREATE TABLE agent_targets (
  id TEXT PRIMARY KEY NOT NULL,
  session_id TEXT NOT NULL,
  name TEXT NOT NULL,
  host TEXT NOT NULL,
  transport TEXT NOT NULL,
  endpoint TEXT,
  capabilities_json TEXT NOT NULL,
  priority INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'online',
  token_hash TEXT NOT NULL,
  last_error TEXT,
  last_heartbeat_at TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX agent_targets_session_status_idx
  ON agent_targets(session_id, status, priority DESC, last_heartbeat_at DESC);

CREATE TABLE task_deliveries (
  id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL,
  target_id TEXT NOT NULL,
  status TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  next_attempt_at TEXT,
  last_error TEXT,
  delivered_at TEXT,
  acknowledged_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(task_id, target_id),
  FOREIGN KEY (task_id) REFERENCES project_tasks(id) ON DELETE CASCADE,
  FOREIGN KEY (target_id) REFERENCES agent_targets(id) ON DELETE CASCADE
);

CREATE INDEX task_deliveries_due_idx
  ON task_deliveries(status, next_attempt_at, updated_at);
CREATE INDEX task_deliveries_task_idx
  ON task_deliveries(task_id, updated_at DESC);

CREATE TABLE task_delivery_attempts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  delivery_id TEXT NOT NULL,
  attempt INTEGER NOT NULL,
  status TEXT NOT NULL,
  error TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (delivery_id) REFERENCES task_deliveries(id) ON DELETE CASCADE
);

CREATE INDEX task_delivery_attempts_delivery_idx
  ON task_delivery_attempts(delivery_id, attempt DESC);
"#;

const MIGRATION_0003_SQL: &str = r#"
CREATE TABLE agent_operations (
  id TEXT PRIMARY KEY NOT NULL,
  owner_context_id TEXT NOT NULL,
  intent TEXT NOT NULL,
  status TEXT NOT NULL,
  session_id TEXT NOT NULL,
  panel_id TEXT NOT NULL,
  panel_kind TEXT NOT NULL,
  guide_id TEXT,
  protocol_version INTEGER NOT NULL,
  target_json TEXT NOT NULL,
  input_json TEXT NOT NULL,
  result_json TEXT,
  error_json TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT,
  operation_json TEXT NOT NULL
);

CREATE INDEX agent_operations_owner_status_idx
  ON agent_operations(owner_context_id, status, updated_at DESC);
CREATE INDEX agent_operations_target_idx
  ON agent_operations(session_id, panel_id, updated_at DESC);
"#;

const MIGRATION_0004_SQL: &str = r#"
CREATE UNIQUE INDEX panels_session_kind_unique_idx
  ON panels(session_id, kind);
"#;

const MIGRATION_0005_SQL: &str = include_str!("../migrations/0005_storage_v2.sql");
const MIGRATION_0006_SQL: &str = include_str!("../migrations/0006_asset_url_repair.sql");
const MIGRATION_0007_SQL: &str = include_str!("../migrations/0007_project_json_repair.sql");
const MIGRATION_0008_SQL: &str = "Allow the writing Panel kind in panels.kind.";
const MIGRATION_0009_SQL: &str = "Allow the typesetting Panel kind in panels.kind.";
const MIGRATION_0010_SQL: &str = "Allow the publishing Panel kind in panels.kind.";

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
    &[
        Migration {
            id: "0001_initial",
            description: "Create initial MyOpenPanels SQLite storage schema",
            checksum_material: MIGRATION_0001_SQL,
            up: migration_0001,
        },
        Migration {
            id: "0002_agent_task_dispatch",
            description: "Add generic agent targets and task delivery state",
            checksum_material: MIGRATION_0002_SQL,
            up: migration_0002,
        },
        Migration {
            id: "0003_agent_operations",
            description: "Add persistent agent operations",
            checksum_material: MIGRATION_0003_SQL,
            up: migration_0003,
        },
        Migration {
            id: "0004_unique_panel_kind",
            description: "Require one panel of each kind per Project",
            checksum_material: MIGRATION_0004_SQL,
            up: migration_0004,
        },
        Migration {
            id: "0005_storage_v2",
            description: "Rebuild storage around Projects and bounded revisions",
            checksum_material: MIGRATION_0005_SQL,
            up: migration_0005,
        },
        Migration {
            id: "0006_asset_url_repair",
            description: "Rewrite legacy Canvas asset URLs to Project routes",
            checksum_material: MIGRATION_0006_SQL,
            up: migration_0006,
        },
        Migration {
            id: "0007_project_json_repair",
            description: "Rewrite legacy Session keys and paths in normalized JSON",
            checksum_material: MIGRATION_0007_SQL,
            up: migration_0007,
        },
        Migration {
            id: "0008_writing_panel",
            description: "Allow the Writing panel kind",
            checksum_material: MIGRATION_0008_SQL,
            up: migration_0008,
        },
        Migration {
            id: "0009_typesetting_panel",
            description: "Allow the Typesetting panel kind",
            checksum_material: MIGRATION_0009_SQL,
            up: migration_0009,
        },
        Migration {
            id: "0010_publishing_panel",
            description: "Allow the Publishing panel kind",
            checksum_material: MIGRATION_0010_SQL,
            up: migration_0010,
        },
    ]
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
        Err(error) => Err(error),
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

fn migration_0002(tx: &Transaction<'_>) -> Result<(), CliError> {
    tx.execute_batch(MIGRATION_0002_SQL).map_err(to_cli_error)
}

fn migration_0003(tx: &Transaction<'_>) -> Result<(), CliError> {
    tx.execute_batch(MIGRATION_0003_SQL).map_err(to_cli_error)
}

fn migration_0004(tx: &Transaction<'_>) -> Result<(), CliError> {
    let duplicate = tx
        .query_row(
            r#"
            SELECT session_id, kind, COUNT(*)
            FROM panels
            GROUP BY session_id, kind
            HAVING COUNT(*) > 1
            ORDER BY session_id, kind
            LIMIT 1
            "#,
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(to_cli_error)?;
    if let Some((session_id, kind, count)) = duplicate {
        return Err(CliError::with_recovery(
            "duplicate_panel_kind",
            format!(
                "Project {session_id} contains {count} panels of kind {kind}; MyOpenPanels 0.4 requires one panel per kind."
            ),
            false,
            "Back up the MyOpenPanels storage and remove or merge the duplicate panel records before retrying the upgrade.",
        ));
    }
    tx.execute_batch(MIGRATION_0004_SQL).map_err(to_cli_error)
}

fn migration_0005(tx: &Transaction<'_>) -> Result<(), CliError> {
    tx.execute_batch(MIGRATION_0005_SQL).map_err(to_cli_error)
}

fn migration_0006(tx: &Transaction<'_>) -> Result<(), CliError> {
    for (table, column, hash_column) in [
        ("panel_states", "state_json", Some("content_hash")),
        ("panel_selections", "selection_json", Some("content_hash")),
        ("artifacts", "payload_json", None),
        ("tasks", "input_json", None),
        ("tasks", "source_json", None),
        ("tasks", "result_json", None),
        ("tasks", "error_json", None),
        ("agent_operations", "target_json", None),
        ("agent_operations", "input_json", None),
        ("agent_operations", "result_json", None),
        ("agent_operations", "error_json", None),
        ("settings", "value_json", None),
    ] {
        rewrite_json_column(tx, table, column, hash_column, rewrite_legacy_asset_urls)?;
    }
    Ok(())
}

fn migration_0007(tx: &Transaction<'_>) -> Result<(), CliError> {
    for (table, column, hash_column) in [
        ("panel_states", "state_json", Some("content_hash")),
        ("panel_selections", "selection_json", Some("content_hash")),
        ("artifacts", "payload_json", None),
        ("tasks", "input_json", None),
        ("tasks", "source_json", None),
        ("tasks", "result_json", None),
        ("tasks", "error_json", None),
        ("agent_operations", "target_json", None),
        ("agent_operations", "input_json", None),
        ("agent_operations", "result_json", None),
        ("agent_operations", "error_json", None),
        ("settings", "value_json", None),
    ] {
        rewrite_json_column(tx, table, column, hash_column, rewrite_legacy_project_json)?;
    }
    Ok(())
}

fn migration_0008(tx: &Transaction<'_>) -> Result<(), CliError> {
    let schema: String = tx
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'panels'",
            [],
            |row| row.get(0),
        )
        .map_err(to_cli_error)?;
    if schema.contains("'writing'") {
        return Ok(());
    }
    let next_schema = schema.replace(
        "kind IN ('wiki', 'canvas')",
        "kind IN ('wiki', 'writing', 'canvas')",
    );
    if next_schema == schema {
        return Err(CliError::new(
            "Could not locate the panels.kind constraint for Writing migration.",
        ));
    }
    tx.execute_batch("PRAGMA writable_schema = ON;")
        .map_err(to_cli_error)?;
    let result = (|| {
        tx.execute(
            "UPDATE sqlite_schema SET sql = ? WHERE type = 'table' AND name = 'panels'",
            [next_schema],
        )
        .map_err(to_cli_error)?;
        let schema_version: i64 = tx
            .query_row("PRAGMA schema_version", [], |row| row.get(0))
            .map_err(to_cli_error)?;
        tx.execute_batch(&format!("PRAGMA schema_version = {};", schema_version + 1))
            .map_err(to_cli_error)
    })();
    let disable_result = tx
        .execute_batch("PRAGMA writable_schema = OFF;")
        .map_err(to_cli_error);
    result?;
    disable_result
}

fn migration_0009(tx: &Transaction<'_>) -> Result<(), CliError> {
    let schema: String = tx
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'panels'",
            [],
            |row| row.get(0),
        )
        .map_err(to_cli_error)?;
    if schema.contains("'typesetting'") {
        return Ok(());
    }
    let next_schema = schema.replace(
        "kind IN ('wiki', 'writing', 'canvas')",
        "kind IN ('wiki', 'writing', 'canvas', 'typesetting')",
    );
    if next_schema == schema {
        return Err(CliError::new(
            "Could not locate the panels.kind constraint for Typesetting migration.",
        ));
    }
    tx.execute_batch("PRAGMA writable_schema = ON;")
        .map_err(to_cli_error)?;
    let result = (|| {
        tx.execute(
            "UPDATE sqlite_schema SET sql = ? WHERE type = 'table' AND name = 'panels'",
            [next_schema],
        )
        .map_err(to_cli_error)?;
        let schema_version: i64 = tx
            .query_row("PRAGMA schema_version", [], |row| row.get(0))
            .map_err(to_cli_error)?;
        tx.execute_batch(&format!("PRAGMA schema_version = {};", schema_version + 1))
            .map_err(to_cli_error)
    })();
    let disable_result = tx
        .execute_batch("PRAGMA writable_schema = OFF;")
        .map_err(to_cli_error);
    result?;
    disable_result
}

fn migration_0010(tx: &Transaction<'_>) -> Result<(), CliError> {
    let schema: String = tx
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'panels'",
            [],
            |row| row.get(0),
        )
        .map_err(to_cli_error)?;
    if schema.contains("'publishing'") {
        return Ok(());
    }
    let next_schema = schema.replace(
        "kind IN ('wiki', 'writing', 'canvas', 'typesetting')",
        "kind IN ('wiki', 'writing', 'canvas', 'typesetting', 'publishing')",
    );
    if next_schema == schema {
        return Err(CliError::new(
            "Could not locate the panels.kind constraint for Publishing migration.",
        ));
    }
    tx.execute_batch("PRAGMA writable_schema = ON;")
        .map_err(to_cli_error)?;
    let result = (|| {
        tx.execute(
            "UPDATE sqlite_schema SET sql = ? WHERE type = 'table' AND name = 'panels'",
            [next_schema],
        )
        .map_err(to_cli_error)?;
        let schema_version: i64 = tx
            .query_row("PRAGMA schema_version", [], |row| row.get(0))
            .map_err(to_cli_error)?;
        tx.execute_batch(&format!("PRAGMA schema_version = {};", schema_version + 1))
            .map_err(to_cli_error)
    })();
    let disable_result = tx
        .execute_batch("PRAGMA writable_schema = OFF;")
        .map_err(to_cli_error);
    result?;
    disable_result
}

fn rewrite_json_column(
    connection: &Connection,
    table: &str,
    column: &str,
    hash_column: Option<&str>,
    rewriter: fn(&mut Value) -> bool,
) -> Result<(), CliError> {
    let rows = {
        let mut statement = connection
            .prepare(&format!(
                "SELECT rowid, {column} FROM {table} WHERE {column} IS NOT NULL"
            ))
            .map_err(to_cli_error)?;
        let mapped = statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(to_cli_error)?;
        mapped
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_cli_error)?
    };
    for (rowid, raw) in rows {
        let mut value = serde_json::from_str::<Value>(&raw).map_err(to_cli_error)?;
        if !rewriter(&mut value) {
            continue;
        }
        let rewritten = serde_json::to_string(&value).map_err(to_cli_error)?;
        match hash_column {
            Some(hash_column) => connection
                .execute(
                    &format!("UPDATE {table} SET {column} = ?, {hash_column} = ? WHERE rowid = ?"),
                    params![rewritten, hash_text(&rewritten), rowid],
                )
                .map_err(to_cli_error)?,
            None => connection
                .execute(
                    &format!("UPDATE {table} SET {column} = ? WHERE rowid = ?"),
                    params![rewritten, rowid],
                )
                .map_err(to_cli_error)?,
        };
    }
    Ok(())
}

fn rewrite_legacy_project_json(value: &mut Value) -> bool {
    match value {
        Value::String(text) => {
            if !text.contains("sessions/") {
                return false;
            }
            *text = text.replace("sessions/", "projects/");
            true
        }
        Value::Array(values) => {
            let mut changed = false;
            for value in values {
                changed |= rewrite_legacy_project_json(value);
            }
            changed
        }
        Value::Object(values) => {
            let mut changed = false;
            if let Some(project_id) = values.remove("sessionId") {
                values.entry("projectId".to_owned()).or_insert(project_id);
                changed = true;
            }
            for value in values.values_mut() {
                changed |= rewrite_legacy_project_json(value);
            }
            changed
        }
        _ => false,
    }
}

fn rewrite_legacy_asset_urls(value: &mut Value) -> bool {
    match value {
        Value::String(text) => rewrite_legacy_asset_url(text),
        Value::Array(values) => {
            let mut changed = false;
            for value in values {
                changed |= rewrite_legacy_asset_urls(value);
            }
            changed
        }
        Value::Object(values) => {
            let mut changed = false;
            for value in values.values_mut() {
                changed |= rewrite_legacy_asset_urls(value);
            }
            changed
        }
        _ => false,
    }
}

fn rewrite_legacy_asset_url(value: &mut String) -> bool {
    const LEGACY_PREFIX: &str = "/api/panels/";
    let Some(prefix_index) = value.find(LEGACY_PREFIX) else {
        return false;
    };
    let rest = &value[prefix_index + LEGACY_PREFIX.len()..];
    let mut parts = rest.splitn(3, '/');
    let (Some(project_id), Some(panel_id), Some(tail)) = (parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    if project_id.is_empty() || panel_id.is_empty() || !tail.starts_with("assets/") {
        return false;
    }
    let prefix = &value[..prefix_index];
    *value = format!("{prefix}/api/projects/{project_id}/panels/{panel_id}/{tail}");
    true
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::MyOpenPanelsPaths;
    use crate::types::{Panel, PanelKind, Project};
    use serde_json::json;
    use std::sync::{Arc, Barrier};
    use tempfile::tempdir;

    fn paths_for(storage_dir: PathBuf) -> MyOpenPanelsPaths {
        let studio_dir = storage_dir.join("studio");
        MyOpenPanelsPaths {
            context_dir: storage_dir.join("contexts").join("test"),
            context_id: "test".to_owned(),
            context_id_source: "test".to_owned(),
            focus_dir: studio_dir.join("focus"),
            project_dir: storage_dir.join("project"),
            studio_dir,
            storage_dir,
        }
    }

    #[test]
    fn storage_writes_advance_change_seq() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");

        assert_eq!(storage.read_change_seq().expect("initial seq"), 0);

        let session = Project {
            id: "session:test".to_owned(),
            title: "Test".to_owned(),
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            updated_at: "2026-01-01T00:00:00.000Z".to_owned(),
            panel_ids: vec!["panel:canvas".to_owned()],
        };
        storage.write_project(&session).expect("write session");
        let after_session = storage.read_change_seq().expect("session seq");
        assert!(after_session > 0);

        let panel = Panel {
            id: "panel:canvas".to_owned(),
            project_id: session.id.clone(),
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
        storage
            .write_panel_state(
                &session.id,
                &panel.id,
                &json!({ "schema": { "schemaVersion": 1 }, "store": {} }),
            )
            .expect("repeat identical state");
        assert_eq!(
            storage.read_change_seq().expect("unchanged seq"),
            after_state
        );
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
        storage
            .write_panel_selection(&session.id, &panel.id, &json!({ "selectedShapeIds": [] }))
            .expect("repeat identical selection");
        assert_eq!(
            storage.read_change_seq().expect("unchanged selection seq"),
            after_selection
        );
        let selection_scope_count: i64 = storage
            .connection
            .query_row(
                "SELECT COUNT(*) FROM change_scopes WHERE kind = 'panel_selection' AND project_id = ? AND panel_id = ?",
                params![session.id, panel.id],
                |row| row.get(0),
            )
            .expect("selection scope count");
        assert_eq!(selection_scope_count, 1);

        let schema_version: i64 = storage
            .connection
            .query_row(
                "SELECT schema_version FROM panel_states WHERE project_id = ? AND panel_id = ?",
                params![session.id, panel.id],
                |row| row.get(0),
            )
            .expect("schema version");
        assert_eq!(schema_version, 1);
    }

    #[test]
    fn concurrent_panel_state_cas_allows_exactly_one_writer() {
        let temp = tempdir().expect("tempdir");
        let paths = paths_for(temp.path().join(".myopenpanels"));
        let storage = Storage::open(&paths).expect("storage");
        let project = Project {
            id: "project:cas".to_owned(),
            title: "CAS".to_owned(),
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            updated_at: "2026-01-01T00:00:00.000Z".to_owned(),
            panel_ids: vec!["panel:canvas".to_owned()],
        };
        storage.write_project(&project).expect("project");
        storage
            .write_panel(&Panel {
                id: "panel:canvas".to_owned(),
                project_id: project.id.clone(),
                kind: PanelKind::Canvas,
                title: "Canvas".to_owned(),
                created_at: project.created_at.clone(),
                updated_at: project.updated_at.clone(),
                state_ref: None,
            })
            .expect("panel");
        let base_revision = storage
            .write_panel_state(&project.id, "panel:canvas", &json!({ "value": 0 }))
            .expect("initial state");
        drop(storage);

        let barrier = Arc::new(Barrier::new(2));
        let handles = [1, 2].map(|value| {
            let paths = paths.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let storage = Storage::open(&paths).expect("concurrent storage");
                barrier.wait();
                storage
                    .write_panel_state_if_current(
                        "project:cas",
                        "panel:canvas",
                        &json!({ "value": value }),
                        Some(base_revision),
                    )
                    .expect("CAS write")
            })
        });
        let results = handles.map(|handle| handle.join().expect("writer"));
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
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

        let dispatch_migration_count: i64 = storage
            .connection
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE id = '0002_agent_task_dispatch'",
                [],
                |row| row.get(0),
            )
            .expect("dispatch migration count");
        assert_eq!(dispatch_migration_count, 1);
        let operations_migration_count: i64 = storage
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE id = '0003_agent_operations'",
                [],
                |row| row.get(0),
            )
            .expect("operations migration count");
        assert_eq!(operations_migration_count, 1);

        let typesetting_migration_count: i64 = storage
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE id = '0009_typesetting_panel'",
                [],
                |row| row.get(0),
            )
            .expect("typesetting migration count");
        assert_eq!(typesetting_migration_count, 1);
        let panels_schema: String = storage
            .connection()
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'panels'",
                [],
                |row| row.get(0),
            )
            .expect("panels schema");
        assert!(panels_schema.contains("'typesetting'"));

        let publishing_migration_count: i64 = storage
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE id = '0010_publishing_panel'",
                [],
                |row| row.get(0),
            )
            .expect("publishing migration count");
        assert_eq!(publishing_migration_count, 1);
        assert!(panels_schema.contains("'publishing'"));

        let table_count: i64 = storage
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'tasks'",
                [],
                |row| row.get(0),
            )
            .expect("tasks table");
        assert_eq!(table_count, 1);

        let runtime_column_count: i64 = storage
            .connection
            .query_row(
                r#"
                SELECT COUNT(*)
                FROM pragma_table_info('tasks')
                WHERE name IN (
                  'attempts', 'max_attempts', 'lease_owner', 'lease_expires_at',
                  'last_heartbeat_at', 'retry_after'
                )
                "#,
                [],
                |row| row.get(0),
            )
            .expect("tasks runtime columns");
        assert_eq!(runtime_column_count, 6);

        let dispatch_column_count: i64 = storage
            .connection
            .query_row(
                r#"
                SELECT COUNT(*)
                FROM pragma_table_info('tasks')
                WHERE name IN (
                  'capability', 'assigned_agent_id', 'lease_token_hash',
                  'result_json', 'error_json', 'completed_at'
                )
                "#,
                [],
                |row| row.get(0),
            )
            .expect("tasks dispatch columns");
        assert_eq!(dispatch_column_count, 6);

        let dispatch_table_count: i64 = storage
            .connection
            .query_row(
                r#"
                SELECT COUNT(*) FROM sqlite_master
                WHERE type = 'table'
                  AND name IN ('agent_targets', 'task_deliveries', 'task_delivery_attempts')
                "#,
                [],
                |row| row.get(0),
            )
            .expect("dispatch tables");
        assert_eq!(dispatch_table_count, 3);
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
    fn unique_panel_kind_migration_rejects_duplicates_without_deleting_data() {
        let temp = tempdir().expect("tempdir");
        let mut connection =
            Connection::open(temp.path().join("legacy.sqlite3")).expect("database");
        connection
            .execute_batch(MIGRATION_0001_SQL)
            .expect("initial schema");
        connection.execute_batch(
                r#"
                INSERT INTO sessions (
                  id, title, created_at, updated_at, panel_ids_json, session_json
                ) VALUES (
                  'session:duplicate', 'Duplicate', '2026-01-01T00:00:00.000Z',
                  '2026-01-01T00:00:00.000Z', '["panel:one","panel:two"]',
                  '{"id":"session:duplicate","title":"Duplicate","createdAt":"2026-01-01T00:00:00.000Z","updatedAt":"2026-01-01T00:00:00.000Z","panelIds":["panel:one","panel:two"]}'
                );
                INSERT INTO panels (
                  id, session_id, kind, title, created_at, updated_at, state_ref, panel_json
                ) VALUES
                  ('panel:one', 'session:duplicate', 'wiki', 'Wiki one',
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z', NULL,
                   '{"id":"panel:one","sessionId":"session:duplicate","kind":"wiki","title":"Wiki one","createdAt":"2026-01-01T00:00:00.000Z","updatedAt":"2026-01-01T00:00:00.000Z","stateRef":null}'),
                  ('panel:two', 'session:duplicate', 'wiki', 'Wiki two',
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z', NULL,
                   '{"id":"panel:two","sessionId":"session:duplicate","kind":"wiki","title":"Wiki two","createdAt":"2026-01-01T00:00:00.000Z","updatedAt":"2026-01-01T00:00:00.000Z","stateRef":null}');
                "#,
            )
            .expect("duplicate fixture");
        let tx = connection.transaction().expect("transaction");
        let error = migration_0004(&tx).expect_err("duplicate kinds must block migration");
        assert_eq!(error.code(), Some("duplicate_panel_kind"));
        drop(tx);
        let duplicate_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM panels WHERE session_id = 'session:duplicate' AND kind = 'wiki'",
                [],
                |row| row.get(0),
            )
            .expect("duplicate count");
        assert_eq!(duplicate_count, 2);
    }

    #[test]
    fn storage_v2_backs_up_and_removes_unsupported_panels() {
        let temp = tempdir().expect("tempdir");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&storage_dir).expect("storage dir");
        let mut connection = Connection::open(storage_dir.join(DATABASE_FILE_NAME)).expect("db");
        connection
            .execute_batch(SCHEMA_MIGRATIONS_SQL)
            .expect("migration table");
        apply_migration(&mut connection, &migrations()[0]).expect("initial migration");
        connection
            .execute_batch(
                r#"
                INSERT INTO sessions (id, title, created_at, updated_at, panel_ids_json, session_json)
                VALUES ('project-test', 'Test', '2026-01-01T00:00:00.000Z',
                        '2026-01-01T00:00:00.000Z', '["panel-image"]',
                        '{"id":"project-test","title":"Test","createdAt":"2026-01-01T00:00:00.000Z","updatedAt":"2026-01-01T00:00:00.000Z","panelIds":["panel-image"]}');
                INSERT INTO panels (id, session_id, kind, title, created_at, updated_at, state_ref, panel_json)
                VALUES ('panel-image', 'project-test', 'image', 'Images',
                        '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z', NULL,
                        '{"id":"panel-image","sessionId":"project-test","kind":"image","title":"Images","createdAt":"2026-01-01T00:00:00.000Z","updatedAt":"2026-01-01T00:00:00.000Z"}');
                INSERT INTO panel_states (session_id, panel_id, schema_version, state_json, updated_at)
                VALUES ('project-test', 'panel-image', 1, '{"items":["kept"]}',
                        '2026-01-01T00:00:00.000Z');
                "#,
            )
            .expect("legacy unsupported panel");
        drop(connection);
        let legacy_files = storage_dir.join("sessions/project-test/panels/panel-image/assets");
        fs::create_dir_all(&legacy_files).expect("legacy files");
        fs::write(legacy_files.join("source.png"), b"image").expect("legacy asset");

        let paths = paths_for(storage_dir.clone());
        let storage = Storage::open(&paths).expect("upgrade");
        assert!(storage
            .read_panel("project-test", "panel-image")
            .expect("read")
            .is_none());
        let marker: Value = serde_json::from_slice(
            &fs::read(storage_dir.join("unsupported-panel-backup.json")).expect("marker"),
        )
        .expect("marker json");
        let backup = PathBuf::from(marker["backupDir"].as_str().expect("backup dir"))
            .join("project-test/panel-image");
        let metadata: Value =
            serde_json::from_slice(&fs::read(backup.join("panel.json")).expect("panel backup"))
                .expect("panel backup json");
        assert_eq!(metadata["state"]["items"][0], "kept");
        assert_eq!(
            fs::read(backup.join("files/assets/source.png")).expect("asset backup"),
            b"image"
        );
    }

    #[test]
    fn migration_upgrades_existing_task_queue_to_dispatch_protocol() {
        let temp = tempdir().expect("tempdir");
        let storage_dir = temp.path().join(".myopenpanels");
        fs::create_dir_all(&storage_dir).expect("storage dir");
        let database_path = storage_dir.join(DATABASE_FILE_NAME);
        let mut connection = Connection::open(&database_path).expect("legacy database");
        connection
            .execute_batch(SCHEMA_MIGRATIONS_SQL)
            .expect("migration table");
        let migration = &migrations()[0];
        let tx = connection.transaction().expect("legacy transaction");
        migration_0001(&tx).expect("initial schema");
        tx.execute(
            "INSERT INTO schema_migrations (id, description, checksum, applied_at) VALUES (?, ?, ?, ?)",
            params![
                migration.id,
                migration.description,
                migration_checksum(migration),
                "2026-01-01T00:00:00.000Z"
            ],
        )
        .expect("initial migration record");
        tx.execute(
            r#"
            INSERT INTO sessions (id, title, created_at, updated_at, panel_ids_json, session_json)
            VALUES ('session-test', 'Test', '2026-01-01T00:00:00.000Z',
                    '2026-01-01T00:00:00.000Z', '["panel-wiki"]',
                    '{"id":"session-test","title":"Test","createdAt":"2026-01-01T00:00:00.000Z","updatedAt":"2026-01-01T00:00:00.000Z","panelIds":["panel-wiki"]}')
            "#,
            [],
        )
        .expect("legacy session");
        tx.execute(
            r#"
            INSERT INTO panels (id, session_id, kind, title, created_at, updated_at, state_ref, panel_json)
            VALUES ('panel-wiki', 'session-test', 'wiki', 'Wiki',
                    '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z', NULL,
                    '{"id":"panel-wiki","sessionId":"session-test","kind":"wiki","title":"Wiki","createdAt":"2026-01-01T00:00:00.000Z","updatedAt":"2026-01-01T00:00:00.000Z","stateRef":null}')
            "#,
            [],
        )
        .expect("legacy panel");
        tx.execute(
            r#"
            INSERT INTO project_tasks (
              id, queue, session_id, panel_id, panel_kind, type, status,
              target_id, created_at, updated_at, task_json
            ) VALUES (
              'task-test', 'wiki', 'session-test', 'panel-wiki', 'wiki',
              'convert_document_to_markdown', 'queued', 'raw-test',
              '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z',
              '{"id":"task-test","type":"convert_document_to_markdown","status":"queued","targetId":"raw-test","documentId":"raw-test","wikiSpaceId":"wiki-default"}'
            )
            "#,
            [],
        )
        .expect("legacy task");
        tx.execute(
            r#"
            INSERT INTO panel_states (session_id, panel_id, schema_version, state_json, updated_at)
            VALUES ('session-test', 'panel-wiki', 3,
                    '{"schemaVersion":3,"rawDocuments":[],"generatedDocuments":[],"ruleSets":[],"wikiSpaces":[],"tasks":[{"id":"task-test","type":"convert_document_to_markdown","status":"failed"}],"assetRef":"sessions/session-test/panels/panel-wiki/assets/source.md","previewUrl":"/api/panels/session-test/panel-wiki/assets/source.md"}',
                    '2026-01-01T00:00:00.000Z')
            "#,
            [],
        )
        .expect("legacy wiki state");
        tx.commit().expect("legacy commit");
        drop(connection);

        let legacy_asset = storage_dir
            .join("sessions")
            .join("session-test")
            .join("panels")
            .join("panel-wiki")
            .join("assets")
            .join("source.md");
        fs::create_dir_all(legacy_asset.parent().expect("asset parent")).expect("asset directory");
        fs::write(&legacy_asset, "# Source").expect("legacy asset");
        let context_dir = storage_dir.join("contexts").join("ctx");
        fs::create_dir_all(&context_dir).expect("context directory");
        fs::write(
            context_dir.join("active-session.json"),
            r#"{"sessionId":"session-test"}"#,
        )
        .expect("legacy context");

        let paths = paths_for(storage_dir.clone());
        let storage = Storage::open(&paths).expect("upgraded storage");
        let capability: String = storage
            .connection
            .query_row(
                "SELECT capability FROM tasks WHERE id = 'task-test'",
                [],
                |row| row.get(0),
            )
            .expect("backfilled capability");
        assert_eq!(capability, "wiki.convertDocument");
        let migrated_task = storage
            .list_tasks("session-test")
            .expect("migrated task")
            .pop()
            .expect("task");
        assert_eq!(migrated_task["status"], "queued");
        assert_eq!(migrated_task["source"]["wikiSpaceId"], "wiki-default");
        let wiki_state = storage
            .read_panel_state("session-test", "panel-wiki")
            .expect("wiki state")
            .expect("state");
        assert_eq!(wiki_state["schemaVersion"], 4);
        assert!(wiki_state.get("tasks").is_none());
        assert_eq!(
            wiki_state["assetRef"],
            "projects/session-test/panels/panel-wiki/assets/source.md"
        );
        assert_eq!(
            wiki_state["previewUrl"],
            "/api/projects/session-test/panels/panel-wiki/assets/source.md"
        );
        assert!(storage_dir
            .join("projects/session-test/panels/panel-wiki/assets/source.md")
            .is_file());
        assert!(!storage_dir.join("sessions").exists());
        let active_project = fs::read_to_string(context_dir.join("active-project.json"))
            .expect("active project pointer");
        assert!(active_project.contains("projectId"));
        assert!(!context_dir.join("active-session.json").exists());
        for legacy_table in [
            "sessions",
            "wiki_tasks",
            "project_tasks",
            "storage_changes",
            "key_values",
        ] {
            assert!(!table_exists(&storage.connection, legacy_table).expect("table check"));
        }
        let foreign_key_errors: i64 = storage
            .connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("foreign key check");
        assert_eq!(foreign_key_errors, 0);
        let migration_count: i64 = storage
            .connection
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE id = '0002_agent_task_dispatch'",
                [],
                |row| row.get(0),
            )
            .expect("dispatch migration");
        assert_eq!(migration_count, 1);
    }

    #[test]
    fn asset_url_repair_rewrites_every_nested_legacy_url() {
        let mut value = json!({
            "assets": [
                "/api/panels/project:one/panel:canvas/assets/one.png",
                { "src": "/api/panels/project:one/panel:canvas/assets/two.png" }
            ],
            "unrelated": "/api/tasks/task:one"
        });
        assert!(rewrite_legacy_asset_urls(&mut value));
        assert_eq!(
            value["assets"][0],
            "/api/projects/project:one/panels/panel:canvas/assets/one.png"
        );
        assert_eq!(
            value["assets"][1]["src"],
            "/api/projects/project:one/panels/panel:canvas/assets/two.png"
        );
        assert_eq!(value["unrelated"], "/api/tasks/task:one");
    }

    #[test]
    fn project_json_repair_rewrites_nested_keys_and_paths() {
        let mut value = json!({
            "sessionId": "session:one",
            "assetRef": "sessions/session:one/panels/panel:one/assets/image.png",
            "nested": [{
                "sessionId": "session:one",
                "file": "/tmp/.myopenpanels/sessions/session:one/file.png"
            }]
        });
        assert!(rewrite_legacy_project_json(&mut value));
        assert_eq!(value["projectId"], "session:one");
        assert!(value.get("sessionId").is_none());
        assert_eq!(
            value["assetRef"],
            "projects/session:one/panels/panel:one/assets/image.png"
        );
        assert_eq!(value["nested"][0]["projectId"], "session:one");
        assert_eq!(
            value["nested"][0]["file"],
            "/tmp/.myopenpanels/projects/session:one/file.png"
        );
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
        assert!(error.message().contains("no such table"));
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
        let session = Project {
            id: "session:test".to_owned(),
            title: "Test".to_owned(),
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            updated_at: "2026-01-01T00:00:00.000Z".to_owned(),
            panel_ids: vec!["panel:wiki".to_owned()],
        };
        storage.write_project(&session).expect("write session");
        let panel = Panel {
            id: "panel:wiki".to_owned(),
            project_id: session.id.clone(),
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
            .upsert_tasks(
                &session.id,
                &panel.id,
                "wiki",
                state["tasks"].as_array().unwrap(),
            )
            .expect("initial sync");
        storage
            .connection
            .execute(
                r#"
                UPDATE tasks
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
            .upsert_tasks(
                &session.id,
                &panel.id,
                "wiki",
                state["tasks"].as_array().unwrap(),
            )
            .expect("repeat sync");

        let tasks = storage.list_tasks(&session.id).expect("project tasks");
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
