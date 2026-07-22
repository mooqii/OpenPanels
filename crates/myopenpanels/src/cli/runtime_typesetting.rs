fn run_typesetting_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let paths = parsed_paths(parsed)?;
    match parsed.intent() {
        "typesetting.title.skill.list" => {
            let skills = crate::typesetting::title_skills(&paths)?;
            write_result(parsed, stdout, &skills, &format!("{} Title Skills", skills.len()))
        }
        "typesetting.title.generate" => {
            let publication_id = required_flag(parsed, "publication-id")?;
            let skill_id = required_flag(parsed, "skill-id")?;
            let request_id = string_flag(parsed, "request-id")
                .map(str::to_owned)
                .unwrap_or_else(|| crate::ids::random_id("title-request"));
            let instruction = string_flag(parsed, "instruction").unwrap_or_default();
            let task = crate::typesetting::create_title_request(
                &paths,
                publication_id,
                skill_id,
                instruction,
                &request_id,
            )?;
            let task_id = task
                .pointer("/task/id")
                .and_then(Value::as_str)
                .unwrap_or("created");
            write_result(
                parsed,
                stdout,
                &task,
                &format!("Created title generation Task {task_id}"),
            )
        }
        _ => Err(CliError::new("Unknown typesetting command.")),
    }
}
