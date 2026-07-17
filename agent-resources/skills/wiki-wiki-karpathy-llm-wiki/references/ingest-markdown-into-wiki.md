# Ingest Source Markdown Into A Karpathy LLM Wiki

Use this reference when integrating one source markdown document into the
structured wiki.

Workflow:

1. Claim the task only when lifecycle is not bridge-managed.
2. Read the source markdown.
3. Read `wiki-conventions.md`, list the target wiki space's pages, and inspect
   `SCHEMA.md`, `index.md`, `log.md`, plus any existing pages likely to change.
4. If this is the first ingest, establish `SCHEMA.md`, `index.md`, and `log.md`
   before adding knowledge pages. Infer an initial narrow domain from the source
   when none is available, and keep it easy to refine later.
5. Classify the useful evidence and create or update focused pages under
   `entities/`, `concepts/`, `comparisons/`, or `summaries/`.
6. Update affected pages with synthesis, provenance, contradictions, caveats,
   tags, and meaningful relative Markdown links.
7. Update `index.md` so every created page is discoverable in the right section.
8. Append one concise ingest entry to `log.md` that names all changed paths.
9. Write every changed page with the task id.
10. Complete the task only when lifecycle is not bridge-managed.

Writing rules:

- Integrate, do not dump: each source should strengthen the existing wiki graph.
- Prefer durable pages that can compound across sources.
- Create new pages only when a concept, entity, comparison, or summary has
  durable value. A topic central to one source can qualify.
- Do not create source pages or raw-document mirror pages. The raw document list
  already preserves original sources outside the generated wiki.
- Update existing pages when new evidence confirms, refines, contradicts, or
  supersedes earlier claims.
- Keep source grounding visible with `sourceDocumentIds` and concise source
  notes where needed; do not copy raw Markdown into a page.
- Use the folder hierarchy and page conventions from `wiki-conventions.md`.
- Keep pages concise, focused, and navigable.
- Do not rewrite unrelated pages.
- Do not regenerate the whole wiki because the selected skill changed.

Completion criteria:

- Source markdown has been synthesized into the target wiki space where useful.
- Related wiki pages reflect the new evidence.
- `SCHEMA.md`, `index.md`, and `log.md` are present and consistent whenever
  this task initializes or materially changes the wiki.
- The task is marked complete by the agent or its bridge-managed executor.
