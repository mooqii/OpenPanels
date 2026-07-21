fn read_publishing_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let payload = crate::tasks::inspect_task(paths, task_id)?;
    let task = &payload["task"];
    let task_type = task.get("type").and_then(Value::as_str).unwrap_or_default();
    if task.get("queue").and_then(Value::as_str) != Some("publishing")
        || !is_publishing_task_type(task_type)
    {
        return Err(CliError::with_code(
            "invalid_target",
            "Publishing lifecycle requires a supported publishing Task.",
        ));
    }
    Ok(payload)
}

// Publishing business phases are checkpoint-driven; the generic Task runtime owns lease status.
pub fn claim_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let mut payload = read_publishing_task(paths, task_id)?;
    let attempt = payload["task"]["attempt"].as_i64().unwrap_or(0) + 1;
    payload["task"]["status"] = json!("running");
    payload["task"]["attempt"] = json!(attempt);
    Ok(payload)
}

pub fn heartbeat_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    read_publishing_task(paths, task_id)
}

pub fn fail_task(
    paths: &MyOpenPanelsPaths,
    task_id: &str,
    _message: &str,
) -> Result<Value, CliError> {
    read_publishing_task(paths, task_id)
}

pub fn release_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    read_publishing_task(paths, task_id)
}

pub fn cancel_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
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
    let mut state = normalize_state(
        storage
            .read_panel_state(project_id, panel_id)?
            .unwrap_or_else(empty_state),
    );
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
    let revision = if changed {
        storage.write_panel_state(project_id, panel_id, &state)?
    } else {
        storage.read_panel_state_revision(project_id, panel_id)?
    };
    Ok(json!({
        "task": task,
        "state": state,
        "revision": revision,
    }))
}
