fn run_publishing_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let paths = parsed_paths(parsed)?;
    match parsed.intent() {
        "publishing.checkpoint" => {
            let task_id = required_flag(parsed, "task-id")?;
            let phase = required_flag(parsed, "phase")?;
            let result = crate::publishing::checkpoint_attempt(&paths, task_id, phase)?;
            write_result(parsed, stdout, &result, &format!("Publishing attempt {phase}"))
        }
        _ => Err(CliError::new("Unknown publishing command.")),
    }
}
