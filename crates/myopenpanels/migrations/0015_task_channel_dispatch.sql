ALTER TABLE agent_targets
  ADD COLUMN model_gateway_connection_id TEXT
  REFERENCES model_gateway_connections(id) ON DELETE SET NULL;

UPDATE agent_targets
SET model_gateway_connection_id = CASE
  WHEN name LIKE 'model-gateway:%:codex' THEN 'local-cli:codex'
  WHEN name LIKE 'model-gateway:%:hermes' THEN 'local-cli:hermes'
END
WHERE model_gateway_connection_id IS NULL
  AND transport = 'command'
  AND (
    name LIKE 'model-gateway:%:codex'
    OR name LIKE 'model-gateway:%:hermes'
  );

CREATE UNIQUE INDEX agent_targets_gateway_connection_unique_idx
  ON agent_targets(project_id, model_gateway_connection_id)
  WHERE model_gateway_connection_id IS NOT NULL;

ALTER TABLE task_attempts
  ADD COLUMN model_gateway_connection_id TEXT
  REFERENCES model_gateway_connections(id) ON DELETE SET NULL;

UPDATE task_attempts
SET model_gateway_connection_id = (
  SELECT model_gateway_connection_id
  FROM agent_targets
  WHERE agent_targets.id = task_attempts.agent_target_id
)
WHERE model_gateway_connection_id IS NULL;

ALTER TABLE task_attempts
  ADD COLUMN executor_snapshot_json TEXT
  CHECK (executor_snapshot_json IS NULL OR json_valid(executor_snapshot_json));

ALTER TABLE task_attempts
  ADD COLUMN failure_class TEXT
  CHECK (
    failure_class IS NULL OR failure_class IN (
      'retryable_channel', 'retryable_output', 'terminal_task', 'cancelled'
    )
  );

CREATE INDEX task_attempts_task_channel_idx
  ON task_attempts(task_id, model_gateway_connection_id, attempt_number);

ALTER TABLE tasks
  ADD COLUMN dispatch_mode TEXT NOT NULL DEFAULT 'auto'
  CHECK (dispatch_mode IN ('auto', 'prefer', 'only'));

ALTER TABLE tasks
  ADD COLUMN requested_gateway_connection_id TEXT
  REFERENCES model_gateway_connections(id) ON DELETE SET NULL;

CREATE INDEX tasks_requested_gateway_connection_idx
  ON tasks(project_id, requested_gateway_connection_id, status);

-- Eight executions allow a default route of four channels to complete two rounds.
-- Explicitly configured retry budgets are preserved.
UPDATE tasks
SET max_attempts = 8
WHERE max_attempts = 3
  AND status IN (
    'waiting', 'queued', 'failed', 'reserved', 'running', 'claimed',
    'converting', 'indexing'
  );
