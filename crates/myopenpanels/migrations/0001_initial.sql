CREATE TABLE storage_meta (
  id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
  global_revision INTEGER NOT NULL DEFAULT 0 CHECK (global_revision >= 0)
);

INSERT INTO storage_meta (id, global_revision) VALUES (1, 0);

CREATE TABLE projects (
  id TEXT PRIMARY KEY NOT NULL,
  title TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE panels (
  project_id TEXT NOT NULL,
  id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('wiki', 'writing', 'canvas', 'typesetting', 'publishing')),
  title TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (project_id, id),
  UNIQUE (project_id, kind),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE TABLE panel_states (
  project_id TEXT NOT NULL,
  panel_id TEXT NOT NULL,
  schema_version INTEGER NOT NULL,
  revision INTEGER NOT NULL CHECK (revision >= 0),
  content_hash TEXT NOT NULL,
  state_json TEXT NOT NULL CHECK (json_valid(state_json)),
  updated_at TEXT NOT NULL,
  PRIMARY KEY (project_id, panel_id),
  FOREIGN KEY (project_id, panel_id) REFERENCES panels(project_id, id) ON DELETE CASCADE
);

CREATE TABLE panel_selections (
  project_id TEXT NOT NULL,
  panel_id TEXT NOT NULL,
  revision INTEGER NOT NULL CHECK (revision >= 0),
  content_hash TEXT NOT NULL,
  selection_json TEXT NOT NULL CHECK (json_valid(selection_json)),
  updated_at TEXT NOT NULL,
  PRIMARY KEY (project_id, panel_id),
  FOREIGN KEY (project_id, panel_id) REFERENCES panels(project_id, id) ON DELETE CASCADE
);

CREATE TABLE artifacts (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL,
  panel_id TEXT,
  kind TEXT NOT NULL,
  title TEXT,
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY (project_id, panel_id) REFERENCES panels(project_id, id) ON DELETE CASCADE
);

CREATE TABLE settings (
  namespace TEXT NOT NULL,
  key TEXT NOT NULL,
  value_json TEXT NOT NULL CHECK (json_valid(value_json)),
  updated_at TEXT NOT NULL,
  PRIMARY KEY (namespace, key)
);

CREATE TABLE change_scopes (
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

CREATE TABLE model_gateway_connections (
  id TEXT PRIMARY KEY NOT NULL,
  transport TEXT NOT NULL CHECK (transport IN ('local_cli', 'byok')),
  provider_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  executable_path TEXT,
  base_url TEXT,
  credential_ref TEXT,
  model TEXT,
  reasoning TEXT,
  config_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(config_json)),
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  priority INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE model_gateway_config (
  id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
  mode TEXT NOT NULL CHECK (mode IN ('local_cli', 'byok')),
  active_local_connection_id TEXT,
  active_byok_connection_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (active_local_connection_id) REFERENCES model_gateway_connections(id) ON DELETE SET NULL,
  FOREIGN KEY (active_byok_connection_id) REFERENCES model_gateway_connections(id) ON DELETE SET NULL
);

INSERT INTO model_gateway_config (
  id, mode, active_local_connection_id, active_byok_connection_id, created_at, updated_at
) VALUES (
  1, 'local_cli', NULL, NULL,
  strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
  strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
);

CREATE TABLE agent_targets (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL,
  name TEXT NOT NULL,
  host TEXT NOT NULL,
  transport TEXT NOT NULL CHECK (transport IN ('poll', 'command')),
  capabilities_json TEXT NOT NULL CHECK (json_valid(capabilities_json)),
  priority INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'online',
  token_hash TEXT NOT NULL,
  last_error TEXT,
  last_heartbeat_at TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  protocol_version INTEGER NOT NULL DEFAULT 3 CHECK (protocol_version = 3),
  max_concurrency INTEGER NOT NULL DEFAULT 1 CHECK (max_concurrency > 0),
  model_gateway_connection_id TEXT,
  UNIQUE (project_id, name, transport),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY (model_gateway_connection_id) REFERENCES model_gateway_connections(id) ON DELETE SET NULL
);

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

CREATE TABLE tasks (
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
  max_attempts INTEGER NOT NULL DEFAULT 8 CHECK (max_attempts > 0),
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
  workflow_id TEXT NOT NULL,
  idempotency_key TEXT,
  execution_generation INTEGER NOT NULL DEFAULT 0 CHECK (execution_generation >= 0),
  available_at TEXT,
  archived_at TEXT,
  terminal_reason_json TEXT CHECK (terminal_reason_json IS NULL OR json_valid(terminal_reason_json)),
  required_protocol_version INTEGER NOT NULL DEFAULT 3 CHECK (required_protocol_version = 3),
  dispatch_mode TEXT NOT NULL DEFAULT 'auto' CHECK (dispatch_mode IN ('auto', 'prefer')),
  requested_gateway_connection_id TEXT,
  mutation_key TEXT,
  mutation_sequence INTEGER,
  FOREIGN KEY (project_id, panel_id) REFERENCES panels(project_id, id) ON DELETE CASCADE,
  FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE,
  FOREIGN KEY (assigned_agent_id) REFERENCES agent_targets(id) ON DELETE SET NULL,
  FOREIGN KEY (requested_gateway_connection_id) REFERENCES model_gateway_connections(id) ON DELETE SET NULL
);

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

CREATE TABLE task_attempts (
  id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL,
  attempt_number INTEGER NOT NULL CHECK (attempt_number > 0),
  execution_generation INTEGER NOT NULL CHECK (execution_generation > 0),
  agent_target_id TEXT,
  status TEXT NOT NULL CHECK (status IN (
    'leased', 'succeeded', 'failed_retryable', 'failed_terminal',
    'invalid_output', 'cancelled', 'interrupted'
  )),
  started_at TEXT NOT NULL,
  heartbeat_at TEXT,
  finished_at TEXT,
  result_json TEXT CHECK (result_json IS NULL OR json_valid(result_json)),
  error_json TEXT CHECK (error_json IS NULL OR json_valid(error_json)),
  execution_token_hash TEXT,
  execution_token_expires_at TEXT,
  staging_session_id TEXT,
  model_gateway_connection_id TEXT,
  executor_snapshot_json TEXT CHECK (executor_snapshot_json IS NULL OR json_valid(executor_snapshot_json)),
  failure_class TEXT CHECK (failure_class IS NULL OR failure_class IN (
    'retryable_channel', 'retryable_output', 'terminal_task', 'cancelled'
  )),
  UNIQUE (task_id, execution_generation),
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
  FOREIGN KEY (agent_target_id) REFERENCES agent_targets(id) ON DELETE SET NULL,
  FOREIGN KEY (model_gateway_connection_id) REFERENCES model_gateway_connections(id) ON DELETE SET NULL
);

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

CREATE TABLE agent_operations (
  id TEXT PRIMARY KEY NOT NULL,
  owner_context_id TEXT NOT NULL,
  intent TEXT NOT NULL,
  status TEXT NOT NULL,
  project_id TEXT NOT NULL,
  panel_id TEXT NOT NULL,
  guide_id TEXT,
  protocol_version INTEGER NOT NULL DEFAULT 2 CHECK (protocol_version = 2),
  target_json TEXT NOT NULL CHECK (json_valid(target_json)),
  input_json TEXT NOT NULL CHECK (json_valid(input_json)),
  result_json TEXT CHECK (result_json IS NULL OR json_valid(result_json)),
  error_json TEXT CHECK (error_json IS NULL OR json_valid(error_json)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT,
  FOREIGN KEY (project_id, panel_id) REFERENCES panels(project_id, id) ON DELETE CASCADE
);

CREATE TABLE content_resources (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL,
  panel_id TEXT,
  resource_kind TEXT NOT NULL CHECK (resource_kind IN (
    'wiki_markdown', 'wiki_space', 'generated_document', 'writing_skill'
  )),
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

CREATE TABLE content_objects (
  hash TEXT PRIMARY KEY NOT NULL,
  size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
  storage_ref TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL
);

CREATE TABLE content_revisions (
  id TEXT PRIMARY KEY NOT NULL,
  content_resource_id TEXT NOT NULL,
  parent_revision_id TEXT,
  revision_number INTEGER NOT NULL CHECK (revision_number > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_hash TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'prunable', 'pruned')),
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

CREATE TABLE task_staging_sessions (
  id TEXT PRIMARY KEY NOT NULL,
  task_id TEXT NOT NULL,
  attempt_id TEXT NOT NULL UNIQUE,
  execution_generation INTEGER NOT NULL CHECK (execution_generation > 0),
  status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'prepared', 'committed', 'abandoned')),
  total_bytes INTEGER NOT NULL DEFAULT 0 CHECK (total_bytes >= 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  committed_at TEXT,
  abandoned_at TEXT,
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
  FOREIGN KEY (attempt_id) REFERENCES task_attempts(id) ON DELETE CASCADE
);

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

CREATE TABLE content_pins (
  task_id TEXT NOT NULL,
  revision_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY (task_id, revision_id),
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
  FOREIGN KEY (revision_id) REFERENCES content_revisions(id) ON DELETE CASCADE
);

CREATE INDEX projects_updated_at_idx ON projects(updated_at DESC, id ASC);
CREATE INDEX panels_project_kind_idx ON panels(project_id, kind ASC);
CREATE INDEX change_scopes_project_revision_idx ON change_scopes(project_id, revision);
CREATE UNIQUE INDEX model_gateway_local_provider_unique_idx
  ON model_gateway_connections(provider_id) WHERE transport = 'local_cli';
CREATE INDEX model_gateway_connections_transport_idx
  ON model_gateway_connections(transport, enabled, updated_at DESC);
CREATE INDEX model_gateway_connections_dispatch_idx
  ON model_gateway_connections(transport, enabled, priority DESC, id ASC);
CREATE INDEX agent_targets_project_status_idx
  ON agent_targets(project_id, status, priority DESC, last_heartbeat_at DESC);
CREATE UNIQUE INDEX agent_targets_gateway_connection_unique_idx
  ON agent_targets(project_id, model_gateway_connection_id)
  WHERE model_gateway_connection_id IS NOT NULL;
CREATE INDEX workflows_project_updated_idx ON workflows(project_id, updated_at DESC, id);
CREATE INDEX tasks_project_status_idx ON tasks(project_id, status, updated_at DESC);
CREATE INDEX tasks_project_capability_idx ON tasks(project_id, capability, status, retry_after);
CREATE INDEX tasks_lease_idx ON tasks(lease_expires_at, status);
CREATE UNIQUE INDEX tasks_active_idempotency_idx
  ON tasks(project_id, idempotency_key)
  WHERE idempotency_key IS NOT NULL AND archived_at IS NULL
    AND status IN ('waiting', 'queued', 'reserved', 'running', 'claimed', 'converting', 'indexing');
CREATE INDEX tasks_workflow_idx ON tasks(workflow_id, created_at, id);
CREATE INDEX tasks_ready_idx ON tasks(project_id, status, available_at, archived_at);
CREATE INDEX tasks_requested_gateway_connection_idx
  ON tasks(project_id, requested_gateway_connection_id, status);
CREATE INDEX tasks_mutation_lane_idx
  ON tasks(project_id, mutation_key, mutation_sequence, status);
CREATE INDEX task_dependencies_prerequisite_idx
  ON task_dependencies(prerequisite_task_id, task_id);
CREATE INDEX task_inputs_resource_idx ON task_inputs(resource_kind, resource_id, task_id);
CREATE INDEX task_attempts_task_started_idx ON task_attempts(task_id, started_at DESC);
CREATE INDEX task_attempts_task_channel_idx
  ON task_attempts(task_id, model_gateway_connection_id, attempt_number);
CREATE INDEX task_events_task_created_idx ON task_events(task_id, id DESC);
CREATE INDEX task_events_workflow_created_idx ON task_events(workflow_id, id DESC);
CREATE INDEX agent_routes_target_idx ON agent_routes(agent_target_id);
CREATE INDEX agent_operations_owner_status_idx
  ON agent_operations(owner_context_id, status, updated_at DESC);
CREATE INDEX agent_operations_target_idx
  ON agent_operations(project_id, panel_id, updated_at DESC);
CREATE INDEX content_resources_panel_kind_idx
  ON content_resources(project_id, panel_id, resource_kind, updated_at DESC);
CREATE INDEX content_revisions_resource_idx
  ON content_revisions(content_resource_id, revision_number DESC);
CREATE INDEX content_revisions_prunable_idx ON content_revisions(status, prunable_at);
CREATE INDEX content_revision_files_object_idx ON content_revision_files(object_hash);
CREATE INDEX task_staging_sessions_cleanup_idx
  ON task_staging_sessions(status, updated_at);
CREATE INDEX task_staged_files_object_idx ON task_staged_files(object_hash);

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
