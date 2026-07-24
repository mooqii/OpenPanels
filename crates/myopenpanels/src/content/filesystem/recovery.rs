use super::*;

pub fn recover_filesystem(paths: &MyOpenPanelsPaths) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    let authoritative = {
        let mut statement = storage
            .connection()
            .prepare(
                r#"
                SELECT r.project_id, r.id, r.kind, d.document_kind,
                       r.active_content_revision_id, r.content_version,
                       r.content_manifest_hash, r.content_hash
                FROM resources r
                LEFT JOIN documents d ON d.resource_id = r.id
                WHERE r.deleted_at IS NULL AND r.active_content_revision_id IS NOT NULL
                  AND r.kind IN ('asset', 'document', 'wiki_space')
                ORDER BY r.project_id, r.id
                "#,
            )
            .map_err(to_cli_error)?;
        let collected = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .map_err(to_cli_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_cli_error)?;
        collected
    };
    let completed = {
        let mut statement = storage
            .connection()
            .prepare(
                r#"
                SELECT project_id, result_json, completed_at, id
                FROM tasks
                WHERE status = 'succeeded' AND result_json IS NOT NULL
                UNION ALL
                SELECT project_id, json_extract(payload_json, '$.result'), completed_at, id
                FROM direct_operations
                WHERE status = 'completed' AND json_type(payload_json, '$.result') = 'object'
                ORDER BY completed_at, id
                "#,
            )
            .map_err(to_cli_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(to_cli_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(to_cli_error)?;
        rows
    };
    drop(storage);

    let mut retained_revisions = BTreeSet::new();
    for (
        project_id,
        resource_key,
        resource_kind,
        document_kind,
        revision_id,
        content_version,
        manifest_hash,
        content_hash,
    ) in authoritative
    {
        let kind = match (resource_kind.as_str(), document_kind.as_deref()) {
            ("asset", _) => ResourceKind::Asset,
            ("document", Some("wiki_source")) => ResourceKind::WikiMarkdown,
            ("document", Some("my_document")) => ResourceKind::MyDocument,
            ("wiki_space", _) => ResourceKind::WikiSpace,
            _ => continue,
        };
        let resource = resource_dir(paths, &project_id, kind, &resource_key);
        let revision = revision_dir(&resource, &revision_id);
        let snapshot =
            resource_snapshot_at_revision(paths, &project_id, kind, &resource_key, &revision_id)?
                .ok_or_else(|| {
                CliError::with_code(
                    "content_integrity_failed",
                    format!(
                        "The active content revision {revision_id} for {resource_key} is missing."
                    ),
                )
            })?;
        let snapshot_content_hash = if kind == ResourceKind::Asset && snapshot.files.len() == 1 {
            &snapshot.files[0].object_hash
        } else {
            &snapshot.content_hash
        };
        if snapshot.content_version != content_version
            || snapshot.manifest_hash != manifest_hash
            || snapshot_content_hash != &content_hash
        {
            return Err(CliError::with_code(
                "content_integrity_failed",
                format!(
                    "The active content revision {revision_id} for {resource_key} does not match its database authority."
                ),
            ));
        }
        write_json_atomic(
            &resource.join("active.json"),
            &ActivePointer {
                revision_id,
                content_version,
                manifest_hash,
                content_hash,
                archived: false,
            },
        )?;
        let pending = resource.join("pending.json");
        if pending.is_file() {
            fs::remove_file(pending).map_err(to_cli_error)?;
        }
        retained_revisions.insert(revision);
    }
    for (project_id, result_json) in completed {
        let result: Value = serde_json::from_str(&result_json).map_err(to_cli_error)?;
        for commit in result
            .get("contentCommits")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(kind) = commit
                .get("resourceKind")
                .and_then(Value::as_str)
                .map(ResourceKind::parse)
                .transpose()?
            else {
                continue;
            };
            if kind != ResourceKind::WritingSkill {
                continue;
            }
            let Some(resource_key) = commit.get("resourceKey").and_then(Value::as_str) else {
                continue;
            };
            let Some(revision_id) = commit.get("revisionId").and_then(Value::as_str) else {
                continue;
            };
            let resource = resource_dir(paths, &project_id, kind, resource_key);
            let revision_dir = revision_dir(&resource, revision_id);
            let manifest_path = revision_dir.join("manifest.json");
            if !manifest_path.is_file() {
                continue;
            }
            let manifest_bytes = fs::read(&manifest_path).map_err(to_cli_error)?;
            let manifest: RevisionManifest =
                serde_json::from_slice(&manifest_bytes).map_err(to_cli_error)?;
            let pointer = ActivePointer {
                revision_id: manifest.revision_id,
                content_version: manifest.content_version,
                manifest_hash: hash_bytes(&manifest_bytes),
                content_hash: revision_content_hash(&manifest.files)?,
                archived: false,
            };
            let current = read_active_pointer(paths, &project_id, kind, resource_key)?;
            if current
                .as_ref()
                .is_none_or(|value| value.content_version < pointer.content_version)
            {
                write_json_atomic(&resource.join("active.json"), &pointer)?;
            }
            retained_revisions.insert(revision_dir);
        }
    }

    let projects_root = paths.storage_dir.join("projects");
    for project_dir in read_dirs(&projects_root)? {
        let content_dir = project_dir.join("content");
        let _ = fs::remove_dir_all(content_dir.join(".staging"));
        for kind_dir in read_dirs(&content_dir)? {
            let kind_name = kind_dir
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if ResourceKind::parse(kind_name).is_err() {
                continue;
            }
            for resource in read_dirs(&kind_dir)? {
                let pending = resource.join("pending.json");
                if pending.is_file() {
                    fs::remove_file(pending).map_err(to_cli_error)?;
                }
                // Never prune a resource unless its complete active chain is readable.
                if !retain_active_revision_chain(&resource, &mut retained_revisions)? {
                    continue;
                }
                for revision in read_dirs(&resource)? {
                    let name = revision
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("");
                    if name.starts_with('.') || !retained_revisions.contains(&revision) {
                        fs::remove_dir_all(revision).map_err(to_cli_error)?;
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn retain_active_revision_chain(
    resource: &Path,
    retained: &mut BTreeSet<PathBuf>,
) -> Result<bool, CliError> {
    let active_path = resource.join("active.json");
    if !active_path.is_file() {
        return Ok(true);
    }
    let active: ActivePointer = read_json(&active_path)?;
    let mut revision_id = Some(active.revision_id);
    let mut chain = BTreeSet::new();
    while let Some(current) = revision_id {
        let revision = revision_dir(resource, &current);
        if !chain.insert(revision.clone()) {
            return Ok(false);
        }
        let manifest_path = revision.join("manifest.json");
        if !manifest_path.is_file() {
            return Ok(false);
        }
        revision_id = read_json::<RevisionManifest>(&manifest_path)?.parent_revision_id;
    }
    retained.extend(chain);
    Ok(true)
}
