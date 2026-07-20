UPDATE task_attempts
SET status = 'cancelled',
    finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    error_json = json_object('code', 'poll_transport_removed'),
    failure_class = 'cancelled'
WHERE status = 'leased'
  AND agent_target_id IN (
    SELECT id FROM agent_targets WHERE transport = 'poll'
  );

INSERT INTO task_events (
  task_id, workflow_id, event_type, from_status, to_status,
  reason_json, agent_target_id, created_at
)
SELECT id, workflow_id, 'poll_transport_removed', status, 'cancelled',
       json_object('code', 'poll_transport_removed'), assigned_agent_id,
       strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
FROM tasks
WHERE assigned_agent_id IN (
  SELECT id FROM agent_targets WHERE transport = 'poll'
)
  AND status IN ('reserved', 'running', 'claimed', 'converting', 'indexing');

UPDATE tasks
SET status = 'cancelled',
    assigned_agent_id = NULL,
    lease_owner = NULL,
    lease_token_hash = NULL,
    lease_expires_at = NULL,
    last_heartbeat_at = NULL,
    retry_after = NULL,
    terminal_reason_json = json_object('code', 'poll_transport_removed'),
    execution_generation = execution_generation + 1,
    completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE assigned_agent_id IN (
  SELECT id FROM agent_targets WHERE transport = 'poll'
)
  AND status IN ('reserved', 'running', 'claimed', 'converting', 'indexing');

UPDATE workflows
SET status = CASE
      WHEN EXISTS (
        SELECT 1 FROM tasks
        WHERE workflow_id = workflows.id
          AND status NOT IN ('succeeded', 'cancelled', 'stale', 'superseded')
          AND NOT (status = 'failed' AND attempts >= max_attempts)
      ) THEN 'active'
      WHEN EXISTS (
        SELECT 1 FROM tasks WHERE workflow_id = workflows.id AND status = 'succeeded'
      ) AND NOT EXISTS (
        SELECT 1 FROM tasks
        WHERE workflow_id = workflows.id
          AND status IN ('cancelled', 'stale', 'superseded')
      ) THEN 'succeeded'
      WHEN EXISTS (
        SELECT 1 FROM tasks
        WHERE workflow_id = workflows.id
          AND status IN ('cancelled', 'stale', 'superseded')
      ) THEN 'cancelled'
      ELSE 'failed'
    END,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE id IN (
  SELECT workflow_id FROM task_events WHERE event_type = 'poll_transport_removed'
);

DELETE FROM agent_targets WHERE transport = 'poll';

UPDATE agent_targets
SET token_hash = 'command-only'
WHERE transport = 'command';

CREATE TRIGGER agent_targets_command_only_insert
BEFORE INSERT ON agent_targets
WHEN NEW.transport <> 'command'
BEGIN
  SELECT RAISE(ABORT, 'agent target transport must be command');
END;

CREATE TRIGGER agent_targets_command_only_update
BEFORE UPDATE OF transport ON agent_targets
WHEN NEW.transport <> 'command'
BEGIN
  SELECT RAISE(ABORT, 'agent target transport must be command');
END;
