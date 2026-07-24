PRAGMA defer_foreign_keys = ON;

UPDATE publications
SET source_document_id = NULL
WHERE source_document_id IS NOT NULL
  AND NOT EXISTS (
    SELECT 1
    FROM resources owner
    JOIN resources source ON source.id = publications.source_document_id
    JOIN documents document ON document.resource_id = source.id
    WHERE owner.id = publications.resource_id
      AND owner.project_id = source.project_id
  );

UPDATE publications
SET cover_document_id = NULL
WHERE cover_document_id IS NOT NULL
  AND NOT EXISTS (
    SELECT 1
    FROM resources owner
    JOIN resources cover ON cover.id = publications.cover_document_id
    JOIN documents document ON document.resource_id = cover.id
    WHERE owner.id = publications.resource_id
      AND owner.project_id = cover.project_id
  );

UPDATE resources
SET deleted_at = COALESCE(deleted_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE id IN (
  SELECT release.resource_id
  FROM releases release
  JOIN resources owner ON owner.id = release.resource_id
  LEFT JOIN resources publication ON publication.id = release.publication_id
  LEFT JOIN publications typed_publication
    ON typed_publication.resource_id = publication.id
  WHERE publication.id IS NULL
     OR typed_publication.resource_id IS NULL
     OR owner.project_id <> publication.project_id
);

UPDATE tasks
SET status = 'cancelled',
    error_json = json_object(
      'code', 'invalid_project_relationship',
      'relationship', 'dependsOnTaskId',
      'prerequisiteTaskId', depends_on_task_id
    ),
    execution_generation = execution_generation + 1,
    execution_token_hash = NULL,
    lease_owner = NULL,
    lease_expires_at = NULL,
    heartbeat_at = NULL,
    current_runner_key = NULL,
    completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE status IN ('queued', 'running')
  AND depends_on_task_id IS NOT NULL
  AND NOT EXISTS (
    SELECT 1
    FROM tasks dependency
    WHERE dependency.id = tasks.depends_on_task_id
      AND dependency.project_id = tasks.project_id
  );

WITH RECURSIVE dependency_chain(start_id, project_id, id) AS (
  SELECT task.id, task.project_id, task.depends_on_task_id
  FROM tasks task
  WHERE task.depends_on_task_id IS NOT NULL
  UNION
  SELECT dependency_chain.start_id, dependency_chain.project_id,
         dependency.depends_on_task_id
  FROM dependency_chain
  JOIN tasks dependency
    ON dependency.project_id = dependency_chain.project_id
   AND dependency.id = dependency_chain.id
  WHERE dependency.depends_on_task_id IS NOT NULL
)
UPDATE tasks
SET status = CASE
      WHEN status IN ('queued', 'running') THEN 'cancelled'
      ELSE status
    END,
    error_json = CASE
      WHEN status IN ('queued', 'running') THEN json_object(
        'code', 'invalid_task_dependency_cycle'
      )
      ELSE error_json
    END,
    execution_generation = CASE
      WHEN status IN ('queued', 'running') THEN execution_generation + 1
      ELSE execution_generation
    END,
    execution_token_hash = CASE
      WHEN status IN ('queued', 'running') THEN NULL
      ELSE execution_token_hash
    END,
    lease_owner = CASE
      WHEN status IN ('queued', 'running') THEN NULL
      ELSE lease_owner
    END,
    lease_expires_at = CASE
      WHEN status IN ('queued', 'running') THEN NULL
      ELSE lease_expires_at
    END,
    heartbeat_at = CASE
      WHEN status IN ('queued', 'running') THEN NULL
      ELSE heartbeat_at
    END,
    current_runner_key = CASE
      WHEN status IN ('queued', 'running') THEN NULL
      ELSE current_runner_key
    END,
    completed_at = CASE
      WHEN status IN ('queued', 'running')
        THEN strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
      ELSE completed_at
    END,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    depends_on_task_id = NULL
WHERE id IN (
  SELECT start_id
  FROM dependency_chain
  WHERE id = start_id
);

CREATE TABLE publications_v6 (
  project_id TEXT NOT NULL,
  resource_id TEXT PRIMARY KEY NOT NULL,
  source_document_id TEXT,
  cover_document_id TEXT,
  config_version INTEGER NOT NULL DEFAULT 0 CHECK (config_version >= 0),
  selected_title TEXT NOT NULL DEFAULT '',
  position INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
  config_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(config_json)),
  FOREIGN KEY (project_id, resource_id)
    REFERENCES resources(project_id, id) ON DELETE CASCADE,
  FOREIGN KEY (source_document_id)
    REFERENCES documents(resource_id) ON DELETE SET NULL,
  FOREIGN KEY (cover_document_id)
    REFERENCES documents(resource_id) ON DELETE SET NULL,
  FOREIGN KEY (project_id, source_document_id)
    REFERENCES resources(project_id, id) DEFERRABLE INITIALLY DEFERRED,
  FOREIGN KEY (project_id, cover_document_id)
    REFERENCES resources(project_id, id) DEFERRABLE INITIALLY DEFERRED
);

