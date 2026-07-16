---
id: karpathy-llm-wiki-zh
title: Karpathy LLM Wiki 中文
description: Use when creating, adding to, editing, or maintaining a persistent, structured Karpathy-style LLM wiki and generating new wiki content in Chinese.
source: builtin
appliesTo:
  - wiki
taskTypes:
  - ingest_markdown_into_wiki
  - maintain_wiki
requiresCommands:
  - task.claim
  - task.heartbeat
  - task.complete
  - task.fail
  - wiki.raw.list
  - wiki.raw.read
  - wiki.raw.update
  - wiki.page.list
  - wiki.page.read
  - wiki.page.create
  - wiki.page.update
loadWhen:
  - The current wiki task should maintain a Karpathy-style LLM-generated wiki in Chinese.
tokens: medium
---

Use this skill when the current MyOpenPanels wiki task should create, add to,
edit, or maintain a persistent, compounding wiki from curated source documents,
with newly generated wiki content written in Chinese.

This skill follows the pattern from Andrej Karpathy's LLM Wiki idea, adapted for
MyOpenPanels: the left-side raw document list remains the source-of-truth layer,
while the LLM incrementally builds and maintains an interlinked markdown wiki as
the generated synthesis layer. The wiki should accumulate synthesis over time
instead of rediscovering knowledge from scratch for every question.

This is a wiki-authoring skill only. It defines how the generated wiki is
created and maintained; reading or using a completed wiki belongs in a separate
skill.

Task routing:

- `ingest_markdown_into_wiki`: read `references/ingest-markdown-into-wiki.md`.
- `maintain_wiki`: read `references/rebuild-index.md`.

For every task that writes generated wiki pages, first read
`references/wiki-conventions.md`.

Core rules:

- Write newly generated wiki pages, summaries, index entries, and log entries in
  Chinese.
- Treat the left-side raw document list as the raw source layer; do not mirror
  raw sources into wiki pages or translate raw source content unless a task
  requires a generated summary or wiki page.
- Treat the wiki as an LLM-owned generated layer with `SCHEMA.md`, `index.md`,
  `log.md`, and structured entity, concept, comparison, and summary pages.
- Integrate each source into the existing wiki; do not dump isolated notes.
- Do not create pages whose only purpose is to represent one raw document.
- Update cross-links, contradictions, stale claims, and synthesis when new
  evidence changes the picture.
- Keep source provenance on generated pages with raw document ids, not copied
  raw Markdown.
- Keep `SCHEMA.md`, `index.md`, and `log.md` consistent with the current wiki.
- Do not rewrite, translate, or regenerate the whole wiki just because this
  skill was selected. Only update pages needed by the current task.
- Do not invent source content.

Completion criteria:

- The selected task-specific reference workflow has been followed.
- Newly generated wiki content for this task is in Chinese.
- The wiki remains navigable through index pages and cross-links.
- All relevant markdown source or wiki page writes include the current task id.
- The task is marked complete, or failed with an actionable error.
