---
id: wiki-query
title: Wiki Query
description: Use only when an agent genuinely needs to read the OpenPanels Wiki for project, product, domain, or research knowledge, or when the user explicitly asks to use the Wiki.
source: builtin
appliesTo:
  - wiki
taskTypes:
requiresCapabilities:
  - wiki.selection.read
  - wiki.page.list
  - wiki.page.search
  - wiki.page.read
  - wiki.markdown.read
loadWhen:
  - The request needs knowledge from the selected Wiki.
  - The user explicitly asks to answer from the Wiki or project knowledge.
tokens: short
---

Use this skill only when the current request genuinely requires knowledge from
the OpenPanels Wiki. A selected Wiki is an available knowledge source, not a
requirement to read it for unrelated coding, terminal, or file operations.

This is a read-only query skill. Do not create or update Wiki pages, `index.md`,
or `log.md` unless the user explicitly asks to preserve the result. Wiki
authoring and maintenance belong to the selected Wiki authoring skill.

Workflow:

1. Read `wiki selection read` and confirm whether the whole Wiki or individual
   raw documents were selected by the user.
2. When querying the Wiki, read `SCHEMA.md` and `index.md` from the selected
   Wiki space before drilling into content. Read recent `log.md` entries only
   when recent changes matter to the question.
3. Use `wiki pages search` for relevant terms, then read only the pages needed
   to answer. The Wiki is selected as a whole; do not treat an open page as a
   user-selected page.
4. Treat selected raw documents as direct user references. Read their Markdown
   with `wiki markdown read`; use the returned original local file path when a
   format-specific file tool is required.
5. Synthesize the answer and name the Wiki page paths and raw document titles
   used. Distinguish sourced Wiki knowledge from inference.

Rules:

- If no Wiki is selected, query it only when the user explicitly asks to use
  the Wiki or project knowledge.
- Do not read every page. Start from the index and search results.
- Do not silently substitute the Wiki for an explicitly selected raw document.
- Do not modify source documents or generated Wiki pages during a query.
- If the selected Wiki does not contain enough evidence, say so plainly and
  continue with other sources only when the user allows it.

Completion criteria:

- Only relevant Wiki pages or selected raw documents were read.
- The answer identifies the local knowledge sources it used.
- No Wiki content was changed unless the user explicitly requested a write.
