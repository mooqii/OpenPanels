use crate::control::{
    ensure_project_bootstrap, now_iso, write_active_session_id, BootstrapRequest,
};
use crate::error::CliError;
use crate::paths::OpenPanelsPaths;
use crate::storage::Storage;
use crate::types::PanelKind;
use rand::Rng;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;

const DEFAULT_CANVAS_GAP: f64 = 80.0;
const DEFAULT_PLACEHOLDER_SIZE: f64 = 512.0;
const MAX_POSITION_SCAN: usize = 40;

#[derive(Debug, Clone)]
pub struct InsertPlaceholderInput<'a> {
    pub anchor_shape_id: Option<&'a str>,
    pub display_height: Option<f64>,
    pub display_width: Option<f64>,
    pub text: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct InsertImageInput<'a> {
    pub anchor_shape_id: Option<&'a str>,
    pub display_height: Option<f64>,
    pub display_width: Option<f64>,
    pub file_name: Option<&'a str>,
    pub image_path: &'a str,
    pub placement: Option<&'a str>,
    pub replace_shape_id: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct CanvasBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertPlaceholderPayload {
    pub session_id: String,
    pub panel_id: String,
    pub shape_id: String,
    pub bounds: CanvasBounds,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertImagePayload {
    pub session_id: String,
    pub panel_id: String,
    pub asset_id: String,
    pub shape_id: String,
    pub asset_ref: String,
    pub asset_file: String,
    pub asset_url: String,
    pub replaced_shape_id: Option<String>,
    pub bounds: CanvasBounds,
}

pub fn insert_placeholder(
    paths: &OpenPanelsPaths,
    input: InsertPlaceholderInput<'_>,
) -> Result<InsertPlaceholderPayload, CliError> {
    let bootstrap = canvas_bootstrap(paths)?;
    let storage = Storage::open(paths)?;
    let mut state = bootstrap.state;
    let mut store = state_store(&state);
    let page_id = ensure_page(
        &mut store,
        state.get("currentPageId").and_then(Value::as_str),
    );
    let width = input.display_width.unwrap_or(DEFAULT_PLACEHOLDER_SIZE);
    let height = input.display_height.unwrap_or(DEFAULT_PLACEHOLDER_SIZE);
    let anchor = input
        .anchor_shape_id
        .and_then(|shape_id| store.get(shape_id))
        .filter(|shape| shape.get("typeName").and_then(Value::as_str) == Some("shape"));
    let anchor_bounds = anchor.map(shape_bounds);
    let position = find_canvas_placement_position(&store, anchor_bounds, width, height);
    let parent_id = anchor
        .and_then(|shape| shape.get("parentId").and_then(Value::as_str))
        .unwrap_or(&page_id)
        .to_owned();
    let shape_id = create_id("shape");
    store.insert(
        shape_id.clone(),
        json!({
            "id": shape_id,
            "typeName": "shape",
            "type": "placeholder",
            "parentId": parent_id,
            "index": next_shape_index(&store, &page_id),
            "props": {
                "cornerRadius": 0,
                "height": height,
                "text": input.text.unwrap_or("正在生成图片"),
                "width": width,
                "x": position.x,
                "y": position.y,
            },
            "meta": {
                "openpanelsGenerationPlaceholder": true,
                "createdAt": now_iso(),
            },
        }),
    );
    write_canvas_state(
        &storage,
        paths,
        &bootstrap.session.id,
        &bootstrap.panel.id,
        &mut state,
        store,
        &page_id,
        &shape_id,
    )?;
    Ok(InsertPlaceholderPayload {
        session_id: bootstrap.session.id,
        panel_id: bootstrap.panel.id,
        shape_id,
        bounds: CanvasBounds {
            x: position.x,
            y: position.y,
            width,
            height,
        },
    })
}

pub fn insert_image(
    paths: &OpenPanelsPaths,
    input: InsertImageInput<'_>,
) -> Result<InsertImagePayload, CliError> {
    let bootstrap = canvas_bootstrap(paths)?;
    let storage = Storage::open(paths)?;
    let source = Path::new(input.image_path);
    let image = fs::read(source).map_err(to_cli_error)?;
    let dimensions = read_image_dimensions(&image);
    let requested_name = input
        .file_name
        .or_else(|| source.file_name().and_then(|name| name.to_str()))
        .unwrap_or("image.png");
    let written = storage.write_asset_from_buffer(
        &bootstrap.session.id,
        &bootstrap.panel.id,
        requested_name,
        &image,
        false,
    )?;

    let mut state = bootstrap.state;
    let mut store = state_store(&state);
    let page_id = ensure_page(
        &mut store,
        state.get("currentPageId").and_then(Value::as_str),
    );
    let replace_shape = input
        .replace_shape_id
        .and_then(|shape_id| store.get(shape_id))
        .filter(|shape| shape.get("typeName").and_then(Value::as_str) == Some("shape"))
        .cloned();
    let replace_bounds = replace_shape.as_ref().map(shape_bounds);
    let width = input
        .display_width
        .or_else(|| replace_bounds.map(|bounds| bounds.width))
        .or_else(|| dimensions.map(|dimensions| dimensions.width))
        .unwrap_or(512.0);
    let height = input
        .display_height
        .or_else(|| replace_bounds.map(|bounds| bounds.height))
        .or_else(|| {
            dimensions.and_then(|dimensions| {
                input.display_width.map(|display_width| {
                    (display_width * dimensions.height / dimensions.width).round()
                })
            })
        })
        .or_else(|| dimensions.map(|dimensions| dimensions.height))
        .unwrap_or(512.0);
    let anchor = input
        .anchor_shape_id
        .and_then(|shape_id| store.get(shape_id))
        .filter(|shape| shape.get("typeName").and_then(Value::as_str) == Some("shape"));
    let anchor_bounds = anchor.map(shape_bounds);
    let position = replace_bounds
        .unwrap_or_else(|| place_image(anchor_bounds, width, input.placement.unwrap_or("right")));
    let parent_id = replace_shape
        .as_ref()
        .or(anchor)
        .and_then(|shape| shape.get("parentId").and_then(Value::as_str))
        .unwrap_or(&page_id)
        .to_owned();
    let asset_id = create_id("asset");
    let shape_id = create_id("shape");
    let asset_url = format!(
        "/api/panels/{}/{}/assets/{}",
        bootstrap.session.id, bootstrap.panel.id, written.file_name
    );
    let mime_type = mime_guess::from_path(&written.file_name)
        .first_raw()
        .unwrap_or("application/octet-stream");
    store.insert(
        asset_id.clone(),
        json!({
            "id": asset_id,
            "typeName": "asset",
            "type": "image",
            "props": {
                "name": written.file_name,
                "src": asset_url,
                "w": dimensions.map(|value| value.width).unwrap_or(width),
                "h": dimensions.map(|value| value.height).unwrap_or(height),
                "mimeType": mime_type,
                "isAnimated": false,
            },
            "meta": { "assetRef": written.asset_ref },
        }),
    );
    store.insert(
        shape_id.clone(),
        json!({
            "id": shape_id,
            "typeName": "shape",
            "type": "image",
            "parentId": parent_id,
            "index": next_shape_index(&store, &page_id),
            "props": {
                "x": position.x,
                "y": position.y,
                "width": width,
                "height": height,
                "assetId": asset_id,
            },
        }),
    );
    let replaced_shape_id = replace_shape
        .as_ref()
        .and(input.replace_shape_id)
        .map(str::to_owned);
    if let Some(shape_id) = replaced_shape_id.as_deref() {
        store.remove(shape_id);
    }
    write_canvas_state(
        &storage,
        paths,
        &bootstrap.session.id,
        &bootstrap.panel.id,
        &mut state,
        store,
        &page_id,
        &shape_id,
    )?;
    Ok(InsertImagePayload {
        session_id: bootstrap.session.id,
        panel_id: bootstrap.panel.id,
        asset_id,
        shape_id,
        asset_ref: written.asset_ref,
        asset_file: written.file_path.display().to_string(),
        asset_url,
        replaced_shape_id,
        bounds: CanvasBounds {
            x: position.x,
            y: position.y,
            width,
            height,
        },
    })
}

fn canvas_bootstrap(paths: &OpenPanelsPaths) -> Result<crate::types::ProjectBootstrap, CliError> {
    let mut request = BootstrapRequest::new();
    request.requested_panel_kind = Some(PanelKind::Canvas);
    ensure_project_bootstrap(paths, request)
}

fn state_store(state: &Value) -> Map<String, Value> {
    state
        .get("store")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn ensure_page(store: &mut Map<String, Value>, current_page_id: Option<&str>) -> String {
    let page_id = current_page_id
        .filter(|id| store.contains_key(*id))
        .map(str::to_owned)
        .or_else(|| {
            store.iter().find_map(|(id, record)| {
                (record.get("typeName").and_then(Value::as_str) == Some("page")).then(|| id.clone())
            })
        })
        .unwrap_or_else(|| "page:main".to_owned());
    store.entry(page_id.clone()).or_insert_with(|| {
        json!({
            "id": page_id,
            "typeName": "page",
            "name": "Page 1",
            "index": 1,
        })
    });
    page_id
}

fn write_canvas_state(
    storage: &Storage,
    paths: &OpenPanelsPaths,
    session_id: &str,
    panel_id: &str,
    state: &mut Value,
    store: Map<String, Value>,
    page_id: &str,
    selected_shape_id: &str,
) -> Result<(), CliError> {
    ensure_state_object(state).insert("store".to_owned(), Value::Object(store));
    ensure_state_object(state).insert("currentPageId".to_owned(), json!(page_id));
    ensure_state_object(state).insert("selectedShapeIds".to_owned(), json!([selected_shape_id]));
    storage.write_panel_state(session_id, panel_id, state)?;
    write_active_session_id(paths, session_id)?;
    Ok(())
}

fn ensure_state_object(state: &mut Value) -> &mut Map<String, Value> {
    if !state.is_object() {
        *state = json!({});
    }
    state.as_object_mut().expect("state object")
}

fn next_shape_index(store: &Map<String, Value>, page_id: &str) -> i64 {
    store
        .values()
        .filter(|record| {
            record.get("typeName").and_then(Value::as_str) == Some("shape")
                && record.get("parentId").and_then(Value::as_str) == Some(page_id)
        })
        .filter_map(|record| record.get("index").and_then(Value::as_i64))
        .max()
        .unwrap_or(0)
        + 1
}

fn shape_bounds(shape: &Value) -> CanvasBounds {
    let props = shape.get("props").and_then(Value::as_object);
    CanvasBounds {
        x: number_prop(props, "x").unwrap_or(0.0),
        y: number_prop(props, "y").unwrap_or(0.0),
        width: number_prop(props, "width")
            .or_else(|| number_prop(props, "w"))
            .unwrap_or(160.0),
        height: number_prop(props, "height")
            .or_else(|| number_prop(props, "h"))
            .unwrap_or(120.0),
    }
}

fn number_prop(props: Option<&Map<String, Value>>, name: &str) -> Option<f64> {
    props?.get(name)?.as_f64().filter(|value| value.is_finite())
}

#[derive(Clone, Copy)]
struct OccupiedBounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

fn to_occupied(bounds: CanvasBounds) -> OccupiedBounds {
    OccupiedBounds {
        min_x: bounds.x,
        min_y: bounds.y,
        max_x: bounds.x + bounds.width,
        max_y: bounds.y + bounds.height,
    }
}

fn canvas_occupied_bounds(store: &Map<String, Value>) -> Vec<OccupiedBounds> {
    store
        .values()
        .filter(|record| {
            record.get("typeName").and_then(Value::as_str) == Some("shape")
                && matches!(
                    record.get("type").and_then(Value::as_str),
                    Some("image" | "placeholder")
                )
        })
        .map(|record| to_occupied(shape_bounds(record)))
        .collect()
}

fn has_overlap(target: OccupiedBounds, occupied: &[OccupiedBounds], padding: f64) -> bool {
    occupied.iter().any(|bounds| {
        !(target.max_x <= bounds.min_x - padding
            || target.min_x >= bounds.max_x + padding
            || target.max_y <= bounds.min_y - padding
            || target.min_y >= bounds.max_y + padding)
    })
}

fn overall_bounds(bounds: &[OccupiedBounds]) -> Option<OccupiedBounds> {
    let first = *bounds.first()?;
    Some(
        bounds
            .iter()
            .copied()
            .fold(first, |current, bounds| OccupiedBounds {
                min_x: current.min_x.min(bounds.min_x),
                min_y: current.min_y.min(bounds.min_y),
                max_x: current.max_x.max(bounds.max_x),
                max_y: current.max_y.max(bounds.max_y),
            }),
    )
}

fn find_canvas_placement_position(
    store: &Map<String, Value>,
    anchor_bounds: Option<CanvasBounds>,
    width: f64,
    height: f64,
) -> CanvasBounds {
    let occupied = canvas_occupied_bounds(store);
    if occupied.is_empty() {
        return CanvasBounds {
            x: 160.0,
            y: 160.0,
            width,
            height,
        };
    }
    if let Some(anchor) = anchor_bounds {
        let candidate = CanvasBounds {
            x: anchor.x + anchor.width + DEFAULT_CANVAS_GAP,
            y: anchor.y,
            width,
            height,
        };
        if !has_overlap(to_occupied(candidate), &occupied, DEFAULT_CANVAS_GAP) {
            return candidate;
        }
    }
    let base = placement_below_existing(&occupied, DEFAULT_CANVAS_GAP).unwrap_or(CanvasBounds {
        x: 160.0,
        y: 160.0,
        width,
        height,
    });
    scan_for_available_position(base, width, height, &occupied, DEFAULT_CANVAS_GAP)
}

fn placement_below_existing(bounds: &[OccupiedBounds], padding: f64) -> Option<CanvasBounds> {
    let overall = overall_bounds(bounds)?;
    let bottom = bounds.iter().copied().reduce(|current, candidate| {
        if candidate.max_y > current.max_y
            || (candidate.max_y == current.max_y && candidate.min_x < current.min_x)
        {
            candidate
        } else {
            current
        }
    })?;
    Some(CanvasBounds {
        x: bottom.min_x,
        y: overall.max_y + padding,
        width: 0.0,
        height: 0.0,
    })
}

fn scan_for_available_position(
    base: CanvasBounds,
    width: f64,
    height: f64,
    occupied: &[OccupiedBounds],
    padding: f64,
) -> CanvasBounds {
    let initial = CanvasBounds {
        x: base.x,
        y: base.y,
        width,
        height,
    };
    if !has_overlap(to_occupied(initial), occupied, padding) {
        return initial;
    }
    let step_x = (width + padding).max(padding);
    let step_y = (height + padding).max(padding);
    for row in 0..MAX_POSITION_SCAN {
        for col in 0..MAX_POSITION_SCAN {
            let candidate = CanvasBounds {
                x: base.x + col as f64 * step_x,
                y: base.y + row as f64 * step_y,
                width,
                height,
            };
            if !has_overlap(to_occupied(candidate), occupied, padding) {
                return candidate;
            }
        }
    }
    overall_bounds(occupied)
        .map(|overall| CanvasBounds {
            x: overall.min_x,
            y: overall.max_y + padding,
            width,
            height,
        })
        .unwrap_or(initial)
}

fn place_image(anchor_bounds: Option<CanvasBounds>, width: f64, placement: &str) -> CanvasBounds {
    let Some(anchor) = anchor_bounds else {
        return CanvasBounds {
            x: 160.0,
            y: 160.0,
            width,
            height: 0.0,
        };
    };
    match placement {
        "left" => CanvasBounds {
            x: anchor.x - width - 40.0,
            y: anchor.y,
            width,
            height: 0.0,
        },
        "below" => CanvasBounds {
            x: anchor.x,
            y: anchor.y + anchor.height + 40.0,
            width,
            height: 0.0,
        },
        _ => CanvasBounds {
            x: anchor.x + anchor.width + 40.0,
            y: anchor.y,
            width,
            height: 0.0,
        },
    }
}

#[derive(Clone, Copy)]
struct ImageDimensions {
    width: f64,
    height: f64,
}

fn read_image_dimensions(buffer: &[u8]) -> Option<ImageDimensions> {
    if buffer.len() >= 24 && buffer[0] == 0x89 && &buffer[1..4] == b"PNG" {
        return Some(ImageDimensions {
            width: u32::from_be_bytes(buffer[16..20].try_into().ok()?) as f64,
            height: u32::from_be_bytes(buffer[20..24].try_into().ok()?) as f64,
        });
    }
    if buffer.len() >= 10 && &buffer[0..3] == b"GIF" {
        return Some(ImageDimensions {
            width: u16::from_le_bytes(buffer[6..8].try_into().ok()?) as f64,
            height: u16::from_le_bytes(buffer[8..10].try_into().ok()?) as f64,
        });
    }
    if buffer.len() >= 4 && buffer[0] == 0xff && buffer[1] == 0xd8 {
        let mut offset = 2;
        while offset + 8 < buffer.len() {
            if buffer[offset] != 0xff {
                break;
            }
            let marker = buffer[offset + 1];
            let length =
                u16::from_be_bytes(buffer[offset + 2..offset + 4].try_into().ok()?) as usize;
            if (0xc0..=0xc3).contains(&marker) {
                return Some(ImageDimensions {
                    height: u16::from_be_bytes(buffer[offset + 5..offset + 7].try_into().ok()?)
                        as f64,
                    width: u16::from_be_bytes(buffer[offset + 7..offset + 9].try_into().ok()?)
                        as f64,
                });
            }
            offset += 2 + length;
        }
    }
    None
}

fn create_id(prefix: &str) -> String {
    let random: u128 = rand::rng().random();
    format!("{prefix}:{random:032x}")
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}
