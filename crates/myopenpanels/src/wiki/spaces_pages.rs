pub fn list_spaces(paths: &MyOpenPanelsPaths) -> Result<Value, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    Ok(json!({
        "spaces": wiki.state.get("wikiSpaces").cloned().unwrap_or_else(|| json!([])),
        "state": wiki.state,
    }))
}

pub fn set_active_space(paths: &MyOpenPanelsPaths, wiki_space_id: &str) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    state_object_mut(&mut wiki.state)?.insert("activeWikiSpaceId".to_owned(), json!(space.id));
    save_wiki_state(paths, &wiki)?;
    Ok(json!({ "wikiSpace": space.value, "state": wiki.state }))
}

pub fn list_pages(paths: &MyOpenPanelsPaths, wiki_space_id: &str) -> Result<Value, CliError> {
    let wiki = get_wiki_bootstrap(paths)?;
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    Ok(json!({ "pages": space.value.get("pageIndex").cloned().unwrap_or_else(|| json!([])) }))
}

pub fn search_pages(
    paths: &MyOpenPanelsPaths,
    wiki_space_id: &str,
    query: &str,
    limit: usize,
) -> Result<Value, CliError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(CliError::new("Wiki page search query cannot be empty."));
    }
    let wiki = get_wiki_bootstrap(paths)?;
    let storage = Storage::open(paths)?;
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    let panel_dir = storage.panel_dir(&wiki.project.id, &wiki.panel.id);
    let query_lower = query.to_lowercase();
    let terms = query_lower
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    let mut results = space
        .value
        .get("pageIndex")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|page| {
            let page_path = page.get("path").and_then(Value::as_str)?;
            let path = wiki_page_path(&panel_dir, &space.id, page_path).ok()?;
            let markdown = crate::content::read_active_text(
                paths,
                &wiki.project.id,
                crate::content::ResourceKind::WikiSpace,
                &space.id,
                page_path,
            )
            .ok()
            .flatten()
            .or_else(|| fs::read_to_string(path).ok())?;
            let title = page
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or(page_path);
            let summary = page.get("summary").and_then(Value::as_str).unwrap_or("");
            let searchable = format!("{title}\n{page_path}\n{summary}\n{markdown}").to_lowercase();
            let matched_terms = terms
                .iter()
                .filter(|term| searchable.contains(**term))
                .count();
            if !searchable.contains(&query_lower) && matched_terms == 0 {
                return None;
            }
            let title_lower = title.to_lowercase();
            let path_lower = page_path.to_lowercase();
            let score = usize::from(title_lower.contains(&query_lower)) * 8
                + usize::from(path_lower.contains(&query_lower)) * 5
                + usize::from(searchable.contains(&query_lower)) * 3
                + matched_terms;
            Some((
                score,
                page_path.to_owned(),
                json!({
                    "path": page_path,
                    "title": title,
                    "summary": summary,
                    "snippet": search_snippet(&markdown, &query_lower, &terms),
                    "score": score,
                }),
            ))
        })
        .collect::<Vec<_>>();
    results.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    let matches = results
        .into_iter()
        .take(limit.clamp(1, 100))
        .map(|(_, _, value)| value)
        .collect::<Vec<_>>();
    Ok(json!({
        "query": query,
        "wikiSpace": space.value,
        "matches": matches,
    }))
}

pub fn read_page(
    paths: &MyOpenPanelsPaths,
    wiki_space_id: &str,
    page_path: &str,
) -> Result<Value, CliError> {
    if crate::content::broker_execution_available() {
        let payload = crate::content::broker_read_file(&crate::content::ReadFileRequest {
            resource_kind: crate::content::ResourceKind::WikiSpace.as_str().to_owned(),
            resource_key: wiki_space_id.to_owned(),
            logical_path: page_path.to_owned(),
        })?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload["contentBase64"].as_str().unwrap_or_default())
            .map_err(to_cli_error)?;
        return Ok(
            json!({ "pagePath": page_path, "markdown": String::from_utf8(bytes).map_err(to_cli_error)?, "staged": true }),
        );
    }
    crate::content::require_broker_for_task_execution()?;
    let wiki = get_wiki_bootstrap(paths)?;
    let storage = Storage::open(paths)?;
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    let path = wiki_page_path(
        &storage.panel_dir(&wiki.project.id, &wiki.panel.id),
        &space.id,
        page_path,
    )?;
    let markdown = crate::content::read_active_text(
        paths,
        &wiki.project.id,
        crate::content::ResourceKind::WikiSpace,
        wiki_space_id,
        page_path,
    )?
    .map(Ok)
    .unwrap_or_else(|| fs::read_to_string(path).map_err(to_cli_error))?;
    Ok(json!({ "pagePath": page_path, "wikiSpace": space.value, "markdown": markdown }))
}

