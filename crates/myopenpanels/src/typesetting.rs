use crate::control::{read_project_bootstrap, BootstrapRequest};
use crate::error::CliError;
use crate::paths::MyOpenPanelsPaths;
use crate::storage::{Storage, TaskInsert};
use crate::types::PanelKind;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

include!("typesetting/task_lifecycle.rs");

pub const DEFAULT_COVER_SKILL_ID: &str = "typesetting-cover-default";
pub const COVER_TASK_TYPE: &str = "generate_typesetting_cover";
pub const COVER_CAPABILITY: &str = "typesetting.generateCover";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasAssetListing {
    pub id: String,
    pub project_id: String,
    pub project_title: String,
    pub canvas_panel_id: String,
    pub asset_id: String,
    pub asset_ref: String,
    pub src: String,
    pub name: String,
    pub mime_type: String,
    pub width: Option<f64>,
    pub height: Option<f64>,
}

pub fn cover_skills(
    paths: &MyOpenPanelsPaths,
) -> Result<Vec<crate::agent::AgentSkillListing>, CliError> {
    crate::agent::list_typesetting_cover_skills(paths)
}

pub fn create_cover_request(
    paths: &MyOpenPanelsPaths,
    publication_id: &str,
    skill_id: &str,
    instruction: &str,
    request_id: &str,
) -> Result<Value, CliError> {
    let publication_id = publication_id.trim();
    let skill_id = skill_id.trim();
    let request_id = request_id.trim();
    if publication_id.is_empty() || skill_id.is_empty() || request_id.is_empty() {
        return Err(CliError::with_code(
            "invalid_cover_request",
            "Publication, Cover Skill, and request id are required.",
        ));
    }
    if instruction.chars().count() > 4_000 {
        return Err(CliError::with_code(
            "cover_instruction_too_long",
            "Cover instructions cannot exceed 4000 characters.",
        ));
    }

    let mut request = BootstrapRequest::new();
    request.requested_panel_kind = Some(PanelKind::Typesetting);
    let bootstrap = read_project_bootstrap(paths, request)?;
    let storage = Storage::open(paths)?;
    if let Some(existing) = storage
        .list_tasks(&bootstrap.project.id)?
        .into_iter()
        .find(|task| {
            task.get("queue").and_then(Value::as_str) == Some("typesetting")
                && task.get("type").and_then(Value::as_str) == Some(COVER_TASK_TYPE)
                && task.pointer("/input/requestId").and_then(Value::as_str) == Some(request_id)
        })
    {
        return Ok(json!({ "task": existing }));
    }

    let publication = bootstrap
        .state
        .get("publications")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|publication| publication.get("id").and_then(Value::as_str) == Some(publication_id))
        .cloned()
        .ok_or_else(|| {
            CliError::with_code(
                "typesetting_publication_not_found",
                format!("Typesetting publication not found: {publication_id}"),
            )
        })?;
    let title = publication
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("");
    let body_text = plain_text(publication.get("content").unwrap_or(&Value::Null));
    if title.trim().is_empty() && body_text.trim().is_empty() {
        return Err(CliError::with_code(
            "cover_source_empty",
            "Add a title or article content before creating a cover.",
        ));
    }

    let skill = crate::agent::typesetting_cover_skill(paths, skill_id)?;
    let task_id = crate::ids::random_id("task");
    let skill_files = snapshot_skill_package(
        &storage,
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &task_id,
        Path::new(&skill.local_dir),
    )?;
    let skill_hash = hash_file_manifest(&skill_files);
    let task_insert = TaskInsert {
        id: task_id,
        queue: "typesetting".to_owned(),
        task_type: COVER_TASK_TYPE.to_owned(),
        capability: COVER_CAPABILITY.to_owned(),
        target_ref: publication_id.to_owned(),
        input: json!({
            "requestId": request_id,
            "publicationId": publication_id,
            "publicationUpdatedAt": publication.get("updatedAt").cloned().unwrap_or(Value::Null),
            "instruction": instruction.trim(),
            "snapshot": { "title": title, "bodyText": body_text },
            "coverSkillId": skill_id,
            "coverSkillSnapshot": {
                "id": skill_id,
                "name": skill.skill.name,
                "source": skill.source,
                "contentHash": skill_hash,
                "files": skill_files,
            },
        }),
        source: json!({
            "typesettingPanelId": bootstrap.panel.id,
            "sourcePublicationId": publication_id,
        }),
        max_attempts: 3,
        dispatch_mode: "auto".to_owned(),
        idempotency_key: Some(request_id.to_owned()),
    };
    let (mut tasks, _) = storage.insert_tasks_with_panel_states(
        &bootstrap.project.id,
        &bootstrap.panel.id,
        &[task_insert],
        &[],
    )?;
    let task = tasks
        .pop()
        .ok_or_else(|| CliError::new("Created Cover Task was not found."))?;
    Ok(json!({ "task": task }))
}

