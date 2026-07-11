---
id: wiki.knowledge-context
title: Use Wiki Knowledge Context
source: builtin
appliesTo:
  - wiki
taskTypes:
requiresCapabilities:
  - wiki.selection.read
  - agent.skill.read
  - wiki.markdown.read
  - wiki.generatedDocument.read
  - wiki.page.search
  - wiki.page.read
loadWhen:
  - The Project exposes a Wiki or selected raw documents as agent knowledge context.
tokens: short
---

Use this guide to decide whether the current request should read OpenPanels Wiki
knowledge, user-selected raw documents, or user-selected generated documents.

Knowledge context rules:

- Treat a selected Wiki as one complete, indexed knowledge source. Do not treat
  an open Wiki page as a user-selected page.
- A selected Wiki is available background knowledge, not a requirement to read
  it for every task.
- Load the CLI-advertised `wiki-query` skill only when the request genuinely
  needs project, product, domain, or research knowledge, or when the user
  explicitly asks to use the Wiki.
- Do not query the Wiki for unrelated coding, terminal, or file operations.
- Treat selected raw documents as direct user references that can be read
  individually without querying the whole Wiki.
- Treat selected generated documents as direct user references and read them
  with `wiki generated-documents read --document-id <id>`.
- If no Wiki is selected, query it only when the user explicitly asks to use
  the Wiki or project knowledge.

Workflow:

1. Read `wiki selection read` when Wiki or raw-document context may matter.
2. Decide whether the request needs the whole Wiki, selected raw documents,
   selected generated documents, or neither.
3. Load `wiki-query` only when querying the Wiki. Follow that skill for index,
   search, page reads, citations, and read-only behavior.
4. Read selected raw documents directly with `wiki markdown read` when the user
   refers to those documents. Use the original local file path returned by the
   selection command when a format-specific file tool is required.
5. Read selected generated documents with `wiki generated-documents read` when
   the user refers to those documents.

Do not infer a user selection from the currently open Wiki page or raw-document
preview. An open generated-document viewer is not a selection either.
