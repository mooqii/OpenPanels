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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelGatewaySettings {
    pub mode: String,
    pub local_cli: LocalCliSettings,
    pub byok: ByokSettings,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalCliSettings {
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub reasoning: Option<String>,
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
        Self {
            mode: "localCli".to_owned(),
            local_cli: LocalCliSettings {
                provider_id: default_provider_from_env(),
                model: None,
                reasoning: None,
                executable_paths: BTreeMap::new(),
            },
            byok: ByokSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelOption {
    pub id: String,
    pub label: String,
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

const DEFAULT_MODELS: &[(&str, &str)] = &[("default", "Default (CLI config)")];
const CODEX_FALLBACK_MODELS: &[(&str, &str)] = &[
    ("default", "Default (CLI config)"),
    ("gpt-5.6-sol", "gpt-5.6-sol"),
    ("gpt-5.4", "gpt-5.4"),
    ("gpt-5.3-codex", "gpt-5.3-codex"),
    ("gpt-5.1", "gpt-5.1"),
    ("gpt-5", "gpt-5"),
];
const CODEX_REASONING: &[(&str, &str)] = &[
    ("default", "Default"),
    ("low", "Low"),
    ("medium", "Medium"),
    ("high", "High"),
    ("xhigh", "Extra high"),
];

const LOCAL_CLI_DEFINITIONS: &[LocalCliDefinition] = &[
    LocalCliDefinition {
        id: "codex",
        name: "Codex CLI",
        bin: "codex",
        adapter_version: 1,
        version_args: &["--version"],
        auth_args: &["login", "status"],
        fallback_models: CODEX_FALLBACK_MODELS,
        reasoning_options: CODEX_REASONING,
        probe_models: probe_codex_models,
        smoke_invocation: codex_smoke_invocation,
        task_command: codex_task_command,
    },
    LocalCliDefinition {
        id: "hermes",
        name: "Hermes",
        bin: "hermes",
        adapter_version: 1,
        version_args: &["--version"],
        auth_args: &["config", "check"],
        fallback_models: DEFAULT_MODELS,
        reasoning_options: &[],
        probe_models: probe_hermes_models,
        smoke_invocation: hermes_smoke_invocation,
        task_command: hermes_task_command,
    },
];

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
                  enabled, created_at, updated_at
                )
                VALUES (?, 'local_cli', ?, ?, ?, 1, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                  provider_id = excluded.provider_id,
                  display_name = excluded.display_name,
                  config_json = json_patch(
                    model_gateway_connections.config_json,
                    excluded.config_json
                  ),
                  enabled = 1,
                  updated_at = excluded.updated_at
                WHERE model_gateway_connections.transport != 'local_cli'
                   OR model_gateway_connections.provider_id != excluded.provider_id
                   OR model_gateway_connections.display_name != excluded.display_name
                   OR model_gateway_connections.enabled != 1
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
                SELECT provider_id, display_name, enabled,
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
                        row.get::<_, i64>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<i64>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(to_cli_error)?;
        let expected = (
            definition.id,
            definition.name,
            1,
            Some("builtin"),
            Some(definition.adapter_version),
            Some(definition.bin),
        );
        if current.as_ref().map(|value| {
            (
                value.0.as_str(),
                value.1.as_str(),
                value.2,
                value.3.as_deref(),
                value.4,
                value.5.as_deref(),
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

    let local_selection = active_local_id
        .as_deref()
        .map(|connection_id| {
            connection
                .query_row(
                    "SELECT provider_id, model, reasoning FROM model_gateway_connections WHERE id = ? AND transport = 'local_cli' AND enabled = 1",
                    [connection_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                        ))
                    },
                )
                .optional()
        })
        .transpose()
        .map_err(to_cli_error)?
        .flatten();
    let mut executable_paths = BTreeMap::new();
    let mut statement = connection
        .prepare(
            "SELECT provider_id, executable_path FROM model_gateway_connections WHERE transport = 'local_cli' AND executable_path IS NOT NULL",
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(to_cli_error)?;
    for row in rows {
        let (provider_id, executable_path) = row.map_err(to_cli_error)?;
        executable_paths.insert(provider_id, executable_path);
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
    let (provider_id, model, reasoning) = local_selection
        .map(|(provider_id, model, reasoning)| (Some(provider_id), model, reasoning))
        .unwrap_or((None, None, None));
    normalize_settings(ModelGatewaySettings {
        mode: if mode == "byok" { "byok" } else { "localCli" }.to_owned(),
        local_cli: LocalCliSettings {
            provider_id,
            model,
            reasoning,
            executable_paths,
        },
        byok,
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
    for definition in LOCAL_CLI_DEFINITIONS {
        let connection_id = format!("local-cli:{}", definition.id);
        let executable_path = settings
            .local_cli
            .executable_paths
            .get(definition.id)
            .map(String::as_str);
        if settings.local_cli.provider_id.as_deref() == Some(definition.id) {
            tx.execute(
                "UPDATE model_gateway_connections SET executable_path = ?, model = ?, reasoning = ?, updated_at = ? WHERE id = ? AND transport = 'local_cli'",
                params![executable_path, settings.local_cli.model, settings.local_cli.reasoning, now, connection_id],
            )
            .map_err(to_cli_error)?;
        } else {
            tx.execute(
                "UPDATE model_gateway_connections SET executable_path = ?, updated_at = ? WHERE id = ? AND transport = 'local_cli'",
                params![executable_path, now, connection_id],
            )
            .map_err(to_cli_error)?;
        }
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
    Ok(json!({ "localClis": agents }))
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
    let resolution = resolve_executable(definition.bin, configured);
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
    let ok = output.success && !sample.trim().is_empty();
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
    let (mode, active_connection_id) = connection
        .query_row(
            "SELECT mode, active_local_connection_id FROM model_gateway_config WHERE id = 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(to_cli_error)?
        .unwrap_or_else(|| ("local_cli".to_owned(), None));
    if mode != "local_cli" {
        return Ok(Vec::new());
    }
    let mut statement = connection
        .prepare(
            r#"
            SELECT id, provider_id, executable_path, model, reasoning
            FROM model_gateway_connections
            WHERE transport = 'local_cli' AND enabled = 1
            ORDER BY CASE WHEN id = ? THEN 0 ELSE 1 END, updated_at DESC, id ASC
            "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([active_connection_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    let mut specs = Vec::new();
    for (connection_id, provider_id, configured_path, model, reasoning) in rows {
        let Some(definition) = definition(&provider_id) else {
            continue;
        };
        let Some(executable) = resolve_executable(definition.bin, configured_path.as_deref()).path
        else {
            continue;
        };
        let command =
            (definition.task_command)(&executable, model.as_deref(), reasoning.as_deref());
        let key = format!(
            "{}:{}:{}:{}",
            connection_id,
            executable,
            model.as_deref().unwrap_or("default"),
            reasoning.as_deref().unwrap_or("default")
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

pub fn selected_worker_spec(
    paths: &MyOpenPanelsPaths,
) -> Result<Option<GatewayWorkerSpec>, CliError> {
    Ok(worker_specs(paths)?.into_iter().next())
}

fn normalize_settings(
    mut settings: ModelGatewaySettings,
) -> Result<ModelGatewaySettings, CliError> {
    settings.mode = settings.mode.trim().to_owned();
    if !matches!(settings.mode.as_str(), "localCli" | "byok") {
        return Err(CliError::with_code(
            "invalid_model_gateway_settings",
            "Model gateway mode must be localCli or byok.",
        ));
    }
    settings.local_cli.provider_id = clean_owned(settings.local_cli.provider_id);
    settings.local_cli.model = clean_owned(settings.local_cli.model);
    settings.local_cli.reasoning = clean_owned(settings.local_cli.reasoning);
    settings.byok.provider_id = clean_owned(settings.byok.provider_id);
    settings.byok.base_url = clean_owned(settings.byok.base_url);
    settings.byok.model = clean_owned(settings.byok.model);
    settings
        .local_cli
        .executable_paths
        .retain(|id, value| definition(id).is_some() && !value.trim().is_empty());
    for value in settings.local_cli.executable_paths.values_mut() {
        *value = value.trim().to_owned();
    }
    if let Some(provider_id) = settings.local_cli.provider_id.as_deref() {
        if definition(provider_id).is_none() {
            return Err(CliError::with_code(
                "unsupported_model_provider",
                format!("Unsupported Local CLI provider: {provider_id}"),
            ));
        }
    }
    if let Some(model) = settings.local_cli.model.as_deref() {
        validate_cli_value("model", model)?;
    }
    if let Some(reasoning) = settings.local_cli.reasoning.as_deref() {
        validate_cli_value("reasoning", reasoning)?;
    }
    Ok(settings)
}

fn clean_owned(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}

fn clean_optional(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn validate_cli_value(name: &str, value: &str) -> Result<(), CliError> {
    if value.len() > 160
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | ':' | '/'))
    {
        return Err(CliError::with_code(
            "invalid_model_gateway_settings",
            format!("Invalid {name} value."),
        ));
    }
    Ok(())
}

fn default_provider_from_env() -> Option<String> {
    match std::env::var("MYOPENPANELS_AGENT_PROVIDER").ok().as_deref() {
        Some("none") => None,
        Some("hermes") => Some("hermes".to_owned()),
        _ => Some("codex".to_owned()),
    }
}

fn definition(id: &str) -> Option<LocalCliDefinition> {
    LOCAL_CLI_DEFINITIONS
        .iter()
        .copied()
        .find(|item| item.id == id)
}

fn scan_local_cli(
    definition: LocalCliDefinition,
    configured_path: Option<String>,
    cwd: &Path,
) -> LocalCliInfo {
    let resolution = resolve_executable(definition.bin, configured_path.as_deref());
    let Some(path) = resolution.path.clone() else {
        let mut unavailable = unavailable_cli(definition.id, definition.name, definition.bin);
        unavailable.configured_path = configured_path;
        unavailable.diagnostic = resolution.diagnostic;
        return unavailable;
    };
    let version_output = run_process(
        &path,
        &owned_args(definition.version_args),
        None,
        Some(cwd),
        Duration::from_secs(5),
    );
    let Ok(version_output) = version_output else {
        let mut unavailable = unavailable_cli(definition.id, definition.name, definition.bin);
        unavailable.path = Some(path);
        unavailable.configured_path = configured_path;
        unavailable.diagnostic =
            Some("The executable was found but could not be started.".to_owned());
        return unavailable;
    };
    let version = first_non_empty(&version_output.stdout, &version_output.stderr)
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned);
    let (auth_status, auth_message) = probe_auth(definition, &path, cwd);
    let (models, models_source) = probe_models(definition, &path, cwd);
    LocalCliInfo {
        id: definition.id.to_owned(),
        name: definition.name.to_owned(),
        bin: definition.bin.to_owned(),
        available: true,
        path: Some(path),
        configured_path,
        version,
        auth_status,
        auth_message,
        diagnostic: resolution.diagnostic,
        models,
        models_source,
        reasoning_options: model_options(definition.reasoning_options),
    }
}

fn unavailable_cli(id: &str, name: &str, bin: &str) -> LocalCliInfo {
    let definition = definition(id);
    LocalCliInfo {
        id: id.to_owned(),
        name: name.to_owned(),
        bin: bin.to_owned(),
        available: false,
        path: None,
        configured_path: None,
        version: None,
        auth_status: "unknown".to_owned(),
        auth_message: None,
        diagnostic: None,
        models: definition
            .map(|item| model_options(item.fallback_models))
            .unwrap_or_else(|| model_options(DEFAULT_MODELS)),
        models_source: "fallback".to_owned(),
        reasoning_options: definition
            .map(|item| model_options(item.reasoning_options))
            .unwrap_or_default(),
    }
}

fn probe_auth(
    definition: LocalCliDefinition,
    executable: &str,
    cwd: &Path,
) -> (String, Option<String>) {
    if definition.auth_args.is_empty() {
        return ("unknown".to_owned(), None);
    }
    let args = owned_args(definition.auth_args);
    match run_process(executable, &args, None, Some(cwd), Duration::from_secs(8)) {
        Ok(output) if output.success => ("ok".to_owned(), None),
        Ok(output) => (
            "missing".to_owned(),
            Some(
                first_non_empty(&output.stderr, &output.stdout)
                    .chars()
                    .take(500)
                    .collect(),
            ),
        ),
        Err(error) => ("unknown".to_owned(), Some(error.message().to_owned())),
    }
}

fn probe_models(
    definition: LocalCliDefinition,
    executable: &str,
    cwd: &Path,
) -> (Vec<ModelOption>, String) {
    let probed = (definition.probe_models)(executable, cwd);
    match probed.filter(|models| models.len() > 1) {
        Some(models) => (models, "live".to_owned()),
        None => (
            model_options(definition.fallback_models),
            "fallback".to_owned(),
        ),
    }
}

fn probe_codex_models(executable: &str, cwd: &Path) -> Option<Vec<ModelOption>> {
    run_process(
        executable,
        &owned_args(&["debug", "models"]),
        None,
        Some(cwd),
        Duration::from_secs(10),
    )
    .ok()
    .filter(|output| output.success)
    .and_then(|output| parse_codex_models(&output.stdout))
}

fn probe_hermes_models(executable: &str, cwd: &Path) -> Option<Vec<ModelOption>> {
    probe_hermes_acp_models(executable, cwd).ok()
}

fn parse_codex_models(stdout: &str) -> Option<Vec<ModelOption>> {
    let parsed = serde_json::from_str::<Value>(stdout).ok()?;
    let entries = parsed.get("models")?.as_array()?;
    let mut result = model_options(DEFAULT_MODELS);
    let mut seen = HashSet::from(["default".to_owned()]);
    for entry in entries {
        if entry.get("visibility").and_then(Value::as_str) == Some("hidden") {
            continue;
        }
        let Some(id) = entry
            .get("slug")
            .or_else(|| entry.get("id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if !seen.insert(id.to_owned()) {
            continue;
        }
        let label = entry
            .get("display_name")
            .or_else(|| entry.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(id);
        result.push(ModelOption {
            id: id.to_owned(),
            label: label.to_owned(),
        });
    }
    Some(result)
}

fn probe_hermes_acp_models(executable: &str, cwd: &Path) -> Result<Vec<ModelOption>, CliError> {
    let mut command = Command::new(executable);
    command
        .args(["acp", "--accept-hooks"])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group(&mut command);
    let mut child = command.spawn().map_err(to_cli_error)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CliError::new("Hermes ACP stdout is unavailable."))?;
    let stderr = child.stderr.take();
    let (sender, receiver) = mpsc::channel::<String>();
    let stdout_reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let stderr_reader = thread::spawn(move || read_pipe(stderr));
    let stdin = child
        .stdin
        .as_mut()
        .ok_or_else(|| CliError::new("Hermes ACP stdin is unavailable."))?;
    write_json_line(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": 1,
                "clientCapabilities": { "terminal": false },
                "clientInfo": { "name": "myopenpanels-detect", "version": env!("CARGO_PKG_VERSION") }
            }
        }),
    )?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let result = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break Err(CliError::new("Hermes ACP model detection timed out."));
        }
        let line = match receiver.recv_timeout(remaining) {
            Ok(line) => line,
            Err(_) => break Err(CliError::new("Hermes ACP model detection timed out.")),
        };
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if message.get("id").and_then(Value::as_i64) == Some(1) {
            if let Some(error) = message.get("error") {
                break Err(CliError::new(format!(
                    "Hermes ACP initialize failed: {error}"
                )));
            }
            if let Some(stdin) = child.stdin.as_mut() {
                write_json_line(
                    stdin,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": 2,
                        "method": "session/new",
                        "params": { "cwd": cwd.display().to_string(), "mcpServers": [] }
                    }),
                )?;
            }
            continue;
        }
        if message.get("id").and_then(Value::as_i64) == Some(2) {
            if let Some(error) = message.get("error") {
                break Err(CliError::new(format!("Hermes ACP session failed: {error}")));
            }
            break Ok(normalize_acp_models(
                message.pointer("/result/models"),
                message.pointer("/result/configOptions"),
            ));
        }
    };
    terminate_process(&mut child);
    drop(child.stdin.take());
    let _ = stdout_reader.join();
    let _ = stderr_reader.join();
    result
}

fn normalize_acp_models(
    models: Option<&Value>,
    config_options: Option<&Value>,
) -> Vec<ModelOption> {
    let mut result = model_options(DEFAULT_MODELS);
    let mut seen = HashSet::from(["default".to_owned()]);
    if let Some(options) = config_options.and_then(Value::as_array) {
        for option in options {
            let id = option.get("id").and_then(Value::as_str).unwrap_or("");
            let category = option.get("category").and_then(Value::as_str).unwrap_or("");
            if normalize_token(id) != "model" && normalize_token(category) != "model" {
                continue;
            }
            let current = option.get("currentValue").and_then(Value::as_str);
            for value in option
                .get("options")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let id = value
                    .get("value")
                    .or_else(|| value.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                push_acp_model(
                    &mut result,
                    &mut seen,
                    id,
                    value.get("name").and_then(Value::as_str),
                    current == Some(id),
                );
            }
        }
    }
    if result.len() == 1 {
        let current = models
            .and_then(|value| value.get("currentModelId"))
            .and_then(Value::as_str);
        for model in models
            .and_then(|value| value.get("availableModels"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let id = model.get("modelId").and_then(Value::as_str).unwrap_or("");
            push_acp_model(
                &mut result,
                &mut seen,
                id,
                model.get("name").and_then(Value::as_str),
                current == Some(id),
            );
        }
    }
    result
}

fn push_acp_model(
    result: &mut Vec<ModelOption>,
    seen: &mut HashSet<String>,
    id: &str,
    name: Option<&str>,
    current: bool,
) {
    let id = id.trim();
    if id.is_empty() || !seen.insert(id.to_owned()) {
        return;
    }
    let label = name
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != id)
        .map(|name| format!("{name} ({id})"))
        .unwrap_or_else(|| id.to_owned());
    result.push(ModelOption {
        id: id.to_owned(),
        label: if current {
            format!("{label} (current)")
        } else {
            label
        },
    });
}

fn normalize_token(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !matches!(ch, ' ' | '_' | '-'))
        .flat_map(char::to_lowercase)
        .collect()
}

fn write_json_line(writer: &mut impl Write, value: &Value) -> Result<(), CliError> {
    serde_json::to_writer(&mut *writer, value).map_err(to_cli_error)?;
    writer.write_all(b"\n").map_err(to_cli_error)?;
    writer.flush().map_err(to_cli_error)
}

fn model_options(entries: &[(&str, &str)]) -> Vec<ModelOption> {
    entries
        .iter()
        .map(|(id, label)| ModelOption {
            id: (*id).to_owned(),
            label: (*label).to_owned(),
        })
        .collect()
}

fn owned_args(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

struct ExecutableResolution {
    path: Option<String>,
    diagnostic: Option<String>,
}

fn resolve_executable(bin: &str, configured_path: Option<&str>) -> ExecutableResolution {
    let configured_path = configured_path
        .map(str::trim)
        .filter(|path| !path.is_empty());
    if let Some(configured) = configured_path {
        let path = PathBuf::from(configured);
        if is_invocable_file(&path) {
            return ExecutableResolution {
                path: Some(path.display().to_string()),
                diagnostic: None,
            };
        }
    }
    let detected = executable_search_dirs()
        .into_iter()
        .flat_map(|directory| executable_candidates(&directory, bin))
        .find(|path| is_invocable_file(path))
        .map(|path| path.display().to_string());
    ExecutableResolution {
        path: detected,
        diagnostic: configured_path.map(|path| {
            format!("Configured executable is not usable: {path}. PATH detection was used instead.")
        }),
    }
}

fn executable_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join(".cargo/bin"));
        dirs.push(home.join("bin"));
    }
    dirs.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ]);
    let mut seen = HashSet::new();
    dirs.into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

fn executable_candidates(directory: &Path, bin: &str) -> Vec<PathBuf> {
    let base = directory.join(bin);
    if cfg!(windows) {
        vec![
            base.clone(),
            directory.join(format!("{bin}.exe")),
            directory.join(format!("{bin}.cmd")),
            directory.join(format!("{bin}.bat")),
        ]
    } else {
        vec![base]
    }
}

fn is_invocable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn codex_smoke_invocation(
    cwd: &Path,
    model: Option<&str>,
    reasoning: Option<&str>,
) -> LocalCliInvocation {
    let mut args = vec![
        "exec".to_owned(),
        "--json".to_owned(),
        "--ephemeral".to_owned(),
        "--ignore-rules".to_owned(),
        "--skip-git-repo-check".to_owned(),
        "--sandbox".to_owned(),
        "workspace-write".to_owned(),
        "-C".to_owned(),
        cwd.display().to_string(),
    ];
    push_codex_model_args(&mut args, model, reasoning);
    LocalCliInvocation {
        args,
        input: Some(SMOKE_PROMPT),
    }
}

fn hermes_smoke_invocation(
    _cwd: &Path,
    model: Option<&str>,
    _reasoning: Option<&str>,
) -> LocalCliInvocation {
    let mut args = vec!["--ignore-rules".to_owned()];
    if let Some(model) = model.filter(|value| *value != "default") {
        args.push("--model".to_owned());
        args.push(model.to_owned());
    }
    args.extend(["--oneshot".to_owned(), SMOKE_PROMPT.to_owned()]);
    LocalCliInvocation { args, input: None }
}

fn codex_task_command(executable: &str, model: Option<&str>, reasoning: Option<&str>) -> String {
    let mut args = vec![
        shell_quote(executable),
        "exec".to_owned(),
        "--json".to_owned(),
        "--ephemeral".to_owned(),
        "--ignore-rules".to_owned(),
        "--skip-git-repo-check".to_owned(),
        "--sandbox".to_owned(),
        "workspace-write".to_owned(),
        "-c".to_owned(),
        "sandbox_workspace_write.network_access=true".to_owned(),
        "-C".to_owned(),
        "\"$MYOPENPANELS_EXECUTION_WORKSPACE\"".to_owned(),
    ];
    push_codex_model_args(&mut args, clean_optional(model), clean_optional(reasoning));
    args.join(" ")
}

fn hermes_task_command(executable: &str, model: Option<&str>, _reasoning: Option<&str>) -> String {
    let mut args = vec![shell_quote(executable), "--ignore-rules".to_owned()];
    if let Some(model) = clean_optional(model).filter(|value| *value != "default") {
        args.push("--model".to_owned());
        args.push(shell_quote(model));
    }
    args.extend(["--oneshot".to_owned(), "\"$(cat)\"".to_owned()]);
    args.join(" ")
}

fn push_codex_model_args(args: &mut Vec<String>, model: Option<&str>, reasoning: Option<&str>) {
    if let Some(model) = model.filter(|value| *value != "default") {
        args.push("--model".to_owned());
        args.push(model.to_owned());
    }
    if let Some(reasoning) = reasoning.filter(|value| *value != "default") {
        args.push("-c".to_owned());
        args.push(format!("model_reasoning_effort=\"{reasoning}\""));
    }
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':'))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

struct ProcessOutput {
    success: bool,
    status_code: Option<i32>,
    timed_out: bool,
    stdout: String,
    stderr: String,
}

fn run_process(
    executable: &str,
    args: &[String],
    input: Option<&str>,
    cwd: Option<&Path>,
    timeout: Duration,
) -> Result<ProcessOutput, CliError> {
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    configure_process_group(&mut command);
    let mut child = command.spawn().map_err(to_cli_error)?;
    if let (Some(input), Some(stdin)) = (input, child.stdin.as_mut()) {
        stdin.write_all(input.as_bytes()).map_err(to_cli_error)?;
        stdin.write_all(b"\n").map_err(to_cli_error)?;
    }
    drop(child.stdin.take());
    let stdout_reader = {
        let stdout = child.stdout.take();
        thread::spawn(move || read_pipe(stdout))
    };
    let stderr_reader = {
        let stderr = child.stderr.take();
        thread::spawn(move || read_pipe(stderr))
    };
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(to_cli_error)? {
            break status;
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            terminate_process(&mut child);
            break child.wait().map_err(to_cli_error)?;
        }
        thread::sleep(Duration::from_millis(25));
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| CliError::new("Process stdout reader failed."))?
        .map_err(to_cli_error)?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| CliError::new("Process stderr reader failed."))?
        .map_err(to_cli_error)?;
    Ok(ProcessOutput {
        success: status.success() && !timed_out,
        status_code: status.code(),
        timed_out,
        stdout: String::from_utf8_lossy(&stdout[..stdout.len().min(PROCESS_OUTPUT_LIMIT)])
            .to_string(),
        stderr: String::from_utf8_lossy(&stderr[..stderr.len().min(PROCESS_OUTPUT_LIMIT)])
            .to_string(),
    })
}

