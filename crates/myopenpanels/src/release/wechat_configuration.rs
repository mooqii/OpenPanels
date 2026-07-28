const WECHAT_CREDENTIALS_FILE: &str = "secrets/wechat-official-account.json";
const WECHAT_PUBLIC_IP_URL: &str = "https://api.ipify.org";
const WECHAT_CONFIGURATION_TIMEOUT_SECS: u64 = 8;

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredWechatCredentials {
    app_id: String,
    app_secret: String,
    validated_at: String,
    validated_public_ip: String,
}

struct WechatCredentials {
    app_id: String,
    app_secret: String,
}

pub fn wechat_configuration_status(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let stored = read_stored_wechat_credentials(paths)?;
    let environment = environment_wechat_credentials();
    let credentials = stored
        .as_ref()
        .map(|value| WechatCredentials {
            app_id: value.app_id.clone(),
            app_secret: value.app_secret.clone(),
        })
        .or(environment);
    let public_ip = current_public_ip().ok();
    let ready = stored.as_ref().is_some_and(|value| {
        public_ip.as_deref() == Some(value.validated_public_ip.as_str())
    });
    Ok(wechat_configuration_payload(
        credentials.as_ref().map(|value| value.app_id.as_str()),
        stored.as_ref().map(|value| value.validated_at.as_str()),
        stored
            .as_ref()
            .map(|value| value.validated_public_ip.as_str()),
        public_ip.as_deref(),
        credentials.is_some(),
        ready,
        if ready {
            None
        } else if credentials.is_none() {
            Some("wechat_credentials_missing")
        } else if public_ip.is_none() {
            Some("wechat_public_ip_unavailable")
        } else {
            Some("wechat_configuration_validation_required")
        },
        false,
    ))
}

pub fn validate_and_save_wechat_configuration(
    paths: &MyOpenPanelsPaths,
    requested_app_id: &str,
    requested_app_secret: Option<&str>,
) -> Result<Value, CliError> {
    let app_id = requested_app_id.trim();
    if app_id.is_empty() {
        return Ok(configuration_failure(
            None,
            current_public_ip().ok().as_deref(),
            "wechat_app_id_missing",
            "Enter the WeChat Official Account AppID.",
            "credentials",
        ));
    }
    let requested_secret = requested_app_secret
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let existing = match read_wechat_credentials(paths) {
        Ok(value) => value,
        Err(_) if requested_secret.is_some() => None,
        Err(error) => return Err(error),
    };
    let has_reusable_existing = existing
        .as_ref()
        .is_some_and(|value| value.app_id == app_id);
    let app_secret = match requested_secret {
        Some(value) => Some(value),
        None => existing
            .filter(|value| value.app_id == app_id)
            .map(|value| value.app_secret),
    };
    let Some(app_secret) = app_secret else {
        return Ok(configuration_failure(
            Some(app_id),
            current_public_ip().ok().as_deref(),
            "wechat_app_secret_missing",
            "Enter the WeChat Official Account AppSecret.",
            "credentials",
        ));
    };
    let public_ip = match current_public_ip() {
        Ok(value) => value,
        Err(_) => {
            let mut failure = configuration_failure(
                Some(app_id),
                None,
                "wechat_public_ip_unavailable",
                "The Studio server could not determine its public egress IP.",
                "publicIp",
            );
            failure["configured"] = json!(has_reusable_existing);
            return Ok(failure);
        }
    };
    let api = WechatHttpApi::new();
    let access_token = match api.access_token(app_id, &app_secret) {
        Ok(value) => value,
        Err(error) => {
            return Ok(configuration_api_failure(
                Some(app_id),
                Some(&public_ip),
                error,
                "credentials",
                has_reusable_existing,
            ));
        }
    };
    if let Err(error) = api.validate_draft_permission(&access_token) {
        return Ok(configuration_api_failure(
            Some(app_id),
            Some(&public_ip),
            error,
            "draftApi",
            has_reusable_existing,
        ));
    }
    let validated_at = now_iso();
    write_stored_wechat_credentials(
        paths,
        &StoredWechatCredentials {
            app_id: app_id.to_owned(),
            app_secret,
            validated_at: validated_at.clone(),
            validated_public_ip: public_ip.clone(),
        },
    )?;
    Ok(wechat_configuration_payload(
        Some(app_id),
        Some(&validated_at),
        Some(&public_ip),
        Some(&public_ip),
        true,
        true,
        None,
        true,
    ))
}

fn wechat_configuration_payload(
    app_id: Option<&str>,
    validated_at: Option<&str>,
    validated_public_ip: Option<&str>,
    public_ip: Option<&str>,
    configured: bool,
    ready: bool,
    reason_code: Option<&str>,
    saved: bool,
) -> Value {
    json!({
        "appId": app_id,
        "configured": configured,
        "publicIp": public_ip,
        "ready": ready,
        "reasonCode": reason_code,
        "saved": saved,
        "validatedAt": validated_at,
        "validatedPublicIp": validated_public_ip,
        "wechatObservedIp": null,
        "checks": {
            "credentials": if ready { "passed" } else if configured { "pending" } else { "failed" },
            "draftApi": if ready { "passed" } else { "pending" },
            "ipAllowlist": if ready { "passed" } else { "pending" },
            "publicIp": if public_ip.is_some() { "passed" } else { "failed" },
        },
    })
}

