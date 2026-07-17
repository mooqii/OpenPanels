# Use Wiki Knowledge Context

Use this reference to decide whether the current request should read an
MyOpenPanels Wiki, selected raw documents, or selected generated documents.

Rules:

- Treat a selected Wiki as one complete indexed knowledge source. An open Wiki
  page is not a user-selected page.
- A selected Wiki is available background knowledge, not a requirement to read
  it for every unrelated task.
- If no Wiki is selected, query it only when the user explicitly asks to use the
  Wiki or project knowledge.
- Treat selected raw and generated documents as direct user references.
- Do not modify Wiki content while answering a read-only query unless the user
  explicitly asks to preserve the result.

Workflow:

1. Read Wiki selection when Wiki or document context may matter.
2. Decide whether the request needs the whole Wiki, selected raw documents,
   selected generated documents, or neither.
3. For a Wiki query, list and search the generated pages, then read only the
   pages required to answer. Do not assume any particular entry page, schema,
   index, log, or directory structure.
4. Read selected raw documents through their Markdown representation. Use the
   returned Markdown path only for oversized content or file-oriented tools,
   and use the original path only when a format-specific file tool is required.
5. Read selected generated documents through the generated-document command.
6. Name the Wiki page paths and document titles used. Distinguish sourced
   knowledge from inference and say plainly when the Wiki lacks evidence.

Do not read every page or silently substitute a selected Wiki for an explicitly
selected document.

Outside a claimed Task, a selected Wiki is also exposed as a complete local
Markdown tree. Read it directly only when `wiki.localAccess.status` is `ready`.
For an unselected Wiki, run the returned materialize action before using its
root path. During a claimed Task, never use this live tree; use task-scoped CLI
reads so the Attempt base revision and overlay remain authoritative.
