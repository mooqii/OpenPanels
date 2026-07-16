ALTER TABLE tasks ADD COLUMN workflow_id TEXT;
ALTER TABLE tasks ADD COLUMN idempotency_key TEXT;
ALTER TABLE tasks ADD COLUMN execution_generation INTEGER NOT NULL DEFAULT 0 CHECK (execution_generation >= 0);
ALTER TABLE tasks ADD COLUMN available_at TEXT;
ALTER TABLE tasks ADD COLUMN archived_at TEXT;
ALTER TABLE tasks ADD COLUMN terminal_reason_json TEXT CHECK (terminal_reason_json IS NULL OR json_valid(terminal_reason_json));
ALTER TABLE tasks ADD COLUMN required_protocol_version INTEGER NOT NULL DEFAULT 1 CHECK (required_protocol_version IN (1, 2));

ALTER TABLE agent_targets ADD COLUMN protocol_version INTEGER NOT NULL DEFAULT 1 CHECK (protocol_version IN (1, 2));
ALTER TABLE agent_targets ADD COLUMN max_concurrency INTEGER NOT NULL DEFAULT 1 CHECK (max_concurrency > 0);

CREATE TABLE workflows (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL,
  panel_id TEXT NOT NULL,
  type TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active'
    CHECK (status IN ('active', 'succeeded', 'failed', 'cancelled', 'archived')),
  source_workflow_id TEXT,
  source_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(source_json)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  archived_at TEXT,
  FOREIGN KEY (project_id, panel_id) REFERENCES panels(project_id, id) ON DELETE CASCADE,
  FOREIGN KEY (source_workflow_id) REFERENCES workflows(id) ON DELETE SET NULL
);

CREATE INDEX workflows_project_updated_idx
  ON workflows(project_id, updated_at DESC, id);

CREATE TABLE task_dependencies (
  task_id TEXT NOT NULL,
  prerequisite_task_id TEXT NOT NULL,
  success_condition TEXT NOT NULL DEFAULT 'succeeded'
    CHECK (success_condition IN ('succeeded', 'terminal')),
  failure_policy TEXT NOT NULL DEFAULT 'cancel'
    CHECK (failure_policy IN ('cancel', 'supersede', 'continue_snapshot')),
  created_at TEXT NOT NULL,
  PRIMARY KEY (task_id, prerequisite_task_id),
  CHECK (task_id <> prerequisite_task_id),
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
  FOREIGN KEY (prerequisite_task_id) REFERENCES tasks(id) ON DELETE RESTRICT
);

CREATE INDEX task_dependencies_prerequisite_idx
  ON task_dependencies(prerequisite_task_id, task_id);

CREATE TABLE task_inputs (
  id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL,
  resource_kind TEXT NOT NULL,
  resource_id TEXT NOT NULL,
  resource_version TEXT,
  content_hash TEXT,
  snapshot_ref TEXT,
  missing_policy TEXT NOT NULL DEFAULT 'cancel'
    CHECK (missing_policy IN ('cancel', 'continue_snapshot')),
  changed_policy TEXT NOT NULL DEFAULT 'supersede'
    CHECK (changed_policy IN ('supersede', 'continue_snapshot', 'fail')),
  created_at TEXT NOT NULL,
  UNIQUE (task_id, resource_kind, resource_id),
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
);

CREATE INDEX task_inputs_resource_idx
  ON task_inputs(resource_kind, resource_id, task_id);

CREATE TABLE task_attempts (
  id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL,
  attempt_number INTEGER NOT NULL CHECK (attempt_number > 0),
  execution_generation INTEGER NOT NULL CHECK (execution_generation > 0),
  agent_target_id TEXT,
  status TEXT NOT NULL
    CHECK (status IN ('leased', 'succeeded', 'failed_retryable', 'failed_terminal', 'invalid_output', 'cancelled', 'interrupted')),
  started_at TEXT NOT NULL,
  heartbeat_at TEXT,
  finished_at TEXT,
  result_json TEXT CHECK (result_json IS NULL OR json_valid(result_json)),
  error_json TEXT CHECK (error_json IS NULL OR json_valid(error_json)),
  UNIQUE (task_id, execution_generation),
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
  FOREIGN KEY (agent_target_id) REFERENCES agent_targets(id) ON DELETE SET NULL
);

