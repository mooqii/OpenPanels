import { createHash } from "node:crypto"
import type { DatabaseSync } from "node:sqlite"

export interface SQLiteMigration {
  description: string
  id: string
  up(db: DatabaseSync): void
}

export const migrations: SQLiteMigration[] = [
  {
    id: "0001_initial",
    description: "Create initial OpenPanels SQLite storage schema",
    up(db) {
      db.exec(`
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

        CREATE TABLE key_values (
          namespace TEXT NOT NULL,
          key TEXT NOT NULL,
          value_json TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          PRIMARY KEY (namespace, key)
        );
      `)
    },
  },
]

export function migrationChecksum(migration: SQLiteMigration): string {
  return createHash("sha256")
    .update(migration.id)
    .update("\n")
    .update(migration.description)
    .update("\n")
    .update(String(migration.up))
    .digest("hex")
}