pub fn is_cover_task_type(task_type: &str) -> bool {
    task_type == COVER_TASK_TYPE
}

pub fn plain_text(document: &Value) -> String {
    let mut output = String::new();
    append_plain_text(document, &mut output);
    output.trim().to_owned()
}

fn append_plain_text(node: &Value, output: &mut String) {
    if let Some(text) = node.get("text").and_then(Value::as_str) {
        output.push_str(text);
    }
    let node_type = node.get("type").and_then(Value::as_str).unwrap_or("");
    if node_type == "hardBreak" {
        output.push('\n');
    }
    for child in node
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        append_plain_text(child, output);
    }
    if matches!(
        node_type,
        "paragraph" | "heading" | "listItem" | "blockquote"
    ) && !output.ends_with('\n')
    {
        output.push('\n');
    }
}

fn snapshot_skill_package(
    storage: &Storage,
    project_id: &str,
    panel_id: &str,
    task_id: &str,
    root: &Path,
) -> Result<Vec<Value>, CliError> {
    let mut source_files = Vec::new();
    collect_regular_files(root, root, &mut source_files)?;
    let mut files = Vec::new();
    for (relative, bytes) in source_files {
        let relative_text = relative.to_string_lossy().replace('\\', "/");
        let requested = format!("cover-tasks/{task_id}/skill/{relative_text}");
        let written =
            storage.write_asset_from_buffer(project_id, panel_id, &requested, &bytes, true)?;
        files.push(json!({
            "path": relative_text,
            "assetRef": written.asset_ref,
            "contentHash": format!("sha256:{:x}", Sha256::digest(&bytes)),
            "sizeBytes": bytes.len(),
        }));
    }
    files.sort_by(|left, right| left["path"].as_str().cmp(&right["path"].as_str()));
    Ok(files)
}

fn collect_regular_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(PathBuf, Vec<u8>)>,
) -> Result<(), CliError> {
    for entry in fs::read_dir(directory).map_err(to_cli_error)? {
        let entry = entry.map_err(to_cli_error)?;
        let file_type = entry.file_type().map_err(to_cli_error)?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_regular_files(root, &entry.path(), files)?;
        } else if file_type.is_file() {
            files.push((
                entry
                    .path()
                    .strip_prefix(root)
                    .map_err(to_cli_error)?
                    .to_owned(),
                fs::read(entry.path()).map_err(to_cli_error)?,
            ));
        }
    }
    Ok(())
}

fn hash_file_manifest(files: &[Value]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(
            file.get("path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .as_bytes(),
        );
        hasher.update(
            file.get("contentHash")
                .and_then(Value::as_str)
                .unwrap_or("")
                .as_bytes(),
        );
    }
    format!("sha256:{:x}", hasher.finalize())
}

pub fn list_canvas_assets(
    paths: &MyOpenPanelsPaths,
    current_project_id: &str,
    scope: &str,
) -> Result<Value, CliError> {
    if !matches!(scope, "current" | "all") {
        return Err(CliError::with_code(
            "invalid_target",
            "Typesetting asset scope must be current or all.",
        ));
    }
    let storage = Storage::open(paths)?;
    let mut projects = storage.list_projects()?;
    if !projects
        .iter()
        .any(|project| project.id == current_project_id)
    {
        return Err(CliError::with_code(
            "project_not_found",
            format!("MyOpenPanels project not found: {current_project_id}"),
        ));
    }
    if scope == "current" {
        projects.retain(|project| project.id == current_project_id);
    } else {
        projects.sort_by_key(|project| usize::from(project.id != current_project_id));
    }

    let mut assets = Vec::new();
    for project in projects {
        for panel_id in &project.panel_ids {
            let Some(panel) = storage.read_panel(&project.id, panel_id)? else {
                continue;
            };
            if panel.kind != PanelKind::Canvas {
                continue;
            }
            let Some(state) = storage.read_panel_state(&project.id, &panel.id)? else {
                continue;
            };
            assets.extend(canvas_assets_from_state(
                &project.id,
                &project.title,
                &panel.id,
                &state,
            ));
        }
    }
    Ok(json!({ "assets": assets, "scope": scope }))
}

