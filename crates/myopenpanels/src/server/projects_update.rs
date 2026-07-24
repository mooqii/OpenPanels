#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectEventsQuery {
    project_id: Option<String>,
}

async fn api_project_events(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ProjectEventsQuery>,
) -> Response {
    let permit = match state.project_event_slots.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => return StatusCode::NO_CONTENT.into_response(),
    };
    let paths = state.paths.clone();
    let project_id = query.project_id;
    let (sender, receiver) = mpsc::unbounded_channel::<Result<Event, std::convert::Infallible>>();

    tokio::spawn(run_project_event_stream(
        paths,
        project_id,
        sender,
        permit,
        Duration::from_millis(PROJECT_EVENT_POLL_INTERVAL_MS),
    ));

    Sse::new(UnboundedReceiverStream::new(receiver))
        .keep_alive(KeepAlive::default())
        .into_response()
}

async fn run_project_event_stream(
    paths: MyOpenPanelsPaths,
    project_id: Option<String>,
    sender: mpsc::UnboundedSender<Result<Event, std::convert::Infallible>>,
    _permit: OwnedSemaphorePermit,
    poll_interval: Duration,
) {
    if sender.is_closed() {
        return;
    }
    let mut last_seq = read_storage_change_seq(&paths).unwrap_or(0);
    let mut last_focus_revision = read_focus_revision(&paths).unwrap_or(0);
    let mut interval = tokio::time::interval(poll_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;
        if sender.is_closed() {
            return;
        }
        if let Ok(focus_revision) = read_focus_revision(&paths) {
            if focus_revision != last_focus_revision {
                last_focus_revision = focus_revision;
                let payload = json!({
                    "kind": "focus",
                    "focusRevision": focus_revision,
                });
                if sender
                    .send(Ok(Event::default()
                        .event("project")
                        .data(payload.to_string())))
                    .is_err()
                {
                    return;
                }
            }
        }
        let (next_seq, changes) =
            match read_change_scopes_after(&paths, last_seq, project_id.as_deref()) {
                Ok(result) => result,
                Err(error) => {
                    let payload = json!({
                        "kind": "error",
                        "message": error.message(),
                    });
                    if sender
                        .send(Ok(Event::default()
                            .event("error")
                            .data(payload.to_string())))
                        .is_err()
                    {
                        break;
                    }
                    continue;
                }
            };
        if next_seq == last_seq {
            continue;
        }
        last_seq = next_seq;
        for change in changes {
            if sender
                .send(Ok(Event::default().event("project").data(
                    serde_json::to_string(&change).unwrap_or_else(|_| "{}".to_owned()),
                )))
                .is_err()
            {
                return;
            }
        }
    }
}

fn read_storage_change_seq(paths: &MyOpenPanelsPaths) -> Result<i64, CliError> {
    Storage::open(paths).and_then(|storage| storage.read_change_seq())
}

fn read_change_scopes_after(
    paths: &MyOpenPanelsPaths,
    revision: i64,
    project_id: Option<&str>,
) -> Result<(i64, Vec<crate::storage::ChangeScope>), CliError> {
    Storage::open(paths).and_then(|storage| storage.read_changes_after(revision, project_id))
}

