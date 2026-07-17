async fn api_skills(State(state): State<Arc<AppState>>) -> Response {
    match crate::agent::managed_skills(&state.paths) {
        Ok(payload) => no_store_response(json_response(StatusCode::OK, &payload)),
        Err(error) => json_cli_error(&error),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillImportBody {
    source_type: String,
    module_kind: String,
    #[serde(default)]
    replace_existing: bool,
    #[serde(default)]
    url: String,
    #[serde(default)]
    files: Vec<crate::agent::SkillImportFile>,
    #[serde(default)]
    archive_base64: String,
}

async fn api_import_skill(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SkillImportBody>,
) -> Response {
    let result = match body.source_type.as_str() {
        "url" => crate::agent::import_skill_from_url(
            &state.paths,
            &body.url,
            &body.module_kind,
            body.replace_existing,
        ),
        "folder" => crate::agent::import_skill_from_files(
            &state.paths,
            &body.files,
            &body.module_kind,
            body.replace_existing,
        ),
        "zip" => base64::engine::general_purpose::STANDARD
            .decode(&body.archive_base64)
            .map_err(|_| {
                CliError::with_code(
                    "invalid_skill_package",
                    "The uploaded Skill zip is not valid base64 data.",
                )
            })
            .and_then(|archive| {
                crate::agent::import_skill_from_zip(
                    &state.paths,
                    &archive,
                    &body.module_kind,
                    body.replace_existing,
                )
            }),
        _ => Err(CliError::with_code(
            "unsupported_skill_source",
            "Choose a supported Skill source.",
        )),
    };
    match result {
        Ok(payload) => {
            let status = if payload["status"] == "conflict" {
                StatusCode::OK
            } else {
                StatusCode::CREATED
            };
            no_store_response(json_response(status, &payload))
        }
        Err(error) => json_cli_error(&error),
    }
}

async fn api_skill_files(
    State(state): State<Arc<AppState>>,
    Path(skill_id): Path<String>,
) -> Response {
    match crate::agent::read_managed_skill_files(&state.paths, &skill_id) {
        Ok(payload) => no_store_response(json_response(StatusCode::OK, &payload)),
        Err(error) => json_cli_error(&error),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagedSkillFileBody {
    path: String,
    content: String,
}

async fn api_write_skill_file(
    State(state): State<Arc<AppState>>,
    Path(skill_id): Path<String>,
    Json(body): Json<ManagedSkillFileBody>,
) -> Response {
    match crate::agent::write_managed_skill_file(
        &state.paths,
        &skill_id,
        &body.path,
        &body.content,
    ) {
        Ok(payload) => no_store_response(json_response(StatusCode::OK, &payload)),
        Err(error) => json_cli_error(&error),
    }
}

async fn api_delete_skill(
    State(state): State<Arc<AppState>>,
    Path(skill_id): Path<String>,
) -> Response {
    match crate::agent::delete_managed_skill(&state.paths, &skill_id) {
        Ok(payload) => no_store_response(json_response(StatusCode::OK, &payload)),
        Err(error) => json_cli_error(&error),
    }
}

async fn api_device_skills(State(state): State<Arc<AppState>>) -> Response {
    match crate::agent::discover_device_skills(&state.paths) {
        Ok(payload) => no_store_response(json_response(StatusCode::OK, &payload)),
        Err(error) => json_cli_error(&error),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceSkillInstallBody {
    location_path: String,
    module_kind: String,
}

async fn api_install_device_skill(
    State(state): State<Arc<AppState>>,
    Json(body): Json<DeviceSkillInstallBody>,
) -> Response {
    match crate::agent::install_device_skill(
        &state.paths,
        &body.location_path,
        &body.module_kind,
    ) {
        Ok(payload) => no_store_response(json_response(StatusCode::CREATED, &payload)),
        Err(error) => json_cli_error(&error),
    }
}

async fn api_remove_skill_module(
    State(state): State<Arc<AppState>>,
    Path((skill_id, module_kind)): Path<(String, String)>,
) -> Response {
    match crate::agent::remove_skill_module(&state.paths, &skill_id, &module_kind) {
        Ok(payload) => no_store_response(json_response(StatusCode::OK, &payload)),
        Err(error) => json_cli_error(&error),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceSkillSourceBody {
    location_path: String,
}

async fn api_replace_skill_source(
    State(state): State<Arc<AppState>>,
    Path(skill_id): Path<String>,
    Json(body): Json<DeviceSkillSourceBody>,
) -> Response {
    match crate::agent::replace_skill_source(&state.paths, &skill_id, &body.location_path) {
        Ok(payload) => no_store_response(json_response(StatusCode::OK, &payload)),
        Err(error) => json_cli_error(&error),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillMismatchIgnoreBody {
    location_path: String,
    installed_hash: String,
    device_hash: String,
}

async fn api_ignore_skill_mismatch(
    State(state): State<Arc<AppState>>,
    Path(skill_id): Path<String>,
    Json(body): Json<SkillMismatchIgnoreBody>,
) -> Response {
    match crate::agent::ignore_skill_mismatch(
        &state.paths,
        &skill_id,
        &body.location_path,
        &body.installed_hash,
        &body.device_hash,
    ) {
        Ok(payload) => no_store_response(json_response(StatusCode::OK, &payload)),
        Err(error) => json_cli_error(&error),
    }
}
