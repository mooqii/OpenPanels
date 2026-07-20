#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TaskExecutionScope {
    ProjectDrain { project_id: String },
    ExactTask { task_id: String },
    WikiMutationDrain {
        project_id: String,
        mutation_key: String,
    },
}

impl TaskExecutionScope {
    fn project_id(&self, paths: &MyOpenPanelsPaths) -> Result<String, CliError> {
        match self {
            Self::ProjectDrain { project_id }
            | Self::WikiMutationDrain { project_id, .. } => Ok(project_id.clone()),
            Self::ExactTask { task_id } => task_project_id(paths, task_id),
        }
    }

    pub(crate) fn value(&self, project_id: &str) -> Value {
        match self {
            Self::ProjectDrain { .. } => json!({
                "kind": "project-drain",
                "projectId": project_id,
            }),
            Self::ExactTask { task_id } => json!({
                "kind": "exact-task",
                "projectId": project_id,
                "taskId": task_id,
            }),
            Self::WikiMutationDrain { mutation_key, .. } => json!({
                "kind": "wiki-mutation-drain",
                "projectId": project_id,
                "mutationKey": mutation_key,
            }),
        }
    }
}

pub fn read_task_scope(
    paths: &MyOpenPanelsPaths,
    scope: &TaskExecutionScope,
) -> Result<Value, CliError> {
    scope_payload(paths, scope, None)
}

pub fn claim_task_scope(
    paths: &MyOpenPanelsPaths,
    scope: &TaskExecutionScope,
    target_id: &str,
) -> Result<Value, CliError> {
    claim_task_scope_with_broker_url(paths, scope, target_id, None)
}

pub(crate) fn claim_task_scope_with_broker_url(
    paths: &MyOpenPanelsPaths,
    scope: &TaskExecutionScope,
    target_id: &str,
    task_broker_url: Option<&str>,
) -> Result<Value, CliError> {
    let project_id = validate_task_scope(paths, scope)?;
    heartbeat_target_in_session(paths, &project_id, target_id)?;
    let claimed = match scope {
        TaskExecutionScope::ProjectDrain { .. } => claim_once(
            paths,
            &project_id,
            target_id,
            None,
            None,
            None,
            WikiBatchPolicy::CompatibleWindow,
            task_broker_url,
        )?,
        TaskExecutionScope::ExactTask { task_id } => claim_once(
            paths,
            &project_id,
            target_id,
            Some(task_id),
            None,
            None,
            WikiBatchPolicy::Exact,
            task_broker_url,
        )?,
        TaskExecutionScope::WikiMutationDrain { mutation_key, .. } => {
            let tasks = tasks_for_scope(paths, scope, &project_id)?;
            let candidate = next_wiki_scope_candidate(&tasks, mutation_key);
            match candidate {
                Some(task_id) => claim_once(
                    paths,
                    &project_id,
                    target_id,
                    Some(&task_id),
                    None,
                    None,
                    WikiBatchPolicy::CompatibleWindow,
                    task_broker_url,
                )?,
                None => None,
            }
        }
    };
    scope_payload(paths, scope, claimed)
}

fn validate_task_scope(
    paths: &MyOpenPanelsPaths,
    scope: &TaskExecutionScope,
) -> Result<String, CliError> {
    let project_id = scope.project_id(paths)?;
    let storage = Storage::open(paths)?;
    if storage.read_project(&project_id)?.is_none() {
        return Err(CliError::with_code(
            "project_not_found",
            format!("Project not found: {project_id}"),
        ));
    }
    match scope {
        TaskExecutionScope::ExactTask { task_id } => {
            let task_project = task_project_id(paths, task_id)?;
            if task_project != project_id {
                return Err(CliError::with_code(
                    "task_scope_mismatch",
                    "The exact Task does not belong to the requested Project.",
                ));
            }
        }
        TaskExecutionScope::WikiMutationDrain { mutation_key, .. } => {
            let exists = storage
                .list_tasks(&project_id)?
                .iter()
                .any(|task| task.get("mutationKey").and_then(Value::as_str) == Some(mutation_key));
            if !exists {
                return Err(CliError::with_code(
                    "mutation_scope_not_found",
                    format!("Wiki mutation scope not found: {mutation_key}"),
                ));
            }
        }
        TaskExecutionScope::ProjectDrain { .. } => {}
    }
    Ok(project_id)
}

