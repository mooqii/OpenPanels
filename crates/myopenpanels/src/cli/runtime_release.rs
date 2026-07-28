fn run_release_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    match parsed.intent() {
        "release.list" => {
            let paths = parsed_paths(parsed)?;
            let bootstrap = ensure_project_bootstrap(&paths, BootstrapRequest::new())?;
            let releases = Storage::open(&paths)?.list_releases(&bootstrap.project.id)?;
            write_result(
                parsed,
                stdout,
                &serde_json::json!({ "releases": releases }),
                &format!("{} Release(s)", releases.len()),
            )
        }
        "release.checkpoint" => {
            let paths = parsed_paths(parsed)?;
            let task_id = required_flag(parsed, "task-id")?;
            let phase = required_flag(parsed, "phase")?;
            let result = crate::release::checkpoint_attempt(&paths, task_id, phase)?;
            write_result(parsed, stdout, &result, &format!("Publishing attempt {phase}"))
        }
        "release.wechat.draft" => {
            let task_id = required_flag(parsed, "task-id")?;
            let result = crate::release::save_wechat_draft_for_claimed_task(task_id)?;
            write_result(
                parsed,
                stdout,
                &result,
                result
                    .get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or("WeChat draft API request completed"),
            )
        }
        _ => Err(CliError::new("Unknown release command.")),
    }
}
