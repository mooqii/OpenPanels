ALTER TABLE resources
  ADD COLUMN active_content_revision_id TEXT;
ALTER TABLE resources
  ADD COLUMN content_version INTEGER NOT NULL DEFAULT 0 CHECK (content_version >= 0);
ALTER TABLE resources
  ADD COLUMN content_manifest_hash TEXT NOT NULL DEFAULT '';
ALTER TABLE resources
  ADD COLUMN content_hash TEXT NOT NULL DEFAULT '';

UPDATE resources
SET content_version = COALESCE(
  (SELECT documents.content_version
   FROM documents
   WHERE documents.resource_id = resources.id),
  (SELECT wiki_spaces.content_version
   FROM wiki_spaces
   WHERE wiki_spaces.resource_id = resources.id),
  (SELECT assets.content_version
   FROM assets
   WHERE assets.resource_id = resources.id),
  0
);

UPDATE resources
SET active_content_revision_id = (
      SELECT 'asset-revision:' || assets.content_version
      FROM assets
      WHERE assets.resource_id = resources.id
    ),
    content_hash = COALESCE((
      SELECT assets.content_hash
      FROM assets
      WHERE assets.resource_id = resources.id
    ), '')
WHERE kind = 'asset';

ALTER TABLE documents RENAME COLUMN original_revision_id TO original_content_ref;
ALTER TABLE documents RENAME COLUMN active_revision_id TO active_content_ref;
ALTER TABLE documents RENAME COLUMN content_version TO logical_content_version;
ALTER TABLE documents DROP COLUMN content_hash;

ALTER TABLE wiki_spaces RENAME COLUMN active_revision_id TO root_ref;
ALTER TABLE wiki_spaces DROP COLUMN content_version;
ALTER TABLE wiki_spaces DROP COLUMN content_hash;

ALTER TABLE assets RENAME COLUMN active_revision_id TO active_file_ref;
ALTER TABLE assets DROP COLUMN content_version;
ALTER TABLE assets DROP COLUMN content_hash;

ALTER TABLE publications RENAME COLUMN content_version TO config_version;
ALTER TABLE publications DROP COLUMN active_revision_id;
ALTER TABLE publications DROP COLUMN content_hash;

CREATE TABLE direct_operations_next (
  id TEXT PRIMARY KEY NOT NULL,
  owner_context_id TEXT NOT NULL,
  intent TEXT NOT NULL CHECK (length(trim(intent)) > 0),
  status TEXT NOT NULL CHECK (status IN ('active', 'completed', 'failed', 'cancelled')),
  project_id TEXT NOT NULL,
  panel_id TEXT NOT NULL,
  target_id TEXT NOT NULL CHECK (length(trim(target_id)) > 0),
  base_revision INTEGER NOT NULL CHECK (base_revision >= 0),
  payload_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload_json)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT,
  FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
  FOREIGN KEY (project_id, panel_id) REFERENCES panels(project_id, id) ON DELETE CASCADE
);

INSERT INTO direct_operations_next (
  id, owner_context_id, intent, status, project_id, panel_id, target_id,
  base_revision, payload_json, created_at, updated_at, completed_at
)
SELECT id, owner_context_id, intent, status, project_id, panel_id, target_id,
       base_revision,
       json_remove(
         operation_json,
         '$.id', '$.ownerContextId', '$.intent', '$.status',
         '$.projectId', '$.panelId', '$.targetId', '$.baseRevision',
         '$.createdAt', '$.updatedAt', '$.completedAt',
         '$.target.placeholderShapeId', '$.target.documentId',
         '$.target.baseContentVersion'
       ),
       created_at, updated_at, completed_at
FROM direct_operations;

DROP TABLE direct_operations;
ALTER TABLE direct_operations_next RENAME TO direct_operations;
CREATE INDEX direct_operations_owner_status_idx
  ON direct_operations(owner_context_id, status, updated_at DESC);
CREATE INDEX direct_operations_target_idx
  ON direct_operations(project_id, panel_id, target_id, updated_at DESC);