pub fn import_canvas_asset(
    paths: &MyOpenPanelsPaths,
    target_project_id: &str,
    target_panel_id: &str,
    source_asset_ref: &str,
) -> Result<Value, CliError> {
    let storage = Storage::open(paths)?;
    let target = storage
        .read_panel(target_project_id, target_panel_id)?
        .ok_or_else(|| {
            CliError::with_code(
                "target_not_found",
                format!("Typesetting panel not found: {target_panel_id}"),
            )
        })?;
    if target.kind != PanelKind::Typesetting {
        return Err(CliError::with_code(
            "invalid_target",
            "Canvas assets can only be imported into a Typesetting panel.",
        ));
    }

    let source = parse_canvas_asset_ref(source_asset_ref)?;
    let source_panel = storage
        .read_panel(&source.project_id, &source.panel_id)?
        .ok_or_else(|| {
            CliError::with_code(
                "target_not_found",
                format!("Source Canvas panel not found: {}", source.panel_id),
            )
        })?;
    if source_panel.kind != PanelKind::Canvas {
        return Err(CliError::with_code(
            "invalid_target",
            "The source asset must belong to a Canvas panel.",
        ));
    }
    let source_path = storage.asset_path(source_asset_ref)?;
    if !source_path.is_file() {
        return Err(CliError::with_code(
            "target_not_found",
            format!("Canvas asset not found: {source_asset_ref}"),
        ));
    }
    let bytes = storage.read_asset(source_asset_ref)?;
    let written = storage.write_asset_from_buffer(
        target_project_id,
        target_panel_id,
        &source.file_name,
        &bytes,
        false,
    )?;
    let mime_type = mime_guess::from_path(&written.file_name)
        .first_raw()
        .unwrap_or("application/octet-stream");
    Ok(json!({
        "assetRef": written.asset_ref,
        "fileName": written.file_name,
        "mimeType": mime_type,
        "sourceAssetRef": source_asset_ref,
        "sourceProjectId": source.project_id,
        "sourceCanvasPanelId": source.panel_id,
        "src": format!(
            "/api/projects/{}/panels/{}/assets/{}",
            target_project_id, target_panel_id, written.file_name
        ),
    }))
}

fn canvas_assets_from_state(
    project_id: &str,
    project_title: &str,
    panel_id: &str,
    state: &Value,
) -> Vec<CanvasAssetListing> {
    let Some(store) = state.get("store").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut shapes = store
        .values()
        .filter(|record| {
            record.get("typeName").and_then(Value::as_str) == Some("shape")
                && record.get("type").and_then(Value::as_str) == Some("image")
        })
        .collect::<Vec<_>>();
    shapes.sort_by_key(|shape| {
        std::cmp::Reverse(
            shape
                .get("index")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
        )
    });

    let mut seen = HashSet::new();
    shapes
        .into_iter()
        .filter_map(|shape| {
            let asset_id = shape.pointer("/props/assetId").and_then(Value::as_str)?;
            if !seen.insert(asset_id.to_owned()) {
                return None;
            }
            let asset = store.get(asset_id)?;
            if asset.get("typeName").and_then(Value::as_str) != Some("asset")
                || asset.get("type").and_then(Value::as_str) != Some("image")
            {
                return None;
            }
            let asset_ref = asset
                .pointer("/meta/assetRef")
                .and_then(Value::as_str)?
                .to_owned();
            let parsed = parse_canvas_asset_ref(&asset_ref).ok()?;
            if parsed.project_id != project_id || parsed.panel_id != panel_id {
                return None;
            }
            Some(CanvasAssetListing {
                id: format!("{project_id}:{panel_id}:{asset_id}"),
                project_id: project_id.to_owned(),
                project_title: project_title.to_owned(),
                canvas_panel_id: panel_id.to_owned(),
                asset_id: asset_id.to_owned(),
                asset_ref,
                src: format!(
                    "/api/projects/{project_id}/panels/{panel_id}/assets/{}",
                    parsed.file_name
                ),
                name: asset
                    .pointer("/props/name")
                    .and_then(Value::as_str)
                    .unwrap_or(&parsed.file_name)
                    .to_owned(),
                mime_type: asset
                    .pointer("/props/mimeType")
                    .and_then(Value::as_str)
                    .unwrap_or("image/*")
                    .to_owned(),
                width: asset.pointer("/props/w").and_then(Value::as_f64),
                height: asset.pointer("/props/h").and_then(Value::as_f64),
            })
        })
        .collect()
}

