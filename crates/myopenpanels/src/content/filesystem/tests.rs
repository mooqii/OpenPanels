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
    assert!(resource.join("pending.json").is_file());
    assert!(!resource.join("active.json").exists());
    crate::content::write_panel_state_with_pending_content(
        &paths,
        &bootstrap.project.id,
        &wiki_panel.panel.id,
        &json!({
            "rawDocuments": [],
            "myDocuments": [{
                "id": "my-document:portable",
                "title": "Portable",
                "originalFileName": "content.md",
                "format": "markdown",
                "mimeType": "text/markdown",
                "contentRef": "content.md",
                "contentVersion": 1,
            }],
            "wikiSpaces": [],
        }),
    )
    .expect("state and content commit");
    assert!(!resource.join("pending.json").exists());
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
        Some(crate::paths::stable_path_key("my-document:portable").as_str())
    );
    assert!(revision
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.starts_with("revision_")));
    let manifest: RevisionManifest =
        read_json(&revision.join("manifest.json")).expect("revision manifest");
    assert_eq!(manifest.format_version, CONTENT_FORMAT_VERSION);
    assert_eq!(manifest.files.len(), 1);
    assert!(revision_object_path(&revision, &manifest.files[0])
        .expect("content object path")
        .is_file());

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
fn opaque_objects_preserve_unicode_paths_that_share_a_sanitized_path() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project dir");
    let paths = crate::paths::resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("content-paths"),
    )
    .expect("paths");
    let bootstrap = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let panel = bootstrap.panels.first().expect("panel");
    let first_path = "资料/甲.md";
    let second_path = "文档/乙.md";
    assert_eq!(
        logical_path_buf(first_path).expect("first legacy path"),
        logical_path_buf(second_path).expect("second legacy path")
    );

    commit_immediate_text(
        &paths,
        &bootstrap.project.id,
        Some(&panel.panel.id),
        ResourceKind::MyDocument,
        "my-document:unicode",
        first_path,
        b"first",
        "text/markdown",
        true,
    )
    .expect("first file");
    commit_immediate_text(
        &paths,
        &bootstrap.project.id,
        Some(&panel.panel.id),
        ResourceKind::MyDocument,
        "my-document:unicode",
        second_path,
        b"second",
        "text/markdown",
        false,
    )
    .expect("second file");
    crate::content::write_panel_state_with_pending_content(
        &paths,
        &bootstrap.project.id,
        &panel.panel.id,
        &json!({
            "rawDocuments": [],
            "myDocuments": [{
                "id": "my-document:unicode",
                "title": "Unicode",
                "originalFileName": "unicode.md",
                "format": "markdown",
                "mimeType": "text/markdown",
                "contentRef": second_path,
                "contentVersion": 2,
            }],
            "wikiSpaces": [],
        }),
    )
    .expect("state and content commit");

    let snapshot = active_resource_snapshot(
        &paths,
        &bootstrap.project.id,
        ResourceKind::MyDocument,
        "my-document:unicode",
    )
    .expect("snapshot")
    .expect("active revision");
    let logical_paths = snapshot
        .files
        .iter()
        .map(|file| file.logical_path.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(logical_paths, BTreeSet::from([first_path, second_path]));
    assert_eq!(
        read_active_text(
            &paths,
            &bootstrap.project.id,
            ResourceKind::MyDocument,
            "my-document:unicode",
            first_path,
        )
        .expect("first text")
        .as_deref(),
        Some("first")
    );
    assert_eq!(
        read_active_text(
            &paths,
            &bootstrap.project.id,
            ResourceKind::MyDocument,
            "my-document:unicode",
            second_path,
        )
        .expect("second text")
        .as_deref(),
        Some("second")
    );
}

