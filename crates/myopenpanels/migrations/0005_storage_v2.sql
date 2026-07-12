CREATE TABLE storage_meta_v2 (
  id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
  global_revision INTEGER NOT NULL DEFAULT 0 CHECK (global_revision >= 0),
  layout_version INTEGER NOT NULL DEFAULT 2 CHECK (layout_version >= 2)
);

INSERT INTO storage_meta_v2 (id, global_revision, layout_version)
SELECT 1, COALESCE((SELECT MAX(seq) FROM storage_changes), 0), 2;

CREATE TABLE projects_v2 (
  id TEXT PRIMARY KEY NOT NULL,
  title TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

INSERT INTO projects_v2 (id, title, created_at, updated_at)
SELECT id, title, created_at, updated_at FROM sessions;

CREATE TABLE panels_v2 (
  project_id TEXT NOT NULL,
  id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('wiki', 'canvas')),
  title TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (project_id, id),
  UNIQUE (project_id, kind),
  FOREIGN KEY (project_id) REFERENCES projects_v2(id) ON DELETE CASCADE
);

INSERT INTO panels_v2 (project_id, id, kind, title, created_at, updated_at)
SELECT
  p.session_id,
  p.id,
  p.kind,
  p.title,
  p.created_at,
  p.updated_at
FROM panels p
WHERE p.kind IN ('wiki', 'canvas');

CREATE TABLE panel_states_v2 (
  project_id TEXT NOT NULL,
  panel_id TEXT NOT NULL,
  schema_version INTEGER,
  revision INTEGER NOT NULL CHECK (revision >= 0),
  content_hash TEXT NOT NULL,
  state_json TEXT NOT NULL CHECK (json_valid(state_json)),
  updated_at TEXT NOT NULL,
  PRIMARY KEY (project_id, panel_id),
  FOREIGN KEY (project_id, panel_id)
    REFERENCES panels_v2(project_id, id) ON DELETE CASCADE
);

INSERT INTO panel_states_v2 (
  project_id, panel_id, schema_version, revision, content_hash, state_json, updated_at
)
SELECT
  ps.session_id,
  ps.panel_id,
  CASE WHEN p.kind = 'wiki' THEN 4 ELSE ps.schema_version END,
  COALESCE((
    SELECT MAX(sc.seq) FROM storage_changes sc
    WHERE sc.kind = 'panel_state'
      AND sc.session_id = ps.session_id
      AND sc.panel_id = ps.panel_id
  ), 1),
  '',
  CASE
    WHEN p.kind = 'wiki' THEN json_set(
      json_remove(replace(replace(ps.state_json, '"sessionId"', '"projectId"'), 'sessions/', 'projects/'), '$.tasks'),
      '$.schemaVersion', 4
    )
    ELSE replace(replace(ps.state_json, '"sessionId"', '"projectId"'), 'sessions/', 'projects/')
  END,
  ps.updated_at
FROM panel_states ps
JOIN panels p ON p.session_id = ps.session_id AND p.id = ps.panel_id
WHERE p.kind IN ('wiki', 'canvas');

CREATE TABLE panel_selections_v2 (
  project_id TEXT NOT NULL,
  panel_id TEXT NOT NULL,
  revision INTEGER NOT NULL CHECK (revision >= 0),
  content_hash TEXT NOT NULL,
  selection_json TEXT NOT NULL CHECK (json_valid(selection_json)),
  updated_at TEXT NOT NULL,
  PRIMARY KEY (project_id, panel_id),
  FOREIGN KEY (project_id, panel_id)
    REFERENCES panels_v2(project_id, id) ON DELETE CASCADE
);

INSERT INTO panel_selections_v2 (
  project_id, panel_id, revision, content_hash, selection_json, updated_at
)
SELECT
  ps.session_id,
  ps.panel_id,
  COALESCE((
    SELECT MAX(sc.seq) FROM storage_changes sc
    WHERE sc.kind = 'panel_selection'
      AND sc.session_id = ps.session_id
      AND sc.panel_id = ps.panel_id
  ), 1),
  '',
  replace(replace(ps.selection_json, '"sessionId"', '"projectId"'), 'sessions/', 'projects/'),
  ps.updated_at
FROM panel_selections ps
JOIN panels p ON p.session_id = ps.session_id AND p.id = ps.panel_id
WHERE p.kind IN ('wiki', 'canvas');