INSERT INTO publications_v6 (
  project_id, resource_id, source_document_id, cover_document_id,
  config_version, selected_title, position, config_json
)
SELECT resource.project_id, publication.resource_id,
       publication.source_document_id, publication.cover_document_id,
       publication.config_version, publication.selected_title,
       publication.position, publication.config_json
FROM publications publication
JOIN resources resource ON resource.id = publication.resource_id;

CREATE TABLE releases_v6 (
  project_id TEXT NOT NULL,
  resource_id TEXT PRIMARY KEY NOT NULL,
  publication_id TEXT,
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
  FOREIGN KEY (project_id, resource_id)
    REFERENCES resources(project_id, id) ON DELETE CASCADE,
  FOREIGN KEY (publication_id)
    REFERENCES publications_v6(resource_id) DEFERRABLE INITIALLY DEFERRED,
  FOREIGN KEY (project_id, publication_id)
    REFERENCES resources(project_id, id) DEFERRABLE INITIALLY DEFERRED
);

INSERT INTO releases_v6 (
  project_id, resource_id, publication_id, platform_key, request_key,
  published_revision_id, remote_ref, remote_url, position, release_json,
  result_json, published_at, archived_at
)
SELECT owner.project_id, release.resource_id,
       CASE
         WHEN publication.project_id = owner.project_id
           AND typed_publication.resource_id IS NOT NULL
         THEN release.publication_id
         ELSE NULL
       END,
       release.platform_key, release.request_key, release.published_revision_id,
       release.remote_ref, release.remote_url, release.position,
       release.release_json, release.result_json, release.published_at,
       release.archived_at
FROM releases release
JOIN resources owner ON owner.id = release.resource_id
LEFT JOIN resources publication ON publication.id = release.publication_id
LEFT JOIN publications typed_publication
  ON typed_publication.resource_id = publication.id;

CREATE TABLE tasks_v6 (
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
    json_valid(attempt_history_json)
    AND json_type(attempt_history_json) = 'array'
  ),
  current_runner_key TEXT,
  available_at TEXT NOT NULL,
  execution_generation INTEGER NOT NULL DEFAULT 0
    CHECK (execution_generation >= 0),
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
  FOREIGN KEY (project_id)
    REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY (project_id, origin_panel_id)
    REFERENCES panels(project_id, id) DEFERRABLE INITIALLY DEFERRED,
  FOREIGN KEY (project_id, depends_on_task_id)
    REFERENCES tasks_v6(project_id, id) DEFERRABLE INITIALLY DEFERRED,
  FOREIGN KEY (project_id, retry_of_task_id)
    REFERENCES tasks_v6(project_id, id) DEFERRABLE INITIALLY DEFERRED,
  CHECK (depends_on_task_id IS NULL OR depends_on_task_id <> id),
  CHECK (retry_of_task_id IS NULL OR retry_of_task_id <> id)
);

