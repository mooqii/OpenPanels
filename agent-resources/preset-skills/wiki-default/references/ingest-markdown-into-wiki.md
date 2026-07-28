# Ingest Source Markdown Into The Default Wiki

Use this reference when integrating one source Markdown document into the
structured wiki.

Execution Steps:

1. Read the source Markdown.
2. Read `wiki-conventions.md`, review the available wiki paths, and inspect
   `SCHEMA.md`, `index.md`, plus any existing pages likely to change.
3. Determine the source's dominant natural language, main topics, named
   entities, alternate names, stable identity or definitions, dated
   observations or changing states, and useful relationships.
4. Search existing titles, paths, aliases, tags, and links for canonical pages
   that should absorb the evidence.
5. If this is the first ingest, establish `SCHEMA.md` and `index.md` before
   adding knowledge pages. Seed the first useful topic without declaring it the
   permanent boundary of the wiki.
6. Classify useful evidence and create or update focused pages under `topics/`,
   `entities/`, or `views/`. A new unrelated source may create a new top-level
   topic without changing or being forced into existing topics.
7. Write new pages in the source's dominant language. Preserve the established
   language of an existing page and add useful cross-language terms as aliases.
8. Update affected pages with synthesis, provenance, contradictions, caveats,
   aliases, tags, concise first-paragraph summaries, and meaningful relative
   Markdown links. Name a relationship by its meaning when the evidence
   supports one rather than placing everything under a generic "Related"
   heading.
9. Update `index.md` only when its routing needs to expose a new or changed
   topic hub, important entity, or view. Ensure other pages remain reachable
   through meaningful links.

Writing rules:

- Integrate, do not dump: each source should strengthen the existing wiki graph.
- Prefer durable pages that can compound across sources.
- Create new pages only when a topic, entity, or cross-topic view has durable
  value. A topic central to one source can qualify.
- Let the wiki grow from one document into multiple unrelated domains. Do not
  treat the first source's topic, vocabulary, or language as a global
  restriction.
- Keep one canonical page per subject. Use aliases, tags, and links instead of
  creating duplicate pages for translations, synonyms, or multiple topic
  memberships.
- Model the subject described by the evidence, not the source's headings or
  storage shape. Do not turn every source section into a page.
- Keep stable identity and definitions separate from dated observations,
  changing states, forecasts, and source-specific claims. Preserve dates and
  provenance near time-sensitive statements.
- Do not create source pages or document mirror pages. The supplied documents
  remain available outside the generated wiki.
- Update existing pages when new evidence confirms, refines, contradicts, or
  supersedes earlier claims.
- Keep source grounding visible with stable source identifiers and concise
  source notes where needed; do not copy raw Markdown into a page.
- Use the folder hierarchy and page conventions from `wiki-conventions.md`.
- Keep pages concise, focused, and navigable.
- Do not rewrite unrelated pages.
- Do not regenerate the whole wiki because the selected skill changed.

Completion criteria:

- Source Markdown has been synthesized into the wiki where useful.
- Related wiki pages reflect the new evidence.
- New and updated pages use a coherent language appropriate to their sources.
- `SCHEMA.md` and `index.md` are present and consistent whenever this execution
  initializes or materially changes the wiki.
