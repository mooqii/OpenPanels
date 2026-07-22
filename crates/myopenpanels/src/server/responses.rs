fn with_build_info(payload: impl Serialize, state: &AppState) -> Value {
    let mut value = serde_json::to_value(payload).unwrap_or_else(|_| json!({}));
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "buildInfo".to_owned(),
            serde_json::to_value(&state.build_info).unwrap_or_else(|_| json!({})),
        );
        object.insert(
            "agentWorker".to_owned(),
            bridge::read_bridge_status(&state.paths)
                .unwrap_or_else(|_| json!({ "status": "idle" })),
        );
        if let Ok(operations) = Storage::open(&state.paths)
            .and_then(|storage| storage.list_agent_operations(Some(&state.paths.context_id), None))
        {
            object.insert("agentOperations".to_owned(), json!(operations));
        }
        if let Ok(task_payload) = tasks::list_tasks(&state.paths, tasks::TaskListFilter::default())
        {
            for key in [
                "tasks",
                "pendingCount",
                "readyCount",
                "blockedCount",
                "unhandledCount",
                "runningCount",
            ] {
                if let Some(task_value) = task_payload.get(key) {
                    object.insert(key.to_owned(), task_value.clone());
                }
            }
            if let Some(pending_count) = task_payload.get("pendingCount") {
                object.insert("pendingTaskCount".to_owned(), pending_count.clone());
            }
        }
    }
    value
}

fn current_build_info() -> StudioBuildInfo {
    let is_development = std::env::current_exe()
        .ok()
        .and_then(|path| path.to_str().map(str::to_owned))
        .map(|path| {
            path.contains("/target/debug/")
                || path.contains("\\target\\debug\\")
                || std::env::var("MYOPENPANELS_DEV").is_ok()
        })
        .unwrap_or_else(|| std::env::var("MYOPENPANELS_DEV").is_ok());
    if !is_development {
        return StudioBuildInfo {
            agent_cli: crate::cli_identity::agent_cli_executable(),
            build_time: None,
            channel: "release",
            label: env!("CARGO_PKG_VERSION").to_owned(),
            version: env!("CARGO_PKG_VERSION"),
        };
    }

    StudioBuildInfo {
        agent_cli: crate::cli_identity::agent_cli_executable(),
        build_time: development_build_time(),
        channel: "development",
        label: "dev".to_owned(),
        version: env!("CARGO_PKG_VERSION"),
    }
}

