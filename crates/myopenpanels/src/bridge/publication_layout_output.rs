fn build_publication_layout_output_plan(
    _paths: &MyOpenPanelsPaths,
    task: &Value,
    workspace: &Path,
    _attempt_id: &str,
    _execution_generation: i64,
    _execution_unit: &Value,
) -> Result<TaskOutputPlanDraft, CliError> {
    let result = read_execution_result(workspace, "Publication Layout")?;
    validate_result_keys(
        &result,
        &["outcome", "summary", "artifacts"],
        "Publication Layout",
    )?;
    require_outcome(&result, "formatted", "Publication Layout")?;
    let artifact = exactly_one_artifact(workspace, &result, "Publication Layout")?;
    validate_fixed_artifact(
        &artifact,
        "publication-content",
        "outputs/content.json",
        "Publication Layout",
    )?;
    let content: Value = serde_json::from_slice(
        &fs::read(&artifact.absolute_path).map_err(to_cli_error)?,
    )
    .map_err(|_| {
        CliError::with_code(
            "invalid_output",
            "Publication Layout output must be valid JSON.",
        )
    })?;
    validate_layout_document(&content)?;
    let source = task
        .pointer("/input/snapshot/content")
        .ok_or_else(|| CliError::with_code("invalid_target", "Layout source is missing."))?;
    validate_layout_document(source)?;
    if layout_text(source) != layout_text(&content) {
        return Err(CliError::with_code(
            "invalid_output",
            "Publication Layout cannot change publication text.",
        ));
    }
    if layout_images(source) != layout_images(&content) {
        return Err(CliError::with_code(
            "invalid_output",
            "Publication Layout cannot add, remove, reorder, or change images.",
        ));
    }
    if layout_links(source) != layout_links(&content) {
        return Err(CliError::with_code(
            "invalid_output",
            "Publication Layout cannot add, remove, or change links.",
        ));
    }
    let task_id = task
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliError::with_code("invalid_output", "Layout Task id is missing."))?;
    let project_id = task
        .get("projectId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliError::with_code("invalid_target", "Layout Project id is missing."))?;
    let panel_id = task
        .get("panelId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliError::with_code("invalid_target", "Layout panel id is missing."))?;
    Ok(TaskOutputPlanDraft {
        result,
        actions: vec![TaskOutputAction::PrepareTypesettingLayout {
            project_id: project_id.to_owned(),
            panel_id: panel_id.to_owned(),
            task_id: task_id.to_owned(),
            artifact,
            content,
        }],
    })
}

fn validate_layout_document(document: &Value) -> Result<(), CliError> {
    if document.get("type").and_then(Value::as_str) != Some("doc") {
        return Err(CliError::with_code(
            "invalid_output",
            "Publication Layout output must be a TipTap doc.",
        ));
    }
    validate_layout_node(document)
}

fn validate_layout_node(node: &Value) -> Result<(), CliError> {
    let node_type = node.get("type").and_then(Value::as_str).unwrap_or("");
    if !matches!(
        node_type,
        "doc"
            | "paragraph"
            | "heading"
            | "bulletList"
            | "orderedList"
            | "listItem"
            | "blockquote"
            | "text"
            | "hardBreak"
            | "image"
    ) {
        return Err(CliError::with_code(
            "invalid_output",
            format!("Publication Layout output contains unsupported node: {node_type}"),
        ));
    }
    if node_type == "heading"
        && !matches!(node.pointer("/attrs/level").and_then(Value::as_i64), Some(1..=3))
    {
        return Err(CliError::with_code(
            "invalid_output",
            "Publication Layout headings must use levels 1 through 3.",
        ));
    }
    if node_type == "text" && !node.get("text").is_some_and(Value::is_string) {
        return Err(CliError::with_code(
            "invalid_output",
            "Publication Layout text nodes require text.",
        ));
    }
    if node_type == "image" && !node.get("attrs").is_some_and(Value::is_object) {
        return Err(CliError::with_code(
            "invalid_output",
            "Publication Layout image nodes require attributes.",
        ));
    }
    for mark in node
        .get("marks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mark_type = mark.get("type").and_then(Value::as_str).unwrap_or("");
        if !matches!(mark_type, "bold" | "italic" | "link") {
            return Err(CliError::with_code(
                "invalid_output",
                format!("Publication Layout output contains unsupported mark: {mark_type}"),
            ));
        }
        if mark_type == "link"
            && !mark
                .pointer("/attrs/href")
                .and_then(Value::as_str)
                .is_some_and(|href| !href.is_empty())
        {
            return Err(CliError::with_code(
                "invalid_output",
                "Publication Layout links require href attributes.",
            ));
        }
    }
    for child in node
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        validate_layout_node(child)?;
    }
    Ok(())
}

fn layout_text(document: &Value) -> String {
    let mut text = String::new();
    walk_layout(document, &mut |node, _| {
        if let Some(value) = node.get("text").and_then(Value::as_str) {
            text.push_str(value);
        }
    });
    text
}

fn layout_images(document: &Value) -> Vec<Value> {
    let mut images = Vec::new();
    walk_layout(document, &mut |node, _| {
        if node.get("type").and_then(Value::as_str) == Some("image") {
            images.push(node.get("attrs").cloned().unwrap_or(Value::Null));
        }
    });
    images
}

