fn run_wiki_source_command(
    parsed: &Invocation,
    stdout: &mut impl Write,
) -> Result<(), CliError> {
    let paths = parsed_current_paths(parsed)?;
    match parsed.intent() {
        "wiki-source.create-from-my-document" => {
            let document_id = required_flag(parsed, "document-id")?;
            let result = crate::my_document::publish_my_document(
                &paths,
                document_id,
                string_flag(parsed, "space-id"),
            )?;
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Created Wiki Source from My Document {document_id}"),
            )
        }
        _ => Err(CliError::new("Unknown Wiki Source command.")),
    }
}