fn configuration_failure(
    app_id: Option<&str>,
    public_ip: Option<&str>,
    reason_code: &str,
    summary: &str,
    failed_check: &str,
) -> Value {
    let mut payload = wechat_configuration_payload(
        app_id,
        None,
        None,
        public_ip,
        false,
        false,
        Some(reason_code),
        false,
    );
    payload["summary"] = json!(summary);
    payload["checks"][failed_check] = json!("failed");
    payload
}

fn configuration_api_failure(
    app_id: Option<&str>,
    public_ip: Option<&str>,
    error: WechatApiFailure,
    failed_check: &str,
    has_reusable_existing: bool,
) -> Value {
    let wechat_observed_ip = match &error {
        WechatApiFailure::Api {
            code: 40164,
            message,
        } => wechat_error_observed_ip(message.as_deref()),
        _ => None,
    };
    let (reason_code, summary, actual_failed_check) = match error {
        WechatApiFailure::Api {
            code: 40013 | 40125,
            ..
        } => (
            "wechat_credentials_rejected",
            "WeChat rejected the AppID or AppSecret.",
            "credentials",
        ),
        WechatApiFailure::Api { code: 40164, .. } => (
            "wechat_ip_not_allowed",
            "WeChat rejected this public IP. Add it to the Official Account IP allowlist.",
            "ipAllowlist",
        ),
        WechatApiFailure::Api { code: 48001, .. } => (
            "wechat_api_unauthorized",
            "This Official Account does not grant the draft-management API permission.",
            "draftApi",
        ),
        WechatApiFailure::Api { .. } => (
            "wechat_api_rejected",
            "WeChat rejected the configuration validation request.",
            failed_check,
        ),
        WechatApiFailure::InvalidResponse => (
            "wechat_api_invalid_response",
            "WeChat returned an unreadable configuration validation response.",
            failed_check,
        ),
        WechatApiFailure::Transport => (
            "wechat_api_unavailable",
            "Studio could not reach the WeChat API.",
            failed_check,
        ),
    };
    let mut payload = configuration_failure(
        app_id,
        public_ip,
        reason_code,
        summary,
        actual_failed_check,
    );
    payload["configured"] = json!(has_reusable_existing);
    payload["wechatObservedIp"] = json!(wechat_observed_ip);
    match reason_code {
        "wechat_ip_not_allowed" => {
            payload["checks"]["credentials"] = json!("passed");
        }
        "wechat_api_unauthorized" => {
            payload["checks"]["credentials"] = json!("passed");
            payload["checks"]["ipAllowlist"] = json!("passed");
        }
        _ => {}
    }
    payload
}

fn read_wechat_credentials(paths: &MyOpenPanelsPaths) -> Result<Option<WechatCredentials>, CliError> {
    if let Some(stored) = read_stored_wechat_credentials(paths)? {
        return Ok(Some(WechatCredentials {
            app_id: stored.app_id,
            app_secret: stored.app_secret,
        }));
    }
    Ok(environment_wechat_credentials())
}

fn environment_wechat_credentials() -> Option<WechatCredentials> {
    Some(WechatCredentials {
        app_id: non_empty_env(WECHAT_APP_ID_ENV)?,
        app_secret: non_empty_env(WECHAT_APP_SECRET_ENV)?,
    })
}

fn credentials_path(paths: &MyOpenPanelsPaths) -> PathBuf {
    paths.storage_dir.join(WECHAT_CREDENTIALS_FILE)
}

fn read_stored_wechat_credentials(
    paths: &MyOpenPanelsPaths,
) -> Result<Option<StoredWechatCredentials>, CliError> {
    let path = credentials_path(paths);
    let bytes = match fs::read(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(CliError::new(error.to_string())),
    };
    let credentials =
        serde_json::from_slice::<StoredWechatCredentials>(&bytes).map_err(|_| {
            CliError::with_code(
                "wechat_configuration_invalid",
                "The saved WeChat API configuration is invalid.",
            )
        })?;
    if credentials.app_id.trim().is_empty()
        || credentials.app_secret.trim().is_empty()
        || credentials.validated_public_ip.parse::<std::net::IpAddr>().is_err()
    {
        return Err(CliError::with_code(
            "wechat_configuration_invalid",
            "The saved WeChat API configuration is invalid.",
        ));
    }
    Ok(Some(credentials))
}

