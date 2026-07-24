fn annotate_dispatch_state(
    paths: &MyOpenPanelsPaths,
    _project_id: &str,
    tasks: Vec<Value>,
) -> Result<Vec<Value>, CliError> {
    let available_runners = crate::model_gateway::worker_specs(paths)?;
    let storage = Storage::open(paths)?;
    let mut output = Vec::with_capacity(tasks.len());
    for mut task in tasks {
        let task_id = task.get("id").and_then(Value::as_str).unwrap_or_default();
        let dependencies = read_task_dependency_values(storage.connection(), task_id)?;
        let dependency_blocked = dependencies
            .iter()
            .any(|dependency| dependency.get("status").and_then(Value::as_str) != Some("succeeded"));
        let mutation_blocked = mutation_task_blocked(storage.connection(), task_id)?;
        let manual = task
            .pointer("/input/executionMode")
            .and_then(Value::as_str)
            == Some("manual");
        let compatible = if manual { 0 } else { available_runners.len() };
        let dispatch_state = match task.get("status").and_then(Value::as_str) {
            Some("running") => "running",
            Some("queued") if dependency_blocked || mutation_blocked => "waiting",
            Some("queued") if manual => "manual",
            Some("queued") if compatible == 0 => "noTarget",
            Some("queued") => "eligible",
            _ => "done",
        };
        if let Some(object) = task.as_object_mut() {
            object.insert("dispatchState".to_owned(), json!(dispatch_state));
            object.insert("compatibleTargetCount".to_owned(), json!(compatible));
            object.insert("dependencies".to_owned(), json!(dependencies));
            object.insert("mutationBlocked".to_owned(), json!(mutation_blocked));
            if dependency_blocked || mutation_blocked {
                object.insert("ready".to_owned(), json!(false));
                object.insert(
                    "blockedReason".to_owned(),
                    json!(if dependency_blocked { "prerequisite" } else { "mutationPredecessor" }),
                );
            }
        }
        output.push(task);
    }
    Ok(output)
}

fn mutation_task_blocked(
    connection: &rusqlite::Connection,
    task_id: &str,
) -> Result<bool, CliError> {
    connection
        .query_row(
            r#"
            SELECT EXISTS(
              SELECT 1 FROM tasks candidate
              WHERE candidate.id = ? AND candidate.mutation_key IS NOT NULL
                AND EXISTS (
                  SELECT 1 FROM tasks predecessor
                  WHERE predecessor.project_id = candidate.project_id
                    AND predecessor.mutation_key = candidate.mutation_key
                    AND predecessor.id <> candidate.id
                    AND predecessor.status = 'running'
                )
            )
            "#,
            [task_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(to_cli_error)
}

fn read_task_dependency_values(
    connection: &rusqlite::Connection,
    task_id: &str,
) -> Result<Vec<Value>, CliError> {
    connection
        .query_row(
            r#"
            SELECT dependency.id, dependency.status
            FROM tasks task JOIN tasks dependency ON dependency.id = task.depends_on_task_id
            WHERE task.id = ?
            "#,
            [task_id],
            |row| {
                Ok(vec![json!({
                    "prerequisiteTaskId": row.get::<_, String>(0)?,
                    "status": row.get::<_, String>(1)?,
                    "successCondition": "succeeded",
                    "failurePolicy": "fail",
                })])
            },
        )
        .optional()
        .map(|value| value.unwrap_or_default())
        .map_err(to_cli_error)
}

fn normalize_capabilities(capabilities: Vec<String>) -> Vec<String> {
    let mut capabilities = capabilities
        .into_iter()
        .flat_map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    capabilities.sort();
    capabilities.dedup();
    capabilities
}