async fn api_projects(State(state): State<Arc<AppState>>) -> Response {
    match Storage::open(&state.paths).and_then(|storage| storage.list_projects()) {
        Ok(projects) => json_response(StatusCode::OK, &projects),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
struct RenameProjectBody {
    title: Option<String>,
}

async fn api_rename_project(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
    Json(body): Json<RenameProjectBody>,
) -> Response {
    match rename_project(&state.paths, &project_id, body.title.as_deref()) {
        Ok(project) => json_response(StatusCode::OK, &json!({ "project": project })),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_delete_project(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> Response {
    match delete_project(&state.paths, &project_id) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenPanelBody {
    initial_state: Option<Value>,
    kind: String,
    project_id: String,
}

async fn api_open_panel(
    State(state): State<Arc<AppState>>,
    Json(body): Json<OpenPanelBody>,
) -> Response {
    let Some(kind) = PanelKind::parse(&body.kind) else {
        return json_error(
            StatusCode::BAD_REQUEST,
            "Expected kind to be one of: wiki, writing, canvas, typesetting, publishing.",
        );
    };
    match open_runtime_panel(
        &state.paths,
        &body.project_id,
        kind,
        body.initial_state,
    ) {
        Ok(panel) => json_response(StatusCode::OK, &panel),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InsertArtifactBody {
    artifact: Value,
    panel_id: Option<String>,
    project_id: String,
}

async fn api_insert_artifact(
    State(state): State<Arc<AppState>>,
    Json(body): Json<InsertArtifactBody>,
) -> Response {
    let result = Storage::open(&state.paths).and_then(|storage| {
        let timestamp = now_iso();
        let mut artifact = body.artifact;
        let object = artifact
            .as_object_mut()
            .ok_or_else(|| CliError::new("Artifact must be a JSON object."))?;
        object
            .entry("id".to_owned())
            .or_insert_with(|| json!(create_id("artifact")));
        object
            .entry("createdAt".to_owned())
            .or_insert_with(|| json!(timestamp));
        if !object.contains_key("panelId") {
            if let Some(panel_id) = body.panel_id {
                object.insert("panelId".to_owned(), json!(panel_id));
            }
        }
        storage.write_artifact(&body.project_id, &artifact)?;
        Ok(artifact)
    });
    match result {
        Ok(artifact) => json_response(StatusCode::OK, &artifact),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateStatusQuery {
    force: Option<String>,
    refresh: Option<String>,
}

impl UpdateStatusQuery {
    fn bypass_cache(&self) -> bool {
        query_flag(self.refresh.as_deref()) || query_flag(self.force.as_deref())
    }
}

fn query_flag(value: Option<&str>) -> bool {
    value
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "" | "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

async fn api_update_status(Query(query): Query<UpdateStatusQuery>) -> Response {
    match check_for_update(env!("CARGO_PKG_VERSION"), !query.bypass_cache()) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_update_download() -> Response {
    match download_update(env!("CARGO_PKG_VERSION")) {
        Ok(payload) => json_response(StatusCode::OK, &payload),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

async fn api_update_install_restart(State(state): State<Arc<AppState>>) -> Response {
    let install = match install_update(env!("CARGO_PKG_VERSION")) {
        Ok(payload) => payload,
        Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    };

    if !install.updated {
        return json_response(
            StatusCode::OK,
            &json!({
                "ok": true,
                "restarting": false,
                "update": install,
                "message": "MyOpenPanels is already up to date.",
            }),
        );
    }

    match spawn_restarted_studio(&state) {
        Ok(session) => {
            schedule_current_server_exit();
            json_response(
                StatusCode::OK,
                &json!({
                    "ok": true,
                    "restarting": true,
                    "session": session,
                    "update": install,
                    "message": "Restarting MyOpenPanels Studio.",
                }),
            )
        }
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error.message()),
    }
}

fn spawn_restarted_studio(state: &AppState) -> Result<StudioSession, CliError> {
    let _transition_lock = acquire_studio_transition_lock(&state.paths)?;
    let process = spawn_studio_server_process(
        &state.paths,
        state.port,
        &state.host,
        state.static_dir.as_ref(),
        Some(750),
    )?;

    let local_server_url = format!("http://127.0.0.1:{}", state.port);
    let session = StudioSession {
        system_browser_url: Some(local_server_url.clone()),
        host: Some(state.host.clone()),
        lan_server_urls: Some(Vec::new()),
        local_server_url: Some(local_server_url.clone()),
        log_path: process.log_path.display().to_string(),
        pid: process.pid,
        port: state.port,
        server_url: local_server_url,
        started_at: now_iso(),
        storage_dir: state.paths.storage_dir.display().to_string(),
    };
    write_studio_session(&state.paths, &session)?;
    Ok(session)
}

fn schedule_current_server_exit() {
    thread::spawn(|| {
        thread::sleep(Duration::from_millis(150));
        std::process::exit(0);
    });
}
