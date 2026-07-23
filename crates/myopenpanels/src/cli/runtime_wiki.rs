fn run_wiki_command(parsed: &Invocation, stdout: &mut impl Write) -> Result<(), CliError> {
    let subcommand = parsed.positionals.get(1).map(String::as_str);
    let action = parsed.positionals.get(2).map(String::as_str);
    match (subcommand, action) {
        (Some("raw"), Some("create")) => {
            let paths = parsed_current_paths(parsed)?;
            let source_file = string_flag(parsed, "source-file");
            let content = if let Some(file) = source_file {
                std::fs::read(file).map_err(|error| CliError::new(error.to_string()))?
            } else {
                required_flag(parsed, "content")?.as_bytes().to_vec()
            };
            let file_name = string_flag(parsed, "file-name")
                .or_else(|| {
                    source_file.and_then(|file| {
                        std::path::Path::new(file)
                            .file_name()
                            .and_then(|value| value.to_str())
                    })
                })
                .or_else(|| string_flag(parsed, "title"))
                .unwrap_or("document.md");
            let result = wiki::add_raw_document(
                &paths,
                file_name,
                string_flag(parsed, "title"),
                string_flag(parsed, "mime-type")
                    .or_else(|| source_file.is_none().then_some("text/markdown")),
                "agent",
                string_flag(parsed, "space-id"),
                &content,
            )?;
            let text = format!(
                "Added raw document {}",
                result["document"]["id"].as_str().unwrap_or("")
            );
            write_result(parsed, stdout, &result, &text)
        }
        (Some("raw"), Some("list")) => {
            let paths = parsed_current_paths(parsed)?;
            let payload = wiki::list_raw_documents(&paths)?;
            let count = payload["documents"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &payload, &format!("{count} document(s)"))
        }
        (Some("raw"), Some("read")) => {
            let paths = parsed_current_paths(parsed)?;
            let document_id = required_flag(parsed, "raw-document-id")?;
            let result = wiki::read_markdown(&paths, document_id)?;
            let text = result["markdown"].as_str().unwrap_or("");
            write_result(parsed, stdout, &result, text)
        }
        (Some("raw"), Some("update")) => {
            let paths = parsed_current_paths(parsed)?;
            let document_id = required_flag(parsed, "raw-document-id")?;
            let file = required_flag(parsed, "content-file")?;
            let content =
                std::fs::read_to_string(file).map_err(|error| CliError::new(error.to_string()))?;
            let result = wiki::write_markdown(
                &paths,
                document_id,
                &content,
                string_flag(parsed, "task-id"),
            )?;
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Wrote markdown {document_id}"),
            )
        }
        (Some("space"), Some("activate")) => {
            let paths = parsed_current_paths(parsed)?;
            let wiki_space_id = required_flag(parsed, "space-id")?;
            let result = wiki::set_active_space(&paths, wiki_space_id)?;
            write_result(
                parsed,
                stdout,
                &result,
                &format!("Active wiki space {wiki_space_id}"),
            )
        }
        (Some("space"), Some("list")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = wiki::list_spaces(&paths)?;
            let count = result["spaces"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} wiki space(s)"))
        }
        (Some("space"), Some("materialize")) => {
            let paths = parsed_current_paths(parsed)?;
            let wiki_space_id = required_flag(parsed, "space-id")?;
            let result = wiki::materialize_wiki_space(&paths, wiki_space_id)?;
            write_result(
                parsed,
                stdout,
                &result,
                result["localAccess"]["rootPath"]
                    .as_str()
                    .unwrap_or("Materialized Wiki"),
            )
        }
        (Some("page"), Some("read")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = wiki::read_page(
                &paths,
                required_flag(parsed, "space-id")?,
                required_flag(parsed, "path")?,
            )?;
            let text = result["markdown"].as_str().unwrap_or("");
            write_result(parsed, stdout, &result, text)
        }
        (Some("page"), Some("search")) => {
            let paths = parsed_current_paths(parsed)?;
            let limit = number_flag(parsed, "limit")?.unwrap_or(20.0) as usize;
            let result = wiki::search_pages(
                &paths,
                required_flag(parsed, "space-id")?,
                required_flag(parsed, "query")?,
                limit,
            )?;
            let count = result["matches"].as_array().map(Vec::len).unwrap_or(0);
            write_result(
                parsed,
                stdout,
                &result,
                &format!("{count} matching page(s)"),
            )
        }
        (Some("page"), Some(mode @ ("create" | "update"))) => {
            let paths = parsed_current_paths(parsed)?;
            let wiki_space_id = required_flag(parsed, "space-id")?;
            let page_path = required_flag(parsed, "path")?;
            let task_id = string_flag(parsed, "task-id");
            if should_check_live_page_existence(
                task_id,
                crate::content::broker_execution_available(),
            ) {
                let pages = wiki::list_pages(&paths, wiki_space_id)?;
                let exists = pages["pages"].as_array().is_some_and(|pages| {
                    pages
                        .iter()
                        .any(|page| page["path"].as_str() == Some(page_path))
                });
                if mode == "create" && exists {
                    return Err(CliError::with_code(
                        "content_conflict",
                        format!("Wiki page already exists: {page_path}"),
                    ));
                }
                if mode == "update" && !exists {
                    return Err(CliError::with_code(
                        "wiki_page_not_found",
                        format!("Wiki page not found: {page_path}"),
                    ));
                }
            }
            let file = required_flag(parsed, "content-file")?;
            let content =
                std::fs::read_to_string(file).map_err(|error| CliError::new(error.to_string()))?;
            let result = wiki::write_page(
                &paths,
                wiki_space_id,
                page_path,
                &content,
                string_flag(parsed, "title"),
                task_id,
            )?;
            write_result(
                parsed,
                stdout,
                &result,
                &format!("{mode}d page {page_path}"),
            )
        }
        (Some("page"), Some("list")) => {
            let paths = parsed_current_paths(parsed)?;
            let result = wiki::list_pages(&paths, required_flag(parsed, "space-id")?)?;
            let count = result["pages"].as_array().map(Vec::len).unwrap_or(0);
            write_result(parsed, stdout, &result, &format!("{count} page(s)"))
        }
        _ => Err(CliError::new("Unknown wiki command.")),
    }
}

fn should_check_live_page_existence(task_id: Option<&str>, broker_available: bool) -> bool {
    task_id.is_none() || !broker_available
}
