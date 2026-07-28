---
name: wiki-default
description: Use when creating, adding to, editing, or maintaining a persistent, structured wiki from curated documents with the default Wiki method.
---

Build and maintain a persistent, compounding, multi-topic wiki from curated
source documents.

Supplied documents remain the source-of-truth layer, while the model
incrementally builds and maintains an interlinked Markdown wiki as the synthesis
layer. The wiki should accumulate knowledge over time instead of rediscovering
it from scratch for every question. It may begin with one source and one topic,
then grow to include unrelated topics without requiring a redesign.

This Skill governs wiki authoring and maintenance, not answering questions from
a completed wiki.

Read the applicable references before writing:

- Always read `references/wiki-conventions.md`.
- For a new source, also read `references/ingest-markdown-into-wiki.md`.
- For repair or reorganization, also read `references/maintain-wiki.md`.

Core rules:

- Treat supplied documents as the source-of-truth layer. Never create raw-source
  mirrors inside the generated wiki.
- Store the generated wiki as clean Markdown only. Do not depend on a database,
  vector index, sidecar manifest, or another generated data structure.
- Model durable real-world subjects, not the outline or schema of a source
  document.
- Maintain `SCHEMA.md`, `index.md`, and focused topic, entity, and view pages as
  one coherent knowledge graph.
- Integrate each source into the existing wiki; do not dump isolated notes.
- Do not create pages whose only purpose is to represent one raw document.
- Keep one canonical page per durable subject. Separate stable identity or
  definition from dated observations, changing states, and source claims.
- Prefer explicit relationship labels and composition through links and tags
  over vague "related" lists, deep hierarchies, or duplicated pages.
- Let unrelated durable topics coexist. Do not force later sources into the
  domain inferred from the first source.
- Write a new page primarily in the dominant natural language of its source
  material. Preserve an existing page's established language when updating it,
  and record useful names in other languages as aliases.
- Update cross-links, contradictions, stale claims, and synthesis when new
  evidence changes the picture.
- Keep source provenance on generated pages with stable source identifiers, not
  copied source text.
- Keep `SCHEMA.md` and `index.md` consistent with the current wiki.
- Make the smallest coherent update. Do not regenerate unrelated pages.
- Do not invent source content.

Finish only when the affected knowledge is synthesized, provenance is visible,
foundation pages are consistent, page language is coherent, and every changed
page is discoverable through the index or meaningful cross-links.
