use crate::error::CliError;
use crate::paths::{sanitize_path_part, MyOpenPanelsPaths};
use base64::Engine;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime};

const DATABASE_FILE_NAME: &str = "main.sqlite3";
const MATERIALIZATION_WAIT: Duration = Duration::from_secs(5);
const MATERIALIZATION_POLL: Duration = Duration::from_millis(50);
const MATERIALIZATION_RETENTION: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionPayload {
    pub selection: Value,
    pub selection_file: String,
    pub base64: Option<String>,
    pub mime_type: Option<String>,
    pub image: Option<Value>,
    pub context_dir: String,
    pub context_id: String,
    pub context_id_source: String,
    pub storage_dir: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionAssetPayload {
    pub asset_ref: String,
    pub mime_type: String,
    pub output_path: String,
    pub bytes: usize,
}

pub fn read_selection(
    paths: &MyOpenPanelsPaths,
    requested_project_id: Option<&str>,
    include_image_base64: bool,
) -> Result<SelectionPayload, CliError> {
    let database_path = paths.storage_dir.join(DATABASE_FILE_NAME);
    if !database_path.exists() {
        return Err(CliError::new(format!(
            "MyOpenPanels SQLite database not found at {}",
            database_path.display()
        )));
    }

    let connection = open_connection(&database_path)?;
    let project_id = resolve_project_id(&connection, paths, requested_project_id)?;
    let panel_id = resolve_canvas_panel_id(&connection, &project_id)?;
    let mut payload = read_selection_from_connection(
        paths,
        &connection,
        &project_id,
        &panel_id,
        include_image_base64,
        true,
    )?;
    materialize_selection_image(paths, &mut payload)?;
    embed_image_in_selection(&mut payload);
    Ok(payload)
}

pub fn read_selection_for_panel_materialized(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: &str,
) -> Result<SelectionPayload, CliError> {
    let mut payload = read_selection_for_panel(paths, project_id, panel_id)?;
    if let Err(error) = materialize_selection_image(paths, &mut payload) {
        payload.image = Some(json!({
            "kind": "unavailable",
            "errorCode": error.code(),
            "message": error.message(),
        }));
    }
    embed_image_in_selection(&mut payload);
    Ok(payload)
}

fn embed_image_in_selection(payload: &mut SelectionPayload) {
    if let (Some(object), Some(image)) = (payload.selection.as_object_mut(), payload.image.clone())
    {
        object.insert("image".to_owned(), image);
    }
}

pub fn read_selection_for_panel(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: &str,
) -> Result<SelectionPayload, CliError> {
    let database_path = paths.storage_dir.join(DATABASE_FILE_NAME);
    let connection = open_connection(&database_path)?;
    read_selection_from_connection(paths, &connection, project_id, panel_id, false, false)
}

fn open_connection(path: &Path) -> Result<Connection, CliError> {
    let connection = Connection::open(path).map_err(to_cli_error)?;
    connection
        .execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;",
        )
        .map_err(to_cli_error)?;
    Ok(connection)
}

fn read_selection_from_connection(
    paths: &MyOpenPanelsPaths,
    connection: &Connection,
    project_id: &str,
    panel_id: &str,
    include_image_base64: bool,
    include_fallback: bool,
) -> Result<SelectionPayload, CliError> {
    let raw_selection = read_panel_selection(connection, project_id, panel_id)?
        .unwrap_or_else(|| empty_selection(project_id, panel_id));
    let raw_selection = with_explicit_marker(raw_selection);
    let selection = if include_fallback {
        with_last_image_fallback(
            raw_selection,
            read_panel_state(connection, project_id, panel_id)?,
        )
    } else {
        raw_selection
    };
    let asset_ref = selection
        .get("assetRef")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let base64 = if include_image_base64 {
        match asset_ref.as_deref() {
            Some(asset_ref) => Some(read_asset_base64(&paths.storage_dir, asset_ref)?),
            None => None,
        }
    } else {
        None
    };
    let mime_type = asset_ref.as_deref().map(mime_type_for_file);

    Ok(SelectionPayload {
        selection,
        selection_file: panel_file(paths, project_id, panel_id, "selection.json")
            .display()
            .to_string(),
        base64,
        mime_type,
        image: None,
        context_dir: paths.context_dir.display().to_string(),
        context_id: paths.context_id.clone(),
        context_id_source: paths.context_id_source.clone(),
        storage_dir: paths.storage_dir.display().to_string(),
    })
}

