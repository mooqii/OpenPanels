fn run_asset_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let paths = parsed_current_paths(parsed)?;
    match parsed.intent() {
        "asset.list" => {
            let bootstrap = ensure_project_bootstrap(&paths, BootstrapRequest::new())?;
            let assets = Storage::open(&paths)?.list_assets(&bootstrap.project.id)?;
            write_result(
                parsed,
                stdout,
                &serde_json::json!({ "assets": assets }),
                &format!("{} Asset(s)", assets.len()),
            )
        }
        _ => Err(CliError::new("Unknown asset command.")),
    }
}
