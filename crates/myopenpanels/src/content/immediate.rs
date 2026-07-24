use super::filesystem::{
    prepare_staged_resource, read_authoritative_pointer, read_dirs, read_json, resource_dir,
    resource_snapshot_at_revision, revision_dir, revision_object_path, to_cli_error,
    validate_logical_path, write_json_atomic, write_materialized_file, write_staged_file,
    ActivePointer, ContentCommit, ResourceKind, RevisionManifest, StagedResource,
    MAX_STAGING_BYTES, MAX_TEXT_FILE_BYTES,
};
use crate::error::CliError;
use crate::paths::MyOpenPanelsPaths;
use rusqlite::TransactionBehavior;
use serde_json::{json, Value};
use std::path::PathBuf;

pub fn active_file_path(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
) -> Result<Option<PathBuf>, CliError> {
    validate_logical_path(logical_path)?;
    let Some(active) = read_authoritative_pointer(paths, project_id, kind, resource_key)? else {
        return Ok(None);
    };
    let resource = resource_dir(paths, project_id, kind, resource_key);
    let revision = revision_dir(&resource, &active.revision_id);
    let manifest: RevisionManifest = read_json(&revision.join("manifest.json"))?;
    let Some(file) = manifest
        .files
        .iter()
        .find(|file| file.logical_path == logical_path)
    else {
        return Ok(None);
    };
    let object = revision_object_path(&revision, file)?;
    if !object.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(object).map_err(to_cli_error)?;
    if bytes.len() as i64 != file.size_bytes
        || super::filesystem::hash_bytes(&bytes) != file.content_hash
    {
        return Err(CliError::with_code(
            "content_integrity_failed",
            "Active content does not match its manifest.",
        ));
    }
    let materialized = revision.join("materialized").join(logical_path);
    let materialized_matches = std::fs::read(&materialized)
        .ok()
        .is_some_and(|value| value == bytes);
    if !materialized_matches {
        write_materialized_file(&materialized, &bytes)?;
    }
    Ok(Some(materialized))
}

#[allow(clippy::too_many_arguments)]
pub fn commit_immediate_text(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: Option<&str>,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
    content: &[u8],
    mime_type: &str,
    replace_all: bool,
) -> Result<Value, CliError> {
    validate_logical_path(logical_path)?;
    validate_immediate_text_content(content)?;
    commit_immediate_file(
        paths,
        project_id,
        panel_id,
        kind,
        resource_key,
        logical_path,
        content,
        mime_type,
        replace_all,
    )
}

