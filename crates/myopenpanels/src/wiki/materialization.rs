fn project_wiki_action(
    paths: &MyOpenPanelsPaths,
    intent: &str,
    args: Vec<String>,
) -> Value {
    let mut contextual_args = vec![
        "--project-dir".to_owned(),
        paths.project_dir.display().to_string(),
    ];
    contextual_args.extend(args);
    crate::cli::registry::command_action(
        crate::cli::registry::CommandId::registered(intent),
        contextual_args,
    )
    .unwrap_or_else(|| panic!("missing Command Registry action for {intent}"))
}

fn raw_read_action(paths: &MyOpenPanelsPaths, document_id: &str) -> Value {
    project_wiki_action(
        paths,
        "wiki.raw.read",
        vec![
            "--raw-document-id".to_owned(),
            document_id.to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ],
    )
}

fn generated_read_action(paths: &MyOpenPanelsPaths, document_id: &str) -> Value {
    project_wiki_action(
        paths,
        "wiki.document.read",
        vec![
            "--document-id".to_owned(),
            document_id.to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ],
    )
}

fn wiki_materialize_action(paths: &MyOpenPanelsPaths, wiki_space_id: &str) -> Value {
    project_wiki_action(
        paths,
        "wiki.space.materialize",
        vec![
            "--space-id".to_owned(),
            wiki_space_id.to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ],
    )
}

pub(crate) fn reject_live_content_access_for_task() -> Result<(), CliError> {
    if crate::content::task_execution_detected() {
        return Err(CliError::with_code(
            "task_live_content_forbidden",
            "Claimed Tasks must use their captured workspace or Task Broker reads, not live local content mirrors.",
        ));
    }
    Ok(())
}

fn raw_original_access(
    panel_dir: &Path,
    document: &Value,
) -> (Option<PathBuf>, Value) {
    let path = document
        .get("originalRef")
        .and_then(Value::as_str)
        .and_then(|reference| wiki_panel_path(panel_dir, reference).ok());
    let ready_path = path.filter(|path| path.is_file());
    let access = json!({
        "status": if ready_path.is_some() { "ready" } else { "unavailable" },
        "localPath": ready_path,
        "mimeType": document.get("mimeType").cloned().unwrap_or(Value::Null),
        "sha256": document.get("sha256").cloned().unwrap_or(Value::Null),
    });
    (ready_path, access)
}

fn raw_document_listing(paths: &MyOpenPanelsPaths, panel_dir: &Path, document: &Value) -> Value {
    let mut item = document.clone();
    let document_id = document.get("id").and_then(Value::as_str).unwrap_or("");
    let (original_path, original_access) = raw_original_access(panel_dir, document);
    let markdown_version = document
        .get("markdownVersion")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let markdown_status = match (
        markdown_version > 0,
        document.get("markdownRef").and_then(Value::as_str),
    ) {
        (true, Some(_)) => "on_demand",
        (true, None) => "unavailable",
        (false, _) => "processing",
    };
    item["originalFilePath"] = original_path.map_or(Value::Null, |path| json!(path));
    item["markdownFilePath"] = Value::Null;
    item["originalAccess"] = original_access;
    item["markdownAccess"] = json!({
        "status": markdown_status,
        "localPath": null,
        "version": document.get("markdownVersion").cloned().unwrap_or_else(|| json!(0)),
        "readAction": raw_read_action(paths, document_id),
    });
    item
}

fn generated_document_listing(paths: &MyOpenPanelsPaths, document: &Value) -> Value {
    let mut item = document.clone();
    let document_id = document.get("id").and_then(Value::as_str).unwrap_or("");
    item["contentFilePath"] = Value::Null;
    item["contentAccess"] = json!({
        "status": "on_demand",
        "localPath": null,
        "version": document.get("contentVersion").cloned().unwrap_or_else(|| json!(0)),
        "readAction": generated_read_action(paths, document_id),
    });
    item
}

