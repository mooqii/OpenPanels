//! Project-level My Documents module.
//!
//! Content mutation rules live here and commit against the document resource.
//! Catalog creation, import, rename, content updates, and deletion commit
//! directly against that resource. Wiki only supplies the composed read model.

use crate::error::CliError;
use crate::paths::sanitize_file_name;
use serde_json::{json, Value};
use std::path::Path;

pub use crate::wiki::{
    create_my_document, delete_my_document, import_my_document, list_my_documents,
    my_document_import_original, my_document_import_original_for_target, read_my_document,
    rename_my_document, rename_my_document_file, reveal_my_document_import_original,
    write_my_document, write_my_document_for_agent,
};

pub(crate) use crate::wiki::{
    prepare_begin_my_document_for_target, prepare_complete_my_document_for_target,
    prepare_finish_my_document_operation, prepare_pending_writing_document_removal,
};

pub fn publish_my_document(
    paths: &crate::paths::MyOpenPanelsPaths,
    document_id: &str,
    wiki_space_id: Option<&str>,
) -> Result<Value, CliError> {
    crate::wiki::publish_my_document_into_wiki(paths, document_id, wiki_space_id)
}

pub(crate) struct ContentUpdate<'a> {
    pub(crate) expected_version: u64,
    pub(crate) committed_version: u64,
    pub(crate) content_ref: &'a str,
    pub(crate) format: &'a str,
    pub(crate) mime_type: &'a str,
    pub(crate) original_file_name: Option<&'a str>,
    pub(crate) title: Option<&'a str>,
    pub(crate) content: &'a [u8],
    pub(crate) required_operation_id: Option<&'a str>,
    pub(crate) clear_write_operation: bool,
    pub(crate) updated_at: &'a str,
}

pub(crate) fn document_format(
    file_name: &str,
    mime_type: Option<&str>,
) -> Result<(&'static str, &'static str), CliError> {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "md" | "markdown"
            if mime_type.is_none_or(|value| value == "text/markdown" || value == "text/plain") =>
        {
            Ok(("markdown", "text/markdown"))
        }
        "txt" if mime_type.is_none_or(|value| value == "text/plain") => Ok(("text", "text/plain")),
        _ => Err(CliError::with_code(
            "invalid_my_document",
            "My Documents must be UTF-8 .md, .markdown, or .txt files.",
        )),
    }
}

pub(crate) fn my_document_content_descriptor(
    file_name: &str,
) -> Result<(&'static str, &'static str, &'static str), CliError> {
    let (format, mime_type) = document_format(file_name, None)?;
    let content_ref = if format == "markdown" {
        "content.md"
    } else {
        "content.txt"
    };
    Ok((format, mime_type, content_ref))
}

pub(crate) fn apply_content_update(
    document: &mut Value,
    update: ContentUpdate<'_>,
) -> Result<(), CliError> {
    let current_version = document["contentVersion"].as_u64().unwrap_or(0);
    if current_version != update.expected_version {
        return Err(CliError::with_code(
            "content_conflict",
            format!(
                "My Document changed from version {} to {current_version}",
                update.expected_version
            ),
        ));
    }
    if update.committed_version != update.expected_version + 1 {
        return Err(CliError::with_code(
            "content_conflict",
            "Prepared My Document content has an unexpected version.",
        ));
    }
    if let Some(operation_id) = update.required_operation_id {
        if document
            .pointer("/writeOperation/operationId")
            .and_then(Value::as_str)
            != Some(operation_id)
        {
            return Err(CliError::with_code(
                "operation_target_mismatch",
                "The My Document is no longer bound to this Direct Operation.",
            ));
        }
    }
    let text = std::str::from_utf8(update.content).map_err(|_| {
        CliError::with_code(
            "invalid_my_document",
            "My Document content must be valid UTF-8.",
        )
    })?;
    match (update.format, update.mime_type) {
        ("markdown", "text/markdown") | ("text", "text/plain") => {}
        _ => {
            return Err(CliError::with_code(
                "invalid_my_document",
                "My Document format and media type do not match.",
            ))
        }
    }
    if let Some(title) = update
        .title
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        document["title"] = json!(title);
    }
    document["contentRef"] = json!(update.content_ref);
    document["contentVersion"] = json!(update.committed_version);
    document["format"] = json!(update.format);
    document["mimeType"] = json!(update.mime_type);
    if let Some(file_name) = update.original_file_name {
        document["originalFileName"] = json!(sanitize_file_name(file_name));
    }
    document["wordCount"] = json!(text
        .chars()
        .filter(|character| !character.is_whitespace())
        .count());
    if update.clear_write_operation {
        document
            .as_object_mut()
            .map(|object| object.remove("writeOperation"));
    }
    document["updatedAt"] = json!(update.updated_at);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_update_owns_canonical_document_fields() {
        let mut document = json!({
            "id": "document:1",
            "title": "Draft",
            "contentVersion": 2,
            "writeOperation": { "operationId": "operation:1" }
        });
        apply_content_update(
            &mut document,
            ContentUpdate {
                expected_version: 2,
                committed_version: 3,
                content_ref: "content.md",
                format: "markdown",
                mime_type: "text/markdown",
                original_file_name: Some("report.markdown"),
                title: Some("Report"),
                content: "# Report\n\nHello 世界".as_bytes(),
                required_operation_id: Some("operation:1"),
                clear_write_operation: true,
                updated_at: "2026-01-01T00:00:00.000Z",
            },
        )
        .expect("content update");

        assert_eq!(document["title"], "Report");
        assert_eq!(document["contentVersion"], 3);
        assert_eq!(document["format"], "markdown");
        assert_eq!(document["mimeType"], "text/markdown");
        assert_eq!(document["originalFileName"], "report.markdown");
        assert_eq!(document["wordCount"], 14);
        assert!(document.get("writeOperation").is_none());
    }

    #[test]
    fn content_update_rejects_a_stale_document_version() {
        let mut document = json!({ "contentVersion": 3 });
        let error = apply_content_update(
            &mut document,
            ContentUpdate {
                expected_version: 2,
                committed_version: 3,
                content_ref: "content.md",
                format: "markdown",
                mime_type: "text/markdown",
                original_file_name: None,
                title: None,
                content: b"content",
                required_operation_id: None,
                clear_write_operation: false,
                updated_at: "2026-01-01T00:00:00.000Z",
            },
        )
        .expect_err("stale update");
        assert_eq!(error.code(), Some("content_conflict"));
    }
}
