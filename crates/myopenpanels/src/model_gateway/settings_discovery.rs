use crate::error::CliError;
use crate::paths::MyOpenPanelsPaths;
use crate::storage::Storage;
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
const MODEL_GATEWAY_SETTINGS_KEY: &str = "settings";
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

pub fn read_settings(paths: &MyOpenPanelsPaths) -> Result<ModelGatewaySettings, CliError> {
    let storage = Storage::open(paths)?;
    let settings = storage
        .read_setting(MODEL_GATEWAY_SETTINGS_NAMESPACE, MODEL_GATEWAY_SETTINGS_KEY)?
        .map(|raw| serde_json::from_str::<ModelGatewaySettings>(&raw).map_err(to_cli_error))
        .transpose()?
        .unwrap_or_default();
    normalize_settings(settings)
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
    let raw = serde_json::to_string(&settings).map_err(to_cli_error)?;
    Storage::open(paths)?.write_setting(
        MODEL_GATEWAY_SETTINGS_NAMESPACE,
        MODEL_GATEWAY_SETTINGS_KEY,
        &raw,
    )?;
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

    let settings = read_settings(paths)?;
    if settings.mode != "localCli" {
        return Ok(Vec::new());
    }
    let mut specs = Vec::new();
    for (position, provider_id) in settings.local_cli.provider_order.iter().enumerate() {
        let Some(definition) = definition(provider_id) else {
            continue;
        };
        let configured_path = settings.local_cli.executable_paths.get(provider_id);
        let Some(executable) =
            resolve_executable(definition, configured_path.map(String::as_str)).path
        else {
            continue;
        };
        let model = settings.local_cli.provider_models.get(provider_id);
        let reasoning = settings.local_cli.provider_reasoning.get(provider_id);
        let command = (definition.task_command)(
            &executable,
            model.map(String::as_str),
            reasoning.map(String::as_str),
        );
        let connection_id = format!("local-cli:{provider_id}");
        let key = format!(
            "{}:{}:{}:{}:{}",
            connection_id,
            executable,
            model.map(String::as_str).unwrap_or("default"),
            reasoning.map(String::as_str).unwrap_or("default"),
            position,
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