struct ParsedCanvasAssetRef {
    project_id: String,
    panel_id: String,
    file_name: String,
}

fn parse_canvas_asset_ref(asset_ref: &str) -> Result<ParsedCanvasAssetRef, CliError> {
    let parts = asset_ref.split('/').collect::<Vec<_>>();
    if parts.len() < 6
        || parts[0] != "projects"
        || parts[2] != "panels"
        || parts[4] != "assets"
        || parts[1].is_empty()
        || parts[3].is_empty()
    {
        return Err(CliError::with_code(
            "invalid_target",
            "Expected a Project Canvas asset reference.",
        ));
    }
    let file_name = parts[5..].join("/");
    if file_name.is_empty() {
        return Err(CliError::with_code(
            "invalid_target",
            "Canvas asset reference is missing a file name.",
        ));
    }
    Ok(ParsedCanvasAssetRef {
        project_id: parts[1].to_owned(),
        panel_id: parts[3].to_owned(),
        file_name,
    })
}

fn to_cli_error(error: impl std::fmt::Display) -> CliError {
    CliError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{
        create_project, ensure_project_bootstrap, read_active_project_id, BootstrapRequest,
    };
    use crate::paths::resolve_myopenpanels_paths;

    fn test_paths() -> (tempfile::TempDir, MyOpenPanelsPaths) {
        let temp = tempfile::tempdir().expect("temp");
        let project_dir = temp.path().join("project");
        let storage_dir = temp.path().join("storage");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        let paths = resolve_myopenpanels_paths(
            Some(project_dir.to_str().unwrap()),
            Some(storage_dir.to_str().unwrap()),
            Some("ctx"),
        )
        .expect("paths");
        (temp, paths)
    }

    fn panel_id(bootstrap: &crate::types::ProjectBootstrap, kind: PanelKind) -> String {
        bootstrap
            .panels
            .iter()
            .find(|snapshot| snapshot.panel.kind == kind)
            .expect("panel")
            .panel
            .id
            .clone()
    }

    fn add_canvas_asset(
        paths: &MyOpenPanelsPaths,
        project_id: &str,
        panel_id: &str,
        name: &str,
        duplicate_shape: bool,
    ) -> String {
        let storage = Storage::open(paths).expect("storage");
        let written = storage
            .write_asset_from_buffer(
                project_id,
                panel_id,
                name,
                b"independent image bytes",
                false,
            )
            .expect("asset");
        let mut store = serde_json::Map::new();
        store.insert(
            "asset:image".to_owned(),
            json!({
                "id": "asset:image",
                "typeName": "asset",
                "type": "image",
                "props": {
                    "name": name,
                    "mimeType": "image/png",
                    "w": 640,
                    "h": 480
                },
                "meta": { "assetRef": written.asset_ref }
            }),
        );
        store.insert(
            "shape:image:1".to_owned(),
            json!({
                "id": "shape:image:1",
                "typeName": "shape",
                "type": "image",
                "index": 2,
                "props": { "assetId": "asset:image" }
            }),
        );
        if duplicate_shape {
            store.insert(
                "shape:image:2".to_owned(),
                json!({
                    "id": "shape:image:2",
                    "typeName": "shape",
                    "type": "image",
                    "index": 1,
                    "props": { "assetId": "asset:image" }
                }),
            );
        }
        storage
            .write_panel_state(
                project_id,
                panel_id,
                &json!({
                    "schema": { "schemaVersion": 1 },
                    "store": store
                }),
            )
            .expect("canvas state");
        written.asset_ref
    }

    #[test]
    fn canvas_asset_queries_deduplicate_without_changing_active_project() {
        let (_temp, paths) = test_paths();
        let source = create_project(&paths, Some("Source")).expect("source");
        let source_canvas = panel_id(&source, PanelKind::Canvas);
        add_canvas_asset(
            &paths,
            &source.project.id,
            &source_canvas,
            "source.png",
            true,
        );
        let current = create_project(&paths, Some("Current")).expect("current");
        let current_canvas = panel_id(&current, PanelKind::Canvas);
        add_canvas_asset(
            &paths,
            &current.project.id,
            &current_canvas,
            "current.png",
            false,
        );

        let current_assets =
            list_canvas_assets(&paths, &current.project.id, "current").expect("current assets");
        assert_eq!(current_assets["assets"].as_array().unwrap().len(), 1);
        assert_eq!(
            current_assets["assets"][0]["projectId"],
            json!(current.project.id)
        );

        let all = list_canvas_assets(&paths, &current.project.id, "all").expect("all assets");
        let assets = all["assets"].as_array().expect("assets");
        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0]["projectId"], json!(current.project.id));
        assert_eq!(assets[1]["projectId"], json!(source.project.id));
        assert_eq!(
            read_active_project_id(&paths).expect("active project"),
            Some(current.project.id)
        );
    }

    #[test]
    fn imported_assets_validate_panel_kinds_and_outlive_the_source_project() {
        let (_temp, paths) = test_paths();
        let source = create_project(&paths, Some("Source")).expect("source");
        let source_canvas = panel_id(&source, PanelKind::Canvas);
        let source_ref = add_canvas_asset(
            &paths,
            &source.project.id,
            &source_canvas,
            "source.png",
            false,
        );
        let target = create_project(&paths, Some("Target")).expect("target");
        let target_canvas = panel_id(&target, PanelKind::Canvas);
        let target_typesetting = panel_id(&target, PanelKind::Typesetting);

        let wrong_target =
            import_canvas_asset(&paths, &target.project.id, &target_canvas, &source_ref)
                .expect_err("canvas target must fail");
        assert_eq!(wrong_target.code(), Some("invalid_target"));

        let imported =
            import_canvas_asset(&paths, &target.project.id, &target_typesetting, &source_ref)
                .expect("import");
        let imported_ref = imported["assetRef"].as_str().expect("asset ref");
        assert_ne!(imported_ref, source_ref);
        let wrong_source = import_canvas_asset(
            &paths,
            &target.project.id,
            &target_typesetting,
            imported_ref,
        )
        .expect_err("typesetting source must fail");
        assert_eq!(wrong_source.code(), Some("invalid_target"));

        Storage::open(&paths)
            .expect("storage")
            .delete_project(&source.project.id)
            .expect("delete source");
        let copied = Storage::open(&paths)
            .expect("storage")
            .read_asset(imported_ref)
            .expect("copied asset");
        assert_eq!(copied, b"independent image bytes");

        let refreshed = ensure_project_bootstrap(
            &paths,
            BootstrapRequest {
                requested_project_id: Some(target.project.id),
                requested_panel_id: None,
                requested_panel_kind: Some(PanelKind::Typesetting),
            },
        )
        .expect("target remains readable");
        assert_eq!(refreshed.active_panel_kind, PanelKind::Typesetting);
    }

    #[test]
    fn cover_request_snapshots_article_and_skill_and_is_idempotent() {
        let (_temp, paths) = test_paths();
        let bootstrap = create_project(&paths, Some("Covers")).expect("project");
        let typesetting_panel = panel_id(&bootstrap, PanelKind::Typesetting);
        let storage = Storage::open(&paths).expect("storage");
        storage
            .write_panel_state(
                &bootstrap.project.id,
                &typesetting_panel,
                &json!({
                    "schemaVersion": 2,
                    "publications": [{
                        "id": "publication:cover",
                        "title": "A quiet city",
                        "covers": [],
                        "content": {
                            "type": "doc",
                            "content": [{
                                "type": "paragraph",
                                "content": [{ "type": "text", "text": "Streets after rain" }]
                            }]
                        },
                        "createdAt": "2026-07-21T00:00:00Z",
                        "updatedAt": "2026-07-21T00:00:00Z"
                    }]
                }),
            )
            .expect("typesetting state");

        let skills = cover_skills(&paths).expect("cover skills");
        assert!(skills
            .iter()
            .any(|listing| listing.skill.id == DEFAULT_COVER_SKILL_ID));
        let created = create_cover_request(
            &paths,
            "publication:cover",
            DEFAULT_COVER_SKILL_ID,
            "Use restrained colors",
            "cover-request:1",
        )
        .expect("cover request");
        let task = &created["task"];
        assert_eq!(task["queue"], json!("typesetting"));
        assert_eq!(task["type"], json!(COVER_TASK_TYPE));
        assert_eq!(task["capability"], json!(COVER_CAPABILITY));
        assert_eq!(task["targetId"], json!("publication:cover"));
        assert_eq!(task["input"]["snapshot"]["title"], json!("A quiet city"));
        assert_eq!(
            task["input"]["snapshot"]["bodyText"],
            json!("Streets after rain")
        );
        assert_eq!(
            task["input"]["coverSkillSnapshot"]["id"],
            json!(DEFAULT_COVER_SKILL_ID)
        );
        assert!(task["input"]["coverSkillSnapshot"]["files"]
            .as_array()
            .is_some_and(|files| files.iter().any(|file| file["path"] == "SKILL.md")));

        let repeated = create_cover_request(
            &paths,
            "publication:cover",
            DEFAULT_COVER_SKILL_ID,
            "A different retry payload is ignored",
            "cover-request:1",
        )
        .expect("idempotent retry");
        assert_eq!(repeated["task"]["id"], task["id"]);
        assert_eq!(
            storage
                .list_tasks(&bootstrap.project.id)
                .expect("tasks")
                .len(),
            1
        );
    }

    #[test]
    fn cover_completion_appends_once_and_rejects_a_deleted_publication() {
        let (_temp, paths) = test_paths();
        let bootstrap = create_project(&paths, Some("Covers")).expect("project");
        let typesetting_panel = panel_id(&bootstrap, PanelKind::Typesetting);
        let storage = Storage::open(&paths).expect("storage");
        let state = json!({
            "schemaVersion": 2,
            "publications": [{
                "id": "publication:cover",
                "title": "Cover target",
                "covers": [],
                "content": { "type": "doc", "content": [{ "type": "paragraph" }] },
                "createdAt": "2026-07-21T00:00:00Z",
                "updatedAt": "2026-07-21T00:00:00Z"
            }]
        });
        storage
            .write_panel_state(&bootstrap.project.id, &typesetting_panel, &state)
            .expect("typesetting state");
        let created = create_cover_request(
            &paths,
            "publication:cover",
            DEFAULT_COVER_SKILL_ID,
            "",
            "cover-request:complete",
        )
        .expect("cover request");
        let task_id = created["task"]["id"].as_str().expect("task id");
        let result = json!({
            "runtimeFinalization": {
                "artifacts": [{
                    "assetRef": format!("projects/{}/panels/{typesetting_panel}/assets/cover-tasks/{task_id}/cover.png", bootstrap.project.id),
                    "fileName": format!("cover-tasks/{task_id}/cover.png"),
                    "mimeType": "image/png",
                    "width": 1200,
                    "height": 900
                }]
            }
        });
        let (_, completed_state) = prepare_task_completion(&paths, task_id, Some(result.clone()))
            .expect("prepare completion")
            .expect("panel state");
        let covers = completed_state["publications"][0]["covers"]
            .as_array()
            .expect("covers");
        assert_eq!(covers.len(), 1);
        assert_eq!(covers[0]["source"]["taskId"], json!(task_id));
        assert_eq!(
            covers[0]["source"]["skillId"],
            json!(DEFAULT_COVER_SKILL_ID)
        );

        storage
            .write_panel_state(&bootstrap.project.id, &typesetting_panel, &completed_state)
            .expect("persist prepared state");
        let (_, repeated_state) = prepare_task_completion(&paths, task_id, Some(result.clone()))
            .expect("repeat completion")
            .expect("panel state");
        assert_eq!(
            repeated_state["publications"][0]["covers"]
                .as_array()
                .expect("covers")
                .len(),
            1
        );

        storage
            .write_panel_state(
                &bootstrap.project.id,
                &typesetting_panel,
                &json!({ "schemaVersion": 2, "publications": [] }),
            )
            .expect("delete publication");
        let error = prepare_task_completion(&paths, task_id, Some(result))
            .expect_err("deleted publication must reject completion");
        assert_eq!(error.code(), Some("typesetting_publication_not_found"));
    }
}