fn layout_links(document: &Value) -> Vec<(usize, usize, Value)> {
    let mut links = Vec::new();
    let mut offset = 0_usize;
    walk_layout(document, &mut |node, _| {
        let Some(text) = node.get("text").and_then(Value::as_str) else {
            return;
        };
        let length = text.chars().count();
        for mark in node
            .get("marks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if mark.get("type").and_then(Value::as_str) == Some("link") {
                let attrs = mark.get("attrs").cloned().unwrap_or(Value::Null);
                if let Some((_, end, previous_attrs)) = links.last_mut() {
                    if *end == offset && *previous_attrs == attrs {
                        *end = offset + length;
                        continue;
                    }
                }
                links.push((offset, offset + length, attrs));
            }
        }
        offset += length;
    });
    links
}

fn walk_layout(document: &Value, visit: &mut impl FnMut(&Value, usize)) {
    fn walk(node: &Value, depth: usize, visit: &mut impl FnMut(&Value, usize)) {
        visit(node, depth);
        for child in node
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            walk(child, depth + 1, visit);
        }
    }
    walk(document, 0, visit);
}


#[cfg(test)]
mod publication_layout_output_tests {
    use super::*;

    fn write_layout_result(workspace: &Path, relative_path: &str) {
        write_test_result(
            workspace,
            &json!({
                "outcome": "formatted",
                "summary": "layout",
                "artifacts": [{
                    "role": "publication-content",
                    "relativePath": relative_path
                }]
            }),
        )
        .expect("execution result");
    }

    fn layout_source() -> Value {
        json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [
                    { "type": "text", "text": "Section", "marks": [{ "type": "link", "attrs": { "href": "https://example.com" } }] },
                    { "type": "image", "attrs": { "assetRef": "asset:1", "src": "/image.png" } }
                ]
            }]
        })
    }

    fn layout_task(source: &Value) -> Value {
        json!({
            "id": "task:layout",
            "projectId": "project:layout",
            "panelId": "panel:typesetting",
            "input": { "snapshot": { "content": source } }
        })
    }

    #[test]
    fn layout_output_preserves_text_images_and_links() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(workspace.join("outputs")).expect("workspace");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("layout-output-test"),
        )
        .expect("paths");
        let source = layout_source();
        let task = layout_task(&source);
        let formatted = json!({
            "type": "doc",
            "content": [{
                "type": "heading",
                "attrs": { "level": 2 },
                "content": [
                    { "type": "text", "text": "Section", "marks": [{ "type": "link", "attrs": { "href": "https://example.com" } }, { "type": "bold" }] },
                    { "type": "image", "attrs": { "assetRef": "asset:1", "src": "/image.png" } }
                ]
            }]
        });
        fs::write(
            workspace.join("outputs/content.json"),
            serde_json::to_vec(&formatted).unwrap(),
        )
        .expect("content");
        write_layout_result(&workspace, "outputs/content.json");
        assert!(build_publication_layout_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .is_ok());

        let mut changed = formatted;
        changed["content"][0]["content"][0]["text"] = json!("Changed");
        fs::write(
            workspace.join("outputs/content.json"),
            serde_json::to_vec(&changed).unwrap(),
        )
        .expect("changed content");
        write_layout_result(&workspace, "outputs/content.json");
        assert!(build_publication_layout_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .is_err());
    }

    #[test]
    fn layout_output_rejects_unsupported_nodes_changed_images_and_links() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(workspace.join("outputs")).expect("workspace");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("layout-invariants-test"),
        )
        .expect("paths");
        let source = layout_source();
        let task = layout_task(&source);
        let mut unsupported = source.clone();
        unsupported["content"][0]["type"] = json!("codeBlock");
        let mut changed_image = source.clone();
        changed_image["content"][0]["content"][1]["attrs"]["src"] =
            json!("/replacement.png");
        let mut changed_link = source.clone();
        changed_link["content"][0]["content"][0]["marks"][0]["attrs"]["href"] =
            json!("https://changed.example.com");

        for invalid in [unsupported, changed_image, changed_link] {
            fs::write(
                workspace.join("outputs/content.json"),
                serde_json::to_vec(&invalid).unwrap(),
            )
            .expect("content");
            write_layout_result(&workspace, "outputs/content.json");
            let error = build_publication_layout_output_plan(
                &paths,
                &task,
                &workspace,
                "attempt:1",
                1,
                &json!({}),
            )
            .expect_err("invalid layout output");
            assert_eq!(error.code(), Some("invalid_output"));
        }
    }

    #[test]
    fn layout_output_rejects_wrong_paths_and_oversized_content() {
        let temp = tempfile::tempdir().expect("temp");
        let project = temp.path().join("project");
        let storage = temp.path().join("storage");
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(workspace.join("outputs")).expect("workspace");
        fs::create_dir_all(&project).expect("project");
        let paths = crate::paths::resolve_myopenpanels_paths(
            Some(project.to_str().unwrap()),
            Some(storage.to_str().unwrap()),
            Some("layout-limits-test"),
        )
        .expect("paths");
        let source = layout_source();
        let task = layout_task(&source);
        fs::write(
            workspace.join("outputs/other.json"),
            serde_json::to_vec(&source).unwrap(),
        )
        .expect("wrong path content");
        write_layout_result(&workspace, "outputs/other.json");
        assert!(build_publication_layout_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .is_err());

        std::fs::File::create(workspace.join("outputs/content.json"))
            .expect("oversized content")
            .set_len(crate::content::MAX_TEXT_FILE_BYTES as u64 + 1)
            .expect("resize content");
        write_layout_result(&workspace, "outputs/content.json");
        let error = build_publication_layout_output_plan(
            &paths,
            &task,
            &workspace,
            "attempt:1",
            1,
            &json!({}),
        )
        .expect_err("oversized layout output");
        assert_eq!(error.code(), Some("content_too_large"));
    }
}
