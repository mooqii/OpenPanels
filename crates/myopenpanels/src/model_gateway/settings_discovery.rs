use crate::error::CliError;
use crate::paths::MyOpenPanelsPaths;
use crate::storage::Storage;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const PROCESS_OUTPUT_LIMIT: usize = 8 * 1024 * 1024;
const SMOKE_PROMPT: &str = "Reply with only: ok";
const MODEL_GATEWAY_SETTINGS_NAMESPACE: &str = "model_gateway";
const MAX_CONCURRENCY_SETTING_KEY: &str = "max_concurrency";
const LOCAL_CLI_SCAN_CACHE_SETTING_KEY: &str = "local_cli_scan_cache";

pub const DEFAULT_MAX_CONCURRENCY: i64 = 2;

fn default_max_concurrency() -> i64 {
    DEFAULT_MAX_CONCURRENCY
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelGatewaySettings {
    pub mode: String,
    pub local_cli: LocalCliSettings,
    pub byok: ByokSettings,
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalCliSettings {
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub reasoning: Option<String>,
    #[serde(default)]
    pub provider_models: BTreeMap<String, String>,
    #[serde(default)]
    pub provider_reasoning: BTreeMap<String, String>,
    #[serde(default)]
    pub enabled_provider_ids: Vec<String>,
    #[serde(default)]
    pub provider_order: Vec<String>,
    #[serde(default)]
    pub executable_paths: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ByokSettings {
    pub provider_id: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

impl Default for ModelGatewaySettings {
    fn default() -> Self {
        let provider_id = default_provider_from_env();
        let provider_order = provider_id.iter().cloned().collect::<Vec<_>>();
        Self {
            mode: "localCli".to_owned(),
            local_cli: LocalCliSettings {
                provider_id,
                model: None,
                reasoning: None,
                provider_models: BTreeMap::new(),
                provider_reasoning: BTreeMap::new(),
                enabled_provider_ids: provider_order.clone(),
                provider_order,
                executable_paths: BTreeMap::new(),
            },
            byok: ByokSettings::default(),
            max_concurrency: DEFAULT_MAX_CONCURRENCY,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelOption {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasoning_options: Vec<ModelOption>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalCliInfo {
    pub id: String,
    pub name: String,
    pub bin: String,
    pub available: bool,
    pub path: Option<String>,
    pub configured_path: Option<String>,
    pub version: Option<String>,
    pub auth_status: String,
    pub auth_message: Option<String>,
    pub diagnostic: Option<String>,
    pub models: Vec<ModelOption>,
    pub models_source: String,
    pub reasoning_options: Vec<ModelOption>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestRequest {
    pub provider_id: String,
    pub model: Option<String>,
    pub reasoning: Option<String>,
    pub executable_path: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct LocalCliScanRequest {
    #[serde(default)]
    pub executable_paths: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct GatewayWorkerSpec {
    pub agent_prompt: bool,
    pub connection_id: String,
    pub key: String,
    pub provider_id: String,
    pub provider_name: String,
    pub host: String,
    pub command: String,
}

#[derive(Clone, Copy)]
struct LocalCliDefinition {
    id: &'static str,
    name: &'static str,
    bin: &'static str,
    fallback_bins: &'static [&'static str],
    adapter_version: i64,
    version_args: &'static [&'static str],
    auth_args: &'static [&'static str],
    fallback_models: &'static [(&'static str, &'static str)],
    reasoning_options: &'static [(&'static str, &'static str)],
    probe_models: fn(&str, &Path) -> Option<Vec<ModelOption>>,
    smoke_invocation: fn(&Path, Option<&str>, Option<&str>) -> LocalCliInvocation,
    task_command: fn(&str, Option<&str>, Option<&str>) -> String,
}

struct LocalCliInvocation {
    args: Vec<String>,
    input: Option<&'static str>,
}

fn sync_builtin_local_cli_connections(
    storage: &mut Storage,
    definitions: &[LocalCliDefinition],
) -> Result<(), CliError> {
    if !builtin_local_cli_registry_needs_sync(storage, definitions)? {
        return Ok(());
    }
    let tx = storage
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let initializing = tx
        .query_row(
            r#"
            SELECT COUNT(*) = 0
              AND (SELECT mode FROM model_gateway_config WHERE id = 1) = 'local_cli'
              AND (SELECT active_local_connection_id FROM model_gateway_config WHERE id = 1) IS NULL
            FROM model_gateway_connections
            WHERE transport = 'local_cli'
            "#,
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(to_cli_error)?;
    let mut changed = 0usize;
    for definition in definitions {
        let connection_id = format!("local-cli:{}", definition.id);
        let metadata = json!({
            "origin": "builtin",
            "adapterVersion": definition.adapter_version,
            "binaryName": definition.bin,
        })
        .to_string();
        changed += tx
            .execute(
                r#"
                INSERT INTO model_gateway_connections (
                  id, transport, provider_id, display_name, config_json,
                  enabled, priority, created_at, updated_at
                )
                VALUES (?, 'local_cli', ?, ?, ?, 0, 0, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                  provider_id = excluded.provider_id,
                  display_name = excluded.display_name,
                  config_json = json_patch(
                    model_gateway_connections.config_json,
                    excluded.config_json
                  ),
                  updated_at = excluded.updated_at
                WHERE model_gateway_connections.transport != 'local_cli'
                   OR model_gateway_connections.provider_id != excluded.provider_id
                   OR model_gateway_connections.display_name != excluded.display_name
                   OR json_extract(
                        model_gateway_connections.config_json,
                        '$.origin'
                      ) IS NOT 'builtin'
                   OR json_extract(
                        model_gateway_connections.config_json,
                        '$.adapterVersion'
                      ) IS NOT json_extract(excluded.config_json, '$.adapterVersion')
                   OR json_extract(
                        model_gateway_connections.config_json,
                        '$.binaryName'
                      ) IS NOT json_extract(excluded.config_json, '$.binaryName')
                "#,
                params![
                    connection_id,
                    definition.id,
                    definition.name,
                    metadata,
                    now,
                    now
                ],
            )
            .map_err(to_cli_error)?;
    }

    if initializing {
        if let Some(provider_id) = default_provider_from_env().filter(|provider_id| {
            definitions
                .iter()
                .any(|definition| definition.id == provider_id)
        }) {
            let connection_id = format!("local-cli:{provider_id}");
            changed += tx
                .execute(
                    "UPDATE model_gateway_connections SET enabled = 1, priority = 1000, updated_at = ? WHERE id = ?",
                    params![now, connection_id],
                )
                .map_err(to_cli_error)?;
            changed += tx
                .execute(
                    "UPDATE model_gateway_config SET mode = 'local_cli', active_local_connection_id = ?, updated_at = ? WHERE id = 1",
                    params![connection_id, now],
                )
                .map_err(to_cli_error)?;
        }
    }

    let registered = definitions
        .iter()
        .map(|definition| definition.id)
        .collect::<HashSet<_>>();
    let stale_connections = {
        let mut statement = tx
            .prepare(
                r#"
                SELECT id, provider_id
                FROM model_gateway_connections
                WHERE transport = 'local_cli'
                  AND enabled = 1
                  AND json_extract(config_json, '$.origin') = 'builtin'
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(to_cli_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_cli_error)?;
        rows
    };
    for (connection_id, provider_id) in stale_connections {
        if registered.contains(provider_id.as_str()) {
            continue;
        }
        changed += tx
            .execute(
                "UPDATE model_gateway_connections SET enabled = 0, updated_at = ? WHERE id = ?",
                params![now, connection_id],
            )
            .map_err(to_cli_error)?;
    }
    changed += tx
        .execute(
            r#"
            UPDATE model_gateway_config
            SET active_local_connection_id = NULL, updated_at = ?
            WHERE id = 1
              AND active_local_connection_id IN (
                SELECT id FROM model_gateway_connections WHERE enabled = 0
              )
            "#,
            [now.as_str()],
        )
        .map_err(to_cli_error)?;
    if changed > 0 {
        crate::storage::record_scope(&tx, "catalog", None, None)?;
    }
    tx.commit().map_err(to_cli_error)
}

pub(crate) fn sync_builtin_local_cli_registry(storage: &mut Storage) -> Result<(), CliError> {
    sync_builtin_local_cli_connections(storage, LOCAL_CLI_DEFINITIONS)
}

fn builtin_local_cli_registry_needs_sync(
    storage: &Storage,
    definitions: &[LocalCliDefinition],
) -> Result<bool, CliError> {
    let connection = storage.connection();
    for definition in definitions {
        let connection_id = format!("local-cli:{}", definition.id);
        let current = connection
            .query_row(
                r#"
                SELECT provider_id, display_name,
                       json_extract(config_json, '$.origin'),
                       json_extract(config_json, '$.adapterVersion'),
                       json_extract(config_json, '$.binaryName')
                FROM model_gateway_connections
                WHERE id = ?
                "#,
                [connection_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(to_cli_error)?;
        let expected = (
            definition.id,
            definition.name,
            Some("builtin"),
            Some(definition.adapter_version),
            Some(definition.bin),
        );
        if current.as_ref().map(|value| {
            (
                value.0.as_str(),
                value.1.as_str(),
                value.2.as_deref(),
                value.3,
                value.4.as_deref(),
            )
        }) != Some(expected)
        {
            return Ok(true);
        }
    }
    let registered = definitions
        .iter()
        .map(|definition| definition.id)
        .collect::<HashSet<_>>();
    let mut statement = connection
        .prepare(
            r#"
            SELECT provider_id
            FROM model_gateway_connections
            WHERE transport = 'local_cli'
              AND enabled = 1
              AND json_extract(config_json, '$.origin') = 'builtin'
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(to_cli_error)?;
    for row in rows {
        if !registered.contains(row.map_err(to_cli_error)?.as_str()) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn read_settings(paths: &MyOpenPanelsPaths) -> Result<ModelGatewaySettings, CliError> {
    let mut storage = Storage::open(paths)?;
    sync_builtin_local_cli_connections(&mut storage, LOCAL_CLI_DEFINITIONS)?;
    let max_concurrency = storage
        .read_setting(
            MODEL_GATEWAY_SETTINGS_NAMESPACE,
            MAX_CONCURRENCY_SETTING_KEY,
        )?
        .and_then(|raw| serde_json::from_str::<i64>(&raw).ok())
        .unwrap_or(DEFAULT_MAX_CONCURRENCY);
    let connection = storage.connection();
    let config = connection
        .query_row(
            "SELECT mode, active_local_connection_id, active_byok_connection_id FROM model_gateway_config WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(to_cli_error)?;
    let Some((mode, active_local_id, active_byok_id)) = config else {
        return Ok(ModelGatewaySettings::default());
    };

    let mut executable_paths = BTreeMap::new();
    let mut provider_models = BTreeMap::new();
    let mut provider_reasoning = BTreeMap::new();
    let mut enabled_provider_ids = Vec::new();
    let mut statement = connection
        .prepare(
            r#"
            SELECT provider_id, executable_path, model, reasoning, enabled
            FROM model_gateway_connections
            WHERE transport = 'local_cli'
            ORDER BY priority DESC, id ASC
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, bool>(4)?,
            ))
        })
        .map_err(to_cli_error)?;
    for row in rows {
        let (provider_id, executable_path, model, reasoning, enabled) =
            row.map_err(to_cli_error)?;
        if let Some(executable_path) = executable_path {
            executable_paths.insert(provider_id.clone(), executable_path);
        }
        if let Some(model) = model {
            provider_models.insert(provider_id.clone(), model);
        }
        if let Some(reasoning) = reasoning {
            provider_reasoning.insert(provider_id.clone(), reasoning);
        }
        if enabled {
            enabled_provider_ids.push(provider_id);
        }
    }
    drop(statement);

    let byok = active_byok_id
        .as_deref()
        .map(|connection_id| {
            connection
                .query_row(
                    "SELECT provider_id, base_url, model FROM model_gateway_connections WHERE id = ? AND transport = 'byok' AND enabled = 1",
                    [connection_id],
                    |row| {
                        Ok(ByokSettings {
                            provider_id: Some(row.get(0)?),
                            base_url: row.get(1)?,
                            model: row.get(2)?,
                        })
                    },
                )
                .optional()
        })
        .transpose()
        .map_err(to_cli_error)?
        .flatten()
        .unwrap_or_default();
    let provider_id = active_local_id
        .as_deref()
        .and_then(|connection_id| connection_id.strip_prefix("local-cli:"))
        .filter(|provider_id| enabled_provider_ids.iter().any(|id| id == provider_id))
        .map(str::to_owned);
    let model = provider_id
        .as_ref()
        .and_then(|provider_id| provider_models.get(provider_id).cloned());
    let reasoning = provider_id
        .as_ref()
        .and_then(|provider_id| provider_reasoning.get(provider_id).cloned());
    normalize_settings(ModelGatewaySettings {
        mode: if mode == "byok" { "byok" } else { "localCli" }.to_owned(),
        local_cli: LocalCliSettings {
            provider_id,
            model,
            reasoning,
            provider_models,
            provider_reasoning,
            provider_order: enabled_provider_ids.clone(),
            enabled_provider_ids,
            executable_paths,
        },
        byok,
        max_concurrency,
    })
}

pub fn write_settings(
    paths: &MyOpenPanelsPaths,
    settings: ModelGatewaySettings,
) -> Result<ModelGatewaySettings, CliError> {
    let settings = normalize_settings(settings)?;
    if settings.mode == "byok" {
        return Err(CliError::with_code(
            "byok_not_available",
            "BYOK providers are reserved for a later release. Select Local CLI for now.",
        ));
    }
    let mut storage = Storage::open(paths)?;
    sync_builtin_local_cli_connections(&mut storage, LOCAL_CLI_DEFINITIONS)?;
    let tx = storage
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    let enabled_positions = settings
        .local_cli
        .provider_order
        .iter()
        .enumerate()
        .map(|(position, provider_id)| (provider_id.as_str(), position))
        .collect::<BTreeMap<_, _>>();
    for definition in LOCAL_CLI_DEFINITIONS {
        let connection_id = format!("local-cli:{}", definition.id);
        let executable_path = settings
            .local_cli
            .executable_paths
            .get(definition.id)
            .map(String::as_str);
        let position = enabled_positions.get(definition.id).copied();
        let enabled = position.is_some();
        let priority = position
            .map(|position| 1000_i64.saturating_sub(position as i64))
            .unwrap_or(0);
        let model = settings.local_cli.provider_models.get(definition.id);
        let reasoning = settings.local_cli.provider_reasoning.get(definition.id);
        tx.execute(
            "UPDATE model_gateway_connections SET executable_path = ?, model = ?, reasoning = ?, enabled = ?, priority = ?, updated_at = ? WHERE id = ? AND transport = 'local_cli'",
            params![executable_path, model, reasoning, enabled, priority, now, connection_id],
        )
        .map_err(to_cli_error)?;
    }
    let active_local_connection_id = settings
        .local_cli
        .provider_id
        .as_deref()
        .map(|provider_id| format!("local-cli:{provider_id}"));
    tx.execute(
        "UPDATE model_gateway_config SET mode = 'local_cli', active_local_connection_id = ?, updated_at = ? WHERE id = 1",
        params![active_local_connection_id, now],
    )
    .map_err(to_cli_error)?;
    tx.execute(
        r#"
        INSERT INTO settings (namespace, key, value_json, updated_at)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(namespace, key) DO UPDATE SET
          value_json = excluded.value_json,
          updated_at = excluded.updated_at
        "#,
        params![
            MODEL_GATEWAY_SETTINGS_NAMESPACE,
            MAX_CONCURRENCY_SETTING_KEY,
            settings.max_concurrency.to_string(),
            now,
        ],
    )
    .map_err(to_cli_error)?;
    crate::storage::record_scope(&tx, "catalog", None, None)?;
    tx.commit().map_err(to_cli_error)?;
    Ok(settings)
}

pub fn settings_payload(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let local_cli_providers = LOCAL_CLI_DEFINITIONS
        .iter()
        .map(|definition| definition.id)
        .collect::<Vec<_>>();
    Ok(json!({
        "settings": read_settings(paths)?,
        "capabilities": {
            "localCli": { "available": true, "providers": local_cli_providers },
            "byok": { "available": false, "providers": [] },
        }
    }))
}

pub fn scan_local_clis(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let settings = read_settings(paths)?;
    scan_local_clis_with_overrides(paths, settings.local_cli.executable_paths)
}

pub fn cached_local_clis(paths: &MyOpenPanelsPaths) -> Result<Option<Value>, CliError> {
    let storage = Storage::open(paths)?;
    let Some(raw) = storage.read_setting(
        MODEL_GATEWAY_SETTINGS_NAMESPACE,
        LOCAL_CLI_SCAN_CACHE_SETTING_KEY,
    )?
    else {
        return Ok(None);
    };
    let mut payload = match serde_json::from_str::<Value>(&raw) {
        Ok(payload) if payload.get("localClis").and_then(Value::as_array).is_some() => payload,
        _ => return Ok(None),
    };
    if let Some(object) = payload.as_object_mut() {
        object.insert("cached".to_owned(), Value::Bool(true));
    }
    Ok(Some(payload))
}

pub fn scan_local_clis_with_overrides(
    paths: &MyOpenPanelsPaths,
    executable_paths: BTreeMap<String, String>,
) -> Result<Value, CliError> {
    let mut handles = Vec::new();
    for definition in LOCAL_CLI_DEFINITIONS.iter().copied() {
        let configured_path = executable_paths.get(definition.id).cloned();
        let cwd = paths.project_dir.clone();
        handles.push(thread::spawn(move || {
            scan_local_cli(definition, configured_path, &cwd)
        }));
    }
    let agents = handles
        .into_iter()
        .map(|handle| {
            handle
                .join()
                .unwrap_or_else(|_| unavailable_cli("unknown", "Unknown CLI", ""))
        })
        .collect::<Vec<_>>();
    let payload = json!({ "cached": false, "localClis": agents });
    Storage::open(paths)?.write_setting(
        MODEL_GATEWAY_SETTINGS_NAMESPACE,
        LOCAL_CLI_SCAN_CACHE_SETTING_KEY,
        &payload.to_string(),
    )?;
    Ok(payload)
}

pub fn test_local_cli(
    paths: &MyOpenPanelsPaths,
    request: ConnectionTestRequest,
) -> Result<Value, CliError> {
    let started = Instant::now();
    let definition = definition(&request.provider_id).ok_or_else(|| {
        CliError::with_code(
            "unsupported_model_provider",
            format!("Unsupported Local CLI provider: {}", request.provider_id),
        )
    })?;
    let settings = read_settings(paths)?;
    let configured = request.executable_path.as_deref().or_else(|| {
        settings
            .local_cli
            .executable_paths
            .get(definition.id)
            .map(String::as_str)
    });
    let resolution = resolve_executable(definition, configured);
    let Some(path) = resolution.path else {
        return Ok(json!({
            "ok": false,
            "kind": "agentNotInstalled",
            "providerId": definition.id,
            "providerName": definition.name,
            "latencyMs": started.elapsed().as_millis(),
            "detail": resolution.diagnostic.unwrap_or_else(|| format!("{} was not found on PATH.", definition.bin)),
        }));
    };

    let temp = tempfile::tempdir().map_err(to_cli_error)?;
    let model = clean_optional(request.model.as_deref());
    let reasoning = clean_optional(request.reasoning.as_deref());
    let invocation = (definition.smoke_invocation)(temp.path(), model, reasoning);
    let output = run_process(
        &path,
        &invocation.args,
        invocation.input,
        Some(temp.path()),
        Duration::from_secs(120),
    )?;
    let sample = assistant_sample(definition.id, &output.stdout);
    let ok = output.success
        && output_semantically_succeeded(&output.stdout)
        && !sample.trim().is_empty();
    let detail = if ok {
        None
    } else {
        Some(
            first_non_empty(&output.stderr, &output.stdout)
                .chars()
                .take(1200)
                .collect::<String>(),
        )
    };
    Ok(json!({
        "ok": ok,
        "kind": if ok { "success" } else if output.timed_out { "timeout" } else { "agentSpawnFailed" },
        "providerId": definition.id,
        "providerName": definition.name,
        "model": model,
        "latencyMs": started.elapsed().as_millis(),
        "sample": sample.chars().take(300).collect::<String>(),
        "detail": detail,
        "diagnostics": {
            "binaryPath": path,
            "exitCode": output.status_code,
            "timedOut": output.timed_out,
        }
    }))
}

pub fn worker_specs(paths: &MyOpenPanelsPaths) -> Result<Vec<GatewayWorkerSpec>, CliError> {
    if std::env::var("MYOPENPANELS_DISABLE_LOCAL_AGENT")
        .ok()
        .as_deref()
        == Some("1")
    {
        return Ok(Vec::new());
    }
    if let Ok(command) = std::env::var("MYOPENPANELS_AGENT_COMMAND") {
        if !command.trim().is_empty() {
            return Ok(vec![GatewayWorkerSpec {
                agent_prompt: false,
                connection_id: "custom".to_owned(),
                key: format!("custom:{command}"),
                provider_id: "custom".to_owned(),
                provider_name: "Configured command".to_owned(),
                host: "configured-agent".to_owned(),
                command,
            }]);
        }
    }

    let mut storage = Storage::open(paths)?;
    sync_builtin_local_cli_connections(&mut storage, LOCAL_CLI_DEFINITIONS)?;
    let connection = storage.connection();
    let mode = connection
        .query_row(
            "SELECT mode FROM model_gateway_config WHERE id = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(to_cli_error)?
        .unwrap_or_else(|| "local_cli".to_owned());
    if mode != "local_cli" {
        return Ok(Vec::new());
    }
    let mut statement = connection
        .prepare(
            r#"
            SELECT id, provider_id, executable_path, model, reasoning, priority
            FROM model_gateway_connections
            WHERE transport = 'local_cli' AND enabled = 1
            ORDER BY priority DESC, id ASC
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    let mut specs = Vec::new();
    for (connection_id, provider_id, configured_path, model, reasoning, priority) in rows {
        let Some(definition) = definition(&provider_id) else {
            continue;
        };
        let Some(executable) = resolve_executable(definition, configured_path.as_deref()).path
        else {
            continue;
        };
        let command =
            (definition.task_command)(&executable, model.as_deref(), reasoning.as_deref());
        let key = format!(
            "{}:{}:{}:{}:{}",
            connection_id,
            executable,
            model.as_deref().unwrap_or("default"),
            reasoning.as_deref().unwrap_or("default"),
            priority,
        );
        specs.push(GatewayWorkerSpec {
            agent_prompt: true,
            connection_id,
            key,
            provider_id: definition.id.to_owned(),
            provider_name: definition.name.to_owned(),
            host: format!("{}-cli", definition.id),
            command,
        });
    }
    Ok(specs)
}