fn search_snippet(markdown: &str, query: &str, terms: &[&str]) -> String {
    let lines = markdown
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let selected = lines
        .clone()
        .find(|line| {
            let lower = line.to_lowercase();
            lower.contains(query) || terms.iter().any(|term| lower.contains(*term))
        })
        .or_else(|| lines.into_iter().find(|line| !line.starts_with("---")))
        .unwrap_or("");
    selected.chars().take(320).collect()
}

pub fn write_page(
    paths: &MyOpenPanelsPaths,
    wiki_space_id: &str,
    page_path: &str,
    content: &str,
    title: Option<&str>,
    task_id: Option<&str>,
) -> Result<Value, CliError> {
    if task_id.is_some() && crate::content::broker_execution_available() {
        return crate::content::broker_stage_file(&crate::content::StageFileRequest {
            resource_kind: crate::content::ResourceKind::WikiSpace.as_str().to_owned(),
            resource_key: wiki_space_id.to_owned(),
            logical_path: page_path.to_owned(),
            content_base64: base64::engine::general_purpose::STANDARD.encode(content.as_bytes()),
            mime_type: "text/markdown".to_owned(),
            metadata: json!({ "title": title, "wikiSpaceId": wiki_space_id }),
        });
    }
    crate::content::require_broker_for_task_execution()?;
    if let Some(task_id) = task_id {
        crate::tasks::verify_task_write_access(paths, task_id)?;
    }
    let mut wiki = match task_id {
        Some(task_id) => get_wiki_task_target(paths, task_id)?,
        None => get_wiki_bootstrap(paths)?,
    };
    let storage = Storage::open(paths)?;
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    if task_id.is_none() {
        let mutation_key = wiki_mutation_key(&wiki.project.id, &wiki.panel.id, &space.id);
        crate::tasks::supersede_active_wiki_mutations(paths, &wiki.project.id, &mutation_key)?;
    }
    let path = wiki_page_path(
        &storage.panel_dir(&wiki.project.id, &wiki.panel.id),
        &space.id,
        page_path,
    )?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(to_cli_error)?;
    }
    fs::write(path, content).map_err(to_cli_error)?;
    if task_id.is_none() {
        crate::content::commit_immediate_text(
            paths,
            &wiki.project.id,
            Some(&wiki.panel.id),
            crate::content::ResourceKind::WikiSpace,
            &space.id,
            page_path,
            content.as_bytes(),
            "text/markdown",
            false,
        )?;
    }
    let now = now_iso();
    let page_existed = space
        .value
        .get("pageIndex")
        .and_then(Value::as_array)
        .is_some_and(|pages| {
            pages
                .iter()
                .any(|page| page.get("path").and_then(Value::as_str) == Some(page_path))
        });
    upsert_page_index(&mut wiki.state, &space.id, page_path, content, title, &now)?;
    update_wiki_space_timestamp(&mut wiki.state, &space.id, &now)?;
    state_object_mut(&mut wiki.state)?.insert("activeWikiSpaceId".to_owned(), json!(space.id));
    state_object_mut(&mut wiki.state)?.insert("activeWikiPagePath".to_owned(), json!(page_path));
    let task = if task_id.is_none() {
        let mutation_key = wiki_mutation_key(&wiki.project.id, &wiki.panel.id, &space.id);
        Some(create_wiki_maintenance_task(
            &wiki.state,
            &mut wiki.tasks,
            Some(space.id.as_str()),
            &mutation_key,
            json!({
                "kind": "wiki_page_written",
                "path": page_path,
                "operation": if page_existed { "updated" } else { "created" },
            }),
        )?)
    } else {
        None
    };
    save_wiki_state(paths, &wiki)?;
    trace::record_simple(
        "task",
        "wiki",
        Some("write"),
        format!("Wrote wiki page {page_path}"),
        Some(format!("Updated wiki page {page_path}")),
        Some(json!({
            "wikiSpaceId": space.id,
            "pagePath": page_path,
            "taskId": task_id,
        })),
    );
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    Ok(
        json!({ "pagePath": page_path, "task": task, "wikiSpace": space.value, "state": wiki.state }),
    )
}

