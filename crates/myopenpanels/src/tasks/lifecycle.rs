pub fn cancel_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let task = inspect_task(paths, task_id)?;
    if matches!(
        task["task"]["status"].as_str(),
        Some("succeeded" | "failed" | "cancelled" | "superseded")
    ) {
        return Err(CliError::with_code(
            "invalid_task_transition",
            "Terminal Tasks cannot be cancelled.",
        ));
    }
    let project_id = task["task"]["projectId"].as_str().unwrap_or_default();
    match task_queue_adapter(task["task"]["queue"].as_str().unwrap_or(""))? {
        TaskQueueAdapter::Wiki => crate::wiki::cancel_task(paths, task_id)?,
        TaskQueueAdapter::Writing => crate::writing::cancel_task(paths, task_id)?,
        TaskQueueAdapter::Publication => crate::publication::cancel_task(paths, task_id)?,
        TaskQueueAdapter::Release => crate::release::cancel_task(paths, task_id)?,
    };
    finalize_task_runtime(
        paths,
        project_id,
        task_id,
        "cancelled",
        None,
        Some(json!({ "code": "user_cancelled" })),
        None,
        None,
        None,
        None,
    )?;
    inspect_task_in_session(paths, project_id, task_id)
}

pub fn archive_task(paths: &MyOpenPanelsPaths, task_id: &str) -> Result<Value, CliError> {
    let task = inspect_task(paths, task_id)?;
    let status = task["task"]["status"].as_str().unwrap_or_default();
    if !matches!(status, "succeeded" | "failed" | "cancelled" | "superseded") {
        return Err(CliError::with_code(
            "invalid_task_transition",
            "Only terminal Tasks can be archived.",
        ));
    }
    let project_id = task["task"]["projectId"].as_str().unwrap_or_default();
    let storage = Storage::open(paths)?;
    let tx = storage
        .connection()
        .unchecked_transaction()
        .map_err(to_cli_error)?;
    let now = crate::control::now_iso();
    tx.execute(
        "UPDATE tasks SET archived_at = ?, updated_at = ? WHERE id = ? AND archived_at IS NULL",
        params![now, now, task_id],
    )
    .map_err(to_cli_error)?;
    crate::storage::record_scope(&tx, "tasks", Some(project_id), None)?;
    tx.commit().map_err(to_cli_error)?;
    inspect_task_in_session(paths, project_id, task_id)
}
