# Maintain The Default Wiki

Use this reference when wiki navigation, summaries, indexes, or related pages
need maintenance.

Execution Steps:

1. Read `wiki-conventions.md` and review the available wiki paths.
2. Read `SCHEMA.md`, `index.md`, and pages that affect navigation or need
   maintenance.
3. Repair the foundation pages when missing or inconsistent. Preserve useful
   existing structure rather than flattening it.
4. Keep `index.md` as a concise router to major topic hubs, important entities,
   and useful views. Ensure other active pages are reachable through a short
   chain of meaningful links instead of listing every page at the root.
5. Make focused maintenance changes when needed: repair broken or stale links,
   improve missing aliases or first-paragraph summaries, update outdated
   provenance, surface unresolved contradictions, separate stable identity
   from dated observations, replace vague relationship labels when their
   meaning is known, and split oversized pages into the established hierarchy.
6. Check that unrelated topics remain independently discoverable and that no
   page was duplicated merely because it uses another language or belongs to
   several topics.

Rules:

- Keep `index.md` useful as the first page an agent reads before drilling into
  the wiki.
- Keep summaries concise and scannable.
- Preserve user-authored structure when it is still useful.
- Use the structure, minimal frontmatter, aliases, tags, provenance, language,
  and link conventions in `wiki-conventions.md`.
- Do not add source inventories to the generated wiki; supplied documents
  remain available in the surrounding source collection.
- Do not create `log.md` by default. Preserve an existing log, but maintain it
  only when the user has chosen to keep a human-readable audit log.
- Preserve each existing page's established language. Do not translate a page
  merely to normalize the wiki, and do not mix languages paragraph by
  paragraph.
- Preserve useful legacy directories. Do not move pages solely to match the
  current default structure.
- Repair source-shaped or duplicated pages incrementally. Merge them into the
  canonical subject page only when identity and provenance can be preserved.
- Do not rewrite all pages just to normalize style.
- Do not translate or regenerate existing wiki content merely because a
  different wiki skill is now selected.

Completion criteria:

- Foundation pages, aliases, summaries, links, and navigation reflect the
  current generated wiki without requiring a non-Markdown index.