fn development_build_time() -> Option<String> {
    let modified = std::env::current_exe()
        .ok()
        .and_then(|path| fs::metadata(path).ok())
        .and_then(|metadata| metadata.modified().ok())?;
    let datetime: chrono::DateTime<chrono::Utc> = modified.into();
    Some(datetime.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

fn json_response(status: StatusCode, payload: &impl serde::Serialize) -> Response {
    match serde_json::to_vec(payload) {
        Ok(bytes) => bytes_response(status, bytes, "application/json"),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

fn no_store_response(response: Response) -> Response {
    cache_control_response(response, "no-store")
}

fn cache_control_response(mut response: Response, value: &'static str) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(value));
    response
}

fn json_error(status: StatusCode, message: &str) -> Response {
    bytes_response(
        status,
        serde_json::json!({ "error": message })
            .to_string()
            .into_bytes(),
        "application/json",
    )
}

fn json_cli_error(error: &CliError) -> Response {
    bytes_response(
        status_for_cli_error(error),
        serde_json::json!({
            "code": error.code(),
            "error": error.message(),
        })
        .to_string()
        .into_bytes(),
        "application/json",
    )
}

fn status_for_cli_error(error: &CliError) -> StatusCode {
    match error.code() {
        Some(
            "no_current_project"
            | "panel_not_found"
            | "project_not_found"
            | "task_not_found"
            | "workflow_run_not_found"
            | "target_not_found"
            | "model_gateway_connection_not_found"
            | "writing_skill_not_found"
            | "writing_refinement_skill_not_found"
            | "publishing_skill_not_found"
            | "publishing_release_not_found"
            | "publishing_attempt_not_found"
            | "publishing_source_not_found"
            | "typesetting_publication_not_found"
            | "typesetting_cover_skill_not_found"
            | "typesetting_layout_skill_not_found"
            | "writing_skill_file_not_found"
            | "skill_not_found"
            | "recommended_skill_not_found"
            | "device_skill_not_found"
            | "skill_module_not_found"
            | "skill_file_not_found",
        ) => StatusCode::NOT_FOUND,
        Some("unauthorized_target" | "invalid_lease" | "lease_expired" | "execution_fenced") => {
            StatusCode::UNAUTHORIZED
        }
        Some(
            "task_not_claimable"
            | "invalid_task_transition"
            | "writing_skill_name_conflict"
            | "skill_name_conflict"
            | "publishing_unknown_unacknowledged"
            | "skill_reserved_name"
            | "skill_content_changed"
            | "skill_local_modifications"
            | "recommended_skill_conflict"
            | "content_conflict"
            | "typesetting_layout_in_progress"
            | "typesetting_content_locked",
        ) => StatusCode::CONFLICT,
        Some("content_not_found") => StatusCode::NOT_FOUND,
        Some("content_too_large" | "skill_package_too_large") => StatusCode::PAYLOAD_TOO_LARGE,
        Some("broker_unavailable") => StatusCode::SERVICE_UNAVAILABLE,
        Some(
            "invalid_output"
            | "publishing_snapshot_corrupt"
            | "invalid_content_path"
            | "invalid_content_resource"
            | "skill_file_invalid"
            | "writing_skill_file_invalid",
        ) => {
            StatusCode::UNPROCESSABLE_ENTITY
        }
        Some(
            "skill_read_only" | "writing_skill_read_only" | "skill_update_unavailable",
        ) => StatusCode::FORBIDDEN,
        Some(
            "invalid_target"
            | "invalid_dispatch_mode"
            | "invalid_model_gateway_settings"
            | "unsupported_model_provider"
            | "byok_not_available"
            | "invalid_retry_after"
            | "writing_refinement_source_required"
            | "writing_refinement_source_not_ready"
            | "writing_skill_name_required"
            | "writing_skill_name_too_long"
            | "invalid_skill_module"
            | "invalid_skill_package"
            | "invalid_recommended_skill_catalog"
            | "recommended_skill_name_mismatch"
            | "invalid_publishing_request"
            | "publishing_source_incomplete"
            | "publishing_skill_not_supported"
            | "publishing_skill_target_mismatch"
            | "invalid_publishing_phase"
            | "invalid_cover_request"
            | "cover_instruction_too_long"
            | "cover_source_empty"
            | "invalid_layout_request"
            | "layout_instruction_too_long"
            | "layout_source_empty"
            | "skill_source_ambiguous"
            | "unsupported_skill_source",
        ) => StatusCode::BAD_REQUEST,
        Some("skill_source_unavailable") => StatusCode::BAD_GATEWAY,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn bytes_response(status: StatusCode, bytes: Vec<u8>, content_type: &str) -> Response {
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    response
}

fn mime_type(path: &str) -> &'static str {
    mime_guess::from_path(path)
        .first_raw()
        .unwrap_or("application/octet-stream")
}

fn content_disposition_inline(file_name: &str) -> String {
    let fallback = std::path::Path::new(file_name)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("document")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("inline; filename=\"{}\"", fallback.replace('"', "_"))
}

struct DataUrlBytes {
    bytes: Vec<u8>,
    mime_type: String,
}

fn data_url_to_buffer(data_url: &str) -> Result<DataUrlBytes, CliError> {
    let Some(rest) = data_url.strip_prefix("data:") else {
        return Err(CliError::new("Expected a data URL"));
    };
    let Some((meta, data)) = rest.split_once(',') else {
        return Err(CliError::new("Expected a data URL"));
    };
    let is_base64 = meta.split(';').any(|part| part == "base64");
    let mime_type = meta
        .split(';')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("application/octet-stream")
        .to_owned();
    let bytes = if is_base64 {
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(to_cli_error)?
    } else {
        percent_decode(data)?
    };
    Ok(DataUrlBytes { bytes, mime_type })
}

fn percent_decode(value: &str) -> Result<Vec<u8>, CliError> {
    let mut bytes = Vec::new();
    let raw = value.as_bytes();
    let mut index = 0;
    while index < raw.len() {
        if raw[index] == b'%' && index + 2 < raw.len() {
            let hex = std::str::from_utf8(&raw[index + 1..index + 3]).map_err(to_cli_error)?;
            let byte = u8::from_str_radix(hex, 16).map_err(to_cli_error)?;
            bytes.push(byte);
            index += 3;
        } else {
            bytes.push(raw[index]);
            index += 1;
        }
    }
    Ok(bytes)
}

fn create_id(prefix: &str) -> String {
    crate::ids::random_id(prefix)
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
