---
id: wiki.convert-document
title: Convert Source Document To Markdown
source: builtin
appliesTo:
  - wiki
taskTypes:
  - convert_document_to_markdown
requiresCapabilities:
  - wiki.task.claim
  - wiki.source.write
  - wiki.task.complete
  - wiki.task.fail
loadWhen:
  - Task type is convert_document_to_markdown.
tokens: medium
---

Use this guide when a raw source document needs a markdown representation before
wiki ingestion.

Workflow:

1. Claim the task.
2. Inspect the source document metadata and available original file.
3. Convert the source into clean markdown.
4. Preserve headings, tables, lists, code blocks, and citations when possible.
5. Write the markdown source with the task id.
6. Complete the task.

Rules:

- Do not invent content that is not present in the source.
- Keep extraction notes concise.
- If conversion cannot be completed, fail the task with a clear message.

Completion criteria:

- The raw document has markdown content.
- The task is completed or failed with an actionable error.