CREATE TABLE artifacts_v2 (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL,
  panel_id TEXT,
  kind TEXT NOT NULL,
  title TEXT,
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (project_id) REFERENCES projects_v2(id) ON DELETE CASCADE,
  FOREIGN KEY (project_id, panel_id)
    REFERENCES panels_v2(project_id, id) ON DELETE CASCADE
);

INSERT INTO artifacts_v2 (
  id, project_id, panel_id, kind, title, payload_json, created_at, updated_at
)
SELECT id, session_id,
       CASE WHEN panel_id IN (
         SELECT id FROM panels WHERE session_id = artifacts.session_id AND kind IN ('wiki', 'canvas')
       ) THEN panel_id ELSE NULL END,
       kind, title,
       replace(replace(artifact_json, '"sessionId"', '"projectId"'), 'sessions/', 'projects/'),
       created_at, created_at
FROM artifacts;

CREATE TABLE agent_targets_v2 (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL,
  name TEXT NOT NULL,
  host TEXT NOT NULL,
  transport TEXT NOT NULL CHECK (transport IN ('webhook', 'poll', 'command')),
  endpoint TEXT,
  capabilities_json TEXT NOT NULL CHECK (json_valid(capabilities_json)),
  priority INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'online',
  token_hash TEXT NOT NULL,
  last_error TEXT,
  last_heartbeat_at TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE (project_id, name, transport),
  FOREIGN KEY (project_id) REFERENCES projects_v2(id) ON DELETE CASCADE
);

INSERT INTO agent_targets_v2
SELECT id, session_id, name, host, transport, endpoint, capabilities_json,
       priority, status, token_hash, last_error, last_heartbeat_at, created_at, updated_at
FROM agent_targets;

