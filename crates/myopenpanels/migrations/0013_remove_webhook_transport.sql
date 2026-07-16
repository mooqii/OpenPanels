INSERT INTO task_events (
  task_id, workflow_id, event_type, from_status, to_status, reason_json,
  attempt_id, agent_target_id, created_at
)
SELECT
  tasks.id,
  tasks.workflow_id,
  'executor_removed',
  tasks.status,
  'failed',
  json_object('code', 'webhook_transport_removed'),
  (
    SELECT task_attempts.id
    FROM task_attempts
    WHERE task_attempts.task_id = tasks.id
      AND task_attempts.status = 'leased'
    ORDER BY task_attempts.started_at DESC
    LIMIT 1
  ),
  tasks.assigned_agent_id,
  strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
FROM tasks
JOIN agent_targets ON agent_targets.id = tasks.assigned_agent_id
WHERE agent_targets.transport = 'webhook'
  AND tasks.status NOT IN ('succeeded', 'cancelled', 'stale', 'superseded');

UPDATE task_attempts
SET status = 'interrupted',
    finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    error_json = json_object('code', 'webhook_transport_removed'),
    execution_token_hash = NULL,
    execution_token_expires_at = NULL
WHERE status = 'leased'
  AND agent_target_id IN (
    SELECT id FROM agent_targets WHERE transport = 'webhook'
  );

UPDATE tasks
SET status = 'failed',
    assigned_agent_id = NULL,
    lease_owner = NULL,
    lease_token_hash = NULL,
    lease_expires_at = NULL,
    last_heartbeat_at = NULL,
    retry_after = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    error_json = json_object('code', 'webhook_transport_removed'),
    max_attempts = CASE
      WHEN attempts >= max_attempts THEN attempts + 1
      ELSE max_attempts
    END,
    execution_generation = execution_generation + 1,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE assigned_agent_id IN (
    SELECT id FROM agent_targets WHERE transport = 'webhook'
  )
  AND status NOT IN ('succeeded', 'cancelled', 'stale', 'superseded');

DELETE FROM agent_routes
WHERE agent_target_id IN (
  SELECT id FROM agent_targets WHERE transport = 'webhook'
);

UPDATE agent_targets
SET status = 'offline',
    last_error = 'Webhook transport was removed.',
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE transport = 'webhook';

UPDATE dispatch_outbox
SET status = 'cancelled',
    next_attempt_at = NULL,
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE status IN ('pending', 'delivering');
