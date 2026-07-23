fn run_release_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let paths = parsed_paths(parsed)?;
    match parsed.intent() {
        "release.list" => {
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
            let task_id = required_flag(parsed, "task-id")?;
            let phase = required_flag(parsed, "phase")?;
            let result = crate::release::checkpoint_attempt(&paths, task_id, phase)?;
            write_result(parsed, stdout, &result, &format!("Publishing attempt {phase}"))
        }
        _ => Err(CliError::new("Unknown release command.")),
    }
}
