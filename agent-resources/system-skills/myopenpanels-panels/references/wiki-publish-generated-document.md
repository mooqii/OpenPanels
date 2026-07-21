# Publish A Generated Wiki Document

Use this reference only when the user explicitly asks to publish a standalone
generated document into the Wiki raw-source layer.

1. Resolve and read exactly one generated document.
2. Resolve the destination Wiki space without changing the visible panel.
3. Run the advertised high-risk `wiki.document.publish` command once.
4. Report the resulting raw document and any downstream Task state.

Publication is distinct from generation and revision. Never publish merely
because a generated document was completed.
