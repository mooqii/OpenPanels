---
id: wiki-panel
title: MyOpenPanels Wiki Panel
description: Use before reading, generating, editing, importing, or maintaining content through an MyOpenPanels Wiki panel.
source: builtin
appliesTo:
  - wiki
taskTypes:
requiresCommands:
  - panel.selection.read
  - wiki.raw.read
  - wiki.document.read
  - wiki.document.generate
  - operation.complete
  - wiki.page.search
  - wiki.page.read
  - agent.skill.read
loadWhen:
  - Before beginning a new operation that targets an MyOpenPanels Wiki panel.
tokens: short
---

Use this skill as the required operating contract for an MyOpenPanels Wiki panel.
It defines how to select the right workflow and use the panel reliably. It does
not define how an authoring workflow structures or writes generated Wiki pages.

Intent routing:

- To answer from Wiki knowledge, selected raw documents, or selected generated
  documents, read `references/knowledge-context.md`.
- To create or revise a standalone report, plan, proposal, research summary, or
  specification in the generated-document area, read
  `references/generated-documents.md`.
- To convert a raw document, synthesize Markdown into Wiki pages, or rebuild Wiki
  navigation, read `references/authoring-skill-routing.md`, then load the
  selected Wiki authoring skill for the current task.

Core rules:

- Wiki reads and writes target the Project's Wiki panel without changing the
  active panel. During a claimed v3 Task, the CLI transparently reads the
  Attempt overlay and stages writes through Task Broker; never access storage
  paths directly.
- CLI context, selection, command catalogs, and operation state are authoritative.
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
