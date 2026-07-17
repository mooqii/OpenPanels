# Maintain Karpathy LLM Wiki

Use this reference when wiki navigation, summaries, indexes, or related pages
need maintenance.

Workflow:

1. Claim the task only when lifecycle is not bridge-managed.
2. Read `wiki-conventions.md` and list pages in the target wiki space.
3. Read `SCHEMA.md`, `index.md`, `log.md`, and pages that affect navigation or
   need maintenance.
4. Repair the foundation pages when missing or inconsistent. Preserve useful
   existing structure rather than flattening it.
5. Update `index.md` by folder/type so every active generated page has one
   concise, relative Markdown link and summary.
6. Make focused maintenance changes when needed: repair broken or stale links,
   update outdated provenance, surface unresolved contradictions, and split
   oversized pages into the established hierarchy.
7. Append a concise `maintenance` entry to `log.md` naming every
   changed path.
8. Write changed pages with the task id.
9. Complete the task only when lifecycle is not bridge-managed.

Rules:

- Keep `index.md` useful as the first page an agent reads before drilling into
  the wiki.
- Keep summaries concise and scannable.
- Preserve user-authored structure when it is still useful.
- Use the hierarchy, frontmatter, tags, provenance, and link conventions in
  `wiki-conventions.md`.
- Do not add raw source inventories to the generated wiki; raw documents are
  already maintained in the wiki panel's source list.
- Do not rewrite all pages just to normalize style.
- Do not translate or regenerate existing wiki content merely because a
  different wiki skill is now selected.

Completion criteria:

- Foundation pages and index/navigation reflect the current generated wiki.
- The task is marked complete by the agent or its bridge-managed executor.