#[test]
fn mismatched_pending_content_rolls_back_new_resource_state() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project dir");
    let paths = crate::paths::resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("pending-rollback"),
    )
    .expect("paths");
    let bootstrap = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let panel = bootstrap
        .panels
        .iter()
        .find(|panel| panel.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");
    let resource_id = "document:kind-mismatch";
    commit_immediate_text(
        &paths,
        &bootstrap.project.id,
        Some(&panel.panel.id),
        ResourceKind::MyDocument,
        resource_id,
        "content.md",
        b"pending",
        "text/markdown",
        true,
    )
    .expect("pending content");

    let error = crate::content::write_panel_state_with_pending_content(
        &paths,
        &bootstrap.project.id,
        &panel.panel.id,
        &json!({
            "rawDocuments": [{
                "id": resource_id,
                "title": "Wrong kind",
                "originalFileName": "content.md",
                "mimeType": "text/markdown",
                "source": "user",
                "markdownRef": "content.md",
                "markdownVersion": 1,
            }],
            "myDocuments": [],
            "wikiSpaces": [],
        }),
    )
    .expect_err("kind mismatch");
    assert_eq!(error.code(), Some("content_resource_mismatch"));

    let resource = resource_dir(
        &paths,
        &bootstrap.project.id,
        ResourceKind::MyDocument,
        resource_id,
    );
    assert!(resource.join("pending.json").is_file());
    assert!(!resource.join("active.json").exists());
    let storage = Storage::open(&paths).expect("storage");
    assert_eq!(
        storage
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM resources WHERE project_id = ? AND id = ?",
                params![bootstrap.project.id, resource_id],
                |row| row.get::<_, i64>(0),
            )
            .expect("resource count"),
        0
    );
    drop(storage);

    recover_filesystem(&paths).expect("recovery");
    assert!(!resource.join("pending.json").exists());
    assert!(read_dirs(&resource).expect("orphan revisions").is_empty());
}