CREATE INDEX task_attempts_task_started_idx
  ON task_attempts(task_id, started_at DESC);

CREATE TABLE task_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id TEXT NOT NULL,
  workflow_id TEXT NOT NULL,
  event_type TEXT NOT NULL,
  from_status TEXT,
  to_status TEXT,
  reason_json TEXT CHECK (reason_json IS NULL OR json_valid(reason_json)),
  attempt_id TEXT,
  agent_target_id TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
  FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE,
  FOREIGN KEY (attempt_id) REFERENCES task_attempts(id) ON DELETE SET NULL,
  FOREIGN KEY (agent_target_id) REFERENCES agent_targets(id) ON DELETE SET NULL
);

CREATE INDEX task_events_task_created_idx
  ON task_events(task_id, id DESC);
CREATE INDEX task_events_workflow_created_idx
  ON task_events(workflow_id, id DESC);

CREATE TABLE dispatch_outbox (
  id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL,
  workflow_id TEXT NOT NULL,
  capability TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'delivering', 'delivered', 'exhausted', 'cancelled')),
  attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  next_attempt_at TEXT,
  last_error TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
  FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE
);

CREATE INDEX dispatch_outbox_pending_idx
  ON dispatch_outbox(status, next_attempt_at, created_at);
CREATE UNIQUE INDEX dispatch_outbox_active_task_idx
  ON dispatch_outbox(task_id)
  WHERE status IN ('pending', 'delivering');

