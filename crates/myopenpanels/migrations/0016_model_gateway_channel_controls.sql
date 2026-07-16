ALTER TABLE model_gateway_connections
  ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;

-- Before channel controls existed every built-in adapter was implicitly enabled.
-- Keep the configured primary channel active and let users opt into fallbacks.
UPDATE model_gateway_connections
SET enabled = CASE
      WHEN id = (
        SELECT active_local_connection_id FROM model_gateway_config WHERE id = 1
      ) THEN 1
      WHEN id = (
        SELECT active_byok_connection_id FROM model_gateway_config WHERE id = 1
      ) THEN 1
      ELSE 0
    END,
    priority = CASE
      WHEN id = (
        SELECT active_local_connection_id FROM model_gateway_config WHERE id = 1
      ) THEN 1000
      WHEN id = (
        SELECT active_byok_connection_id FROM model_gateway_config WHERE id = 1
      ) THEN 1000
      ELSE 0
    END;

CREATE INDEX model_gateway_connections_dispatch_idx
  ON model_gateway_connections(transport, enabled, priority DESC, id ASC);