fn materialize_selection_image(
    paths: &MyOpenPanelsPaths,
    payload: &mut SelectionPayload,
) -> Result<(), CliError> {
    if !payload
        .selection
        .get("isExplicitSelection")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(());
    }
    let project_id = payload
        .selection
        .get("projectId")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::new("Canvas selection has no project id."))?;
    let panel_id = payload
        .selection
        .get("panelId")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::new("Canvas selection has no panel id."))?;
    let selected_shapes = payload
        .selection
        .get("selectedShapes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if selected_shapes.len() == 1 && is_simple_source_image(&selected_shapes[0]) {
        if let Some(asset_ref) = selected_shapes[0]
            .pointer("/asset/assetRef")
            .and_then(Value::as_str)
            .or_else(|| payload.selection.get("assetRef").and_then(Value::as_str))
        {
            let storage = crate::storage::Storage::open(paths)?;
            let local_path = storage.asset_path(asset_ref)?;
            if local_path.is_file() {
                payload.image = Some(json!({
                    "kind": "source",
                    "assetRef": asset_ref,
                    "localPath": local_path,
                    "mimeType": mime_type_for_file(asset_ref),
                }));
                return Ok(());
            }
        }
    }

    cleanup_materializations(paths);
    if !paths.studio_dir.join("instance.json").is_file() {
        return Err(CliError::with_code(
            "selection_render_unavailable",
            "Studio is not running, so the Canvas selection cannot be rendered.",
        ));
    }
    let storage = crate::storage::Storage::open(paths)?;
    let selection_revision = storage.read_panel_selection_revision(project_id, panel_id)?;
    let panel_revision = storage.read_panel_state_revision(project_id, panel_id)?;
    let selected_shape_ids = payload
        .selection
        .get("selectedShapeIds")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let fingerprint_source = serde_json::to_vec(&json!({
        "projectId": project_id,
        "panelId": panel_id,
        "selectionRevision": selection_revision,
        "selectedShapeIds": selected_shape_ids,
        "selectedShapes": selected_shapes,
    }))
    .map_err(to_cli_error)?;
    let request_id = format!("{:x}", Sha256::digest(fingerprint_source));
    let artifact_path = materialization_artifacts_dir(paths).join(format!("{request_id}.png"));
    if artifact_path.is_file() {
        touch_materialization_lease(paths, &request_id);
        payload.image = Some(materialized_image(
            &request_id,
            &artifact_path,
            selection_revision,
            panel_revision,
        ));
        return Ok(());
    }

    let requests_dir = materialization_requests_dir(paths);
    fs::create_dir_all(&requests_dir).map_err(to_cli_error)?;
    let request_path = requests_dir.join(format!("{request_id}.request.json"));
    let result_path = requests_dir.join(format!("{request_id}.result.json"));
    fs::write(
        &request_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&json!({
                "requestId": request_id,
                "projectId": project_id,
                "panelId": panel_id,
                "selectionRevision": selection_revision,
                "panelRevision": panel_revision,
                "selectedShapeIds": selected_shape_ids,
                "createdAt": crate::control::now_iso(),
            }))
            .map_err(to_cli_error)?
        ),
    )
    .map_err(to_cli_error)?;

    let started = std::time::Instant::now();
    while started.elapsed() < MATERIALIZATION_WAIT {
        if artifact_path.is_file() && result_path.is_file() {
            let _ = fs::remove_file(&request_path);
            let _ = fs::remove_file(&result_path);
            touch_materialization_lease(paths, &request_id);
            payload.image = Some(materialized_image(
                &request_id,
                &artifact_path,
                selection_revision,
                panel_revision,
            ));
            return Ok(());
        }
        thread::sleep(MATERIALIZATION_POLL);
    }
    let _ = fs::remove_file(&request_path);
    Err(CliError::with_code(
        "selection_render_timeout",
        "Studio did not render the current Canvas selection within 5 seconds.",
    ))
}