CREATE TABLE agent_routes (
  project_id TEXT NOT NULL,
  capability TEXT NOT NULL,
  agent_target_id TEXT NOT NULL,
  position INTEGER NOT NULL CHECK (position >= 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (project_id, capability, agent_target_id),
  UNIQUE (project_id, capability, position),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY (agent_target_id) REFERENCES agent_targets(id) ON DELETE CASCADE
);

CREATE INDEX agent_routes_target_idx ON agent_routes(agent_target_id);

INSERT INTO task_attempts (
  id, task_id, attempt_number, execution_generation, agent_target_id,
  status, started_at, heartbeat_at, finished_at, error_json
)
SELECT
  'attempt:upgrade:' || substr(id, 6), id, MAX(attempts, 1), 1,
  assigned_agent_id, 'interrupted', created_at, last_heartbeat_at, updated_at,
  json_object('code', 'upgrade_interrupted')
FROM tasks
WHERE status IN ('reserved', 'running', 'claimed', 'converting', 'indexing');

UPDATE tasks
SET status = 'failed',
    execution_generation = 1,
    assigned_agent_id = NULL,
    lease_owner = NULL,
    lease_token_hash = NULL,
    lease_expires_at = NULL,
    last_heartbeat_at = NULL,
    retry_after = updated_at,
    error_json = json_object('code', 'upgrade_interrupted')
WHERE status IN ('reserved', 'running', 'claimed', 'converting', 'indexing');

UPDATE tasks
SET status = 'cancelled',
    assigned_agent_id = NULL,
    lease_owner = NULL,
    lease_token_hash = NULL,
    lease_expires_at = NULL,
    last_heartbeat_at = NULL,
    completed_at = updated_at,
    terminal_reason_json = json_object('code', 'prerequisite_missing')
WHERE queue = 'wiki'
  AND status IN ('queued', 'failed', 'waiting')
  AND json_extract(input_json, '$.documentId') IS NOT NULL
  AND NOT EXISTS (
    SELECT 1
    FROM panel_states ps, json_each(json_extract(ps.state_json, '$.rawDocuments')) document
    WHERE ps.project_id = tasks.project_id
      AND ps.panel_id = tasks.panel_id
      AND json_extract(document.value, '$.id') = json_extract(tasks.input_json, '$.documentId')
  );

INSERT INTO workflows (
  id, project_id, panel_id, type, status, source_json, created_at, updated_at, archived_at
)
SELECT
  'workflow:' || substr(id, 6), project_id, panel_id, 'legacy.' || type,
  CASE
    WHEN status = 'succeeded' THEN 'succeeded'
    WHEN status IN ('cancelled', 'stale') THEN 'cancelled'
    WHEN status = 'failed' AND attempts >= max_attempts THEN 'failed'
    ELSE 'active'
  END,
  json_object('migration', '0011_atomic_workflows', 'legacyTaskId', id),
  created_at, updated_at, NULL
FROM tasks;

UPDATE tasks
SET workflow_id = 'workflow:' || substr(id, 6),
    available_at = COALESCE(retry_after, created_at),
    terminal_reason_json = CASE
      WHEN status = 'stale' THEN COALESCE(terminal_reason_json, json_object('code', 'legacy_stale'))
      WHEN status = 'cancelled' THEN COALESCE(terminal_reason_json, json_object('code', 'legacy_cancelled'))
      ELSE terminal_reason_json
    END;

INSERT OR IGNORE INTO task_inputs (
  id, task_id, resource_kind, resource_id, resource_version,
  content_hash, snapshot_ref, missing_policy, changed_policy, created_at
)
SELECT
  'task-input:migrated:' || substr(id, 6) || ':document', id,
  CASE WHEN queue = 'writing' THEN 'writing.targetDocument' ELSE 'wiki.rawDocument' END,
  json_extract(input_json, '$.documentId'),
  CAST(json_extract(input_json, '$.markdownVersion') AS TEXT),
  json_extract(input_json, '$.contentHash'),
  json_extract(input_json, '$.snapshotRef'),
  'cancel', 'supersede', created_at
FROM tasks
WHERE json_extract(input_json, '$.documentId') IS NOT NULL;

INSERT OR IGNORE INTO task_inputs (
  id, task_id, resource_kind, resource_id, resource_version,
  content_hash, snapshot_ref, missing_policy, changed_policy, created_at
)
SELECT
  'task-input:migrated:' || substr(t.id, 6) || ':raw:' || json_extract(document.value, '$.id'),
  t.id, 'wiki.rawDocument', json_extract(document.value, '$.id'),
  CAST(json_extract(document.value, '$.markdownVersion') AS TEXT),
  json_extract(document.value, '$.snapshotHash'),
  'inline:input.contextSnapshot.rawDocuments',
  'continue_snapshot', 'continue_snapshot', t.created_at
FROM tasks t, json_each(json_extract(t.input_json, '$.contextSnapshot.rawDocuments')) document
WHERE json_extract(document.value, '$.id') IS NOT NULL;

INSERT OR IGNORE INTO task_inputs (
  id, task_id, resource_kind, resource_id, resource_version,
  content_hash, snapshot_ref, missing_policy, changed_policy, created_at
)
SELECT
  'task-input:migrated:' || substr(t.id, 6) || ':generated:' || json_extract(document.value, '$.id'),
  t.id, 'wiki.generatedDocument', json_extract(document.value, '$.id'),
  CAST(json_extract(document.value, '$.contentVersion') AS TEXT),
  json_extract(document.value, '$.snapshotHash'),
  'inline:input.contextSnapshot.generatedDocuments',
  'continue_snapshot', 'continue_snapshot', t.created_at
FROM tasks t, json_each(json_extract(t.input_json, '$.contextSnapshot.generatedDocuments')) document
WHERE json_extract(document.value, '$.id') IS NOT NULL;

INSERT OR IGNORE INTO task_inputs (
  id, task_id, resource_kind, resource_id, content_hash, snapshot_ref,
  missing_policy, changed_policy, created_at
)
SELECT
  'task-input:migrated:' || substr(id, 6) || ':skill', id, 'writing.skill',
  json_extract(input_json, '$.writingSkillSnapshot.id'),
  json_extract(input_json, '$.writingSkillSnapshot.contentHash'),
  'inline:input.writingSkillSnapshot', 'continue_snapshot', 'continue_snapshot', created_at
FROM tasks
WHERE json_extract(input_json, '$.writingSkillSnapshot.id') IS NOT NULL;

INSERT OR IGNORE INTO task_inputs (
  id, task_id, resource_kind, resource_id, resource_version,
  missing_policy, changed_policy, created_at
)
SELECT
  'task-input:migrated:' || substr(id, 6) || ':target', id, 'writing.targetDocument',
  json_extract(input_json, '$.targetGeneratedDocumentId'),
  CAST(json_extract(input_json, '$.targetContentVersion') AS TEXT),
  'cancel', 'supersede', created_at
FROM tasks
WHERE json_extract(input_json, '$.targetGeneratedDocumentId') IS NOT NULL;

INSERT INTO task_events (
  task_id, workflow_id, event_type, from_status, to_status, reason_json, created_at
)
SELECT id, workflow_id, 'migrated', NULL, status,
       json_object('migration', '0011_atomic_workflows'), updated_at
FROM tasks;

INSERT INTO dispatch_outbox (
  id, task_id, workflow_id, capability, status, next_attempt_at, created_at, updated_at
)
SELECT 'outbox:' || substr(id, 6), id, workflow_id, capability, 'pending', retry_after, created_at, updated_at
FROM tasks
WHERE status IN ('queued', 'failed') AND attempts < max_attempts;

CREATE UNIQUE INDEX tasks_active_idempotency_idx
  ON tasks(project_id, idempotency_key)
  WHERE idempotency_key IS NOT NULL AND archived_at IS NULL
    AND status IN ('waiting', 'queued', 'reserved', 'running', 'claimed', 'converting', 'indexing');

CREATE INDEX tasks_workflow_idx ON tasks(workflow_id, created_at, id);
CREATE INDEX tasks_ready_idx ON tasks(project_id, status, available_at, archived_at);

CREATE TRIGGER tasks_status_insert_guard
BEFORE INSERT ON tasks
WHEN NEW.status NOT IN (
  'waiting', 'queued', 'reserved', 'running', 'claimed', 'converting', 'indexing',
  'failed', 'succeeded', 'cancel_requested', 'cancelled', 'stale', 'superseded'
)
BEGIN
  SELECT RAISE(ABORT, 'invalid task status');
END;

CREATE TRIGGER tasks_status_update_guard
BEFORE UPDATE OF status ON tasks
WHEN NEW.status NOT IN (
  'waiting', 'queued', 'reserved', 'running', 'claimed', 'converting', 'indexing',
  'failed', 'succeeded', 'cancel_requested', 'cancelled', 'stale', 'superseded'
)
BEGIN
  SELECT RAISE(ABORT, 'invalid task status');
END;

CREATE TRIGGER tasks_status_transition_guard
BEFORE UPDATE OF status ON tasks
WHEN NEW.status <> OLD.status
  AND NOT (
    (OLD.status = 'waiting' AND NEW.status IN ('queued', 'cancelled', 'superseded')) OR
    (OLD.status = 'queued' AND NEW.status IN ('reserved', 'failed', 'cancelled', 'stale', 'superseded')) OR
    (OLD.status = 'failed' AND NEW.status IN ('queued', 'reserved', 'cancelled', 'stale', 'superseded')) OR
    (OLD.status = 'reserved' AND NEW.status IN ('queued', 'running', 'claimed', 'converting', 'indexing', 'failed', 'cancel_requested', 'cancelled', 'superseded')) OR
    (OLD.status IN ('running', 'claimed', 'converting', 'indexing') AND NEW.status IN ('queued', 'failed', 'succeeded', 'cancel_requested', 'cancelled', 'stale', 'superseded')) OR
    (OLD.status = 'cancel_requested' AND NEW.status IN ('failed', 'cancelled'))
  )
BEGIN
  SELECT RAISE(ABORT, 'invalid task status transition');
END;