pub fn list_raw_documents(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    reject_live_content_access_for_task()?;
    let wiki = get_wiki_bootstrap(paths)?;
    let panel_dir = Storage::open(paths)?.panel_dir(&wiki.project.id, &wiki.panel.id);
    let documents = wiki
        .state
        .get("rawDocuments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|document| raw_document_listing(paths, &panel_dir, document))
        .collect::<Vec<_>>();
    Ok(json!({ "documents": documents }))
}

fn list_generated_documents_with_access(
    paths: &MyOpenPanelsPaths,
) -> Result<Value, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    let documents = wiki
        .state
        .get("generatedDocuments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|document| generated_document_listing(paths, document))
        .collect::<Vec<_>>();
    Ok(json!({ "documents": documents }))
}

fn materialize_raw_markdown(
    paths: &MyOpenPanelsPaths,
    wiki: &WikiBootstrapValue,
    document: &Value,
) -> (Option<PathBuf>, Value) {
    let document_id = document.get("id").and_then(Value::as_str).unwrap_or("");
    let read_action = raw_read_action(paths, document_id);
    let markdown_version = document
        .get("markdownVersion")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if markdown_version <= 0 {
        return (
            None,
            json!({
                "status": "processing",
                "localPath": null,
                "version": document.get("markdownVersion").cloned().unwrap_or_else(|| json!(0)),
                "readAction": read_action,
            }),
        );
    }
    if document.get("markdownRef").and_then(Value::as_str).is_none() {
        return (
            None,
            json!({
                "status": "unavailable",
                "localPath": null,
                "version": markdown_version,
                "errorCode": "content_unavailable",
                "message": "Raw document Markdown reference is missing.",
                "readAction": read_action,
            }),
        );
    }
    let panel_dir = Storage::open(paths)
        .map(|storage| storage.panel_dir(&wiki.project.id, &wiki.panel.id));
    let result = panel_dir.and_then(|panel_dir| {
        let reference = document
            .get("markdownRef")
            .and_then(Value::as_str)
            .ok_or_else(|| CliError::new("Raw document Markdown reference is missing."))?;
        let destination = wiki_panel_path(&panel_dir, reference)?;
        let materialized = crate::content::materialize_active_file(
            paths,
            &wiki.project.id,
            crate::content::ResourceKind::WikiMarkdown,
            document_id,
            "source.md",
            &destination,
        )?;
        if materialized.is_none() && !destination.is_file() {
            return Err(CliError::with_code(
                "content_unavailable",
                format!("Raw document Markdown is unavailable: {document_id}"),
            ));
        }
        Ok((destination, materialized))
    });
    match result {
        Ok((path, materialized)) => (
            Some(path.clone()),
            json!({
                "status": "ready",
                "localPath": path,
                "version": document.get("markdownVersion").cloned().unwrap_or_else(|| json!(0)),
                "revisionId": materialized.as_ref().and_then(|value| value.get("revisionId")).cloned().unwrap_or(Value::Null),
                "contentVersion": materialized.as_ref().and_then(|value| value.get("contentVersion")).cloned().unwrap_or(Value::Null),
                "readAction": read_action,
            }),
        ),
        Err(error) => (
            None,
            json!({
                "status": "unavailable",
                "localPath": null,
                "version": document.get("markdownVersion").cloned().unwrap_or_else(|| json!(0)),
                "errorCode": error.code(),
                "message": error.message(),
                "readAction": read_action,
            }),
        ),
    }
}

fn materialize_generated_content(
    paths: &MyOpenPanelsPaths,
    wiki: &WikiBootstrapValue,
    document: &Value,
) -> (Option<PathBuf>, Value) {
    let document_id = document.get("id").and_then(Value::as_str).unwrap_or("");
    let read_action = generated_read_action(paths, document_id);
    let result = (|| {
        let content_ref = document
            .get("contentRef")
            .and_then(Value::as_str)
            .ok_or_else(|| CliError::new("Generated document content reference is missing."))?;
        let logical_path = Path::new(content_ref)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("content.md");
        let storage = Storage::open(paths)?;
        let destination = wiki_panel_path(
            &storage.panel_dir(&wiki.project.id, &wiki.panel.id),
            content_ref,
        )?;
        let materialized = crate::content::materialize_active_file(
            paths,
            &wiki.project.id,
            crate::content::ResourceKind::GeneratedDocument,
            document_id,
            logical_path,
            &destination,
        )?;
        if materialized.is_none() && !destination.is_file() {
            return Err(CliError::with_code(
                "content_unavailable",
                format!("Generated document content is unavailable: {document_id}"),
            ));
        }
        Ok((destination, materialized))
    })();
    match result {
        Ok((path, materialized)) => (
            Some(path.clone()),
            json!({
                "status": "ready",
                "localPath": path,
                "version": document.get("contentVersion").cloned().unwrap_or_else(|| json!(0)),
                "revisionId": materialized.as_ref().and_then(|value| value.get("revisionId")).cloned().unwrap_or(Value::Null),
                "contentVersion": materialized.as_ref().and_then(|value| value.get("contentVersion")).cloned().unwrap_or(Value::Null),
                "readAction": read_action,
            }),
        ),
        Err(error) => (
            None,
            json!({
                "status": "unavailable",
                "localPath": null,
                "version": document.get("contentVersion").cloned().unwrap_or_else(|| json!(0)),
                "errorCode": error.code(),
                "message": error.message(),
                "readAction": read_action,
            }),
        ),
    }
}

fn selected_generated_document_context(
    paths: &MyOpenPanelsPaths,
    wiki: &WikiBootstrapValue,
    generated_document_ids: &[String],
) -> Vec<Value> {
    let generated_documents = wiki
        .state
        .get("generatedDocuments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten();
    generated_document_ids
        .iter()
        .filter_map(|id| {
            let document = generated_documents
                .clone()
                .find(|document| document.get("id").and_then(Value::as_str) == Some(id))?;
            let mut item = document.clone();
            let (content_path, content_access) = materialize_generated_content(paths, wiki, document);
            item["contentFilePath"] = content_path.map_or(Value::Null, |path| json!(path));
            item["contentAccess"] = content_access;
            Some(item)
        })
        .collect::<Vec<_>>()
}

pub(crate) fn agent_content_context(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    wiki_panel_id: &str,
    generated_document_ids: &[String],
    is_wiki_selected: bool,
) -> Result<Value, CliError> {
    let wiki = get_wiki_target(paths, project_id, wiki_panel_id)?;
    let selected_generated =
        selected_generated_document_context(paths, &wiki, generated_document_ids);
    let wiki_space = resolve_wiki_space(&wiki.state, None)?;
    let page_count = wiki_space
        .value
        .get("pageIndex")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let local_access = wiki_local_access(paths, &wiki, &wiki_space.id, is_wiki_selected);
    Ok(json!({
        "selectedGeneratedDocuments": selected_generated,
        "wiki": {
            "available": true,
            "selected": is_wiki_selected,
            "wikiSpaceId": wiki_space.id,
            "title": wiki_space.value.get("title").cloned().unwrap_or_else(|| json!("Wiki")),
            "pageCount": page_count,
            "querySkillId": crate::agent::PANELS_SKILL_ID,
            "localAccess": local_access,
        }
    }))
}

fn wiki_local_paths(panel_dir: &Path, wiki_space_id: &str) -> (PathBuf, PathBuf, PathBuf) {
    let wiki_dir = panel_dir
        .join("wikis")
        .join(sanitize_path_part(wiki_space_id));
    (
        wiki_dir.clone(),
        wiki_dir.join("pages"),
        wiki_dir.join("manifest.json"),
    )
}

fn materialized_tree_paths(root_path: &Path) -> Option<BTreeSet<String>> {
    fn collect(root: &Path, directory: &Path, paths: &mut BTreeSet<String>) -> Option<()> {
        for entry in fs::read_dir(directory).ok()?.flatten() {
            let metadata = fs::symlink_metadata(entry.path()).ok()?;
            if metadata.file_type().is_symlink() {
                return None;
            }
            if metadata.is_dir() {
                collect(root, &entry.path(), paths)?;
            } else if metadata.is_file() {
                paths.insert(
                    entry
                        .path()
                        .strip_prefix(root)
                        .ok()?
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
        Some(())
    }
    let mut paths = BTreeSet::new();
    collect(root_path, root_path, &mut paths)?;
    Some(paths)
}

fn current_wiki_manifest(
    wiki: &WikiBootstrapValue,
    wiki_space_id: &str,
    root_path: &Path,
    manifest_path: &Path,
    descriptor: Option<&Value>,
) -> Option<Value> {
    let manifest: Value = serde_json::from_slice(&fs::read(manifest_path).ok()?).ok()?;
    if manifest.get("schemaVersion").and_then(Value::as_u64) != Some(1)
        || manifest.get("projectId").and_then(Value::as_str) != Some(wiki.project.id.as_str())
        || manifest.get("panelId").and_then(Value::as_str) != Some(wiki.panel.id.as_str())
        || manifest.get("wikiSpaceId").and_then(Value::as_str) != Some(wiki_space_id)
    {
        return None;
    }
    let expected_hash = descriptor
        .and_then(|value| value.get("manifestHash"))
        .and_then(Value::as_str);
    if manifest.get("manifestHash").and_then(Value::as_str) != expected_hash {
        return None;
    }
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id)).ok()?;
    let expected_paths = space
        .value
        .get("pageIndex")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|page| page.get("path").and_then(Value::as_str))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let materialized_paths = manifest
        .get("pages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|page| page.get("path").and_then(Value::as_str))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    if expected_paths != materialized_paths
        || materialized_tree_paths(root_path).as_ref() != Some(&materialized_paths)
        || materialized_paths
            .iter()
            .any(|path| {
                Path::new(path)
                    .components()
                    .any(|component| !matches!(component, std::path::Component::Normal(_)))
                    || !root_path.join(path).is_file()
            })
    {
        return None;
    }
    Some(manifest)
}

fn wiki_local_access(
    paths: &MyOpenPanelsPaths,
    wiki: &WikiBootstrapValue,
    wiki_space_id: &str,
    selected: bool,
) -> Value {
    if selected {
        return materialize_wiki_space_for(paths, wiki, wiki_space_id)
            .map(|payload| payload["localAccess"].clone())
            .unwrap_or_else(|error| {
                json!({
                    "status": "unavailable",
                    "rootPath": null,
                    "manifestFilePath": null,
                    "errorCode": error.code(),
                    "message": error.message(),
                    "materializeAction": wiki_materialize_action(paths, wiki_space_id),
                })
            });
    }
    let storage = match Storage::open(paths) {
        Ok(storage) => storage,
        Err(error) => {
            return json!({
                "status": "unavailable",
                "rootPath": null,
                "manifestFilePath": null,
                "errorCode": error.code(),
                "message": error.message(),
                "materializeAction": wiki_materialize_action(paths, wiki_space_id),
            });
        }
    };
    let (root_path, pages_path, manifest_path) = wiki_local_paths(
        &storage.panel_dir(&wiki.project.id, &wiki.panel.id),
        wiki_space_id,
    );
    let descriptor = crate::content::active_resource_descriptor(
        paths,
        &wiki.project.id,
        crate::content::ResourceKind::WikiSpace,
        wiki_space_id,
    )
    .ok()
    .flatten();
    if let Some(manifest) = current_wiki_manifest(
        wiki,
        wiki_space_id,
        &pages_path,
        &manifest_path,
        descriptor.as_ref(),
    ) {
        return json!({
            "status": "ready",
            "rootPath": root_path,
            "manifestFilePath": manifest_path,
            "revisionId": manifest.get("revisionId").cloned().unwrap_or(Value::Null),
            "contentVersion": manifest.get("contentVersion").cloned().unwrap_or(Value::Null),
            "materializeAction": wiki_materialize_action(paths, wiki_space_id),
        });
    }
    json!({
        "status": "on_demand",
        "rootPath": root_path,
        "manifestFilePath": manifest_path,
        "revisionId": descriptor.as_ref().and_then(|value| value.get("revisionId")).cloned().unwrap_or(Value::Null),
        "contentVersion": descriptor.as_ref().and_then(|value| value.get("contentVersion")).cloned().unwrap_or(Value::Null),
        "materializeAction": wiki_materialize_action(paths, wiki_space_id),
    })
}

pub fn materialize_wiki_space(
    paths: &MyOpenPanelsPaths,
    wiki_space_id: &str,
) -> Result<Value, CliError> {
    reject_live_content_access_for_task()?;
    let wiki = get_wiki_bootstrap(paths)?;
    materialize_wiki_space_for(paths, &wiki, wiki_space_id)
}

fn materialize_wiki_space_for(
    paths: &MyOpenPanelsPaths,
    wiki: &WikiBootstrapValue,
    wiki_space_id: &str,
) -> Result<Value, CliError> {
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    let storage = Storage::open(paths)?;
    let panel_dir = storage.panel_dir(&wiki.project.id, &wiki.panel.id);
    let (root_path, pages_path, manifest_path) = wiki_local_paths(&panel_dir, &space.id);
    let descriptor = crate::content::active_resource_descriptor(
        paths,
        &wiki.project.id,
        crate::content::ResourceKind::WikiSpace,
        &space.id,
    )?;
    if let Some(manifest) = current_wiki_manifest(
        wiki,
        &space.id,
        &pages_path,
        &manifest_path,
        descriptor.as_ref(),
    ) {
        return Ok(json!({
            "wikiSpace": space.value,
            "pages": manifest.get("pages").cloned().unwrap_or_else(|| json!([])),
            "localAccess": {
                "status": "ready",
                "rootPath": root_path,
                "manifestFilePath": manifest_path,
                "revisionId": manifest.get("revisionId").cloned().unwrap_or(Value::Null),
                "contentVersion": manifest.get("contentVersion").cloned().unwrap_or(Value::Null),
                "materializeAction": wiki_materialize_action(paths, wiki_space_id),
            }
        }));
    }
    let snapshot = crate::content::active_resource_snapshot(
        paths,
        &wiki.project.id,
        crate::content::ResourceKind::WikiSpace,
        &space.id,
    )?;
    let staging_path = pages_path.with_file_name(format!(
        ".pages-{}",
        sanitize_path_part(&crate::ids::random_id("materialization"))
    ));
    fs::create_dir_all(&staging_path).map_err(to_cli_error)?;
    let materialization = (|| {
        let pages = space
            .value
            .get("pageIndex")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut entries = Vec::with_capacity(pages.len());
        for page in pages {
            let page_path = page
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| CliError::new("Wiki page index contains no path."))?;
            let final_path = wiki_page_path(&panel_dir, &space.id, page_path)?;
            let relative_path = final_path.strip_prefix(&pages_path).map_err(to_cli_error)?;
            let staged_path = staging_path.join(relative_path);
            let active_file = snapshot
                .as_ref()
                .and_then(|snapshot| {
                    snapshot
                        .files
                        .iter()
                        .find(|file| file.logical_path == page_path)
                });
            let bytes = match (&snapshot, active_file) {
                (_, Some(file)) => file.bytes.clone(),
                (Some(_), None) => {
                    return Err(CliError::with_code(
                        "content_unavailable",
                        format!(
                            "Active Wiki revision does not contain the expected page: {page_path}"
                        ),
                    ));
                }
                (None, None) => fs::read(&final_path).map_err(|_| {
                    CliError::with_code(
                        "content_unavailable",
                        format!("Wiki page content is unavailable: {page_path}"),
                    )
                })?,
            };
            std::str::from_utf8(&bytes).map_err(|_| {
                CliError::with_code(
                    "invalid_content",
                    format!("Wiki page is not UTF-8: {page_path}"),
                )
            })?;
            crate::content::write_materialized_file(&staged_path, &bytes)?;
            let mut entry = page.clone();
            entry["localPath"] = json!(final_path);
            entry["objectHash"] = active_file
                .map(|file| json!(file.object_hash))
                .unwrap_or(Value::Null);
            entry["sizeBytes"] = json!(bytes.len());
            entry["mimeType"] = json!(active_file
                .map(|file| file.mime_type.clone())
                .unwrap_or_else(|| "text/markdown".to_owned()));
            entries.push(entry);
        }
        Ok(entries)
    })();
    let entries = match materialization {
        Ok(entries) => entries,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging_path);
            return Err(error);
        }
    };
    let backup_path = pages_path.with_file_name(format!(
        ".pages-backup-{}",
        sanitize_path_part(&crate::ids::random_id("materialization"))
    ));
    if pages_path.exists() {
        fs::rename(&pages_path, &backup_path).map_err(to_cli_error)?;
    }
    if let Err(error) = fs::rename(&staging_path, &pages_path) {
        if backup_path.exists() {
            let _ = fs::rename(&backup_path, &pages_path);
        }
        return Err(to_cli_error(error));
    }
    if backup_path.exists() {
        let _ = fs::remove_dir_all(&backup_path);
    }
    let manifest = json!({
        "schemaVersion": 1,
        "projectId": wiki.project.id,
        "panelId": wiki.panel.id,
        "wikiSpaceId": space.id,
        "revisionId": snapshot.as_ref().map(|value| value.revision_id.clone()),
        "contentVersion": snapshot.as_ref().map(|value| value.content_version),
        "manifestHash": snapshot.as_ref().map(|value| value.manifest_hash.clone()),
        "pages": entries,
        "materializedAt": now_iso(),
    });
    crate::content::write_materialized_file(
        &manifest_path,
        &serde_json::to_vec_pretty(&manifest).map_err(to_cli_error)?,
    )?;
    Ok(json!({
        "wikiSpace": space.value,
        "pages": manifest["pages"],
        "localAccess": {
            "status": "ready",
            "rootPath": root_path,
            "manifestFilePath": manifest_path,
            "revisionId": manifest["revisionId"],
            "contentVersion": manifest["contentVersion"],
            "materializeAction": wiki_materialize_action(paths, wiki_space_id),
        }
    }))
}
