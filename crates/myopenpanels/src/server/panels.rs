async fn api_active_project(State(state): State<Arc<AppState>>) -> Response {
    match ensure_project_bootstrap(&state.paths, BootstrapRequest::new()) {
        Ok(bootstrap) => json_response(
            StatusCode::OK,
            &json!({ "projectId": bootstrap.project.id }),
        ),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActiveProjectBody {
    project_id: Option<String>,
}

async fn api_set_active_project(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ActiveProjectBody>,
) -> Response {
    let Some(project_id) = body.project_id.filter(|value| !value.trim().is_empty()) else {
        return json_error(StatusCode::BAD_REQUEST, "Missing projectId.");
    };
    let result = Storage::open(&state.paths).and_then(|storage| {
        if storage.read_project(&project_id)?.is_none() {
            return Err(CliError::new(format!(
                "MyOpenPanels project not found: {project_id}"
            )));
        }
        write_active_project_id(&state.paths, &project_id)?;
        ensure_project_bootstrap(
            &state.paths,
            BootstrapRequest {
                requested_project_id: Some(project_id.clone()),
                requested_panel_id: None,
                requested_panel_kind: None,
            },
        )?;
        Ok(json!({ "projectId": project_id }))
    });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_active_panel(State(state): State<Arc<AppState>>) -> Response {
    let result = ensure_project_bootstrap(&state.paths, BootstrapRequest::new())
        .and_then(|_| read_active_panel_value(&state.paths));
    match result {
        Ok(active_panel) => json_response(StatusCode::OK, &json!({ "activePanel": active_panel })),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivePanelBody {
    kind: Option<String>,
    panel_id: Option<String>,
    project_id: Option<String>,
}

async fn api_set_active_panel(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ActivePanelBody>,
) -> Response {
    let result = ensure_project_bootstrap(
        &state.paths,
        BootstrapRequest {
            requested_project_id: body.project_id,
            requested_panel_id: body.panel_id,
            requested_panel_kind: body.kind.as_deref().and_then(PanelKind::parse),
        },
    )
    .map(|bootstrap| {
        json!({
            "activePanelId": bootstrap.active_panel_id,
            "activePanelKind": bootstrap.active_panel_kind,
            "panel": bootstrap.panel,
            "revision": bootstrap.revision,
            "state": bootstrap.state,
        })
    });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_save_panel_state(
    State(state): State<Arc<AppState>>,
    Path((project_id, panel_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Response {
    let base_revision = body.get("baseRevision").and_then(Value::as_i64);
    let panel_state = body.get("state").cloned().unwrap_or_else(|| body.clone());
    let result = Storage::open(&state.paths).and_then(|storage| {
        let panel = storage.read_panel(&project_id, &panel_id)?.ok_or_else(|| {
            CliError::with_code("panel_not_found", format!("Panel not found: {panel_id}"))
        })?;
        if panel.kind == PanelKind::Typesetting {
            validate_typesetting_state(&panel_state)?;
        }
        storage
            .write_panel_state_if_current(&project_id, &panel_id, &panel_state, base_revision)
            .map(|result| {
                result.map(|revision| {
                    json!({
                        "saved": true,
                        "projectId": project_id,
                        "panelId": panel_id,
                        "revision": revision,
                    })
                })
            })
    });
    match result {
        Ok(Ok(payload)) => {
            if let Err(error) = write_active_project_id(&state.paths, &project_id) {
                return json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message());
            }
            json_response(StatusCode::OK, &payload)
        }
        Ok(Err(conflict)) => json_response(
            StatusCode::CONFLICT,
            &json!({
                "saved": false,
                "error": "Panel state is stale.",
                "baseRevision": conflict.base_revision,
                "currentRevision": conflict.current_revision,
                "projectId": project_id,
                "panelId": panel_id,
            }),
        ),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_get_panel_state(
    State(state): State<Arc<AppState>>,
    Path((project_id, panel_id)): Path<(String, String)>,
) -> Response {
    let result = Storage::open(&state.paths).and_then(|storage| {
        let panel = storage.read_panel(&project_id, &panel_id)?.ok_or_else(|| {
            CliError::with_code("panel_not_found", format!("Panel not found: {panel_id}"))
        })?;
        let state = storage
            .read_panel_state(&project_id, &panel_id)?
            .ok_or_else(|| {
                CliError::with_code(
                    "panel_state_not_found",
                    format!("Panel state not found: {panel_id}"),
                )
            })?;
        let revision = storage.read_panel_state_revision(&project_id, &panel_id)?;
        Ok(json!({ "panel": panel, "state": state, "revision": revision }))
    });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectionBody {
    selection: Option<Value>,
}

async fn api_get_panel_selection(
    State(state): State<Arc<AppState>>,
    Path((project_id, panel_id)): Path<(String, String)>,
) -> Response {
    let result = Storage::open(&state.paths).and_then(|storage| {
        let panel = storage.read_panel(&project_id, &panel_id)?.ok_or_else(|| {
            CliError::with_code("panel_not_found", format!("Panel not found: {panel_id}"))
        })?;
        let selection = storage.read_panel_selection(&project_id, &panel_id)?;
        let revision = storage.read_panel_selection_revision(&project_id, &panel_id)?;
        Ok(json!({ "panel": panel, "selection": selection, "revision": revision }))
    });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

async fn api_save_panel_selection(
    State(state): State<Arc<AppState>>,
    Path((project_id, panel_id)): Path<(String, String)>,
    Json(body): Json<SelectionBody>,
) -> Response {
    let result = Storage::open(&state.paths).and_then(|storage| {
        let selected_shape_ids = body
            .selection
            .as_ref()
            .and_then(|selection| selection.get("selectedShapeIds"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let selected_shapes = body
            .selection
            .as_ref()
            .and_then(|selection| selection.get("selectedShapes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let asset_ref = selected_shapes
            .iter()
            .find_map(|shape| shape.pointer("/asset/assetRef").and_then(Value::as_str))
            .or_else(|| {
                body.selection
                    .as_ref()
                    .and_then(|selection| selection.get("assetRef"))
                    .and_then(Value::as_str)
            })
            .map(str::to_owned);
        let selection = json!({
            "projectId": project_id,
            "panelId": panel_id,
            "selectedShapeIds": selected_shape_ids,
            "selectedShapes": selected_shapes,
            "assetRef": asset_ref,
            "updatedAt": now_iso(),
        });
        storage.write_panel_selection(&project_id, &panel_id, &selection)?;
        write_active_project_id(&state.paths, &project_id)?;
        Ok(json!({ "saved": true, "selection": selection }))
    });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_get_selection_materialization(
    State(state): State<Arc<AppState>>,
    Path((project_id, panel_id)): Path<(String, String)>,
) -> Response {
    match crate::selection::pending_materialization_request(&state.paths, &project_id, &panel_id) {
        Ok(request) => json_response(StatusCode::OK, &json!({ "request": request })),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectionMaterializationBody {
    image_data_url: String,
}

async fn api_complete_selection_materialization(
    State(state): State<Arc<AppState>>,
    Path((project_id, panel_id, request_id)): Path<(String, String, String)>,
    Json(body): Json<SelectionMaterializationBody>,
) -> Response {
    let result = data_url_to_buffer(&body.image_data_url).and_then(|image| {
        crate::selection::complete_materialization_request(
            &state.paths,
            &project_id,
            &panel_id,
            &request_id,
            &image.bytes,
        )
    });
    match result {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(status_for_cli_error(&error), error.message()),
    }
}
