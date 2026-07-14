# MyOpenPanels LLM Wiki Conventions

Use these conventions whenever creating or changing generated wiki pages.

## Layers and structure

The Wiki panel's raw document list is the immutable source layer. Do not create
`raw/`, `sources/`, or per-document mirror pages in the generated wiki.

The generated wiki uses this structure:

```text
SCHEMA.md
index.md
log.md
entities/<slug>.md
concepts/<slug>.md
comparisons/<slug>.md
summaries/<slug>.md
```

Create a folder when its first page is needed. Do not force empty folders or
create a page for every passing mention.

- `entities/`: people, organizations, products, projects, models, or other
  durable named things.
- `concepts/`: topics, methods, mechanisms, definitions, and recurring ideas.
- `comparisons/`: durable side-by-side analyses and trade-offs.
- `summaries/`: topic maps, timelines, and cross-source syntheses that do not
  fit a single concept or entity.

## Foundation pages

On the first ingest, create the following root pages before adding knowledge
pages. For an existing wiki, preserve useful structure and update these pages
instead of replacing them wholesale.

- `SCHEMA.md`: domain and scope; this folder taxonomy; filename and link
  conventions; the allowed tag taxonomy; page creation threshold; and any
  domain-specific rules.
- `index.md`: a sectioned, content-oriented catalog. Each generated page has a
  Markdown link and a one-line summary under its folder/type. Keep its last
  updated date and page count current.
- `log.md`: an append-only record. Use `## [YYYY-MM-DD] action | subject`, then
  list every wiki path created or updated by that action.

## Page conventions

- Use lowercase hyphenated filenames and paths, such as
  `concepts/attention-mechanism.md`.
- Every generated knowledge page begins with YAML frontmatter:

  ```yaml
  ---
  title: Human-readable title
  created: YYYY-MM-DD
  updated: YYYY-MM-DD
  type: entity | concept | comparison | summary
  tags: [controlled, tags]
  sourceDocumentIds: [raw-document-id]
  confidence: high | medium | low
  contested: false
  contradictions: []
  ---
  ```

- `sourceDocumentIds` records provenance only. Never copy raw Markdown into the
  generated wiki. Add an optional short source note for a claim when its origin
  needs extra context.
- Tags must come from the taxonomy in `SCHEMA.md`; add a new tag there before
  using it.
- Link related pages using standard relative Markdown links, for example
  `[Attention](../concepts/attention.md)`. Index links are relative to the
  root, for example `[Attention](concepts/attention.md)`.
- New or meaningfully updated pages should link to related pages when such pages
  exist. Do not invent links merely to satisfy a quota.
- Keep pages focused and scannable. Split a page that grows beyond roughly 200
  lines into focused pages and update the links and index.

## Editorial policy

- Create a page when a topic is central to one source or recurs across sources;
  otherwise merge the useful detail into an existing page.
- Update an existing page when new evidence confirms, refines, contradicts, or
  supersedes it. Update the `updated` date and provenance.
- Never silently erase a material conflict. Keep both positions with dates and
  source document ids, set `contested: true`, and list the related page paths in
  `contradictions` when applicable.
- Preserve useful user-authored pages and structure. Do not regenerate, rename,
  or translate unrelated pages because the selected skill changed.
