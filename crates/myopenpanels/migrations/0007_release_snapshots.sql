UPDATE resources
SET title = COALESCE(
  (
    SELECT json_extract(release_json, '$.snapshot.title')
    FROM releases
    WHERE releases.resource_id = resources.id
  ),
  title
)
WHERE kind = 'release';

UPDATE resources
SET deleted_at = COALESCE(deleted_at, updated_at)
WHERE kind = 'release'
  AND id IN (
    SELECT resource_id
    FROM releases
    WHERE publication_id IS NULL
  );

CREATE TABLE releases_v7 (
  project_id TEXT NOT NULL,
  resource_id TEXT PRIMARY KEY NOT NULL,
  publication_id TEXT NOT NULL,
  platform_key TEXT NOT NULL,
  source_updated_at TEXT,
  position INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
  snapshot_json TEXT NOT NULL DEFAULT '{}' CHECK (
    json_valid(snapshot_json) AND json_type(snapshot_json) = 'object'
  ),
  result_json TEXT CHECK (
    result_json IS NULL
    OR (json_valid(result_json) AND json_type(result_json) = 'object')
  ),
  FOREIGN KEY (project_id, resource_id)
    REFERENCES resources(project_id, id) ON DELETE CASCADE,
  FOREIGN KEY (publication_id)
    REFERENCES publications(resource_id) ON DELETE RESTRICT,
  FOREIGN KEY (project_id, publication_id)
    REFERENCES resources(project_id, id) ON DELETE RESTRICT
);

INSERT INTO releases_v7 (
  project_id, resource_id, publication_id, platform_key,
  source_updated_at, position, snapshot_json, result_json
)
SELECT project_id, resource_id, publication_id, platform_key,
       json_extract(release_json, '$.sourceUpdatedAt'),
       position,
       CASE
         WHEN json_type(release_json, '$.snapshot') = 'object'
         THEN json_remove(json_extract(release_json, '$.snapshot'), '$.title')
         ELSE '{}'
       END,
       result_json
FROM releases
WHERE publication_id IS NOT NULL;

DROP TABLE releases;
ALTER TABLE releases_v7 RENAME TO releases;

CREATE INDEX idx_releases_project_position
  ON releases(project_id, position, resource_id);