pub(crate) fn validate_immediate_text_content(content: &[u8]) -> Result<(), CliError> {
    if content.len() > MAX_TEXT_FILE_BYTES || std::str::from_utf8(content).is_err() {
        return Err(CliError::with_code(
            "invalid_output",
            "Content must be bounded UTF-8 text.",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn commit_immediate_file(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: Option<&str>,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
    content: &[u8],
    mime_type: &str,
    replace_all: bool,
) -> Result<Value, CliError> {
    commit_immediate_files(
        paths,
        project_id,
        panel_id,
        kind,
        resource_key,
        &[ImmediateFile {
            logical_path,
            content,
            mime_type,
        }],
        replace_all,
    )
}

pub(crate) struct ImmediateFile<'a> {
    pub(crate) logical_path: &'a str,
    pub(crate) content: &'a [u8],
    pub(crate) mime_type: &'a str,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn commit_immediate_files(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: Option<&str>,
    kind: ResourceKind,
    resource_key: &str,
    files: &[ImmediateFile<'_>],
    replace_all: bool,
) -> Result<Value, CliError> {
    let total_size = files.iter().try_fold(0_i64, |total, file| {
        validate_logical_path(file.logical_path)?;
        total
            .checked_add(file.content.len() as i64)
            .ok_or_else(|| CliError::with_code("content_too_large", "Content is too large."))
    })?;
    if total_size > MAX_STAGING_BYTES {
        return Err(CliError::with_code(
            "content_too_large",
            "Content exceeds the per-resource staging limit.",
        ));
    }
    let active = read_authoritative_pointer(paths, project_id, kind, resource_key)?;
    let staging = tempfile::tempdir_in(&paths.storage_dir).map_err(to_cli_error)?;
    let staged = StagedResource {
        project_id: project_id.to_owned(),
        panel_id: panel_id.unwrap_or_default().to_owned(),
        resource_kind: kind.as_str().to_owned(),
        resource_key: resource_key.to_owned(),
        base_revision_id: active.as_ref().map(|value| value.revision_id.clone()),
        base_content_version: active.as_ref().map_or(0, |value| value.content_version),
        metadata: json!({ "replaceAll": replace_all }),
    };
    write_json_atomic(&staging.path().join("resource.json"), &staged)?;
    for file in files {
        write_staged_file(
            staging.path(),
            file.logical_path,
            file.content,
            file.mime_type,
            json!({}),
        )?;
    }
    write_json_atomic(&staging.path().join("replace-all.json"), &replace_all)?;
    let (active_path, pointer) = prepare_staged_resource(paths, &staged, staging.path(), None)?;
    publish_immediate_pointer_with_authority(
        paths,
        project_id,
        kind,
        resource_key,
        &active_path,
        &pointer,
    )?;
    Ok(json!({
        "revisionId": pointer.revision_id,
        "contentVersion": pointer.content_version,
        "manifestHash": pointer.manifest_hash,
        "contentHash": pointer.content_hash,
    }))
}

pub(crate) fn publish_immediate_pointer_with_authority(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    active_path: &std::path::Path,
    pointer: &ActivePointer,
) -> Result<(), CliError> {
    if kind != ResourceKind::WritingSkill {
        let mut storage = crate::storage::Storage::open(paths)?;
        let exists = storage
            .connection()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM resources WHERE project_id = ? AND id = ? AND deleted_at IS NULL)",
                rusqlite::params![project_id, resource_key],
                |row| row.get::<_, bool>(0),
            )
            .map_err(to_cli_error)?;
        if exists {
            let tx = storage
                .connection_mut()
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(to_cli_error)?;
            let commit = ContentCommit {
                resource_kind: kind.as_str().to_owned(),
                resource_key: resource_key.to_owned(),
                revision_id: pointer.revision_id.clone(),
                content_version: pointer.content_version,
                manifest_hash: pointer.manifest_hash.clone(),
                content_hash: pointer.content_hash.clone(),
            };
            crate::storage::Storage::write_content_commit_in_transaction(&tx, project_id, &commit)?;
            tx.commit().map_err(to_cli_error)?;
            return write_json_atomic(active_path, pointer);
        }
        return write_json_atomic(&active_path.with_file_name("pending.json"), pointer);
    }
    write_json_atomic(active_path, pointer)
}

#[derive(Debug)]
struct PendingContentCommit {
    active_path: PathBuf,
    pending_path: PathBuf,
    pointer: ActivePointer,
    commit: ContentCommit,
}

#[cfg(test)]
pub(crate) fn write_panel_state_with_pending_content(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: &str,
    state: &Value,
) -> Result<(), CliError> {
    write_panel_state_and_tasks_with_pending_content(
        paths,
        project_id,
        panel_id,
        None,
        "",
        &[],
        state,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn write_panel_state_and_tasks_with_pending_content(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: &str,
    expected_revision: Option<i64>,
    queue: &str,
    tasks: &[Value],
    state: &Value,
) -> Result<(), CliError> {
    commit_with_pending_content(paths, project_id, |tx| {
        if let Some(expected_revision) = expected_revision {
            crate::storage::Storage::write_panel_state_if_revision_in_transaction(
                tx,
                project_id,
                panel_id,
                expected_revision,
                state,
            )?;
        } else {
            crate::storage::Storage::write_panel_state_in_transaction(
                tx, project_id, panel_id, state,
            )?;
        }
        if !tasks.is_empty() {
            crate::storage::Storage::upsert_tasks_in_transaction(
                tx, project_id, panel_id, queue, tasks,
            )?;
        }
        Ok(())
    })
}

pub(crate) fn create_my_document_with_pending_content(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    panel_id: &str,
    tasks: &[Value],
    document: &Value,
) -> Result<(), CliError> {
    commit_with_pending_content(paths, project_id, |tx| {
        if !tasks.is_empty() {
            crate::storage::Storage::upsert_tasks_in_transaction(
                tx, project_id, panel_id, "wiki", tasks,
            )?;
        }
        crate::storage::Storage::create_my_document_in_transaction(
            tx, project_id, panel_id, document,
        )?;
        Ok(())
    })
}

fn commit_with_pending_content(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    write: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<(), CliError>,
) -> Result<(), CliError> {
    let pending = collect_pending_content(paths, project_id)?;
    let mut storage = crate::storage::Storage::open(paths)?;
    let tx = storage
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(to_cli_error)?;
    write(&tx)?;
    let mut activated = Vec::new();
    for value in pending {
        let exists = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM resources WHERE project_id = ? AND id = ? AND deleted_at IS NULL)",
                rusqlite::params![project_id, value.commit.resource_key],
                |row| row.get::<_, bool>(0),
            )
            .map_err(to_cli_error)?;
        if !exists {
            continue;
        }
        crate::storage::Storage::write_content_commit_in_transaction(
            &tx,
            project_id,
            &value.commit,
        )?;
        activated.push(value);
    }
    tx.commit().map_err(to_cli_error)?;

    for value in activated {
        write_json_atomic(&value.active_path, &value.pointer)?;
        if value.pending_path.is_file() {
            std::fs::remove_file(&value.pending_path).map_err(to_cli_error)?;
        }
    }
    Ok(())
}

fn collect_pending_content(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
) -> Result<Vec<PendingContentCommit>, CliError> {
    let mut pending = Vec::new();
    for kind in [
        ResourceKind::WikiMarkdown,
        ResourceKind::WikiSpace,
        ResourceKind::MyDocument,
    ] {
        let kind_dir = crate::storage::project_storage_dir(&paths.storage_dir, project_id)
            .join("content")
            .join(kind.as_str());
        for resource in read_dirs(&kind_dir)? {
            let pending_path = resource.join("pending.json");
            if !pending_path.is_file() {
                continue;
            }
            let descriptor: Value = read_json(&resource.join("resource.json"))?;
            let resource_key = descriptor
                .get("resourceKey")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    CliError::with_code(
                        "invalid_content_resource",
                        "Pending content resource is missing its logical key.",
                    )
                })?
                .to_owned();
            let pointer: ActivePointer = read_json(&pending_path)?;
            validate_pending_pointer(paths, project_id, kind, &resource_key, &pointer)?;
            let active_path = resource.join("active.json");
            pending.push(PendingContentCommit {
                active_path,
                pending_path,
                commit: ContentCommit {
                    resource_kind: kind.as_str().to_owned(),
                    resource_key,
                    revision_id: pointer.revision_id.clone(),
                    content_version: pointer.content_version,
                    manifest_hash: pointer.manifest_hash.clone(),
                    content_hash: pointer.content_hash.clone(),
                },
                pointer,
            });
        }
    }
    Ok(pending)
}

fn validate_pending_pointer(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    pointer: &ActivePointer,
) -> Result<(), CliError> {
    let snapshot =
        resource_snapshot_at_revision(paths, project_id, kind, resource_key, &pointer.revision_id)?
            .ok_or_else(|| {
                CliError::with_code(
                    "content_integrity_failed",
                    format!(
                        "Pending content revision {} is missing.",
                        pointer.revision_id
                    ),
                )
            })?;
    if snapshot.content_version != pointer.content_version
        || snapshot.manifest_hash != pointer.manifest_hash
        || snapshot.content_hash != pointer.content_hash
    {
        return Err(CliError::with_code(
            "content_integrity_failed",
            format!(
                "Pending content revision {} does not match its pointer.",
                pointer.revision_id
            ),
        ));
    }
    Ok(())
}
