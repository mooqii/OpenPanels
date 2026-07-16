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
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX model_gateway_local_provider_unique_idx
  ON model_gateway_connections(provider_id)
  WHERE transport = 'local_cli';

CREATE INDEX model_gateway_connections_transport_idx
  ON model_gateway_connections(transport, enabled, updated_at DESC);

CREATE TABLE model_gateway_config (
  id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
  mode TEXT NOT NULL CHECK (mode IN ('local_cli', 'byok')),
  active_local_connection_id TEXT,
  active_byok_connection_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (active_local_connection_id)
    REFERENCES model_gateway_connections(id) ON DELETE SET NULL,
  FOREIGN KEY (active_byok_connection_id)
    REFERENCES model_gateway_connections(id) ON DELETE SET NULL
);

INSERT INTO model_gateway_connections (
  id, transport, provider_id, display_name, executable_path, model, reasoning,
  config_json, enabled, created_at, updated_at
)
VALUES
  (
    'local-cli:codex', 'local_cli', 'codex', 'Codex CLI',
    NULLIF(TRIM((
      SELECT json_extract(value_json, '$.localCli.executablePaths.codex')
      FROM settings WHERE namespace = 'model_gateway' AND key = 'settings'
    )), ''),
    CASE WHEN COALESCE((
      SELECT json_extract(value_json, '$.localCli.providerId')
      FROM settings WHERE namespace = 'model_gateway' AND key = 'settings'
    ), 'codex') = 'codex' THEN (
      SELECT json_extract(value_json, '$.localCli.model')
      FROM settings WHERE namespace = 'model_gateway' AND key = 'settings'
    ) END,
    CASE WHEN COALESCE((
      SELECT json_extract(value_json, '$.localCli.providerId')
      FROM settings WHERE namespace = 'model_gateway' AND key = 'settings'
    ), 'codex') = 'codex' THEN (
      SELECT json_extract(value_json, '$.localCli.reasoning')
      FROM settings WHERE namespace = 'model_gateway' AND key = 'settings'
    ) END,
    '{}', 1,
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
  ),
  (
    'local-cli:hermes', 'local_cli', 'hermes', 'Hermes',
    NULLIF(TRIM((
      SELECT json_extract(value_json, '$.localCli.executablePaths.hermes')
      FROM settings WHERE namespace = 'model_gateway' AND key = 'settings'
    )), ''),
    CASE WHEN (
      SELECT json_extract(value_json, '$.localCli.providerId')
      FROM settings WHERE namespace = 'model_gateway' AND key = 'settings'
    ) = 'hermes' THEN (
      SELECT json_extract(value_json, '$.localCli.model')
      FROM settings WHERE namespace = 'model_gateway' AND key = 'settings'
    ) END,
    CASE WHEN (
      SELECT json_extract(value_json, '$.localCli.providerId')
      FROM settings WHERE namespace = 'model_gateway' AND key = 'settings'
    ) = 'hermes' THEN (
      SELECT json_extract(value_json, '$.localCli.reasoning')
      FROM settings WHERE namespace = 'model_gateway' AND key = 'settings'
    ) END,
    '{}', 1,
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
  );

INSERT INTO model_gateway_config (
  id, mode, active_local_connection_id, active_byok_connection_id,
  created_at, updated_at
)
VALUES (
  1,
  CASE WHEN (
    SELECT json_extract(value_json, '$.mode')
    FROM settings WHERE namespace = 'model_gateway' AND key = 'settings'
  ) = 'byok' THEN 'byok' ELSE 'local_cli' END,
  CASE WHEN (
    SELECT json_extract(value_json, '$.localCli.providerId')
    FROM settings WHERE namespace = 'model_gateway' AND key = 'settings'
  ) = 'hermes' THEN 'local-cli:hermes' ELSE 'local-cli:codex' END,
  NULL,
  strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
  strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
);

DELETE FROM settings
WHERE namespace = 'model_gateway' AND key = 'settings';
