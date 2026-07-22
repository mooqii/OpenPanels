# Ingest Source Markdown Into The Default Wiki

Use this reference when integrating one source markdown document into the
structured wiki.

Execution Steps:

1. Read the source Markdown.
2. Read `wiki-conventions.md`, review the available wiki paths, and inspect
   `SCHEMA.md`, `index.md`, `log.md`, plus any existing pages likely to change.
3. If this is the first ingest, establish `SCHEMA.md`, `index.md`, and `log.md`
   before adding knowledge pages. Infer an initial narrow domain from the source
   when none is available, and keep it easy to refine later.
4. Classify the useful evidence and create or update focused pages under
   `entities/`, `concepts/`, `comparisons/`, or `summaries/`.
5. Update affected pages with synthesis, provenance, contradictions, caveats,
   tags, and meaningful relative Markdown links.
6. Update `index.md` so every created page is discoverable in the right section.
7. Append one concise ingest entry to `log.md` that names all changed paths.

Writing rules:

- Integrate, do not dump: each source should strengthen the existing wiki graph.
- Prefer durable pages that can compound across sources.
- Create new pages only when a concept, entity, comparison, or summary has
  durable value. A topic central to one source can qualify.
- Do not create source pages or document mirror pages. The supplied documents
  remain available outside the generated wiki.
- Update existing pages when new evidence confirms, refines, contradicts, or
  supersedes earlier claims.
- Keep source grounding visible with stable source identifiers and concise source
  notes where needed; do not copy raw Markdown into a page.
- Use the folder hierarchy and page conventions from `wiki-conventions.md`.
- Keep pages concise, focused, and navigable.
- Do not rewrite unrelated pages.
- Do not regenerate the whole wiki because the selected skill changed.

Completion criteria:

- Source Markdown has been synthesized into the wiki where useful.
- Related wiki pages reflect the new evidence.
- `SCHEMA.md`, `index.md`, and `log.md` are present and consistent whenever this
  execution initializes or materially changes the wiki.
