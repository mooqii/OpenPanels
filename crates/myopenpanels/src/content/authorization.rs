fn authorize(paths: &MyOpenPanelsPaths, token: &str) -> Result<ExecutionContext, CliError> {
    let storage = Storage::open(paths)?;
    execution_context(storage.connection(), token)
}

fn authorize_in_transaction(
    tx: &Transaction<'_>,
    token: &str,
    expected: &ExecutionContext,
) -> Result<(), CliError> {
    let actual = execution_context(tx, token)?;
    if actual.attempt_id != expected.attempt_id || actual.generation != expected.generation {
        return Err(CliError::with_code(
            "execution_fenced",
            "Execution generation changed.",
        ));
    }
    Ok(())
}

fn execution_context(
    connection: &rusqlite::Connection,
    token: &str,
) -> Result<ExecutionContext, CliError> {
    let now = now_iso();
    connection
        .query_row(
            r#"
        SELECT t.id, a.id, a.staging_session_id, t.project_id, t.panel_id,
               t.type, t.capability, a.execution_generation, t.input_json, t.source_json
        FROM task_attempts a JOIN tasks t ON t.id = a.task_id
        JOIN task_staging_sessions ss ON ss.id = a.staging_session_id
        WHERE a.execution_token_hash = ? AND a.status = 'leased'
          AND a.execution_generation = t.execution_generation
          AND a.execution_token_expires_at > ? AND t.lease_expires_at > ?
          AND t.status IN ('running', 'claimed', 'converting', 'indexing')
          AND ss.status IN ('open', 'prepared')
        "#,
            params![hash_secret(token), now, now],
            |row| {
                let input: String = row.get(8)?;
                let source: String = row.get(9)?;
                Ok(ExecutionContext {
                    task_id: row.get(0)?,
                    attempt_id: row.get(1)?,
                    staging_session_id: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    project_id: row.get(3)?,
                    panel_id: row.get(4)?,
                    task_type: row.get(5)?,
                    capability: row.get(6)?,
                    generation: row.get(7)?,
                    input: serde_json::from_str(&input).unwrap_or_else(|_| json!({})),
                    source: serde_json::from_str(&source).unwrap_or_else(|_| json!({})),
                })
            },
        )
        .optional()
        .map_err(to_cli_error)?
        .ok_or_else(|| {
            CliError::with_code(
                "execution_fenced",
                "The execution token is invalid, expired, or fenced.",
            )
        })
}

fn validate_resource_scope(
    context: &ExecutionContext,
    kind: ResourceKind,
    key: &str,
    write: bool,
) -> Result<(), CliError> {
    let capability_allows_kind = match kind {
        ResourceKind::WikiMarkdown => {
            context.capability == "wiki.convertDocument"
                || (!write && context.capability == "wiki.ingestMarkdown")
        }
        ResourceKind::WikiSpace => {
            matches!(
                context.capability.as_str(),
                "wiki.ingestMarkdown" | "wiki.maintain"
            ) || (!write
                && context.capability == "writing.generateDocument"
                && context
                    .input
                    .pointer("/contextSnapshot/wikiSelection/selected")
                    .and_then(Value::as_bool)
                    == Some(true))
        }
        ResourceKind::GeneratedDocument => context.capability == "writing.generateDocument",
        ResourceKind::WritingSkill => context.capability == "writing.refineSkill",
    };
    let allowed = match kind {
        ResourceKind::WikiMarkdown => {
            context.input.get("documentId").and_then(Value::as_str) == Some(key)
        }
        ResourceKind::WikiSpace => {
            context
                .source
                .get("wikiSpaceId")
                .or_else(|| context.input.get("wikiSpaceId"))
                .or_else(|| {
                    context
                        .input
                        .pointer("/contextSnapshot/wikiSelection/wikiSpaceId")
                })
                .and_then(Value::as_str)
                == Some(key)
        }
        ResourceKind::GeneratedDocument => {
            context
                .input
                .get("targetGeneratedDocumentId")
                .and_then(Value::as_str)
                == Some(key)
                || context.task_type == "generate_document"
        }
        ResourceKind::WritingSkill => {
            context.input.get("skillId").and_then(Value::as_str) == Some(key)
        }
    };
    if !allowed || !capability_allows_kind {
        return Err(CliError::with_code(
            "execution_fenced",
            "The execution token is not scoped to this content resource.",
        ));
    }
    Ok(())
}

