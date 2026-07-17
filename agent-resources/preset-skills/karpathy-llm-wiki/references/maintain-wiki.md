# Maintain Karpathy LLM Wiki

Use this reference when wiki navigation, summaries, indexes, or related pages
need maintenance.

Workflow:

1. Read `wiki-conventions.md` and review the available wiki paths.
2. Read `SCHEMA.md`, `index.md`, `log.md`, and pages that affect navigation or
   need maintenance.
3. Repair the foundation pages when missing or inconsistent. Preserve useful
   existing structure rather than flattening it.
4. Update `index.md` by folder/type so every active generated page has one
   concise, relative Markdown link and summary.
5. Make focused maintenance changes when needed: repair broken or stale links,
   update outdated provenance, surface unresolved contradictions, and split
   oversized pages into the established hierarchy.
6. Append a concise `maintenance` entry to `log.md` naming every
   changed path.

Rules:

- Keep `index.md` useful as the first page an agent reads before drilling into
  the wiki.
- Keep summaries concise and scannable.
- Preserve user-authored structure when it is still useful.
- Use the hierarchy, frontmatter, tags, provenance, and link conventions in
  `wiki-conventions.md`.
- Do not add source inventories to the generated wiki; supplied documents remain
  available in the surrounding source collection.
- Do not rewrite all pages just to normalize style.
- Do not translate or regenerate existing wiki content merely because a
  different wiki skill is now selected.

Completion criteria:

- Foundation pages and index/navigation reflect the current generated wiki.
