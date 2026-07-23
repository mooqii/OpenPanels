# Use Wiki Knowledge Context

Use this reference to decide whether the current request should read a
MyOpenPanels Wiki or selected My Documents.

Rules:

- In the Writing panel, treat a selected Wiki as one complete indexed knowledge
  source. The Documents panel does not expose Wiki selection, and an open Wiki
  page is not a user-selected page.
- A selected Wiki is available background knowledge, not a requirement to read
  it for every unrelated task.
- If no Wiki is selected, query it only when the user explicitly asks to use the
  Wiki or project knowledge.
- Treat selected My Documents as direct user references. Raw documents
  are source material for the Wiki and are never implicit selected context.
- Do not modify Wiki content while answering a read-only query unless the user
  explicitly asks to preserve the result.

Execution Steps:

1. Read Wiki selection when Wiki or document context may matter.
2. Decide whether the request needs the whole Wiki, selected generated
   documents, or neither.
3. For a Wiki query, list and search the generated pages, then read only the
   pages required to answer. Do not assume any particular entry page, schema,
   index, log, or directory structure.
4. Read selected My Documents through the my-document command.
5. Name the Wiki page paths and document titles used. Distinguish sourced
   knowledge from inference and say plainly when the Wiki lacks evidence.

Do not read every page or silently substitute a selected Wiki for an explicitly
selected My Document.

Outside a claimed Task, a selected Wiki is also exposed as a complete local
Markdown tree. Read it directly only when `wiki.localAccess.status` is `ready`.
For an unselected Wiki, run the returned materialize action before using its
root path. During a claimed Task, never use this live tree; use task-scoped CLI
reads so the Attempt base revision and overlay remain authoritative.
