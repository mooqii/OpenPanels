# Convert A Raw Document To Markdown

Use this reference for `convert_document_to_markdown` tasks.

Convert the immutable original file identified by the Task into faithful,
readable Markdown. Preserve headings, paragraphs, lists, tables, code, links,
and other meaningful structure when the source format exposes them. Do not
summarize, reorganize, classify, or apply the selected Wiki authoring style.

Write exactly one UTF-8 Source Markdown result through `wiki.raw.update` with
the current Task id. Do not create or modify generated Wiki pages. The selected
Wiki authoring Skill will process the Markdown in a later Task.
