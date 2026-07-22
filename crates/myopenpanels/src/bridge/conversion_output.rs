fn conversion_output_target(task: &Value) -> (&'static str, &'static str, &'static str) {
    let generated = task
        .pointer("/input/documentKind")
        .or_else(|| task.get("documentKind"))
        .and_then(Value::as_str)
        == Some("generated");
    if generated {
        (
            crate::content::ResourceKind::GeneratedDocument.as_str(),
            "content.md",
            "generated",
        )
    } else {
        (
            crate::content::ResourceKind::WikiMarkdown.as_str(),
            "source.md",
            "raw",
        )
    }
}
