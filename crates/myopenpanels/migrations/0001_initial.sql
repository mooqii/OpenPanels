CREATE TABLE storage_meta (
  id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
  database_id TEXT NOT NULL UNIQUE,
  schema_fingerprint TEXT NOT NULL,
  global_revision INTEGER NOT NULL DEFAULT 0 CHECK (global_revision >= 0)
);

INSERT INTO storage_meta (id, database_id, schema_fingerprint, global_revision)
VALUES (1, lower(hex(randomblob(16))), '', 0);

CREATE TABLE projects (
  id TEXT PRIMARY KEY NOT NULL,
  title TEXT NOT NULL,
  root_path TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE panels (
  project_id TEXT NOT NULL,
  id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (length(trim(kind)) > 0),
  position INTEGER NOT NULL CHECK (position >= 0),
  ui_format_version INTEGER NOT NULL DEFAULT 1 CHECK (ui_format_version > 0),
  ui_state_revision INTEGER NOT NULL DEFAULT 0 CHECK (ui_state_revision >= 0),
  ui_state_hash TEXT NOT NULL DEFAULT '',
  ui_state_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(ui_state_json)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (project_id, id),
  UNIQUE (project_id, kind),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE TABLE panel_selections (
  project_id TEXT NOT NULL,
  panel_id TEXT NOT NULL,
  revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0),
  content_hash TEXT NOT NULL,
  selection_json TEXT NOT NULL CHECK (json_valid(selection_json)),
  updated_at TEXT NOT NULL,
  PRIMARY KEY (project_id, panel_id),
  FOREIGN KEY (project_id, panel_id) REFERENCES panels(project_id, id) ON DELETE CASCADE
);

CREATE TABLE settings (
  key TEXT PRIMARY KEY NOT NULL,
  value_json TEXT NOT NULL CHECK (json_valid(value_json)),
  updated_at TEXT NOT NULL
);

CREATE TABLE resources (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (length(trim(kind)) > 0),
  title TEXT NOT NULL DEFAULT '',
  revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  deleted_at TEXT,
  UNIQUE (project_id, id),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE TABLE documents (
  resource_id TEXT PRIMARY KEY NOT NULL,
  document_kind TEXT NOT NULL CHECK (length(trim(document_kind)) > 0),
  media_type TEXT NOT NULL DEFAULT 'application/octet-stream',
  source TEXT NOT NULL DEFAULT 'user',
  original_file_name TEXT NOT NULL DEFAULT '',
  original_revision_id TEXT,
  active_revision_id TEXT,
  content_version INTEGER NOT NULL DEFAULT 0 CHECK (content_version >= 0),
  content_hash TEXT NOT NULL DEFAULT '',
  character_count INTEGER CHECK (character_count IS NULL OR character_count >= 0),
  position INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
  metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
  FOREIGN KEY (resource_id) REFERENCES resources(id) ON DELETE CASCADE
);

CREATE TABLE wiki_spaces (
  resource_id TEXT PRIMARY KEY NOT NULL,
  active_revision_id TEXT,
  content_version INTEGER NOT NULL DEFAULT 0 CHECK (content_version >= 0),
  content_hash TEXT NOT NULL DEFAULT '',
  selected_skill_id TEXT,
  position INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
  metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
  FOREIGN KEY (resource_id) REFERENCES resources(id) ON DELETE CASCADE
);

CREATE TABLE wiki_source_ingestions (
  project_id TEXT NOT NULL,
  wiki_space_id TEXT NOT NULL,
  document_id TEXT NOT NULL,
  processed_document_version INTEGER NOT NULL CHECK (processed_document_version > 0),
  wiki_version_at_processing INTEGER NOT NULL DEFAULT 0 CHECK (wiki_version_at_processing >= 0),
  disposition TEXT NOT NULL CHECK (disposition IN ('included', 'already_covered', 'excluded')),
  task_id TEXT NOT NULL,
  reason_code TEXT,
  summary TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (project_id, wiki_space_id, document_id),
  FOREIGN KEY (wiki_space_id) REFERENCES wiki_spaces(resource_id) ON DELETE CASCADE,
  FOREIGN KEY (document_id) REFERENCES documents(resource_id) ON DELETE CASCADE,
  FOREIGN KEY (project_id, wiki_space_id) REFERENCES resources(project_id, id) ON DELETE CASCADE,
  FOREIGN KEY (project_id, document_id) REFERENCES resources(project_id, id) ON DELETE CASCADE,
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE RESTRICT,
  FOREIGN KEY (project_id, task_id) REFERENCES tasks(project_id, id) ON DELETE RESTRICT
);

CREATE TABLE canvas_documents (
  resource_id TEXT PRIMARY KEY NOT NULL,
  format_version INTEGER NOT NULL DEFAULT 1 CHECK (format_version > 0),
  state_revision INTEGER NOT NULL DEFAULT 0 CHECK (state_revision >= 0),
  state_hash TEXT NOT NULL DEFAULT '',
  state_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(state_json)),
  FOREIGN KEY (resource_id) REFERENCES resources(id) ON DELETE CASCADE
);

CREATE TABLE assets (
  resource_id TEXT PRIMARY KEY NOT NULL,
  media_type TEXT NOT NULL DEFAULT 'application/octet-stream',
  file_name TEXT NOT NULL DEFAULT '',
  active_revision_id TEXT,
  content_version INTEGER NOT NULL DEFAULT 0 CHECK (content_version >= 0),
  content_hash TEXT NOT NULL DEFAULT '',
  byte_size INTEGER CHECK (byte_size IS NULL OR byte_size >= 0),
  width INTEGER CHECK (width IS NULL OR width > 0),
  height INTEGER CHECK (height IS NULL OR height > 0),
  metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
  FOREIGN KEY (resource_id) REFERENCES resources(id) ON DELETE CASCADE
);

CREATE TABLE publications (
  resource_id TEXT PRIMARY KEY NOT NULL,
  source_document_id TEXT,
  cover_document_id TEXT,
  active_revision_id TEXT,
  content_version INTEGER NOT NULL DEFAULT 0 CHECK (content_version >= 0),
  content_hash TEXT NOT NULL DEFAULT '',
  selected_title TEXT NOT NULL DEFAULT '',
  position INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
  config_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(config_json)),
  FOREIGN KEY (resource_id) REFERENCES resources(id) ON DELETE CASCADE,
  FOREIGN KEY (source_document_id) REFERENCES documents(resource_id) ON DELETE SET NULL,
  FOREIGN KEY (cover_document_id) REFERENCES documents(resource_id) ON DELETE SET NULL
);