#[test]
fn task_validation_failure_rolls_back_panel_state_and_pending_content_together() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project dir");
    let paths = crate::paths::resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("task-state-rollback"),
    )
    .expect("paths");
    let bootstrap = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let panel = bootstrap
        .panels
        .iter()
        .find(|panel| panel.panel.kind == PanelKind::Wiki)
        .expect("wiki panel");
    let storage = Storage::open(&paths).expect("storage");
    let (mut next_state, revision) = storage
        .read_panel_state_snapshot(&bootstrap.project.id, &panel.panel.id)
        .expect("state")
        .expect("wiki state");
    let original_state = next_state.clone();
    drop(storage);

    let resource_id = "my-document:task-rollback";
    commit_immediate_text(
        &paths,
        &bootstrap.project.id,
        Some(&panel.panel.id),
        ResourceKind::MyDocument,
        resource_id,
        "content.md",
        b"pending",
        "text/markdown",
        true,
    )
    .expect("pending content");
    next_state["myDocuments"]
        .as_array_mut()
        .expect("my documents")
        .push(json!({
            "id": resource_id,
            "title": "Rollback",
            "originalFileName": "content.md",
            "format": "markdown",
            "mimeType": "text/markdown",
            "contentRef": "content.md",
            "contentVersion": 1,
        }));
    let error = crate::content::write_panel_state_and_tasks_with_pending_content(
        &paths,
        &bootstrap.project.id,
        &panel.panel.id,
        Some(revision),
        "wiki",
        &[json!({
            "id": "task:invalid-route",
            "type": "not_a_registered_wiki_task",
            "status": "queued",
        })],
        &next_state,
    )
    .expect_err("invalid task route");
    assert_eq!(error.code(), Some("task_route_not_found"));

    let resource = resource_dir(
        &paths,
        &bootstrap.project.id,
        ResourceKind::MyDocument,
        resource_id,
    );
    assert!(resource.join("pending.json").is_file());
    assert!(!resource.join("active.json").exists());
    let storage = Storage::open(&paths).expect("storage");
    assert_eq!(
        storage
            .read_panel_state_snapshot(&bootstrap.project.id, &panel.panel.id)
            .expect("state")
            .expect("wiki state")
            .0,
        original_state
    );
    assert_eq!(
        storage
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM resources WHERE project_id = ? AND id = ?",
                params![bootstrap.project.id, resource_id],
                |row| row.get::<_, i64>(0),
            )
            .expect("resource count"),
        0
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
            content_hash: "unresolvable".to_owned(),
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
    Storage::open(&paths)
        .expect("storage")
        .write_panel_state(
            &bootstrap.project.id,
            &wiki_panel.panel.id,
            &json!({
                "rawDocuments": [],
                "myDocuments": [],
                "wikiSpaces": [{
                    "id": "wiki:default",
                    "title": "Wiki",
                    "rootRef": "wikis/wiki:default",
                    "pageIndex": [],
                    "createdAt": now_iso(),
                    "updatedAt": now_iso()
                }],
                "activeWikiSpaceId": "wiki:default"
            }),
        )
        .expect("wiki state");

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
    assert_eq!(
        Storage::open(&paths)
            .expect("storage")
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM resources WHERE id = 'wiki:default'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("resource after initial commit"),
        1
    );
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

    let mut storage = Storage::open(&paths).expect("storage");
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
            "contentHash": pointer.content_hash,
        }]
    });
    let tx = storage
        .connection_mut()
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .expect("transaction");
    Storage::write_content_commit_in_transaction(
        &tx,
        &bootstrap.project.id,
        &ContentCommit {
            resource_kind: ResourceKind::WikiSpace.as_str().to_owned(),
            resource_key: "wiki:default".to_owned(),
            revision_id: pointer.revision_id.clone(),
            content_version: pointer.content_version,
            manifest_hash: pointer.manifest_hash.clone(),
            content_hash: pointer.content_hash.clone(),
        },
    )
    .expect("content commit");
    tx.execute(
        "UPDATE tasks SET status = 'succeeded', result_json = ?, completed_at = ? WHERE id = ?",
        params![result.to_string(), now_iso(), task["id"].as_str().unwrap()],
    )
    .expect("committed task result");
    tx.commit().expect("commit");
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

#[test]
fn startup_recovery_rejects_corrupt_authoritative_content() {
    let temp = tempfile::tempdir().expect("temp");
    let project_dir = temp.path().join("project");
    let storage_dir = temp.path().join("storage");
    fs::create_dir_all(&project_dir).expect("project dir");
    let paths = crate::paths::resolve_myopenpanels_paths(
        Some(project_dir.to_str().unwrap()),
        Some(storage_dir.to_str().unwrap()),
        Some("corrupt-content"),
    )
    .expect("paths");
    let bootstrap = ensure_project_bootstrap(&paths, BootstrapRequest::new()).expect("bootstrap");
    let panel = bootstrap.panels.first().expect("panel");
    let written = Storage::open(&paths)
        .expect("storage")
        .write_asset_from_buffer(
            &bootstrap.project.id,
            &panel.panel.id,
            "cover.png",
            b"cover bytes",
            false,
        )
        .expect("asset");
    let snapshot = active_resource_snapshot(
        &paths,
        &bootstrap.project.id,
        ResourceKind::Asset,
        &written.resource_id,
    )
    .expect("snapshot")
    .expect("active revision");
    let resource = resource_dir(
        &paths,
        &bootstrap.project.id,
        ResourceKind::Asset,
        &written.resource_id,
    );
    let revision = revision_dir(&resource, &snapshot.revision_id);
    let manifest: RevisionManifest = read_json(&revision.join("manifest.json")).expect("manifest");
    fs::write(
        revision_object_path(&revision, &manifest.files[0]).expect("object path"),
        b"corrupt",
    )
    .expect("corrupt object");

    let error = recover_filesystem(&paths).expect_err("corrupt content");
    assert_eq!(error.code(), Some("content_integrity_failed"));
}
