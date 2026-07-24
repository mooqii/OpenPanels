use super::*;
use crate::content::commit_immediate_text;

use crate::control::{ensure_project_bootstrap, BootstrapRequest};
use crate::types::PanelKind;

#[test]
fn new_document_content_uses_portable_paths_and_survives_recovery() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project dir");
    let paths = crate::paths::resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("content-recovery"),
    )
    .expect("paths");
    let bootstrap = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let wiki_panel = bootstrap
        .panels
        .iter()
        .find(|panel| panel.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");

    commit_immediate_text(
        &paths,
        &bootstrap.project.id,
        Some(&wiki_panel.panel.id),
        ResourceKind::MyDocument,
        "my-document:portable",
        "content.md",
        b"portable document",
        "text/markdown",
        true,
    )
    .expect("document");
    let resource = resource_dir(
        &paths,
        &bootstrap.project.id,
        ResourceKind::MyDocument,
        "my-document:portable",
    );
    let active = read_active_pointer(
        &paths,
        &bootstrap.project.id,
        ResourceKind::MyDocument,
        "my-document:portable",
    )
    .expect("active pointer")
    .expect("active revision");
    let revision = revision_dir(&resource, &active.revision_id);
    assert_eq!(
        resource.file_name().and_then(|value| value.to_str()),
        Some("my-document_portable")
    );
    assert!(revision
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.starts_with("revision_")));
    assert!(revision.join("files/content.md").is_file());

    recover_filesystem(&paths).expect("recovery");

    assert_eq!(
        read_active_text(
            &paths,
            &bootstrap.project.id,
            ResourceKind::MyDocument,
            "my-document:portable",
            "content.md",
        )
        .expect("active text")
        .as_deref(),
        Some("portable document")
    );
}

#[test]
fn startup_recovery_preserves_an_unresolvable_active_revision() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project dir");
    let paths = crate::paths::resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("content-recovery"),
    )
    .expect("paths");
    let bootstrap = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let resource = resource_dir(
        &paths,
        &bootstrap.project.id,
        ResourceKind::MyDocument,
        "my-document:legacy",
    );
    let unexpected_revision = revision_dir(&resource, "revision:unresolvable");
    fs::create_dir_all(unexpected_revision.join("files")).expect("unexpected revision");
    fs::write(unexpected_revision.join("files/content.md"), "preserve me")
        .expect("unexpected content");
    write_json_atomic(
        &resource.join("active.json"),
        &ActivePointer {
            revision_id: "revision:unresolvable".to_owned(),
            content_version: 1,
            manifest_hash: "unresolvable".to_owned(),
            archived: false,
        },
    )
    .expect("active pointer");

    recover_filesystem(&paths).expect("recovery");

    assert_eq!(
        fs::read_to_string(unexpected_revision.join("files/content.md"))
            .expect("unexpected content preserved"),
        "preserve me"
    );
}

#[test]
fn startup_finishes_committed_publication_and_removes_orphans() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project dir");
    let paths = crate::paths::resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("content-recovery"),
    )
    .expect("paths");
    let bootstrap = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let wiki_panel = bootstrap
        .panels
        .iter()
        .find(|panel| panel.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");

    commit_immediate_text(
        &paths,
        &bootstrap.project.id,
        Some(&wiki_panel.panel.id),
        ResourceKind::WikiSpace,
        "wiki:default",
        "index.md",
        b"one",
        "text/markdown",
        true,
    )
    .expect("initial revision");
    let active = read_active_pointer(
        &paths,
        &bootstrap.project.id,
        ResourceKind::WikiSpace,
        "wiki:default",
    )
    .expect("active pointer")
    .expect("active revision");
    let staged = StagedResource {
        project_id: bootstrap.project.id.clone(),
        panel_id: wiki_panel.panel.id.clone(),
        resource_kind: ResourceKind::WikiSpace.as_str().to_owned(),
        resource_key: "wiki:default".to_owned(),
        base_revision_id: Some(active.revision_id),
        base_content_version: active.content_version,
        metadata: json!({ "replaceAll": true }),
    };
    let stage = tempfile::tempdir_in(&storage_dir).expect("stage");
    write_json_atomic(&stage.path().join("resource.json"), &staged).expect("staged metadata");
    write_materialized_file(&stage.path().join("files/index.md"), b"two").expect("staged file");
    let (_, pointer) =
        prepare_staged_resource(&paths, &staged, stage.path(), None).expect("prepared revision");

    let storage = Storage::open(&paths).expect("storage");
    let task = storage
        .insert_task(
            &bootstrap.project.id,
            &wiki_panel.panel.id,
            "wiki",
            "maintain_wiki",
            "wiki.maintain",
            "wiki:default",
            &json!({ "changeEvents": [] }),
            &json!({ "agentSkillId": "wiki-default" }),
        )
        .expect("task");
    let result = json!({
        "contentCommits": [{
            "resourceKind": ResourceKind::WikiSpace.as_str(),
            "resourceKey": "wiki:default",
            "revisionId": pointer.revision_id,
            "contentVersion": pointer.content_version,
            "manifestHash": pointer.manifest_hash,
        }]
    });
    storage
        .connection()
        .execute(
            "UPDATE tasks SET status = 'succeeded', result_json = ?, completed_at = ? WHERE id = ?",
            params![result.to_string(), now_iso(), task["id"].as_str().unwrap()],
        )
        .expect("committed task result");
    drop(storage);

    let resource = resource_dir(
        &paths,
        &bootstrap.project.id,
        ResourceKind::WikiSpace,
        "wiki:default",
    );
    let orphan = resource.join(sanitize_path_part("revision:orphan"));
    fs::create_dir_all(&orphan).expect("orphan");
    let abandoned_stage = storage_dir
        .join("projects")
        .join(sanitize_path_part(&bootstrap.project.id))
        .join("content/.staging")
        .join(sanitize_path_part("task:old"))
        .join("1");
    fs::create_dir_all(&abandoned_stage).expect("abandoned staging");

    recover_filesystem(&paths).expect("recovery");

    assert_eq!(
        read_active_text(
            &paths,
            &bootstrap.project.id,
            ResourceKind::WikiSpace,
            "wiki:default",
            "index.md",
        )
        .expect("active text")
        .as_deref(),
        Some("two")
    );
    assert!(!orphan.exists());
    assert!(!abandoned_stage.exists());
}

#[test]
fn startup_recovery_preserves_asset_revisions() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project dir");
    let paths = crate::paths::resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("asset-recovery"),
    )
    .expect("paths");
    let bootstrap = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let panel = bootstrap.panels.first().expect("panel");
    let storage = Storage::open(&paths).expect("storage");
    let written = storage
        .write_asset_from_buffer(
            &bootstrap.project.id,
            &panel.panel.id,
            "cover.png",
            b"cover bytes",
            false,
        )
        .expect("asset");

    recover_filesystem(&paths).expect("recovery");

    assert_eq!(
        storage.read_asset(&written.asset_ref).expect("asset bytes"),
        b"cover bytes"
    );
}