fn scope_payload(
    paths: &MyOpenPanelsPaths,
    scope: &TaskExecutionScope,
    claimed: Option<Value>,
) -> Result<Value, CliError> {
    let project_id = validate_task_scope(paths, scope)?;
    let tasks = tasks_for_scope(paths, scope, &project_id)?;
    let nonterminal = tasks
        .iter()
        .filter(|task| !scope_task_terminal(task))
        .collect::<Vec<_>>();
    let active_count = nonterminal
        .iter()
        .filter(|task| is_active_task(task))
        .count();
    let claimable_count = nonterminal
        .iter()
        .filter(|task| scope_task_claimable(task))
        .count();
    let pending_count = nonterminal
        .iter()
        .filter(|task| is_pending_task(task))
        .count();
    let blocked_count = nonterminal.len() - active_count - claimable_count;
    let scope_state = if claimed.is_some() {
        "running"
    } else if nonterminal.is_empty() {
        "complete"
    } else if claimable_count > 0 {
        "ready"
    } else if active_count > 0 {
        "running"
    } else {
        "blocked"
    };
    let blockers = nonterminal
        .iter()
        .filter(|task| !scope_task_claimable(task))
        .map(|task| {
            let reason = if is_active_task(task) {
                "leased"
            } else {
                task.get("blockedReason")
                    .and_then(Value::as_str)
                    .unwrap_or("not-ready")
            };
            json!({
                "taskId": task.get("id"),
                "status": task.get("status"),
                "reason": reason,
                "retryAfter": task.get("retryAfter"),
                "nextRunAt": task.get("nextRunAt"),
                "attempts": task.get("attempt"),
                "maxAttempts": task.get("maxAttempts"),
                "lease": task.get("lease"),
                "error": task.get("error"),
                "mutationBlocked": task.get("mutationBlocked"),
                "dependencies": task.get("dependencies"),
            })
        })
        .collect::<Vec<_>>();
    let required_capabilities = nonterminal
        .iter()
        .filter_map(|task| task.get("capability").and_then(Value::as_str))
        .map(str::to_owned)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut payload = claimed.unwrap_or_else(|| {
        json!({
            "task": Value::Null,
            "leaseToken": Value::Null,
        })
    });
    if payload.get("batch").is_none() {
        payload["batch"] = Value::Null;
    }
    if payload.get("leaseToken").is_none() {
        payload["leaseToken"] = Value::Null;
    }
    if payload.get("taskBrokerUrl").is_none() {
        payload["taskBrokerUrl"] = Value::Null;
    }
    if payload.get("executionToken").is_none() {
        payload["executionToken"] = Value::Null;
    }
    if payload.get("executionTokenExpiresAt").is_none() {
        payload["executionTokenExpiresAt"] = Value::Null;
    }
    if payload.get("inputManifest").is_none() {
        payload["inputManifest"] = json!([]);
    }
    payload["lease"] = payload
        .pointer("/task/lease")
        .cloned()
        .unwrap_or(Value::Null);
    payload["scope"] = scope.value(&project_id);
    payload["scopeState"] = json!(scope_state);
    payload["counts"] = json!({
        "pending": pending_count,
        "active": active_count,
        "blocked": blocked_count,
        "claimable": claimable_count,
    });
    payload["blockers"] = json!(blockers);
    payload["requiredCapabilities"] = json!(required_capabilities);
    Ok(payload)
}