CREATE TABLE releases (
  resource_id TEXT PRIMARY KEY NOT NULL,
  publication_id TEXT NOT NULL,
  platform_key TEXT NOT NULL,
  request_key TEXT,
  published_revision_id TEXT,
  remote_ref TEXT,
  remote_url TEXT,
  position INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
  release_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(release_json)),
  result_json TEXT CHECK (result_json IS NULL OR json_valid(result_json)),
  published_at TEXT,
  archived_at TEXT,
  FOREIGN KEY (resource_id) REFERENCES resources(id) ON DELETE CASCADE,
  FOREIGN KEY (publication_id) REFERENCES publications(resource_id) ON DELETE RESTRICT
);

CREATE TABLE tasks (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL,
  origin_panel_id TEXT,
  handler_key TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'queued' CHECK (status IN (
    'queued', 'running', 'succeeded', 'failed', 'cancelled', 'superseded'
  )),
  target_ref TEXT NOT NULL DEFAULT '',
  input_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(input_json)),
  source_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(source_json)),
  result_json TEXT CHECK (result_json IS NULL OR json_valid(result_json)),
  error_json TEXT CHECK (error_json IS NULL OR json_valid(error_json)),
  depends_on_task_id TEXT,
  retry_of_task_id TEXT,
  mutation_key TEXT,
  attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
  attempt_history_json TEXT NOT NULL DEFAULT '[]' CHECK (
    json_valid(attempt_history_json) AND json_type(attempt_history_json) = 'array'
  ),
  current_runner_key TEXT,
  available_at TEXT NOT NULL,
  execution_generation INTEGER NOT NULL DEFAULT 0 CHECK (execution_generation >= 0),
  execution_token_hash TEXT,
  lease_owner TEXT,
  lease_expires_at TEXT,
  heartbeat_at TEXT,
  idempotency_key TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT,
  archived_at TEXT,
  UNIQUE (project_id, id),
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY (depends_on_task_id) REFERENCES tasks(id) ON DELETE RESTRICT,
  FOREIGN KEY (retry_of_task_id) REFERENCES tasks(id) ON DELETE SET NULL,
  CHECK (depends_on_task_id IS NULL OR depends_on_task_id <> id),
  CHECK (retry_of_task_id IS NULL OR retry_of_task_id <> id)
);