fn read_pipe(mut pipe: Option<impl Read>) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    if let Some(pipe) = pipe.as_mut() {
        pipe.read_to_end(&mut bytes)?;
    }
    Ok(bytes)
}

fn assistant_sample(provider_id: &str, stdout: &str) -> String {
    if provider_id != "codex" {
        return stdout.trim().to_owned();
    }
    let mut messages = Vec::new();
    for line in stdout.lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if event.get("type").and_then(Value::as_str) == Some("item.completed")
            && event.pointer("/item/type").and_then(Value::as_str) == Some("agent_message")
        {
            if let Some(text) = event.pointer("/item/text").and_then(Value::as_str) {
                messages.push(text.to_owned());
            }
        }
    }
    messages
        .last()
        .cloned()
        .unwrap_or_else(|| stdout.trim().to_owned())
}

fn first_non_empty<'a>(primary: &'a str, fallback: &'a str) -> &'a str {
    if primary.trim().is_empty() {
        fallback
    } else {
        primary
    }
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_process(child: &mut Child) {
    let group = format!("-{}", child.id());
    let _ = Command::new("kill").args(["-TERM", &group]).status();
    thread::sleep(Duration::from_millis(150));
    if matches!(child.try_wait(), Ok(None)) {
        let _ = Command::new("kill").args(["-KILL", &group]).status();
    }
}

#[cfg(not(unix))]
fn terminate_process(child: &mut Child) {
    let _ = child.kill();
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codex_debug_model_catalog() {
        let models = parse_codex_models(
            r#"{"models":[{"slug":"gpt-5.4","display_name":"GPT 5.4"},{"slug":"hidden","visibility":"hidden"}]}"#,
        )
        .expect("models");
        assert_eq!(models[0].id, "default");
        assert_eq!(models[1].id, "gpt-5.4");
        assert_eq!(models[1].label, "GPT 5.4");
        assert_eq!(models.len(), 2);
    }

    #[test]
    fn codex_task_command_applies_model_and_reasoning() {
        let command = (definition("codex").unwrap().task_command)(
            "/opt/bin/codex",
            Some("gpt-5.4"),
            Some("high"),
        );
        assert!(command.contains("--model gpt-5.4"));
        assert!(command.contains("model_reasoning_effort=\"high\""));
        assert!(command.contains("$MYOPENPANELS_EXECUTION_WORKSPACE"));
        assert!(!command.contains("--ignore-user-config"));
    }

    #[test]
    fn codex_task_command_allows_the_task_broker_connection() {
        let command = (definition("codex").unwrap().task_command)("/opt/bin/codex", None, None);

        assert!(command.contains("sandbox_workspace_write.network_access=true"));
    }

    #[test]
    fn rejects_shell_metacharacters_in_model_values() {
        let mut settings = ModelGatewaySettings::default();
        settings.local_cli.model = Some("gpt-5; touch bad".to_owned());
        assert!(normalize_settings(settings).is_err());
    }

    #[test]
    fn settings_are_persisted_in_normalized_gateway_tables() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("model-gateway-storage-test"),
        )
        .expect("paths");
        let mut settings = ModelGatewaySettings::default();
        settings.local_cli.provider_id = Some("hermes".to_owned());
        settings.local_cli.model = Some("anthropic/claude-sonnet-4".to_owned());
        settings
            .local_cli
            .executable_paths
            .insert("hermes".to_owned(), "/opt/tools/hermes".to_owned());
        write_settings(&paths, settings).expect("write settings");

        let persisted = read_settings(&paths).expect("read settings");
        assert_eq!(persisted.local_cli.provider_id.as_deref(), Some("hermes"));
        assert_eq!(
            persisted.local_cli.model.as_deref(),
            Some("anthropic/claude-sonnet-4")
        );
        assert_eq!(
            persisted
                .local_cli
                .executable_paths
                .get("hermes")
                .map(String::as_str),
            Some("/opt/tools/hermes")
        );
        let storage = Storage::open(&paths).expect("storage");
        let active_connection: Option<String> = storage
            .connection()
            .query_row(
                "SELECT active_local_connection_id FROM model_gateway_config WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .expect("active connection");
        assert_eq!(active_connection.as_deref(), Some("local-cli:hermes"));
        assert!(storage
            .read_setting("model_gateway", "settings")
            .expect("legacy setting")
            .is_none());
    }

    #[cfg(unix)]
    #[test]
    fn worker_specs_include_all_available_local_channels_in_priority_order() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let codex = temp.path().join("codex");
        let hermes = temp.path().join("hermes");
        for executable in [&codex, &hermes] {
            fs::write(executable, "#!/bin/sh\nexit 0\n").expect("executable");
            let mut permissions = fs::metadata(executable).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(executable, permissions).expect("permissions");
        }
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("model-gateway-worker-specs-test"),
        )
        .expect("paths");
        let mut settings = ModelGatewaySettings::default();
        settings.local_cli.provider_id = Some("codex".to_owned());
        settings
            .local_cli
            .executable_paths
            .insert("codex".to_owned(), codex.to_string_lossy().into_owned());
        settings
            .local_cli
            .executable_paths
            .insert("hermes".to_owned(), hermes.to_string_lossy().into_owned());
        write_settings(&paths, settings).expect("settings");

        let specs = worker_specs(&paths).expect("worker specs");
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].connection_id, "local-cli:codex");
        assert_eq!(specs[1].connection_id, "local-cli:hermes");
    }

    #[test]
    fn runtime_registry_upserts_new_cli_adapters_without_schema_changes() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("model-gateway-registry-test"),
        )
        .expect("paths");
        let mut storage = Storage::open(&paths).expect("storage");
        let example = LocalCliDefinition {
            id: "example-agent",
            name: "Example Agent",
            bin: "example-agent",
            adapter_version: 1,
            version_args: &["--version"],
            auth_args: &[],
            fallback_models: DEFAULT_MODELS,
            reasoning_options: &[],
            probe_models: probe_codex_models,
            smoke_invocation: codex_smoke_invocation,
            task_command: codex_task_command,
        };
        sync_builtin_local_cli_connections(&mut storage, &[example]).expect("initial sync");
        storage
            .connection()
            .execute(
                r#"
                UPDATE model_gateway_connections
                SET executable_path = '/opt/example-agent',
                    config_json = json_set(config_json, '$.userOption', 'preserved')
                WHERE id = 'local-cli:example-agent'
                "#,
                [],
            )
            .expect("customize connection");

        let upgraded = LocalCliDefinition {
            adapter_version: 2,
            ..example
        };
        sync_builtin_local_cli_connections(&mut storage, &[upgraded]).expect("upgrade sync");
        let connection: (String, i64, String, String) = storage
            .connection()
            .query_row(
                r#"
                SELECT executable_path,
                       json_extract(config_json, '$.adapterVersion'),
                       json_extract(config_json, '$.binaryName'),
                       json_extract(config_json, '$.userOption')
                FROM model_gateway_connections
                WHERE id = 'local-cli:example-agent'
                "#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("connection");
        assert_eq!(
            connection,
            (
                "/opt/example-agent".to_owned(),
                2,
                "example-agent".to_owned(),
                "preserved".to_owned()
            )
        );
        let revision_before = storage.read_change_seq().expect("revision before");
        sync_builtin_local_cli_connections(&mut storage, &[upgraded]).expect("steady sync");
        assert_eq!(
            storage.read_change_seq().expect("revision after"),
            revision_before
        );

        sync_builtin_local_cli_connections(&mut storage, LOCAL_CLI_DEFINITIONS)
            .expect("remove stale adapter");
        let enabled: i64 = storage
            .connection()
            .query_row(
                "SELECT enabled FROM model_gateway_connections WHERE id = 'local-cli:example-agent'",
                [],
                |row| row.get(0),
            )
            .expect("enabled");
        assert_eq!(enabled, 0);
    }

    #[test]
    fn custom_api_profiles_use_generic_connections_and_credential_references() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("model-gateway-byok-storage-test"),
        )
        .expect("paths");
        let storage = Storage::open(&paths).expect("storage");
        let now = crate::control::now_iso();
        for (id, base_url, credential_ref) in [
            (
                "byok:openai-compatible:primary",
                "https://models.example.test/v1",
                "keychain:model-gateway/primary",
            ),
            (
                "byok:openai-compatible:backup",
                "https://backup.example.test/v1",
                "keychain:model-gateway/backup",
            ),
        ] {
            storage
                .connection()
                .execute(
                    r#"
                    INSERT INTO model_gateway_connections (
                      id, transport, provider_id, display_name, base_url,
                      credential_ref, model, config_json, enabled,
                      created_at, updated_at
                    )
                    VALUES (?, 'byok', 'openai-compatible', ?, ?, ?,
                            'gpt-5.4', '{"requestTimeoutMs":120000}', 1, ?, ?)
                    "#,
                    params![id, id, base_url, credential_ref, now, now],
                )
                .expect("BYOK connection");
        }
        storage
            .connection()
            .execute(
                "UPDATE model_gateway_config SET mode = 'byok', active_byok_connection_id = 'byok:openai-compatible:primary', updated_at = ? WHERE id = 1",
                [now],
            )
            .expect("activate BYOK");
        drop(storage);

        let settings = read_settings(&paths).expect("read settings");
        assert_eq!(settings.mode, "byok");
        assert_eq!(
            settings.byok.provider_id.as_deref(),
            Some("openai-compatible")
        );
        assert_eq!(
            settings.byok.base_url.as_deref(),
            Some("https://models.example.test/v1")
        );
        let storage = Storage::open(&paths).expect("storage");
        let profile_count: i64 = storage
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM model_gateway_connections WHERE transport = 'byok' AND provider_id = 'openai-compatible'",
                [],
                |row| row.get(0),
            )
            .expect("profile count");
        assert_eq!(profile_count, 2);
        let credential_ref: String = storage
            .connection()
            .query_row(
                "SELECT credential_ref FROM model_gateway_connections WHERE id = 'byok:openai-compatible:primary'",
                [],
                |row| row.get(0),
            )
            .expect("credential ref");
        assert_eq!(credential_ref, "keychain:model-gateway/primary");
    }

    #[test]
    fn normalizes_legacy_acp_model_shape() {
        let models = json!({
            "currentModelId": "openai-codex:gpt-5.4",
            "availableModels": [
                { "modelId": "openai-codex:gpt-5.4", "name": "GPT 5.4" },
                { "modelId": "qwen:qwen3", "name": "Qwen 3" }
            ]
        });
        let normalized = normalize_acp_models(Some(&models), None);
        assert_eq!(normalized.len(), 3);
        assert!(normalized[1].label.contains("current"));
    }
}
