ALTER TABLE tasks ADD COLUMN mutation_key TEXT;
ALTER TABLE tasks ADD COLUMN mutation_sequence INTEGER;

UPDATE tasks
SET status = 'cancelled',
    assigned_agent_id = NULL,
    lease_owner = NULL,
    lease_token_hash = NULL,
    lease_expires_at = NULL,
    last_heartbeat_at = NULL,
    completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    execution_generation = execution_generation + 1,
    terminal_reason_json = json_object('code', 'legacy_task_replaced')
WHERE queue = 'wiki'
  AND type = 'rebuild_wiki_index'
  AND status IN ('waiting', 'queued', 'failed', 'reserved', 'running', 'claimed', 'indexing');

UPDATE tasks
SET mutation_key = 'wiki:' || project_id || ':' || panel_id || ':' ||
    COALESCE(json_extract(source_json, '$.wikiSpaceId'), 'wiki:default')
WHERE queue = 'wiki'
  AND type = 'ingest_markdown_into_wiki';

UPDATE tasks AS current
SET mutation_sequence = (
  SELECT COUNT(*)
  FROM tasks AS prior
  WHERE prior.project_id = current.project_id
    AND prior.mutation_key = current.mutation_key
    AND (
      prior.created_at < current.created_at
      OR (prior.created_at = current.created_at AND prior.id <= current.id)
    )
)
WHERE current.mutation_key IS NOT NULL;

CREATE INDEX tasks_mutation_lane_idx
  ON tasks(project_id, mutation_key, mutation_sequence, status);