CREATE TABLE task_resources (
  task_id TEXT NOT NULL,
  resource_id TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('primary', 'input', 'output', 'context')),
  captured_version INTEGER CHECK (captured_version IS NULL OR captured_version >= 0),
  created_at TEXT NOT NULL,
  PRIMARY KEY (task_id, resource_id, role),
  FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
  FOREIGN KEY (resource_id) REFERENCES resources(id) ON DELETE CASCADE
);

CREATE TABLE change_scopes (
  scope_key TEXT PRIMARY KEY NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN (
    'catalog', 'project', 'panel_ui', 'panel_selection', 'resource', 'tasks', 'settings'
  )),
  project_id TEXT,
  panel_id TEXT,
  resource_id TEXT,
  revision INTEGER NOT NULL CHECK (revision >= 0),
  updated_at TEXT NOT NULL,
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY (project_id, panel_id) REFERENCES panels(project_id, id) ON DELETE CASCADE,
  FOREIGN KEY (resource_id) REFERENCES resources(id) ON DELETE CASCADE
    DEFERRABLE INITIALLY DEFERRED
);

CREATE INDEX projects_updated_at_idx ON projects(updated_at DESC, id ASC);
CREATE INDEX panels_project_kind_idx ON panels(project_id, kind ASC);
CREATE UNIQUE INDEX panels_project_position_idx ON panels(project_id, position);
CREATE INDEX resources_project_kind_idx
  ON resources(project_id, kind, deleted_at, updated_at DESC);
CREATE INDEX documents_kind_position_idx ON documents(document_kind, position, resource_id);
CREATE INDEX assets_media_type_idx ON assets(media_type, resource_id);
CREATE INDEX wiki_spaces_position_idx ON wiki_spaces(position, resource_id);
CREATE INDEX publications_position_idx ON publications(position, resource_id);
CREATE INDEX releases_publication_idx ON releases(publication_id, position, resource_id);
CREATE INDEX change_scopes_project_revision_idx ON change_scopes(project_id, revision);
CREATE INDEX change_scopes_resource_revision_idx ON change_scopes(resource_id, revision);
CREATE INDEX tasks_project_status_idx ON tasks(project_id, status, updated_at DESC);
CREATE INDEX tasks_origin_panel_idx ON tasks(project_id, origin_panel_id, created_at DESC);
CREATE INDEX tasks_ready_idx ON tasks(status, available_at, archived_at, created_at);
CREATE INDEX tasks_dependency_idx ON tasks(depends_on_task_id, status);
CREATE INDEX tasks_mutation_idx ON tasks(project_id, mutation_key, status, created_at);
CREATE INDEX tasks_lease_idx ON tasks(status, lease_expires_at);
CREATE INDEX task_resources_resource_idx ON task_resources(resource_id, role, task_id);
CREATE UNIQUE INDEX tasks_active_idempotency_idx
  ON tasks(project_id, idempotency_key)
  WHERE idempotency_key IS NOT NULL AND archived_at IS NULL
    AND status IN ('queued', 'running');
