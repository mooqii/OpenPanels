use crate::error::CliError;
use crate::paths::{sanitize_path_part, MyOpenPanelsPaths};
use base64::Engine;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const DATABASE_FILE_NAME: &str = "main.sqlite3";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionPayload {
    pub selection: Value,
    pub selection_file: String,
    pub base64: Option<String>,
    pub mime_type: Option<String>,
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
    requested_session_id: Option<&str>,
    include_image_base64: bool,
) -> Result<SelectionPayload, CliError> {
    let database_path = paths.storage_dir.join(DATABASE_FILE_NAME);
    if !database_path.exists() {
        return Err(CliError::new(format!(
            "MyOpenPanels SQLite database not found at {}",
            database_path.display()
        )));
    }

    let connection = Connection::open(&database_path).map_err(to_cli_error)?;
    let session_id = resolve_session_id(&connection, paths, requested_session_id)?;
    let panel_id = resolve_canvas_panel_id(&connection, &session_id)?;
    let raw_state = read_panel_state(&connection, &session_id, &panel_id)?;
    let raw_selection = read_panel_selection(&connection, &session_id, &panel_id)?
        .unwrap_or_else(|| empty_selection(&session_id, &panel_id));
    let selection = with_last_image_fallback(raw_selection, raw_state);
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
        selection_file: panel_file(paths, &session_id, &panel_id, "selection.json")
            .display()
            .to_string(),
        base64,
        mime_type,
        context_dir: paths.context_dir.display().to_string(),
        context_id: paths.context_id.clone(),
        context_id_source: paths.context_id_source.clone(),
        storage_dir: paths.storage_dir.display().to_string(),
    })
}

pub fn read_selection_asset_to_file(
    paths: &MyOpenPanelsPaths,
    requested_session_id: Option<&str>,
    output_path: &str,
    allow_fallback: bool,
) -> Result<SelectionAssetPayload, CliError> {
    let selection = read_selection(paths, requested_session_id, true)?;
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
    let asset_ref = selection
        .selection
        .get("assetRef")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::new("No MyOpenPanels selection asset is available."))?
        .to_owned();
    let Some(base64) = selection.base64 else {
        return Err(CliError::new(
            "No MyOpenPanels selection asset is available.",
        ));
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64)
        .map_err(to_cli_error)?;
    let output_path = PathBuf::from(output_path);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(&output_path, &bytes).map_err(to_cli_error)?;
    Ok(SelectionAssetPayload {
        asset_ref,
        mime_type: selection
            .mime_type
            .unwrap_or_else(|| "application/octet-stream".to_owned()),
        output_path: output_path.display().to_string(),
        bytes: bytes.len(),
    })
}

fn resolve_session_id(
    connection: &Connection,
    paths: &MyOpenPanelsPaths,
    requested_session_id: Option<&str>,
) -> Result<String, CliError> {
    if let Some(session_id) = requested_session_id {
        if !session_id.trim().is_empty() {
            return Ok(session_id.to_owned());
        }
    }

    if let Some(active_session_id) = read_active_session_id(paths)? {
        let exists = connection
            .query_row(
                "SELECT 1 FROM sessions WHERE id = ?",
                params![active_session_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(to_cli_error)?
            .is_some();
        if exists {
            return Ok(active_session_id);
        }
    }

    connection
        .query_row(
            "SELECT id FROM sessions ORDER BY updated_at DESC, id ASC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(to_cli_error)?
        .ok_or_else(|| CliError::new("No MyOpenPanels session is available."))
}

fn read_active_session_id(paths: &MyOpenPanelsPaths) -> Result<Option<String>, CliError> {
    let active_session_path = paths.context_dir.join("active-session.json");
    if !active_session_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&active_session_path).map_err(to_cli_error)?;
    let value = serde_json::from_str::<Value>(&content).map_err(to_cli_error)?;
    Ok(value
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_owned))
}

fn resolve_canvas_panel_id(connection: &Connection, session_id: &str) -> Result<String, CliError> {
    connection
        .query_row(
            "SELECT id FROM panels WHERE session_id = ? AND kind = 'canvas' ORDER BY updated_at DESC, id ASC LIMIT 1",
            params![session_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(to_cli_error)?
        .ok_or_else(|| CliError::new("No MyOpenPanels canvas panel is available."))
}

fn read_panel_state(
    connection: &Connection,
    session_id: &str,
    panel_id: &str,
) -> Result<Option<Value>, CliError> {
    let state_json = connection
        .query_row(
            "SELECT state_json FROM panel_states WHERE session_id = ? AND panel_id = ?",
            params![session_id, panel_id],
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
    session_id: &str,
    panel_id: &str,
) -> Result<Option<Value>, CliError> {
    let selection_json = connection
        .query_row(
            "SELECT selection_json FROM panel_selections WHERE session_id = ? AND panel_id = ?",
            params![session_id, panel_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(to_cli_error)?;
    selection_json
        .map(|raw| serde_json::from_str::<Value>(&raw).map_err(to_cli_error))
        .transpose()
}

fn empty_selection(session_id: &str, panel_id: &str) -> Value {
    json!({
        "sessionId": session_id,
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

fn panel_file(paths: &MyOpenPanelsPaths, session_id: &str, panel_id: &str, name: &str) -> PathBuf {
    paths
        .storage_dir
        .join("sessions")
        .join(sanitize_path_part(session_id))
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
