fn conversion_output_target(task: &Value) -> (&'static str, &'static str, &'static str) {
    let is_my_document = task
        .pointer("/input/documentKind")
        .or_else(|| task.get("documentKind"))
        .and_then(Value::as_str)
        == Some("my_document");
    if is_my_document {
        (
            crate::content::ResourceKind::MyDocument.as_str(),
            "content.md",
            "my_document",
        )
    } else {
        (
            crate::content::ResourceKind::WikiMarkdown.as_str(),
            "source.md",
            "raw",
        )
    }
}
