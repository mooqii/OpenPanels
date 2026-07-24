fn read_publishing_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let payload = crate::tasks::inspect_task(paths, task_id)?;
    let task = &payload["task"];
    let task_type = task.get("type").and_then(Value::as_str).unwrap_or_default();
    if task.get("queue").and_then(Value::as_str) != Some("release")
        || !is_publishing_task_type(task_type)
    {
        return Err(CliError::with_code(
            "invalid_target",
            "Publishing lifecycle requires a supported publishing Task.",
        ));
    }
    Ok(payload)
}

pub(crate) fn prepare_task_cancellation(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
) -> Result<Option<crate::tasks::PreparedPanelState>, CliError> {
    let payload = read_publishing_task(paths, task_id)?;
    let task = &payload["task"];
    let project_id = task
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let panel_id = task
        .get("panelId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let attempt_id = task
        .pointer("/input/attemptId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let storage = Storage::open(paths)?;
    let (state, base_revision) = storage
        .read_panel_state_snapshot(project_id, panel_id)?
        .ok_or_else(|| {
            CliError::with_code("target_not_found", "Publishing state not found.")
        })?;
    let mut state = normalize_state(state);
    let now = now_iso();
    let changed = {
        let attempt = find_attempt_mut(&mut state, attempt_id)?;
        if attempt.get("outcome").is_some_and(|value| !value.is_null()) {
            false
        } else {
            attempt["phase"] = json!("completed");
            attempt["outcome"] = json!("not_published");
            attempt["summary"] = json!("Publishing task cancelled.");
            attempt["reasonCode"] = json!("user_cancelled");
            attempt["completedAt"] = json!(now);
            attempt["updatedAt"] = json!(now);
            true
        }
    };
    if changed {
        Ok(Some(crate::tasks::PreparedPanelState::new(
            panel_id,
            base_revision,
            state,
        )))
    } else {
        Ok(None)
    }
}