INSERT INTO tasks_v6 (
  id, project_id, origin_panel_id, handler_key, status, target_ref,
  input_json, source_json, result_json, error_json, depends_on_task_id,
  retry_of_task_id, mutation_key, attempt_count, attempt_history_json,
  current_runner_key, available_at, execution_generation,
  execution_token_hash, lease_owner, lease_expires_at, heartbeat_at,
  idempotency_key, created_at, updated_at, completed_at, archived_at
)
SELECT task.id, task.project_id,
       CASE
         WHEN task.origin_panel_id IS NULL OR EXISTS (
           SELECT 1 FROM panels panel
           WHERE panel.project_id = task.project_id
             AND panel.id = task.origin_panel_id
         )
         THEN task.origin_panel_id
         ELSE NULL
       END,
       task.handler_key, task.status, task.target_ref, task.input_json,
       task.source_json, task.result_json, task.error_json,
       CASE
         WHEN task.depends_on_task_id IS NULL OR EXISTS (
           SELECT 1 FROM tasks dependency
           WHERE dependency.project_id = task.project_id
             AND dependency.id = task.depends_on_task_id
         )
         THEN task.depends_on_task_id
         ELSE NULL
       END,
       CASE
         WHEN task.retry_of_task_id IS NULL OR EXISTS (
           SELECT 1 FROM tasks retried
           WHERE retried.project_id = task.project_id
             AND retried.id = task.retry_of_task_id
         )
         THEN task.retry_of_task_id
         ELSE NULL
       END,
       task.mutation_key, task.attempt_count, task.attempt_history_json,
       task.current_runner_key, task.available_at, task.execution_generation,
       task.execution_token_hash, task.lease_owner, task.lease_expires_at,
       task.heartbeat_at, task.idempotency_key, task.created_at,
       task.updated_at, task.completed_at, task.archived_at
FROM tasks task;

CREATE TABLE wiki_source_ingestions_v6 (
  project_id TEXT NOT NULL,
  wiki_space_id TEXT NOT NULL,
  document_id TEXT NOT NULL,
  processed_document_version INTEGER NOT NULL CHECK (
    processed_document_version > 0
  ),
  wiki_version_at_processing INTEGER NOT NULL DEFAULT 0 CHECK (
    wiki_version_at_processing >= 0
  ),
  disposition TEXT NOT NULL CHECK (
    disposition IN ('included', 'already_covered', 'excluded')
  ),
  task_id TEXT NOT NULL,
  reason_code TEXT,
  summary TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (project_id, wiki_space_id, document_id),
  FOREIGN KEY (wiki_space_id)
    REFERENCES wiki_spaces(resource_id) ON DELETE CASCADE,
  FOREIGN KEY (document_id)
    REFERENCES documents(resource_id) ON DELETE CASCADE,
  FOREIGN KEY (project_id, wiki_space_id)
    REFERENCES resources(project_id, id) ON DELETE CASCADE,
  FOREIGN KEY (project_id, document_id)
    REFERENCES resources(project_id, id) ON DELETE CASCADE,
  FOREIGN KEY (project_id, task_id)
    REFERENCES tasks_v6(project_id, id) ON DELETE RESTRICT
);

INSERT INTO wiki_source_ingestions_v6 (
  project_id, wiki_space_id, document_id, processed_document_version,
  wiki_version_at_processing, disposition, task_id, reason_code, summary,
  created_at, updated_at
)
SELECT project_id, wiki_space_id, document_id, processed_document_version,
       wiki_version_at_processing, disposition, task_id, reason_code, summary,
       created_at, updated_at
FROM wiki_source_ingestions;

CREATE TABLE task_resources_v6 (
  project_id TEXT NOT NULL,
  task_id TEXT NOT NULL,
  resource_id TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('primary', 'input', 'output', 'context')),
  captured_version INTEGER CHECK (
    captured_version IS NULL OR captured_version >= 0
  ),
  created_at TEXT NOT NULL,
  PRIMARY KEY (project_id, task_id, resource_id, role),
  FOREIGN KEY (project_id, task_id)
    REFERENCES tasks_v6(project_id, id) ON DELETE CASCADE,
  FOREIGN KEY (project_id, resource_id)
    REFERENCES resources(project_id, id) ON DELETE CASCADE
);

INSERT INTO task_resources_v6 (
  project_id, task_id, resource_id, role, captured_version, created_at
)
SELECT task.project_id, link.task_id, link.resource_id, link.role,
       link.captured_version, link.created_at
FROM task_resources link
JOIN tasks task ON task.id = link.task_id
JOIN resources resource ON resource.id = link.resource_id
WHERE task.project_id = resource.project_id;

CREATE TABLE change_scopes_v6 (
  scope_key TEXT PRIMARY KEY NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN (
    'catalog', 'project', 'panel_ui', 'panel_selection',
    'resource', 'tasks', 'settings'
  )),
  project_id TEXT,
  panel_id TEXT,
  resource_id TEXT,
  revision INTEGER NOT NULL CHECK (revision >= 0),
  updated_at TEXT NOT NULL,
  CHECK (resource_id IS NULL OR project_id IS NOT NULL),
  FOREIGN KEY (project_id)
    REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY (project_id, panel_id)
    REFERENCES panels(project_id, id) ON DELETE CASCADE,
  FOREIGN KEY (project_id, resource_id)
    REFERENCES resources(project_id, id) ON DELETE CASCADE
      DEFERRABLE INITIALLY DEFERRED
);

