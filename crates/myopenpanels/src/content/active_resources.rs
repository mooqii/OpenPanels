pub fn read_active_text(
    paths: &MyOpenPanelsPaths,
    project_id: &str,
    kind: ResourceKind,
    resource_key: &str,
    logical_path: &str,
) -> Result<Option<String>, CliError> {
    let storage = Storage::open(paths)?;
    let Some((hash, _)) = active_file_entry(
        storage.connection(),
        project_id,
        kind,
        resource_key,
        logical_path,
    )?
    else {
        return Ok(None);
    };
    String::from_utf8(read_object(paths, &hash)?)
        .map(Some)
        .map_err(|_| CliError::with_code("invalid_content", "Stored content is not UTF-8."))
}

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
    if content.len() > MAX_TEXT_FILE_BYTES || std::str::from_utf8(content).is_err() {
        return Err(CliError::with_code(
            "invalid_output",
            "Content must be bounded UTF-8 text.",
        ));
    }
    let object = write_object(paths, content)?;
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let current = tx.query_row(
        "SELECT id, active_revision_id, content_version FROM content_resources WHERE project_id = ? AND resource_kind = ? AND resource_key = ?",
        params![project_id, kind.as_str(), resource_key],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, i64>(2)?)),
    ).optional().map_err(to_cli_error)?;
    let resource_id = current
        .as_ref()
        .map(|value| value.0.clone())
        .unwrap_or_else(|| crate::ids::random_id("content-resource"));
    let parent = current.as_ref().and_then(|value| value.1.clone());
    let version = current.as_ref().map(|value| value.2 + 1).unwrap_or(1);
    let mut manifest = if replace_all {
        BTreeMap::new()
    } else {
        base_manifest(&tx, parent.as_deref())?
    };
    manifest.insert(
        logical_path.to_owned(),
        FileEntry {
            object_hash: object.object_hash.clone(),
            size_bytes: object.size_bytes,
            mime_type: mime_type.to_owned(),
        },
    );
    if kind == ResourceKind::WikiSpace && manifest.len() > MAX_WIKI_FILES {
        return Err(CliError::with_code(
            "content_too_large",
            "Wiki revision contains too many files.",
        ));
    }
    let now = now_iso();
    tx.execute("INSERT OR IGNORE INTO content_resources (id, project_id, panel_id, resource_kind, resource_key, content_version, created_at, updated_at) VALUES (?, ?, ?, ?, ?, 0, ?, ?)", params![resource_id, project_id, panel_id, kind.as_str(), resource_key, now, now]).map_err(to_cli_error)?;
    if let Some(parent) = parent.as_deref() {
        tx.execute("UPDATE content_revisions SET status = 'prunable', prunable_at = ? WHERE id = ? AND status = 'active'", params![now, parent]).map_err(to_cli_error)?;
    }
    let revision_id = crate::ids::random_id("content-revision");
    let manifest_json = manifest_value(&manifest);
    let manifest_text = serde_json::to_string(&manifest_json).map_err(to_cli_error)?;
    let manifest_hash = format!("{:x}", Sha256::digest(manifest_text.as_bytes()));
    tx.execute("INSERT INTO content_revisions (id, content_resource_id, parent_revision_id, revision_number, manifest_json, manifest_hash, status, created_at, activated_at) VALUES (?, ?, ?, ?, ?, ?, 'active', ?, ?)", params![revision_id, resource_id, parent, version, manifest_text, manifest_hash, now, now]).map_err(to_cli_error)?;
    for (path, entry) in &manifest {
        tx.execute("INSERT INTO content_revision_files (revision_id, logical_path, object_hash, size_bytes, mime_type) VALUES (?, ?, ?, ?, ?)", params![revision_id, path, entry.object_hash, entry.size_bytes, entry.mime_type]).map_err(to_cli_error)?;
    }
    tx.execute("UPDATE content_resources SET active_revision_id = ?, content_version = ?, panel_id = COALESCE(panel_id, ?), updated_at = ? WHERE id = ?", params![revision_id, version, panel_id, now, resource_id]).map_err(to_cli_error)?;
    tx.commit().map_err(to_cli_error)?;
    Ok(
        json!({ "revisionId": revision_id, "contentVersion": version, "manifestHash": manifest_hash }),
    )
}