fn write_stored_wechat_credentials(
    paths: &MyOpenPanelsPaths,
    credentials: &StoredWechatCredentials,
) -> Result<(), CliError> {
    use std::io::Write;

    let destination = credentials_path(paths);
    let parent = destination
        .parent()
        .ok_or_else(|| CliError::new("WeChat credential directory is invalid."))?;
    fs::create_dir_all(parent).map_err(|error| CliError::new(error.to_string()))?;
    secure_directory_permissions(parent)?;
    let temporary = parent.join(format!(
        ".wechat-official-account-{}.tmp",
        crate::ids::random_id("credential")
    ));
    let bytes = serde_json::to_vec(credentials).map_err(|error| CliError::new(error.to_string()))?;
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|error| CliError::new(error.to_string()))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| CliError::new(error.to_string()))?;
    fs::rename(&temporary, &destination).map_err(|error| CliError::new(error.to_string()))?;
    secure_file_permissions(&destination)
}

fn secure_directory_permissions(path: &Path) -> Result<(), CliError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| CliError::new(error.to_string()))?;
    }
    Ok(())
}

fn secure_file_permissions(path: &Path) -> Result<(), CliError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| CliError::new(error.to_string()))?;
    }
    Ok(())
}

fn current_public_ip() -> Result<String, CliError> {
    let response = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(
            WECHAT_CONFIGURATION_TIMEOUT_SECS,
        ))
        .build()
        .get(WECHAT_PUBLIC_IP_URL)
        .call()
        .map_err(|_| {
            CliError::with_code(
                "wechat_public_ip_unavailable",
                "Unable to determine the Studio server public IP.",
            )
        })?;
    let value = response.into_string().map_err(|_| {
        CliError::with_code(
            "wechat_public_ip_unavailable",
            "Unable to read the Studio server public IP.",
        )
    })?;
    value
        .trim()
        .parse::<std::net::IpAddr>()
        .map(|value| value.to_string())
        .map_err(|_| {
            CliError::with_code(
                "wechat_public_ip_unavailable",
                "The public IP service returned an invalid address.",
            )
        })
}

#[cfg(test)]
mod wechat_configuration_tests {
    use super::*;

    #[test]
    fn stored_credentials_use_private_permissions_and_never_enter_status_payload() {
        let temp = tempfile::tempdir().expect("temp dir");
        let project = temp.path().join("project");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(temp.path().join("storage").to_str().unwrap()),
            Some("wechat-config"),
        )
        .expect("paths");
        let stored = StoredWechatCredentials {
            app_id: "wx-app-id".to_owned(),
            app_secret: "never-return-this-secret".to_owned(),
            validated_at: "2026-07-28T00:00:00Z".to_owned(),
            validated_public_ip: "203.0.113.4".to_owned(),
        };
        write_stored_wechat_credentials(&paths, &stored).expect("write credentials");
        let raw = fs::read_to_string(credentials_path(&paths)).expect("credential file");
        assert!(raw.contains("never-return-this-secret"));
        let loaded = read_stored_wechat_credentials(&paths)
            .expect("read credentials")
            .expect("stored credentials");
        let payload = wechat_configuration_payload(
            Some(&loaded.app_id),
            Some(&loaded.validated_at),
            Some(&loaded.validated_public_ip),
            Some(&loaded.validated_public_ip),
            true,
            true,
            None,
            false,
        );
        assert!(!payload.to_string().contains("never-return-this-secret"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(credentials_path(&paths))
                    .expect("metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn validation_classifies_credentials_allowlist_and_permission_failures() {
        let credentials = configuration_api_failure(
            Some("wx-app-id"),
            Some("203.0.113.4"),
            WechatApiFailure::Api {
                code: 40125,
                message: None,
            },
            "credentials",
            true,
        );
        assert_eq!(credentials["reasonCode"], "wechat_credentials_rejected");
        assert_eq!(credentials["checks"]["credentials"], "failed");

        let allowlist = configuration_api_failure(
            Some("wx-app-id"),
            Some("203.0.113.4"),
            WechatApiFailure::Api {
                code: 40164,
                message: Some(
                    "invalid ip 198.51.100.9, not in whitelist".to_owned(),
                ),
            },
            "credentials",
            true,
        );
        assert_eq!(allowlist["reasonCode"], "wechat_ip_not_allowed");
        assert_eq!(allowlist["checks"]["credentials"], "passed");
        assert_eq!(allowlist["checks"]["ipAllowlist"], "failed");
        assert_eq!(allowlist["wechatObservedIp"], "198.51.100.9");

        let permission = configuration_api_failure(
            Some("wx-app-id"),
            Some("203.0.113.4"),
            WechatApiFailure::Api {
                code: 48001,
                message: None,
            },
            "draftApi",
            true,
        );
        assert_eq!(permission["reasonCode"], "wechat_api_unauthorized");
        assert_eq!(permission["checks"]["ipAllowlist"], "passed");
        assert_eq!(permission["checks"]["draftApi"], "failed");
    }
}

include!("wechat_direct.rs");
