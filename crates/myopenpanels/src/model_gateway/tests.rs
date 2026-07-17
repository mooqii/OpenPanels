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

    #[test]
    fn new_gateway_storage_activates_only_the_primary_local_channel() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("model-gateway-default-channel-test"),
        )
        .expect("paths");

        let settings = read_settings(&paths).expect("settings");
        assert_eq!(settings.local_cli.provider_id.as_deref(), Some("codex"));
        assert_eq!(settings.local_cli.provider_order, ["codex"]);
        assert_eq!(settings.local_cli.enabled_provider_ids, ["codex"]);
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
        settings.local_cli.enabled_provider_ids = vec!["codex".to_owned(), "hermes".to_owned()];
        settings.local_cli.provider_order = vec!["codex".to_owned(), "hermes".to_owned()];
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

        let mut settings = read_settings(&paths).expect("persisted settings");
        settings.local_cli.provider_order = vec!["hermes".to_owned(), "codex".to_owned()];
        settings.local_cli.enabled_provider_ids = settings.local_cli.provider_order.clone();
        settings.local_cli.provider_id = Some("hermes".to_owned());
        write_settings(&paths, settings).expect("reordered settings");
        let reordered = worker_specs(&paths).expect("reordered worker specs");
        assert_eq!(reordered[0].connection_id, "local-cli:hermes");
        assert_eq!(reordered[1].connection_id, "local-cli:codex");
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
        let connection: (String, i64, String, String, i64) = storage
            .connection()
            .query_row(
                r#"
                SELECT executable_path,
                       json_extract(config_json, '$.adapterVersion'),
                       json_extract(config_json, '$.binaryName'),
                       json_extract(config_json, '$.userOption'),
                       enabled
                FROM model_gateway_connections
                WHERE id = 'local-cli:example-agent'
                "#,
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .expect("connection");
        assert_eq!(
            connection,
            (
                "/opt/example-agent".to_owned(),
                2,
                "example-agent".to_owned(),
                "preserved".to_owned(),
                0
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
