---
id: wiki.rebuild-index
title: Rebuild Wiki Index
source: builtin
appliesTo:
  - wiki
taskTypes:
  - rebuild_wiki_index
requiresCapabilities:
  - wiki.task.claim
  - wiki.page.list
  - wiki.page.read
  - wiki.page.write
  - wiki.task.complete
loadWhen:
  - Task type is rebuild_wiki_index.
tokens: medium
---

Use this guide when wiki navigation, summaries, or index pages need to be
rebuilt.

Workflow:

1. Claim the task.
2. List pages in the target wiki space.
3. Read index or overview pages that summarize the space.
4. Update navigation, summaries, tags, and cross-links as needed.
5. Write changed pages with the task id.
6. Complete the task.

Rules:

- Keep summaries concise and scannable.
- Preserve user-authored structure when it is still useful.
- Do not rewrite all pages just to normalize style.

Completion criteria:

- Index/navigation pages reflect the current wiki content.
- The task is marked complete.
