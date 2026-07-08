---
id: wiki.index-document
title: Create Structured Wiki From Source Markdown
source: builtin
appliesTo:
  - wiki
taskTypes:
  - ingest_markdown_into_wiki
requiresCapabilities:
  - wiki.task.claim
  - wiki.source.read
  - wiki.page.list
  - wiki.page.read
  - wiki.page.write
  - wiki.task.complete
loadWhen:
  - Task type is ingest_markdown_into_wiki.
tokens: medium
---

Use this guide when indexing one source markdown document into the structured
wiki.

Workflow:

1. Claim the task.
2. Read the source markdown.
3. Read the target wiki space page index.
4. Read relevant existing pages before changing them.
5. Create or update concise structured pages.
6. Write changed pages with the task id.
7. Complete the task.

Writing rules:

- Use the wiki generation language from current context.
- Preserve useful existing wiki structure.
- Prefer small, focused pages over a single dumping page.
- Include source document ids or source notes when they help future maintenance.
- Update indexes or summary pages when the new information changes navigation.
- Do not rewrite unrelated existing content.

Completion criteria:

- Source markdown is represented in the target wiki space.
- Relevant page index entries are updated by the write commands.
- The task is marked complete.