CREATE TABLE tasks_v2 (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL,
  panel_id TEXT NOT NULL,
  queue TEXT NOT NULL,
  type TEXT NOT NULL,
  capability TEXT NOT NULL,
  status TEXT NOT NULL,
  target_ref TEXT NOT NULL,
  input_json TEXT NOT NULL CHECK (json_valid(input_json)),
  source_json TEXT NOT NULL CHECK (json_valid(source_json)),
  attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  max_attempts INTEGER NOT NULL DEFAULT 3 CHECK (max_attempts > 0),
  assigned_agent_id TEXT,
  lease_owner TEXT,
  lease_token_hash TEXT,
  lease_expires_at TEXT,
  last_heartbeat_at TEXT,
  retry_after TEXT,
  result_json TEXT CHECK (result_json IS NULL OR json_valid(result_json)),
  error_json TEXT CHECK (error_json IS NULL OR json_valid(error_json)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT,
  FOREIGN KEY (project_id, panel_id)
    REFERENCES panels_v2(project_id, id) ON DELETE CASCADE,
  FOREIGN KEY (assigned_agent_id) REFERENCES agent_targets_v2(id) ON DELETE SET NULL
);

INSERT INTO tasks_v2 (
  id, project_id, panel_id, queue, type, capability, status, target_ref,
  input_json, source_json, attempts, max_attempts, assigned_agent_id,
  lease_owner, lease_token_hash, lease_expires_at, last_heartbeat_at,
  retry_after, result_json, error_json, created_at, updated_at, completed_at
)
SELECT
  id, session_id, panel_id, queue, type,
  COALESCE(NULLIF(capability, ''), queue || '.' || replace(type, '_', '.')),
  status, target_id,
  COALESCE(json_extract(task_json, '$.input'), json_object(
    'documentId', json_extract(task_json, '$.documentId'),
    'markdownVersion', json_extract(task_json, '$.markdownVersion')
  )),
  COALESCE(json_extract(task_json, '$.source'), json_object(
    'wikiSpaceId', json_extract(task_json, '$.wikiSpaceId'),
    'ruleSetId', json_extract(task_json, '$.ruleSetId'),
    'ruleSetVersion', json_extract(task_json, '$.ruleSetVersion'),
    'agentSkillId', json_extract(task_json, '$.agentSkillId')
  )),
  attempts, max_attempts, assigned_target_id,
  lease_owner, lease_token_hash, lease_expires_at, last_heartbeat_at,
  retry_after, result_json, error_json, created_at, updated_at, completed_at
FROM project_tasks
WHERE panel_id IN (
  SELECT id FROM panels WHERE session_id = project_tasks.session_id AND kind IN ('wiki', 'canvas')
);

INSERT OR IGNORE INTO tasks_v2 (
  id, project_id, panel_id, queue, type, capability, status, target_ref,
  input_json, source_json, attempts, max_attempts, created_at, updated_at
)
SELECT
  json_extract(task.value, '$.id'), ps.session_id, ps.panel_id, 'wiki',
  COALESCE(json_extract(task.value, '$.type'), 'unknown'),
  CASE json_extract(task.value, '$.type')
    WHEN 'convert_document_to_markdown' THEN 'wiki.convertDocument'
    WHEN 'ingest_markdown_into_wiki' THEN 'wiki.ingestMarkdown'
    WHEN 'rebuild_wiki_index' THEN 'wiki.rebuildIndex'
    ELSE 'wiki.unknown'
  END,
  COALESCE(json_extract(task.value, '$.status'), 'queued'),
  COALESCE(json_extract(task.value, '$.targetId'), ''),
  json_object(
    'documentId', json_extract(task.value, '$.documentId'),
    'markdownVersion', json_extract(task.value, '$.markdownVersion')
  ),
  json_object(
    'wikiSpaceId', json_extract(task.value, '$.wikiSpaceId'),
    'ruleSetId', json_extract(task.value, '$.ruleSetId'),
    'ruleSetVersion', json_extract(task.value, '$.ruleSetVersion'),
    'agentSkillId', json_extract(task.value, '$.agentSkillId')
  ),
  COALESCE(json_extract(task.value, '$.attempt'), 0),
  COALESCE(json_extract(task.value, '$.maxAttempts'), 3),
  COALESCE(json_extract(task.value, '$.createdAt'), ps.updated_at),
  COALESCE(json_extract(task.value, '$.updatedAt'), ps.updated_at)
FROM panel_states ps
JOIN panels p ON p.session_id = ps.session_id AND p.id = ps.panel_id
JOIN json_each(ps.state_json, '$.tasks') task
WHERE p.kind = 'wiki' AND json_extract(task.value, '$.id') IS NOT NULL;

CREATE TABLE task_deliveries_v2 (
  id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL,
  agent_target_id TEXT NOT NULL,
  status TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  next_attempt_at TEXT,
  last_error TEXT,
  delivered_at TEXT,
  acknowledged_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE (task_id, agent_target_id),
  FOREIGN KEY (task_id) REFERENCES tasks_v2(id) ON DELETE CASCADE,
  FOREIGN KEY (agent_target_id) REFERENCES agent_targets_v2(id) ON DELETE CASCADE
);

INSERT INTO task_deliveries_v2
SELECT id, task_id, target_id, status, attempts, next_attempt_at, last_error,
       delivered_at, acknowledged_at, created_at, updated_at
FROM task_deliveries
WHERE task_id IN (SELECT id FROM tasks_v2);

CREATE TABLE task_delivery_attempts_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  delivery_id TEXT NOT NULL,
  attempt INTEGER NOT NULL CHECK (attempt > 0),
  status TEXT NOT NULL,
  error TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (delivery_id) REFERENCES task_deliveries_v2(id) ON DELETE CASCADE
);

INSERT INTO task_delivery_attempts_v2
SELECT id, delivery_id, attempt, status, error, created_at
FROM task_delivery_attempts
WHERE delivery_id IN (SELECT id FROM task_deliveries_v2);

