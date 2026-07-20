---
name: myopenpanels-wiki-panel
description: Use before reading, generating, editing, importing, or maintaining content through a MyOpenPanels Wiki panel.
---

Use this skill as the required operating contract for an MyOpenPanels Wiki panel.
It defines how to select the right Procedure and use the panel reliably. It does
not define how an authoring Skill structures or writes generated Wiki pages.

Intent routing:

- To answer from Wiki knowledge, selected raw documents, or selected generated
  documents, read `references/knowledge-context.md`.
- To import a source into the raw-document layer, read
  `references/import-raw-document.md`.
- To read a standalone generated document, read
  `references/generated-document-read.md`.
- To create a standalone report, plan, proposal, research summary, or
  specification, read
  `references/generated-documents.md`.
- To revise an existing standalone document, read
  `references/revise-generated-document.md`.
- To publish a generated document into raw Wiki sources, read
  `references/publish-generated-document.md`.
- To delete a generated document, read
  `references/delete-generated-document.md`.
- To list, activate, or materialize Wiki spaces, read
  `references/wiki-space-management.md`.
- To convert a raw document, read `references/convert-document.md`.
- To synthesize Markdown into Wiki pages or maintain generated Wiki content,
  read `references/authoring-skill-routing.md`, then load the selected Wiki
  authoring skill for the current task.

Core rules:

- Wiki reads and writes target the Project's Wiki panel without changing the
  active panel. During a claimed v3 Task, the CLI transparently reads the
  Attempt overlay and stages writes through Task Broker; never access storage
  paths directly.
- CLI context, selection, command catalogs, and operation state are authoritative.
- Outside a claimed Task, selection access metadata may expose verified local
  Markdown paths. Read commands still return the content directly; use local
  paths for oversized content or file-oriented tools.
- A Wiki `localAccess` value is safe to read only when its status is `ready`.
  Run its materialize action when the status is `on_demand`.
- Selection is the exception to panel-kind targeting: read it only when Wiki is
  the active panel.
- Do not infer a user selection from the currently open page or preview.
- Treat raw documents, generated Wiki pages, and generated documents as distinct
  content layers.
- Use commands advertised by the current CLI instead of remembered syntax.
- Start target-bound generation before producing a standalone document.
- Keep task and generation lifecycle failures explicit; do not silently skip a
  required completion, failure, or cancellation step.
- An operation already in progress remains bound to its captured Project and
  panel even if the user changes the active panel.

Completion criteria:

- The relevant reference and, when required, selected authoring skill were
  loaded.
- Writes used the task or generation operation that owns them.
- The result is visible in the intended Wiki panel target.