INSERT INTO change_scopes_v6 (
  scope_key, kind, project_id, panel_id, resource_id, revision, updated_at
)
SELECT scope.scope_key, scope.kind, scope.project_id, scope.panel_id,
       scope.resource_id, scope.revision, scope.updated_at
FROM change_scopes scope
WHERE scope.resource_id IS NULL
   OR EXISTS (
     SELECT 1 FROM resources resource
     WHERE resource.id = scope.resource_id
       AND resource.project_id = scope.project_id
   );

DROP TABLE task_resources;
DROP TABLE wiki_source_ingestions;
DROP TABLE releases;
DROP TABLE publications;
DROP TABLE change_scopes;
DROP TABLE tasks;

ALTER TABLE publications_v6 RENAME TO publications;
ALTER TABLE releases_v6 RENAME TO releases;
ALTER TABLE tasks_v6 RENAME TO tasks;
ALTER TABLE wiki_source_ingestions_v6 RENAME TO wiki_source_ingestions;
ALTER TABLE task_resources_v6 RENAME TO task_resources;
ALTER TABLE change_scopes_v6 RENAME TO change_scopes;

CREATE INDEX publications_position_idx
  ON publications(project_id, position, resource_id);
CREATE INDEX releases_publication_idx
  ON releases(project_id, publication_id, position, resource_id);
CREATE INDEX change_scopes_project_revision_idx
  ON change_scopes(project_id, revision);
CREATE INDEX change_scopes_resource_revision_idx
  ON change_scopes(project_id, resource_id, revision);
CREATE INDEX tasks_project_status_idx
  ON tasks(project_id, status, updated_at DESC);
CREATE INDEX tasks_origin_panel_idx
  ON tasks(project_id, origin_panel_id, created_at DESC);
CREATE INDEX tasks_ready_idx
  ON tasks(status, available_at, archived_at, created_at);
CREATE INDEX tasks_dependency_idx
  ON tasks(project_id, depends_on_task_id, status);
CREATE INDEX tasks_mutation_idx
  ON tasks(project_id, mutation_key, status, created_at);
CREATE INDEX tasks_lease_idx
  ON tasks(status, lease_expires_at);
CREATE INDEX task_resources_resource_idx
  ON task_resources(project_id, resource_id, role, task_id);
CREATE UNIQUE INDEX tasks_active_idempotency_idx
  ON tasks(project_id, idempotency_key)
  WHERE idempotency_key IS NOT NULL AND archived_at IS NULL
    AND status IN ('queued', 'running');

CREATE TRIGGER tasks_dependency_cycle_insert
BEFORE INSERT ON tasks
WHEN NEW.depends_on_task_id IS NOT NULL
BEGIN
  SELECT CASE WHEN EXISTS (
    WITH RECURSIVE ancestors(id) AS (
      SELECT NEW.depends_on_task_id
      UNION
      SELECT task.depends_on_task_id
      FROM tasks task
      JOIN ancestors ON ancestors.id = task.id
      WHERE task.project_id = NEW.project_id
        AND task.depends_on_task_id IS NOT NULL
    )
    SELECT 1 FROM ancestors WHERE id = NEW.id
  ) THEN RAISE(ABORT, 'task dependency cycle') END;
END;

CREATE TRIGGER tasks_dependency_cycle_update
BEFORE UPDATE OF depends_on_task_id, project_id ON tasks
WHEN NEW.depends_on_task_id IS NOT NULL
BEGIN
  SELECT CASE WHEN EXISTS (
    WITH RECURSIVE ancestors(id) AS (
      SELECT NEW.depends_on_task_id
      UNION
      SELECT task.depends_on_task_id
      FROM tasks task
      JOIN ancestors ON ancestors.id = task.id
      WHERE task.project_id = NEW.project_id
        AND task.depends_on_task_id IS NOT NULL
    )
    SELECT 1 FROM ancestors WHERE id = NEW.id
  ) THEN RAISE(ABORT, 'task dependency cycle') END;
END;