fn tasks_for_scope(
    paths: &MyOpenPanelsPaths,
    scope: &TaskExecutionScope,
    project_id: &str,
) -> Result<Vec<Value>, CliError> {
    let tasks = Storage::open(paths)?
        .list_tasks(project_id)?
        .into_iter()
        .filter(|task| task.get("archivedAt").is_none_or(Value::is_null))
        .collect::<Vec<_>>();
    let tasks = annotate_dispatch_state(paths, project_id, annotate_tasks(tasks))?;
    match scope {
        TaskExecutionScope::ProjectDrain { .. } => Ok(tasks
            .into_iter()
            .filter(|task| !scope_task_terminal(task))
            .collect()),
        TaskExecutionScope::ExactTask { task_id } => Ok(tasks
            .into_iter()
            .filter(|task| task.get("id").and_then(Value::as_str) == Some(task_id))
            .collect()),
        TaskExecutionScope::WikiMutationDrain { mutation_key, .. } => {
            let ids = wiki_scope_task_ids(&tasks, mutation_key);
            let mut scoped = tasks
                .into_iter()
                .filter(|task| {
                    task.get("id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| ids.contains(id))
                })
                .collect::<Vec<_>>();
            scoped.sort_by_key(|task| {
                task.get("mutationSequence")
                    .and_then(Value::as_i64)
                    .unwrap_or(i64::MIN)
            });
            Ok(scoped)
        }
    }
}

fn wiki_scope_task_ids(tasks: &[Value], mutation_key: &str) -> std::collections::BTreeSet<String> {
    let by_id = tasks
        .iter()
        .filter_map(|task| Some((task.get("id")?.as_str()?.to_owned(), task)))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut ids = std::collections::BTreeSet::new();
    let seeds = tasks
        .iter()
        .filter(|task| {
            task.get("mutationKey").and_then(Value::as_str) == Some(mutation_key)
                && !scope_task_terminal(task)
        })
        .filter_map(|task| task.get("id").and_then(Value::as_str));
    for seed in seeds {
        collect_nonterminal_prerequisites(seed, &by_id, &mut ids);
    }
    ids
}

fn collect_nonterminal_prerequisites(
    task_id: &str,
    by_id: &std::collections::BTreeMap<String, &Value>,
    ids: &mut std::collections::BTreeSet<String>,
) {
    if !ids.insert(task_id.to_owned()) {
        return;
    }
    let Some(task) = by_id.get(task_id) else {
        return;
    };
    for dependency_id in task
        .get("dependencies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|dependency| dependency.get("prerequisiteTaskId").and_then(Value::as_str))
    {
        if by_id
            .get(dependency_id)
            .is_some_and(|dependency| !scope_task_terminal(dependency))
        {
            collect_nonterminal_prerequisites(dependency_id, by_id, ids);
        }
    }
}

fn next_wiki_scope_candidate(tasks: &[Value], mutation_key: &str) -> Option<String> {
    let head = tasks
        .iter()
        .filter(|task| {
            task.get("mutationKey").and_then(Value::as_str) == Some(mutation_key)
                && !scope_task_terminal(task)
        })
        .min_by_key(|task| {
            task.get("mutationSequence")
                .and_then(Value::as_i64)
                .unwrap_or(i64::MAX)
        })?;
    if scope_task_claimable(head) {
        return head.get("id").and_then(Value::as_str).map(str::to_owned);
    }
    let by_id = tasks
        .iter()
        .filter_map(|task| Some((task.get("id")?.as_str()?.to_owned(), task)))
        .collect::<std::collections::BTreeMap<_, _>>();
    find_ready_prerequisite(head, &by_id, &mut std::collections::BTreeSet::new())
}

fn find_ready_prerequisite(
    task: &Value,
    by_id: &std::collections::BTreeMap<String, &Value>,
    visited: &mut std::collections::BTreeSet<String>,
) -> Option<String> {
    for dependency_id in task
        .get("dependencies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|dependency| dependency.get("prerequisiteTaskId").and_then(Value::as_str))
    {
        if !visited.insert(dependency_id.to_owned()) {
            continue;
        }
        let Some(dependency) = by_id.get(dependency_id) else {
            continue;
        };
        if scope_task_claimable(dependency) {
            return Some(dependency_id.to_owned());
        }
        if let Some(candidate) = find_ready_prerequisite(dependency, by_id, visited) {
            return Some(candidate);
        }
    }
    None
}

fn scope_task_claimable(task: &Value) -> bool {
    task.get("ready").and_then(Value::as_bool) == Some(true)
        && matches!(
            task.get("status").and_then(Value::as_str),
            Some("queued" | "failed")
        )
}

fn scope_task_terminal(task: &Value) -> bool {
    task.get("archivedAt").is_some_and(|value| !value.is_null())
        || matches!(
            task.get("status").and_then(Value::as_str),
            Some("succeeded" | "cancelled" | "stale" | "superseded" | "archived")
        )
}
