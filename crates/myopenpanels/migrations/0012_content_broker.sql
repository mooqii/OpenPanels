ALTER TABLE task_attempts ADD COLUMN execution_token_hash TEXT;
ALTER TABLE task_attempts ADD COLUMN execution_token_expires_at TEXT;
ALTER TABLE task_attempts ADD COLUMN staging_session_id TEXT;

CREATE TABLE content_resources (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL,
  panel_id TEXT,
  resource_kind TEXT NOT NULL
    CHECK (resource_kind IN ('wiki_markdown', 'wiki_space', 'generated_document', 'writing_skill')),
  resource_key TEXT NOT NULL,
  active_revision_id TEXT,
  content_version INTEGER NOT NULL DEFAULT 0 CHECK (content_version >= 0),
  archived_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE (project_id, resource_kind, resource_key),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY (project_id, panel_id) REFERENCES panels(project_id, id) ON DELETE CASCADE
);

CREATE INDEX content_resources_panel_kind_idx
  ON content_resources(project_id, panel_id, resource_kind, updated_at DESC);

CREATE TABLE content_revisions (
  id TEXT PRIMARY KEY NOT NULL,
  content_resource_id TEXT NOT NULL,
  parent_revision_id TEXT,
  revision_number INTEGER NOT NULL CHECK (revision_number > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_hash TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active'
    CHECK (status IN ('active', 'prunable', 'pruned')),
  source_task_id TEXT,
  source_attempt_id TEXT,
  execution_generation INTEGER,
  created_at TEXT NOT NULL,
  activated_at TEXT,
  prunable_at TEXT,
  pruned_at TEXT,
  UNIQUE (content_resource_id, revision_number),
  FOREIGN KEY (content_resource_id) REFERENCES content_resources(id) ON DELETE CASCADE,
  FOREIGN KEY (parent_revision_id) REFERENCES content_revisions(id) ON DELETE SET NULL,
  FOREIGN KEY (source_task_id) REFERENCES tasks(id) ON DELETE SET NULL,
  FOREIGN KEY (source_attempt_id) REFERENCES task_attempts(id) ON DELETE SET NULL
);

CREATE INDEX content_revisions_resource_idx
  ON content_revisions(content_resource_id, revision_number DESC);
CREATE INDEX content_revisions_prunable_idx
  ON content_revisions(status, prunable_at);

CREATE TABLE content_objects (
  hash TEXT PRIMARY KEY NOT NULL,
  size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
  storage_ref TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL
);

CREATE TABLE content_revision_files (
  revision_id TEXT NOT NULL,
  logical_path TEXT NOT NULL,
  object_hash TEXT NOT NULL,
  size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
  mime_type TEXT NOT NULL,
  PRIMARY KEY (revision_id, logical_path),
  FOREIGN KEY (revision_id) REFERENCES content_revisions(id) ON DELETE CASCADE,
  FOREIGN KEY (object_hash) REFERENCES content_objects(hash) ON DELETE RESTRICT
);

CREATE INDEX content_revision_files_object_idx
  ON content_revision_files(object_hash);

CREATE TABLE task_staging_sessions (
  id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL,
  attempt_id TEXT NOT NULL UNIQUE,
  execution_generation INTEGER NOT NULL CHECK (execution_generation > 0),
  status TEXT NOT NULL DEFAULT 'open'
    CHECK (status IN ('open', 'prepared', 'committed', 'abandoned')),
  total_bytes INTEGER NOT NULL DEFAULT 0 CHECK (total_bytes >= 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  committed_at TEXT,
  abandoned_at TEXT,
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
  FOREIGN KEY (attempt_id) REFERENCES task_attempts(id) ON DELETE CASCADE
);

CREATE INDEX task_staging_sessions_cleanup_idx
  ON task_staging_sessions(status, updated_at);

CREATE TABLE task_staging_resources (
  staging_session_id TEXT NOT NULL,
  resource_kind TEXT NOT NULL,
  resource_key TEXT NOT NULL,
  content_resource_id TEXT,
  base_revision_id TEXT,
  base_content_version INTEGER NOT NULL DEFAULT 0,
  metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
  PRIMARY KEY (staging_session_id, resource_kind, resource_key),
  FOREIGN KEY (staging_session_id) REFERENCES task_staging_sessions(id) ON DELETE CASCADE,
  FOREIGN KEY (content_resource_id) REFERENCES content_resources(id) ON DELETE SET NULL,
  FOREIGN KEY (base_revision_id) REFERENCES content_revisions(id) ON DELETE SET NULL
);

CREATE TABLE task_staged_files (
  staging_session_id TEXT NOT NULL,
  resource_kind TEXT NOT NULL,
  resource_key TEXT NOT NULL,
  logical_path TEXT NOT NULL,
  object_hash TEXT,
  size_bytes INTEGER NOT NULL DEFAULT 0 CHECK (size_bytes >= 0),
  mime_type TEXT,
  operation TEXT NOT NULL DEFAULT 'upsert' CHECK (operation IN ('upsert', 'delete')),
  metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (staging_session_id, resource_kind, resource_key, logical_path),
  FOREIGN KEY (staging_session_id) REFERENCES task_staging_sessions(id) ON DELETE CASCADE,
  FOREIGN KEY (object_hash) REFERENCES content_objects(hash) ON DELETE RESTRICT
);

CREATE INDEX task_staged_files_object_idx ON task_staged_files(object_hash);

CREATE TABLE content_pins (
  task_id TEXT NOT NULL,
  revision_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY (task_id, revision_id),
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
  FOREIGN KEY (revision_id) REFERENCES content_revisions(id) ON DELETE CASCADE
);

CREATE TABLE content_migration_state (
  id TEXT PRIMARY KEY NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'completed', 'failed')),
  checkpoint_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(checkpoint_json)),
  updated_at TEXT NOT NULL,
  completed_at TEXT
);

INSERT INTO content_migration_state (id, status, checkpoint_json, updated_at)
VALUES ('legacy_content_v1', 'pending', '{}', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

CREATE TRIGGER task_execution_fence_content_staging
AFTER UPDATE OF status, execution_generation ON tasks
WHEN NEW.execution_generation <> OLD.execution_generation
  OR NEW.status IN ('failed', 'cancelled', 'stale', 'superseded')
BEGIN
  UPDATE task_staging_sessions
  SET status = 'abandoned', abandoned_at = NEW.updated_at, updated_at = NEW.updated_at
  WHERE task_id = NEW.id AND status IN ('open', 'prepared');
  UPDATE task_attempts
  SET execution_token_hash = NULL, execution_token_expires_at = NULL
  WHERE task_id = NEW.id AND status = 'leased';
  UPDATE agent_operations
  SET status = 'failed',
      error_json = json_object('code', 'execution_fenced'),
      updated_at = NEW.updated_at,
      completed_at = NEW.updated_at
  WHERE status IN ('active', 'prepared')
    AND json_extract(input_json, '$.taskId') = NEW.id;
END;