pub fn active_writing_skill_sources(
    paths: &MyOpenPanelsPaths,
) -> Result<Vec<(String, String, Value, String)>, CliError> {
    let storage = Storage::open(paths)?;
    let mut statement = storage
        .connection()
        .prepare(
            r#"
        SELECT r.resource_key, r.active_revision_id,
               skill.object_hash, manifest.object_hash
        FROM content_resources r
        JOIN content_revision_files skill
          ON skill.revision_id = r.active_revision_id AND skill.logical_path = 'SKILL.md'
        JOIN content_revision_files manifest
          ON manifest.revision_id = r.active_revision_id AND manifest.logical_path = 'manifest.json'
        WHERE r.resource_kind = 'writing_skill' AND r.archived_at IS NULL
        ORDER BY r.resource_key
        "#,
        )
        .map_err(to_cli_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(to_cli_error)?;
    rows.map(|row| {
        let (id, revision_id, skill_hash, manifest_hash) = row.map_err(to_cli_error)?;
        let source = String::from_utf8(read_object(paths, &skill_hash)?).map_err(|_| {
            CliError::with_code("invalid_custom_skill", "Writing Skill is not UTF-8.")
        })?;
        let manifest: Value =
            serde_json::from_slice(&read_object(paths, &manifest_hash)?).map_err(to_cli_error)?;
        let materialized = paths
            .storage_dir
            .join("content/materialized/writing-skills")
            .join(sanitize_path_part(&id))
            .join(sanitize_path_part(&revision_id));
        fs::create_dir_all(&materialized).map_err(to_cli_error)?;
        let skill_path = materialized.join("SKILL.md");
        let manifest_path = materialized.join("manifest.json");
        if !skill_path.is_file() {
            fs::write(&skill_path, source.as_bytes()).map_err(to_cli_error)?;
        }
        if !manifest_path.is_file() {
            fs::write(
                &manifest_path,
                serde_json::to_vec_pretty(&manifest).map_err(to_cli_error)?,
            )
            .map_err(to_cli_error)?;
        }
        Ok((id, source, manifest, materialized.display().to_string()))
    })
    .collect()
}

pub fn active_writing_skill_dir(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
) -> Option<std::path::PathBuf> {
    active_writing_skill_sources(paths)
        .ok()?
        .into_iter()
        .find(|value| value.0 == skill_id)
        .map(|value| std::path::PathBuf::from(value.3))
}

pub fn writing_skill_project_id(
    paths: &MyOpenPanelsPaths,
    skill_id: &str,
) -> Result<Option<String>, CliError> {
    let storage = Storage::open(paths)?;
    storage.connection().query_row(
        "SELECT project_id FROM content_resources WHERE resource_kind = 'writing_skill' AND resource_key = ? AND archived_at IS NULL",
        [skill_id],
        |row| row.get::<_, String>(0),
    ).optional().map_err(to_cli_error)
}

pub fn archive_resource(
    paths: &MyOpenPanelsPaths,
    project_id: Option<&str>,
    kind: ResourceKind,
    resource_key: &str,
) -> Result<(), CliError> {
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = now_iso();
    tx.execute(
        "UPDATE content_revisions SET status = 'prunable', prunable_at = ? WHERE status = 'active' AND id IN (SELECT active_revision_id FROM content_resources WHERE resource_kind = ? AND resource_key = ? AND (? IS NULL OR project_id = ?))",
        params![now, kind.as_str(), resource_key, project_id, project_id],
    ).map_err(to_cli_error)?;
    tx.execute(
        "UPDATE content_resources SET active_revision_id = NULL, archived_at = ?, updated_at = ? WHERE resource_kind = ? AND resource_key = ? AND (? IS NULL OR project_id = ?)",
        params![now, now, kind.as_str(), resource_key, project_id, project_id],
    ).map_err(to_cli_error)?;
    tx.commit().map_err(to_cli_error)
}

pub fn start_gc_loop(paths: MyOpenPanelsPaths) {
    if cfg!(test) {
        return;
    }
    std::thread::spawn(move || loop {
        let _ = gc_content(&paths);
        std::thread::sleep(std::time::Duration::from_secs(60 * 60));
    });
}

pub fn gc_content(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::hours(24))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let revisions = {
        let mut statement = tx.prepare(
            "SELECT id FROM content_revisions r WHERE status = 'prunable' AND prunable_at <= ? AND NOT EXISTS (SELECT 1 FROM content_pins p WHERE p.revision_id = r.id)"
        ).map_err(to_cli_error)?;
        let rows = statement
            .query_map([&cutoff], |row| row.get::<_, String>(0))
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    for revision_id in &revisions {
        tx.execute(
            "DELETE FROM content_revision_files WHERE revision_id = ?",
            [revision_id],
        )
        .map_err(to_cli_error)?;
        tx.execute(
            "UPDATE content_revisions SET status = 'pruned', pruned_at = ? WHERE id = ?",
            params![now_iso(), revision_id],
        )
        .map_err(to_cli_error)?;
    }
    let sessions = tx
        .execute(
            "DELETE FROM task_staging_sessions WHERE status = 'abandoned' AND updated_at <= ?",
            [&cutoff],
        )
        .map_err(to_cli_error)?;
    let objects = {
        let mut statement = tx.prepare(
            "SELECT hash, storage_ref FROM content_objects o WHERE NOT EXISTS (SELECT 1 FROM content_revision_files f WHERE f.object_hash = o.hash) AND NOT EXISTS (SELECT 1 FROM task_staged_files sf WHERE sf.object_hash = o.hash)"
        ).map_err(to_cli_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(to_cli_error)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(to_cli_error)?
    };
    for (hash, _) in &objects {
        tx.execute("DELETE FROM content_objects WHERE hash = ?", [hash])
            .map_err(to_cli_error)?;
    }
    tx.commit().map_err(to_cli_error)?;
    for (_, storage_ref) in &objects {
        let _ = fs::remove_file(paths.storage_dir.join(storage_ref));
    }
    Ok(
        json!({ "prunedRevisions": revisions.len(), "removedStagingSessions": sessions, "removedObjects": objects.len() }),
    )
}