fn is_simple_source_image(shape: &Value) -> bool {
    if shape.get("type").and_then(Value::as_str) != Some("image") {
        return false;
    }
    let props = shape.get("props").and_then(Value::as_object);
    let number = |key: &str, default: f64| {
        props
            .and_then(|value| value.get(key))
            .and_then(Value::as_f64)
            .unwrap_or(default)
    };
    let has_crop = props
        .and_then(|value| value.get("crop"))
        .is_some_and(|value| !value.is_null());
    !has_crop
        && number("rotation", 0.0) == 0.0
        && number("scaleX", 1.0) >= 0.0
        && number("scaleY", 1.0) >= 0.0
        && number("opacity", 1.0) == 1.0
        && number("cornerRadius", 0.0) == 0.0
}

fn materialized_image(
    request_id: &str,
    path: &Path,
    selection_revision: i64,
    panel_revision: i64,
) -> Value {
    json!({
        "kind": "composite",
        "materializationId": request_id,
        "localPath": path,
        "mimeType": "image/png",
        "selectionRevision": selection_revision,
        "panelRevision": panel_revision,
    })
}

fn with_explicit_marker(selection: Value) -> Value {
    if selection
        .get("isExplicitSelection")
        .and_then(Value::as_bool)
        .is_some()
    {
        return selection;
    }
    let explicit = selection
        .get("selectedShapeIds")
        .and_then(Value::as_array)
        .is_some_and(|ids| !ids.is_empty())
        || selection
            .get("selectedShapes")
            .and_then(Value::as_array)
            .is_some_and(|shapes| !shapes.is_empty());
    let mut value = selection.as_object().cloned().unwrap_or_default();
    value.insert("isExplicitSelection".to_owned(), json!(explicit));
    Value::Object(value)
}

pub fn read_selection_asset_for_panel(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: &str,
    output_path: &str,
) -> Result<SelectionAssetPayload, CliError> {
    let mut selection = read_selection_for_panel(paths, project_id, panel_id)?;
    materialize_selection_image(paths, &mut selection)?;
    embed_image_in_selection(&mut selection);
    let is_explicit_selection = selection
        .selection
        .get("isExplicitSelection")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !is_explicit_selection {
        return Err(CliError::with_code(
            "explicit_selection_required",
            "No explicit Canvas selection asset is available.",
        ));
    }
    write_selection_asset(selection, output_path)
}

pub fn read_selection_asset_to_file(
    paths: &MyOpenPanelsPaths,
    requested_project_id: Option<&str>,
    output_path: &str,
    allow_fallback: bool,
) -> Result<SelectionAssetPayload, CliError> {
    let selection = read_selection(paths, requested_project_id, false)?;
    let is_explicit_selection = selection
        .selection
        .get("isExplicitSelection")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !is_explicit_selection && !allow_fallback {
        return Err(CliError::new(
            "No explicit MyOpenPanels selection asset is available. Re-select the image or pass --allow-fallback to use the fallback image.",
        ));
    }
    write_selection_asset(selection, output_path)
}

