---
id: wiki.task-intake
title: Inspect And Claim Wiki Tasks
source: builtin
appliesTo:
  - wiki
taskTypes:
requiresCapabilities:
  - wiki.task.list
  - wiki.task.next
  - wiki.task.claim
loadWhen:
  - The wiki panel has queued, failed, claimed, or running tasks.
tokens: short
---

Use this guide when deciding what wiki work to do next.

Workflow:

1. Read the next task.
2. Load the task-specific guide for the task type.
3. Claim the task only when you are ready to work on it.
4. Keep the task id attached to subsequent writes.
5. Complete the task after all required state has been written.

Task-to-guide routing:

- `convert_document_to_markdown` -> `wiki.convert-document`
- `ingest_markdown_into_wiki` -> `wiki.index-document`
- `rebuild_wiki_index` -> `wiki.rebuild-index`

Completion criteria:

- The agent has selected a task and loaded the matching guide before making
  changes.
