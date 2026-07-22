---
name: wiki-default
description: Use when creating, adding to, editing, or maintaining a persistent, structured wiki from curated documents with the default Wiki method.
---

Use this skill to create, extend, edit, or maintain a persistent, compounding
wiki from curated source documents.

Supplied documents remain the source-of-truth layer, while the model
incrementally builds and maintains an interlinked Markdown wiki as the synthesis
layer. The wiki should accumulate knowledge over time instead of rediscovering
it from scratch for every question.

This is a wiki-authoring skill only. It defines how the generated wiki is
created and maintained; reading or using a completed wiki belongs in a separate
skill.

Reference routing:

- When integrating a new source, read
  `references/ingest-markdown-into-wiki.md`.
- When repairing or reorganizing an existing wiki, read
  `references/maintain-wiki.md`.

For every task that writes generated wiki pages, first read
`references/wiki-conventions.md`.

Core rules:

- Treat supplied documents as the source layer; do not mirror whole sources into
  wiki pages.
- Treat the wiki as an LLM-owned generated layer with `SCHEMA.md`, `index.md`,
  `log.md`, and structured entity, concept, comparison, and summary pages.
- Integrate each source into the existing wiki; do not dump isolated notes.
- Do not create pages whose only purpose is to represent one raw document.
- Update cross-links, contradictions, stale claims, and synthesis when new
  evidence changes the picture.
- Keep source provenance on generated pages with stable source identifiers, not
  copied source text.
- Keep `SCHEMA.md`, `index.md`, and `log.md` consistent with the current wiki.
- Do not rewrite or regenerate the whole wiki just because this skill was
  selected. Only update pages needed by the current task.
- Do not invent source content.

Completion criteria:

- The relevant reference steps have been followed.
- The wiki remains navigable through index pages and cross-links.
