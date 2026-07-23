fn run_my_document_command(
    parsed: &Invocation,
    stdout: &mut impl Write,
) -> Result<(), CliError> {
    let action = parsed.positionals.get(1).map(String::as_str);
    let paths = parsed_current_paths(parsed)?;
    match action {
        Some("list") => {
            let result = crate::my_document::list_my_documents(&paths)?;
            let count = result["documents"].as_array().map(Vec::len).unwrap_or(0);
            write_result(
                parsed,
                stdout,
                &result,
                &format!("{count} My Document(s)"),
            )
        }
        Some("import") => {
            let file = required_flag(parsed, "content-file")?;
            let content = fs::read(file).map_err(|error| CliError::new(error.to_string()))?;
            let file_name = std::path::Path::new(file)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("document.md");
            let result = crate::my_document::import_my_document(
                &paths,
                file_name,
                string_flag(parsed, "title"),
                string_flag(parsed, "mime-type"),
                &content,
            )?;
            let id = result["document"]["id"].as_str().unwrap_or_default();
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Imported My Document {id}"),
            )
        }
        Some("read") => {
            let result =
                crate::my_document::read_my_document(
                    &paths,
                    required_flag(parsed, "document-id")?,
                )?;
            let text = result["content"].as_str().unwrap_or("");
            write_result(parsed, stdout, &result, text)
        }
        Some("update") => {
            let document_id = required_flag(parsed, "document-id")?;
            let mut result = if let Some(file) = string_flag(parsed, "content-file") {
                let content = fs::read(file).map_err(|error| CliError::new(error.to_string()))?;
                let file_name = std::path::Path::new(file)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("document.md");
                crate::my_document::write_my_document_for_agent(
                    &paths,
                    document_id,
                    file_name,
                    string_flag(parsed, "mime-type"),
                    &content,
                )?
            } else {
                crate::my_document::read_my_document(&paths, document_id)?
            };
            if let Some(title) = string_flag(parsed, "title") {
                result = crate::my_document::rename_my_document(&paths, document_id, title)?;
            }
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Updated My Document {document_id}"),
            )
        }
        Some("delete") => {
            let document_id = required_flag(parsed, "document-id")?;
            let result = crate::my_document::delete_my_document(&paths, document_id)?;
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Deleted My Document {document_id}"),
            )
        }
        Some("create") => {
            let result = operations::begin_my_document(
                &paths,
                required_flag(parsed, "title")?,
                string_flag(parsed, "document-format").unwrap_or("markdown"),
                None,
            )?;
            write_result(
                parsed,
                stdout,
                &result,
                result["operation"]["id"]
                    .as_str()
                    .unwrap_or("Started My Document creation"),
            )
        }
        Some("revise") => {
            let result = operations::begin_my_document(
                &paths,
                required_flag(parsed, "title")?,
                string_flag(parsed, "document-format").unwrap_or("markdown"),
                Some(required_flag(parsed, "document-id")?),
            )?;
            write_result(
                parsed,
                stdout,
                &result,
                result["operation"]["id"]
                    .as_str()
                    .unwrap_or("Started My Document revision"),
            )
        }
        _ => Err(CliError::new("Unsupported My Document command.")),
    }
}
