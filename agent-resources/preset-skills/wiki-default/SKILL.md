---
name: wiki-default
description: Use when creating, adding to, editing, or maintaining a persistent, structured wiki from curated documents with the default Wiki method.
---

Build and maintain a persistent, compounding wiki from curated source
documents.

Supplied documents remain the source-of-truth layer, while the model
incrementally builds and maintains an interlinked Markdown wiki as the synthesis
layer. The wiki should accumulate knowledge over time instead of rediscovering
it from scratch for every question.

This Skill governs wiki authoring and maintenance, not answering questions from
a completed wiki.

Read the applicable references before writing:

- Always read `references/wiki-conventions.md`.
- For a new source, also read `references/ingest-markdown-into-wiki.md`.
- For repair or reorganization, also read `references/maintain-wiki.md`.

Core rules:

- Treat supplied documents as the source-of-truth layer. Never create raw-source
  mirrors inside the generated wiki.
- Maintain `SCHEMA.md`, `index.md`, `log.md`, and focused entity, concept,
  comparison, and summary pages as one coherent knowledge graph.
- Integrate each source into the existing wiki; do not dump isolated notes.
- Do not create pages whose only purpose is to represent one raw document.
- Update cross-links, contradictions, stale claims, and synthesis when new
  evidence changes the picture.
- Keep source provenance on generated pages with stable source identifiers, not
  copied source text.
- Keep `SCHEMA.md`, `index.md`, and `log.md` consistent with the current wiki.
- Make the smallest coherent update. Do not regenerate unrelated pages.
- Do not invent source content.

Finish only when the affected knowledge is synthesized, provenance is visible,
foundation pages are consistent, and every changed page is discoverable through
the index or meaningful cross-links.