pub fn rename_page(
    paths: &MyOpenPanelsPaths,
    wiki_space_id: &str,
    page_path: &str,
    next_page_path: &str,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let storage = Storage::open(paths)?;
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    let mutation_key = wiki_mutation_key(&wiki.project.id, &wiki.panel.id, &space.id);
    crate::tasks::supersede_active_wiki_mutations(paths, &wiki.project.id, &mutation_key)?;
    let panel_dir = storage.panel_dir(&wiki.project.id, &wiki.panel.id);
    let old_path = wiki_page_path(&panel_dir, &space.id, page_path)?;
    let new_path = wiki_page_path(&panel_dir, &space.id, next_page_path)?;
    if old_path != new_path {
        if new_path.exists() {
            return Err(CliError::new(format!(
                "Wiki page already exists: {next_page_path}"
            )));
        }
        if let Some(parent) = new_path.parent() {
            fs::create_dir_all(parent).map_err(to_cli_error)?;
        }
        fs::rename(old_path, new_path).map_err(to_cli_error)?;
    }
    let now = now_iso();
    let spaces = state_array_mut(&mut wiki.state, "wikiSpaces")?;
    let page_index = spaces
        .iter_mut()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(space.id.as_str()))
        .and_then(|item| item.get_mut("pageIndex"))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| CliError::new("Wiki page index is invalid."))?;
    let page = page_index
        .iter_mut()
        .find(|item| item.get("path").and_then(Value::as_str) == Some(page_path))
        .ok_or_else(|| CliError::new(format!("Wiki page not found: {page_path}")))?;
    page["path"] = json!(next_page_path);
    page["title"] = json!(title_from_file_name(next_page_path));
    page["type"] = json!("page");
    page["updatedAt"] = json!(now);
    state_object_mut(&mut wiki.state)?
        .insert("activeWikiPagePath".to_owned(), json!(next_page_path));
    update_wiki_space_timestamp(&mut wiki.state, &space.id, &now)?;
    let task = create_wiki_maintenance_task(
        &wiki.state,
        &mut wiki.tasks,
        Some(space.id.as_str()),
        &mutation_key,
        json!({
            "kind": "wiki_page_renamed",
            "fromPath": page_path,
            "toPath": next_page_path,
        }),
    )?;
    save_wiki_state(paths, &wiki)?;
    let space = resolve_wiki_space(&wiki.state, Some(wiki_space_id))?;
    Ok(
        json!({ "pagePath": next_page_path, "task": task, "wikiSpace": space.value, "state": wiki.state }),
    )
}

pub fn maintain_wiki_space(
    paths: &MyOpenPanelsPaths,
    wiki_space_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut wiki = get_wiki_bootstrap(paths)?;
    let space = resolve_wiki_space(&wiki.state, wiki_space_id)?;
    let mutation_key = wiki_mutation_key(&wiki.project.id, &wiki.panel.id, &space.id);
    let task = create_wiki_maintenance_task(
        &wiki.state,
        &mut wiki.tasks,
        Some(space.id.as_str()),
        &mutation_key,
        json!({ "kind": "manual_maintenance" }),
    )?;
    save_wiki_state(paths, &wiki)?;
    let space = resolve_wiki_space(&wiki.state, wiki_space_id)?;
    Ok(json!({ "task": task, "state": wiki.state, "wikiSpace": space.value }))
}
