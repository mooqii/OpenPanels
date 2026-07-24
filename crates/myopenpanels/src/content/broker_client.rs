pub fn broker_stage_file(request: &StageFileRequest) -> Result<Value, CliError> {
    broker_json("/api/task-broker/v3/stage", request)
}

pub fn broker_read_file(request: &ReadFileRequest) -> Result<Value, CliError> {
    broker_json("/api/task-broker/v3/read", request)
}

pub fn broker_prepare_skill(request: &PrepareSkillRequest) -> Result<Value, CliError> {
    broker_json("/api/task-broker/v3/skills/prepare", request)
}

pub fn broker_task_context(request: &TaskContextRequest) -> Result<Value, CliError> {
    broker_json("/api/task-broker/v3/task-context", request)
}

pub fn broker_read_skill(request: &SkillReadRequest) -> Result<Value, CliError> {
    broker_json("/api/task-broker/v3/skills/read", request)
}

pub fn broker_publishing_checkpoint(
    request: &PublishingCheckpointRequest,
) -> Result<Value, CliError> {
    broker_json("/api/task-broker/v3/publishing/checkpoint", request)
}

#[cfg(test)]
thread_local! {
    static TEST_TASK_BROKER_ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(crate) struct TestTaskBrokerGuard;

#[cfg(test)]
impl Drop for TestTaskBrokerGuard {
    fn drop(&mut self) {
        TEST_TASK_BROKER_ENABLED.set(false);
    }
}

#[cfg(test)]
pub(crate) fn enable_test_task_broker() -> TestTaskBrokerGuard {
    TEST_TASK_BROKER_ENABLED.set(true);
    TestTaskBrokerGuard
}

pub(crate) fn task_broker_url_for_claim() -> Option<String> {
    #[cfg(test)]
    {
        return TEST_TASK_BROKER_ENABLED
            .get()
            .then(|| "http://127.0.0.1:9".to_owned());
    }
    #[cfg(not(test))]
    {
        std::env::var("MYOPENPANELS_TASK_BROKER_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
    }
}

pub fn broker_execution_available() -> bool {
    std::env::var("MYOPENPANELS_TASK_BROKER_URL").is_ok_and(|value| !value.trim().is_empty())
        && std::env::var("MYOPENPANELS_TASK_TOKEN").is_ok_and(|value| !value.trim().is_empty())
}

pub fn task_execution_detected() -> bool {
    std::env::var("MYOPENPANELS_TASK_ID").is_ok_and(|value| !value.trim().is_empty())
}

pub fn require_broker_for_task_execution() -> Result<(), CliError> {
    if task_execution_detected() && !broker_execution_available() {
        return Err(CliError::with_code(
            "broker_unavailable",
            "Task-scoped content access requires the Studio Task Broker.",
        ));
    }
    Ok(())
}

fn broker_json<T: Serialize>(path: &str, body: &T) -> Result<Value, CliError> {
    let url = std::env::var("MYOPENPANELS_TASK_BROKER_URL").map_err(|_| {
        CliError::with_code(
            "broker_unavailable",
            "This v3 Task requires a running Studio Task Broker.",
        )
    })?;
    let token = std::env::var("MYOPENPANELS_TASK_TOKEN").map_err(|_| {
        CliError::with_code(
            "broker_unavailable",
            "The Task Broker execution token is missing.",
        )
    })?;
    let response = ureq::post(&format!("{}{}", url.trim_end_matches('/'), path))
        .set("authorization", &format!("Bearer {token}"))
        .set("content-type", "application/json")
        .send_json(serde_json::to_value(body).map_err(to_cli_error)?);
    match response {
        Ok(response) => response.into_json::<Value>().map_err(to_cli_error),
        Err(ureq::Error::Status(_, response)) => {
            let payload = response.into_json::<Value>().unwrap_or_else(|_| json!({}));
            Err(CliError::with_code(
                payload
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or("broker_rejected"),
                payload
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("Task Broker rejected the request."),
            ))
        }
        Err(error) => Err(CliError::with_code("broker_unavailable", error.to_string())),
    }
}