fn ensure_staging_resource(
    tx: &Transaction<'_>,
    context: &ExecutionContext,
    kind: ResourceKind,
    key: &str,
    metadata: &Value,
) -> Result<(), CliError> {
    let current = tx.query_row(
        "SELECT id, active_revision_id, content_version FROM content_resources WHERE project_id = ? AND resource_kind = ? AND resource_key = ?",
        params![context.project_id, kind.as_str(), key],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, i64>(2)?)),
    ).optional().map_err(to_cli_error)?;
    let (resource_id, revision_id, version) = current
        .map(|value| (Some(value.0), value.1, value.2))
        .unwrap_or((None, None, 0));
    let mut combined = metadata.clone();
    if kind == ResourceKind::GeneratedDocument {
        combined["targetPanelId"] = context
            .source
            .get("wikiPanelId")
            .cloned()
            .unwrap_or_else(|| json!(context.panel_id));
    }
    tx.execute(
        "INSERT OR IGNORE INTO task_staging_resources (staging_session_id, resource_kind, resource_key, content_resource_id, base_revision_id, base_content_version, metadata_json) VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![context.staging_session_id, kind.as_str(), key, resource_id, revision_id, version, combined.to_string()],
    ).map_err(to_cli_error)?;
    Ok(())
}

fn seed_existing_output_resource(
    tx: &Transaction<'_>,
    task_id: &str,
    staging_id: &str,
) -> Result<(), CliError> {
    let (project_id, panel_id, capability, input_json, source_json) = tx.query_row(
        "SELECT project_id, panel_id, capability, input_json, source_json FROM tasks WHERE id = ?",
        [task_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?)),
    ).map_err(to_cli_error)?;
    let input: Value = serde_json::from_str(&input_json).unwrap_or_else(|_| json!({}));
    let source: Value = serde_json::from_str(&source_json).unwrap_or_else(|_| json!({}));
    let output = match capability.as_str() {
        "wiki.convertDocument" => input
            .get("documentId")
            .and_then(Value::as_str)
            .map(|key| (ResourceKind::WikiMarkdown, key, panel_id.as_str())),
        "wiki.ingestMarkdown" | "wiki.maintain" => source
            .get("wikiSpaceId")
            .or_else(|| input.get("wikiSpaceId"))
            .and_then(Value::as_str)
            .map(|key| (ResourceKind::WikiSpace, key, panel_id.as_str())),
        "writing.generateDocument" => input
            .get("targetGeneratedDocumentId")
            .and_then(Value::as_str)
            .map(|key| {
                (
                    ResourceKind::GeneratedDocument,
                    key,
                    source
                        .get("wikiPanelId")
                        .and_then(Value::as_str)
                        .unwrap_or(panel_id.as_str()),
                )
            }),
        "writing.refineSkill" => input
            .get("skillId")
            .and_then(Value::as_str)
            .map(|key| (ResourceKind::WritingSkill, key, panel_id.as_str())),
        _ => None,
    };
    let Some((kind, key, target_panel_id)) = output else {
        return Ok(());
    };
    let current = tx.query_row(
        "SELECT id, active_revision_id, content_version FROM content_resources WHERE project_id = ? AND resource_kind = ? AND resource_key = ? AND archived_at IS NULL",
        params![project_id, kind.as_str(), key],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, i64>(2)?)),
    ).optional().map_err(to_cli_error)?;
    let Some((resource_id, revision_id, version)) = current else {
        return Ok(());
    };
    tx.execute(
        "INSERT OR IGNORE INTO task_staging_resources (staging_session_id, resource_kind, resource_key, content_resource_id, base_revision_id, base_content_version, metadata_json) VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![staging_id, kind.as_str(), key, resource_id, revision_id, version, json!({ "targetPanelId": target_panel_id }).to_string()],
    ).map_err(to_cli_error)?;
    Ok(())
}

