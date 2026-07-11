---
id: wiki.generated-documents
title: Store Generated Documents In Wiki
source: builtin
appliesTo:
  - wiki
taskTypes:
requiresCapabilities:
  - wiki.generation.begin
  - wiki.generation.complete
  - wiki.generatedDocument.read
loadWhen:
  - The Project has a Wiki panel and the agent is producing a standalone document deliverable.
tokens: short
---

Use the generated-documents module for standalone document deliverables such as
reports, plans, proposals, research summaries, and specifications.

- Write document deliverables as UTF-8 Markdown (`.md`) by default. Plain text
  (`.txt`) is accepted only for compatibility.
- Do not register ordinary code changes, temporary notes, chat explanations, or
  non-document outputs.
- Begin a Wiki generation operation before writing the deliverable. For a
  revision, begin against the existing generated document id rather than
  creating a duplicate.
- The CLI creates a visible generating entry and captures the original Project,
  Wiki panel, and content version. The user may switch panels or Projects while
  work continues.
- Complete the operation with the local UTF-8 document file. If the original
  document changed, stop on `content_conflict` rather than overwriting it.
- Mark model/tool failures as failed and user-requested stops as cancelled.
- Publishing into raw Wiki sources remains a separate explicit user action.