fn write_selection_asset(
    selection: SelectionPayload,
    output_path: &str,
) -> Result<SelectionAssetPayload, CliError> {
    let asset_ref = selection
        .image
        .as_ref()
        .and_then(|image| image.get("assetRef"))
        .and_then(Value::as_str)
        .or_else(|| {
            selection
                .image
                .as_ref()
                .and_then(|image| image.get("materializationId"))
                .and_then(Value::as_str)
        })
        .or_else(|| selection.selection.get("assetRef").and_then(Value::as_str))
        .unwrap_or("materialized-selection")
        .to_owned();
    let bytes = if let Some(local_path) = selection
        .image
        .as_ref()
        .and_then(|image| image.get("localPath"))
        .and_then(Value::as_str)
    {
        fs::read(local_path).map_err(to_cli_error)?
    } else if let Some(base64) = selection.base64 {
        base64::engine::general_purpose::STANDARD
            .decode(base64)
            .map_err(to_cli_error)?
    } else {
        return Err(CliError::new(
            "No MyOpenPanels selection asset is available.",
        ));
    };
    let output_path = PathBuf::from(output_path);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(&output_path, &bytes).map_err(to_cli_error)?;
    Ok(SelectionAssetPayload {
        asset_ref,
        mime_type: selection
            .image
            .as_ref()
            .and_then(|image| image.get("mimeType"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or(selection.mime_type)
            .unwrap_or_else(|| "application/octet-stream".to_owned()),
        output_path: output_path.display().to_string(),
        bytes: bytes.len(),
    })
}

pub fn pending_materialization_request(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: &str,
) -> Result<Option<Value>, CliError> {
    let dir = materialization_requests_dir(paths);
    if !dir.is_dir() {
        return Ok(None);
    }
    let mut entries = fs::read_dir(dir)
        .map_err(to_cli_error)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.ends_with(".request.json"))
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.metadata().and_then(|meta| meta.modified()).ok());
    for entry in entries {
        let raw = fs::read_to_string(entry.path()).map_err(to_cli_error)?;
        let value = serde_json::from_str::<Value>(&raw).map_err(to_cli_error)?;
        if value.get("projectId").and_then(Value::as_str) == Some(project_id)
            && value.get("panelId").and_then(Value::as_str) == Some(panel_id)
        {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

pub fn complete_materialization_request(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: &str,
    request_id: &str,
    bytes: &[u8],
) -> Result<Value, CliError> {
    let request_id = sanitize_path_part(request_id);
    let requests_dir = materialization_requests_dir(paths);
    let request_path = requests_dir.join(format!("{request_id}.request.json"));
    let raw = fs::read_to_string(&request_path).map_err(to_cli_error)?;
    let request = serde_json::from_str::<Value>(&raw).map_err(to_cli_error)?;
    if request.get("projectId").and_then(Value::as_str) != Some(project_id)
        || request.get("panelId").and_then(Value::as_str) != Some(panel_id)
    {
        return Err(CliError::with_code(
            "selection_materialization_mismatch",
            "Selection materialization request targets a different panel.",
        ));
    }
    let storage = crate::storage::Storage::open(paths)?;
    let expected_selection = request
        .get("selectionRevision")
        .and_then(Value::as_i64)
        .unwrap_or(-1);
    if storage.read_panel_selection_revision(project_id, panel_id)? != expected_selection {
        return Err(CliError::with_code(
            "selection_changed",
            "Canvas selection changed before it could be rendered.",
        ));
    }
    let artifacts_dir = materialization_artifacts_dir(paths);
    fs::create_dir_all(&artifacts_dir).map_err(to_cli_error)?;
    let artifact_path = artifacts_dir.join(format!("{request_id}.png"));
    fs::write(&artifact_path, bytes).map_err(to_cli_error)?;
    touch_materialization_lease(paths, &request_id);
    let result_path = requests_dir.join(format!("{request_id}.result.json"));
    fs::write(
        result_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&json!({
                "requestId": request_id,
                "localPath": artifact_path,
                "bytes": bytes.len(),
            }))
            .map_err(to_cli_error)?
        ),
    )
    .map_err(to_cli_error)?;
    Ok(json!({ "requestId": request_id, "localPath": artifact_path, "bytes": bytes.len() }))
}

pub fn cleanup_materializations(paths: &MyOpenPanelsPaths) {
    let now = SystemTime::now();
    let artifacts_dir = materialization_artifacts_dir(paths);
    if let Ok(entries) = fs::read_dir(&artifacts_dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("png") {
                continue;
            }
            let lease_path = path.with_extension("lease");
            let expired = fs::metadata(&lease_path)
                .or_else(|_| entry.metadata())
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age > MATERIALIZATION_RETENTION);
            if expired {
                let _ = fs::remove_file(path);
                let _ = fs::remove_file(lease_path);
            }
        }
    }
    if let Ok(entries) = fs::read_dir(materialization_requests_dir(paths)) {
        for entry in entries.filter_map(Result::ok) {
            let expired = entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age > Duration::from_secs(60));
            if expired {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

fn materialization_root(paths: &MyOpenPanelsPaths) -> PathBuf {
    paths.storage_dir.join("selection-materializations")
}

fn materialization_requests_dir(paths: &MyOpenPanelsPaths) -> PathBuf {
    materialization_root(paths).join("requests")
}

fn materialization_artifacts_dir(paths: &MyOpenPanelsPaths) -> PathBuf {
    materialization_root(paths).join("artifacts")
}

fn touch_materialization_lease(paths: &MyOpenPanelsPaths, request_id: &str) {
    let lease_path = materialization_artifacts_dir(paths).join(format!("{request_id}.lease"));
    let _ = fs::write(lease_path, crate::control::now_iso());
}

fn resolve_project_id(
    connection: &Connection,
    paths: &MyOpenPanelsPaths,
    requested_project_id: Option<&str>,
) -> Result<String, CliError> {
    if let Some(project_id) = requested_project_id {
        if !project_id.trim().is_empty() {
            return Ok(project_id.to_owned());
        }
    }

    if let Some(active_project_id) = read_active_project_id(paths)? {
        let exists = connection
            .query_row(
                "SELECT 1 FROM projects WHERE id = ?",
                params![active_project_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(to_cli_error)?
            .is_some();
        if exists {
            return Ok(active_project_id);
        }
    }

    connection
        .query_row(
            "SELECT id FROM projects ORDER BY updated_at DESC, id ASC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(to_cli_error)?
        .ok_or_else(|| CliError::new("No MyOpenPanels project is available."))
}

fn read_active_project_id(paths: &MyOpenPanelsPaths) -> Result<Option<String>, CliError> {
    let active_project_path = paths.focus_dir.join("active-project.json");
    if !active_project_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&active_project_path).map_err(to_cli_error)?;
    let value = serde_json::from_str::<Value>(&content).map_err(to_cli_error)?;
    Ok(value
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_owned))
}

fn resolve_canvas_panel_id(connection: &Connection, project_id: &str) -> Result<String, CliError> {
    connection
        .query_row(
            "SELECT id FROM panels WHERE project_id = ? AND kind = 'canvas' ORDER BY updated_at DESC, id ASC LIMIT 1",
            params![project_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(to_cli_error)?
        .ok_or_else(|| CliError::new("No MyOpenPanels canvas panel is available."))
}

fn read_panel_state(
    connection: &Connection,
    project_id: &str,
    panel_id: &str,
) -> Result<Option<Value>, CliError> {
    let state_json = connection
        .query_row(
            "SELECT state_json FROM panel_states WHERE project_id = ? AND panel_id = ?",
            params![project_id, panel_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(to_cli_error)?;
    state_json
        .map(|raw| serde_json::from_str::<Value>(&raw).map_err(to_cli_error))
        .transpose()
}

fn read_panel_selection(
    connection: &Connection,
    project_id: &str,
    panel_id: &str,
) -> Result<Option<Value>, CliError> {
    let selection_json = connection
        .query_row(
            "SELECT selection_json FROM panel_selections WHERE project_id = ? AND panel_id = ?",
            params![project_id, panel_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(to_cli_error)?;
    selection_json
        .map(|raw| serde_json::from_str::<Value>(&raw).map_err(to_cli_error))
        .transpose()
}

fn empty_selection(project_id: &str, panel_id: &str) -> Value {
    json!({
        "projectId": project_id,
        "panelId": panel_id,
        "selectedShapeIds": [],
        "selectedShapes": [],
        "assetRef": null,
        "updatedAt": current_timestamp(),
    })
}

fn with_last_image_fallback(selection: Value, state: Option<Value>) -> Value {
    let selected_shapes = selection
        .get("selectedShapes")
        .and_then(Value::as_array)
        .map(|items| !items.is_empty())
        .unwrap_or(false);
    if selected_shapes {
        let mut next = selection.as_object().cloned().unwrap_or_default();
        next.insert("isExplicitSelection".to_owned(), json!(true));
        return Value::Object(next);
    }

    let Some(fallback) = find_last_image_selection_shape(state.as_ref()) else {
        let mut next = selection.as_object().cloned().unwrap_or_default();
        next.insert("isExplicitSelection".to_owned(), json!(false));
        return Value::Object(next);
    };
    let mut next = selection.as_object().cloned().unwrap_or_default();
    let fallback_id = fallback
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let asset_ref = fallback
        .get("asset")
        .and_then(|asset| asset.get("assetRef"))
        .cloned()
        .unwrap_or(Value::Null);
    next.insert("selectedShapeIds".to_owned(), json!([fallback_id]));
    next.insert("selectedShapes".to_owned(), json!([fallback]));
    next.insert("assetRef".to_owned(), asset_ref);
    next.insert("isExplicitSelection".to_owned(), json!(false));
    next.insert(
        "fallback".to_owned(),
        Value::String("last-image".to_owned()),
    );
    Value::Object(next)
}

fn find_last_image_selection_shape(state: Option<&Value>) -> Option<Value> {
    let store = state?.get("store")?.as_object()?;
    let mut images = store
        .values()
        .filter(|record| {
            record.get("typeName").and_then(Value::as_str) == Some("shape")
                && record.get("type").and_then(Value::as_str) == Some("image")
        })
        .collect::<Vec<_>>();
    images.sort_by_key(|record| record.get("index").and_then(Value::as_i64).unwrap_or(0));
    let shape = (*images.last()?).clone();
    let props = shape.get("props").cloned().unwrap_or_else(|| json!({}));
    let asset_id = props.get("assetId").and_then(Value::as_str);
    let asset = asset_id
        .and_then(|id| store.get(id))
        .map(summarize_asset)
        .unwrap_or(Value::Null);
    Some(json!({
        "id": shape.get("id").cloned().unwrap_or(Value::Null),
        "type": "image",
        "parentId": shape.get("parentId").cloned().unwrap_or(Value::Null),
        "props": props,
        "bounds": {
            "x": shape.pointer("/props/x").and_then(Value::as_f64).unwrap_or(0.0),
            "y": shape.pointer("/props/y").and_then(Value::as_f64).unwrap_or(0.0),
            "width": shape.pointer("/props/w").or_else(|| shape.pointer("/props/width")).and_then(Value::as_f64).unwrap_or(0.0),
            "height": shape.pointer("/props/h").or_else(|| shape.pointer("/props/height")).and_then(Value::as_f64).unwrap_or(0.0),
        },
        "asset": asset,
    }))
}

fn summarize_asset(asset: &Value) -> Value {
    json!({
        "id": asset.get("id").cloned().unwrap_or(Value::Null),
        "src": asset.pointer("/props/src").cloned().unwrap_or(Value::Null),
        "name": asset.pointer("/props/name").cloned().unwrap_or(Value::Null),
        "w": asset.pointer("/props/w").cloned().unwrap_or(Value::Null),
        "h": asset.pointer("/props/h").cloned().unwrap_or(Value::Null),
        "mimeType": asset.pointer("/props/mimeType").cloned().unwrap_or(Value::Null),
        "assetRef": asset.pointer("/meta/assetRef").cloned().unwrap_or(Value::Null),
    })
}

fn read_asset_base64(storage_dir: &Path, asset_ref: &str) -> Result<String, CliError> {
    let asset_path = asset_path(storage_dir, asset_ref)?;
    let bytes = fs::read(asset_path).map_err(to_cli_error)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

fn asset_path(storage_dir: &Path, asset_ref: &str) -> Result<PathBuf, CliError> {
    let mut path = PathBuf::from(storage_dir);
    for part in asset_ref.split('/') {
        path.push(sanitize_path_part(part));
    }
    if path.starts_with(storage_dir) {
        Ok(path)
    } else {
        Err(CliError::new("Asset path escapes storage directory."))
    }
}

fn panel_file(paths: &MyOpenPanelsPaths, project_id: &str, panel_id: &str, name: &str) -> PathBuf {
    paths
        .storage_dir
        .join("projects")
        .join(sanitize_path_part(project_id))
        .join("panels")
        .join(sanitize_path_part(panel_id))
        .join(sanitize_path_part(name))
}

fn mime_type_for_file(path: &str) -> String {
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png".to_owned(),
        "jpg" | "jpeg" => "image/jpeg".to_owned(),
        "webp" => "image/webp".to_owned(),
        "gif" => "image/gif".to_owned(),
        "svg" => "image/svg+xml".to_owned(),
        "json" => "application/json".to_owned(),
        _ => "application/octet-stream".to_owned(),
    }
}

fn current_timestamp() -> String {
    "1970-01-01T00:00:00.000Z".to_owned()
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{ensure_project_bootstrap, BootstrapRequest};
    use crate::paths::resolve_myopenpanels_paths;
    use crate::storage::Storage;

    fn test_paths() -> (tempfile::TempDir, MyOpenPanelsPaths) {
        let temp = tempfile::tempdir().expect("temp dir");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("selection-test"),
        )
        .expect("paths");
        (temp, paths)
    }

    fn canvas_target(paths: &MyOpenPanelsPaths) -> (String, String) {
        let bootstrap =
            ensure_project_bootstrap(paths, BootstrapRequest::new()).expect("bootstrap");
        let panel = bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == crate::types::PanelKind::Canvas)
            .expect("canvas panel");
        (bootstrap.project.id, panel.panel.id.clone())
    }

    #[test]
    fn single_selected_image_returns_existing_local_asset_without_rendering() {
        let (_temp, paths) = test_paths();
        let (project_id, panel_id) = canvas_target(&paths);
        let storage = Storage::open(&paths).expect("storage");
        let written = storage
            .write_asset_from_buffer(&project_id, &panel_id, "source.png", b"png", false)
            .expect("asset");
        storage
            .write_panel_selection(
                &project_id,
                &panel_id,
                &json!({
                    "projectId": project_id,
                    "panelId": panel_id,
                    "selectedShapeIds": ["shape:image"],
                    "selectedShapes": [{
                        "id": "shape:image",
                        "type": "image",
                        "asset": { "assetRef": written.asset_ref }
                    }],
                    "assetRef": written.asset_ref,
                }),
            )
            .expect("selection");

        let payload = read_selection_for_panel_materialized(&paths, &project_id, &panel_id)
            .expect("selection");
        assert_eq!(payload.selection["image"]["kind"], "source");
        assert_eq!(
            payload.selection["image"]["localPath"],
            written.file_path.display().to_string()
        );
        assert!(!materialization_root(&paths).exists());
    }

    #[test]
    fn composite_selection_is_rendered_once_to_an_immutable_artifact() {
        let (_temp, paths) = test_paths();
        let (project_id, panel_id) = canvas_target(&paths);
        fs::create_dir_all(&paths.studio_dir).expect("studio");
        fs::write(paths.studio_dir.join("instance.json"), "{}\n").expect("session");
        Storage::open(&paths)
            .expect("storage")
            .write_panel_selection(
                &project_id,
                &panel_id,
                &json!({
                    "projectId": project_id,
                    "panelId": panel_id,
                    "selectedShapeIds": ["shape:1", "shape:2"],
                    "selectedShapes": [
                        { "id": "shape:1", "type": "geo" },
                        { "id": "shape:2", "type": "geo" }
                    ],
                    "assetRef": null,
                }),
            )
            .expect("selection");

        let worker_paths = paths.clone();
        let worker_project = project_id.clone();
        let worker_panel = panel_id.clone();
        let worker = std::thread::spawn(move || {
            read_selection_for_panel_materialized(&worker_paths, &worker_project, &worker_panel)
                .expect("materialized selection")
        });
        let request = (0..100)
            .find_map(|_| {
                let request = pending_materialization_request(&paths, &project_id, &panel_id)
                    .expect("pending request");
                if request.is_none() {
                    std::thread::sleep(Duration::from_millis(10));
                }
                request
            })
            .expect("request");
        complete_materialization_request(
            &paths,
            &project_id,
            &panel_id,
            request["requestId"].as_str().unwrap(),
            b"composite-png",
        )
        .expect("complete");
        let payload = worker.join().expect("worker");
        let local_path = payload.selection["image"]["localPath"]
            .as_str()
            .expect("local path");
        assert_eq!(payload.selection["image"]["kind"], "composite");
        assert_eq!(fs::read(local_path).expect("artifact"), b"composite-png");

        let cached = read_selection_for_panel_materialized(&paths, &project_id, &panel_id)
            .expect("cached selection");
        assert_eq!(cached.selection["image"]["localPath"], local_path);
        assert!(
            pending_materialization_request(&paths, &project_id, &panel_id)
                .expect("pending")
                .is_none()
        );
    }
}