fn validate_manifest(
    paths: &MyOpenPanelsPaths,
    tx: &Transaction<'_>,
    kind: ResourceKind,
    resource_key: &str,
    manifest: &BTreeMap<String, FileEntry>,
) -> Result<(), CliError> {
    if manifest.is_empty() {
        return Err(CliError::with_code(
            "invalid_output",
            "Content manifest is empty.",
        ));
    }
    if kind == ResourceKind::WikiSpace {
        if manifest.len() > MAX_WIKI_FILES {
            return Err(CliError::with_code(
                "content_too_large",
                "Wiki revision contains too many files.",
            ));
        }
    }
    if kind == ResourceKind::WikiMarkdown
        && (manifest.len() != 1 || !manifest.contains_key("source.md"))
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Converted Markdown must contain exactly source.md.",
        ));
    }
    if kind == ResourceKind::GeneratedDocument && manifest.len() != 1 {
        return Err(CliError::with_code(
            "invalid_output",
            "Generated document must contain exactly one content file.",
        ));
    }
    if kind == ResourceKind::WritingSkill
        && (!manifest.contains_key("SKILL.md") || !manifest.contains_key("manifest.json"))
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Writing Skill must contain SKILL.md and manifest.json.",
        ));
    }
    if kind == ResourceKind::WritingSkill {
        validate_writing_skill_manifest(paths, tx, resource_key, manifest)?;
    }
    for (logical_path, entry) in manifest {
        let bytes = read_object(paths, &entry.object_hash)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| {
            CliError::with_code(
                "invalid_output",
                format!("Content is not UTF-8: {logical_path}"),
            )
        })?;
        if matches!(
            kind,
            ResourceKind::WikiMarkdown | ResourceKind::GeneratedDocument
        ) && text.trim().is_empty()
        {
            return Err(CliError::with_code(
                "invalid_output",
                format!("Content is empty: {logical_path}"),
            ));
        }
        if kind == ResourceKind::WikiSpace && logical_path.ends_with(".md") {
            validate_wiki_links(logical_path, text, manifest)?;
        }
    }
    Ok(())
}

fn validate_writing_skill_manifest(
    paths: &MyOpenPanelsPaths,
    tx: &Transaction<'_>,
    resource_key: &str,
    manifest: &BTreeMap<String, FileEntry>,
) -> Result<(), CliError> {
    let skill_entry = manifest
        .get("SKILL.md")
        .ok_or_else(|| CliError::with_code("invalid_output", "Writing Skill has no SKILL.md."))?;
    let manifest_entry = manifest.get("manifest.json").ok_or_else(|| {
        CliError::with_code("invalid_output", "Writing Skill has no manifest.json.")
    })?;
    let source = String::from_utf8(read_object(paths, &skill_entry.object_hash)?)
        .map_err(|_| CliError::with_code("invalid_output", "Writing Skill is not valid UTF-8."))?;
    let server_manifest: Value =
        serde_json::from_slice(&read_object(paths, &manifest_entry.object_hash)?).map_err(
            |_| CliError::with_code("invalid_output", "Writing Skill manifest is invalid."),
        )?;
    let parsed = crate::agent::custom_writing_skill_from_source(
        &source,
        "SKILL.md",
        &server_manifest,
    )?;
    if parsed.metadata.id != resource_key {
        return Err(CliError::with_code(
            "invalid_output",
            "Writing Skill metadata does not match its resource identity.",
        ));
    }
    let mut statement = tx
        .prepare(
            r#"
            SELECT f.object_hash
            FROM content_resources r
            JOIN content_revision_files f ON f.revision_id = r.active_revision_id
            WHERE r.resource_kind = 'writing_skill' AND r.resource_key <> ?
              AND r.archived_at IS NULL AND f.logical_path = 'manifest.json'
            "#,
        )
        .map_err(to_cli_error)?;
    let hashes = statement
        .query_map([resource_key], |row| row.get::<_, String>(0))
        .map_err(to_cli_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_cli_error)?;
    for hash in hashes {
        let existing: Value = match serde_json::from_slice(&read_object(paths, &hash)?) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if existing.get("name").and_then(Value::as_str) == Some(parsed.metadata.name.as_str()) {
            return Err(CliError::with_code(
                "invalid_output",
                "Another active Writing Skill already uses this name.",
            ));
        }
    }
    Ok(())
}