CREATE TABLE agent_operations_v2 (
  id TEXT PRIMARY KEY NOT NULL,
  owner_context_id TEXT NOT NULL,
  intent TEXT NOT NULL,
  status TEXT NOT NULL,
  project_id TEXT NOT NULL,
  panel_id TEXT NOT NULL,
  guide_id TEXT,
  protocol_version INTEGER NOT NULL CHECK (protocol_version > 0),
  target_json TEXT NOT NULL CHECK (json_valid(target_json)),
  input_json TEXT NOT NULL CHECK (json_valid(input_json)),
  result_json TEXT CHECK (result_json IS NULL OR json_valid(result_json)),
  error_json TEXT CHECK (error_json IS NULL OR json_valid(error_json)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT,
  FOREIGN KEY (project_id, panel_id)
    REFERENCES panels_v2(project_id, id) ON DELETE CASCADE
);

INSERT INTO agent_operations_v2
SELECT id, owner_context_id, intent, status, session_id, panel_id, guide_id,
       protocol_version, target_json, input_json, result_json, error_json,
       created_at, updated_at, completed_at
FROM agent_operations
WHERE panel_id IN (
  SELECT id FROM panels WHERE session_id = agent_operations.session_id AND kind IN ('wiki', 'canvas')
);

CREATE TABLE settings_v2 (
  namespace TEXT NOT NULL,
  key TEXT NOT NULL,
  value_json TEXT NOT NULL CHECK (json_valid(value_json)),
  updated_at TEXT NOT NULL,
  PRIMARY KEY (namespace, key)
);

INSERT INTO settings_v2 SELECT namespace, key, value_json, updated_at FROM key_values;

CREATE TABLE change_scopes_v2 (
  scope_key TEXT PRIMARY KEY NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN (
    'catalog', 'project', 'panel_state', 'panel_selection', 'tasks',
    'agent_targets', 'agent_operations', 'artifacts'
  )),
  project_id TEXT,
  panel_id TEXT,
  revision INTEGER NOT NULL CHECK (revision >= 0),
  updated_at TEXT NOT NULL
);

INSERT INTO change_scopes_v2 (scope_key, kind, revision, updated_at)
SELECT 'catalog', 'catalog', global_revision, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
FROM storage_meta_v2;

INSERT INTO change_scopes_v2 (scope_key, kind, project_id, panel_id, revision, updated_at)
SELECT 'panel_state:' || project_id || ':' || panel_id,
       'panel_state', project_id, panel_id, revision, updated_at
FROM panel_states_v2;

DROP TABLE task_delivery_attempts;
DROP TABLE task_deliveries;
DROP TABLE agent_targets;
DROP TABLE project_tasks;
DROP TABLE wiki_tasks;
DROP TABLE panel_selections;
DROP TABLE panel_states;
DROP TABLE artifacts;
DROP TABLE agent_operations;
DROP TABLE panels;
DROP TABLE sessions;
DROP TABLE key_values;
DROP TABLE storage_changes;

ALTER TABLE storage_meta_v2 RENAME TO storage_meta;
ALTER TABLE projects_v2 RENAME TO projects;
ALTER TABLE panels_v2 RENAME TO panels;
ALTER TABLE panel_states_v2 RENAME TO panel_states;
ALTER TABLE panel_selections_v2 RENAME TO panel_selections;
ALTER TABLE artifacts_v2 RENAME TO artifacts;
ALTER TABLE tasks_v2 RENAME TO tasks;
ALTER TABLE agent_targets_v2 RENAME TO agent_targets;
ALTER TABLE task_deliveries_v2 RENAME TO task_deliveries;
ALTER TABLE task_delivery_attempts_v2 RENAME TO task_delivery_attempts;
ALTER TABLE agent_operations_v2 RENAME TO agent_operations;
ALTER TABLE settings_v2 RENAME TO settings;
ALTER TABLE change_scopes_v2 RENAME TO change_scopes;

CREATE INDEX projects_updated_at_idx ON projects(updated_at DESC, id ASC);
CREATE INDEX panels_project_kind_idx ON panels(project_id, kind ASC);
CREATE INDEX change_scopes_project_revision_idx ON change_scopes(project_id, revision);
CREATE INDEX tasks_project_status_idx ON tasks(project_id, status, updated_at DESC);
CREATE INDEX tasks_project_capability_idx ON tasks(project_id, capability, status, retry_after);
CREATE INDEX tasks_lease_idx ON tasks(lease_expires_at, status);
CREATE INDEX agent_targets_project_status_idx
  ON agent_targets(project_id, status, priority DESC, last_heartbeat_at DESC);
CREATE INDEX task_deliveries_due_idx
  ON task_deliveries(status, next_attempt_at, updated_at);
CREATE INDEX task_deliveries_task_idx
  ON task_deliveries(task_id, updated_at DESC);
CREATE INDEX task_delivery_attempts_delivery_idx
  ON task_delivery_attempts(delivery_id, attempt DESC);
CREATE INDEX agent_operations_owner_status_idx
  ON agent_operations(owner_context_id, status, updated_at DESC);
CREATE INDEX agent_operations_target_idx
  ON agent_operations(project_id, panel_id, updated_at DESC);
