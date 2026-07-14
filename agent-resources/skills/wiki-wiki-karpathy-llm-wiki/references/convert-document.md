# Convert Source Document To Markdown

Use this reference when a raw source document needs a markdown representation
before it can be integrated into the generated wiki.

Workflow:

1. Claim the task.
2. Inspect the source document metadata and available original file.
3. Convert the source into clean markdown.
4. Preserve headings, tables, lists, code blocks, citations, and useful media or
   attachment placeholders when possible.
5. Write the markdown source with the task id.
6. Complete the task.

Rules:

- Do not invent content that is not present in the source.
- Keep extraction notes concise.
- Preserve enough structure that a later ingest can extract durable knowledge
  into the generated wiki.
- This task writes only the raw document's Markdown representation. Do not
  create generated wiki pages here; the follow-up ingest task does that.
- If conversion cannot be completed, fail the task with a clear message.

Completion criteria:

- The raw document has markdown content.
- The task is completed or failed with an actionable error.