fn validate_wiki_links(
    page_path: &str,
    markdown: &str,
    manifest: &BTreeMap<String, FileEntry>,
) -> Result<(), CliError> {
    let mut remaining = markdown;
    while let Some(start) = remaining.find("](") {
        remaining = &remaining[start + 2..];
        let Some(end) = remaining.find(')') else {
            break;
        };
        let target = remaining[..end]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .split('#')
            .next()
            .unwrap_or("");
        remaining = &remaining[end + 1..];
        if target.is_empty()
            || target.starts_with('#')
            || target.starts_with('/')
            || target.contains("://")
            || target.starts_with("mailto:")
            || !target.to_ascii_lowercase().ends_with(".md")
        {
            continue;
        }
        let parent = Path::new(page_path)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        let joined = parent.join(target);
        let mut normalized = Vec::new();
        for component in joined.components() {
            match component {
                Component::Normal(value) => normalized.push(value.to_string_lossy().to_string()),
                Component::ParentDir if normalized.pop().is_some() => {}
                Component::CurDir => {}
                _ => {
                    return Err(CliError::with_code(
                        "invalid_output",
                        format!("Unsafe Wiki link in {page_path}: {target}"),
                    ))
                }
            }
        }
        let resolved = normalized.join("/");
        if !manifest.contains_key(&resolved) {
            return Err(CliError::with_code(
                "invalid_output",
                format!("Broken Wiki link in {page_path}: {target}"),
            ));
        }
    }
    Ok(())
}

fn base_manifest(
    tx: &Transaction<'_>,
    revision_id: Option<&str>,
) -> Result<BTreeMap<String, FileEntry>, CliError> {
    let Some(revision_id) = revision_id else {
        return Ok(BTreeMap::new());
    };
    let mut statement = tx.prepare("SELECT logical_path, object_hash, size_bytes, mime_type FROM content_revision_files WHERE revision_id = ? ORDER BY logical_path").map_err(to_cli_error)?;
    let rows = statement
        .query_map([revision_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                FileEntry {
                    object_hash: row.get(1)?,
                    size_bytes: row.get(2)?,
                    mime_type: row.get(3)?,
                },
            ))
        })
        .map_err(to_cli_error)?;
    rows.collect::<Result<BTreeMap<_, _>, _>>()
        .map_err(to_cli_error)
}

fn manifest_value(manifest: &BTreeMap<String, FileEntry>) -> Value {
    Value::Object(manifest.iter().map(|(path, entry)| (path.clone(), json!({ "objectHash": entry.object_hash, "sizeBytes": entry.size_bytes, "mimeType": entry.mime_type }))).collect())
}

fn active_file_entry(
    connection: &rusqlite::Connection,
    project_id: &str,
    kind: ResourceKind,
    key: &str,
    logical_path: &str,
) -> Result<Option<(String, String)>, CliError> {
    connection
        .query_row(
            r#"
        SELECT f.object_hash, f.mime_type FROM content_resources r
        JOIN content_revision_files f ON f.revision_id = r.active_revision_id
        WHERE r.project_id = ? AND r.resource_kind = ? AND r.resource_key = ?
          AND f.logical_path = ? AND r.archived_at IS NULL
        "#,
            params![project_id, kind.as_str(), key, logical_path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(to_cli_error)
}
