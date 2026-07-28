# LLM Wiki Conventions

Use these conventions whenever creating or changing generated wiki pages.

## Storage boundary

The generated wiki must remain a portable tree of clean Markdown files. Do not
require a database, vector store, graph store, sidecar manifest, generated JSON,
or another persistent index to understand or retrieve it. Markdown filenames,
headings, aliases, summaries, tags, and links are the retrieval structure.

The supplied document collection is the source layer. Do not create `raw/`,
`sources/`, or per-document mirror pages in the generated wiki.

## Structure

New wikis use this minimal structure:

```text
SCHEMA.md
index.md
entities/<slug>.md
topics/<slug>.md
views/<slug>.md
```

Create a folder when its first page is needed. Do not force empty folders or
create a page for every passing mention. The first source establishes initial
knowledge, not a permanent domain boundary. Later sources may introduce
unrelated topics.

- `entities/`: people, organizations, products, projects, places, models, or
  other durable named things. An entity page is the canonical home for the
  subject's stable identity, not a snapshot of every observed state.
- `topics/`: concepts, domains, methods, mechanisms, definitions, recurring
  ideas, and topic hubs. A topic hub may summarize and link a broader area.
- `views/`: optional cross-topic retrieval views such as comparisons,
  timelines, commitments, open questions, or current-state summaries. Views
  are derived reading paths: they link to canonical topic and entity pages
  instead of becoming a second source of truth.

Do not create subject-specific directory trees. Represent multiple membership
with tags and Markdown links so one page can participate in several topics.
Comparisons and summaries normally belong in a topic page or an optional view,
not in dedicated top-level directories.

For an existing wiki, preserve useful legacy directories and pages. Do not
duplicate a legacy `concepts/`, `comparisons/`, or `summaries/` page under the
new structure merely to normalize it. Migrate or rename existing pages only
during an explicit reorganization when the benefit outweighs link churn.

## Semantic model

- Model the real subject described by the evidence, not a source document's
  outline, section names, or storage schema. One source may update several
  canonical pages, and several sources may update one page.
- Keep stable identity or definition separate from observations. Dates,
  releases, measurements, changing states, forecasts, and source-specific
  claims remain time-qualified in the page body rather than becoming timeless
  identity fields.
- Use human-readable relationship labels such as `Part of`, `Developed by`,
  `Depends on`, `Supersedes`, `Applies to`, or `Compared with` when supported
  by the evidence. Use a generic related-pages list only when no more precise
  relationship is known.
- Prefer composition through links, aliases, and tags over deep taxonomies or
  duplicated pages. A subject may participate in several topics while keeping
  one canonical page.
- Let the model evolve incrementally. Add a reusable relation label, tag, or
  page convention to `SCHEMA.md` after it proves useful across recurring cases;
  do not design a complete ontology from the first source.

## Foundation pages

On the first ingest, create `SCHEMA.md` and `index.md` before adding knowledge
pages. For an existing wiki, preserve useful structure and update these pages
instead of replacing them wholesale.

- `SCHEMA.md`: state that the wiki may cover multiple unrelated topics; record
  this folder taxonomy, filename and link conventions, the evolving controlled
  tag and alias vocabulary, page creation threshold, and any topic-specific
  rules. Keep it short.
- `index.md`: a concise content router. Link major topic hubs, important
  entities, and useful views with one-line summaries. Do not turn it into an
  exhaustive flat inventory or maintain a page count. Every active page should
  be reachable from the index through a short, meaningful link path, normally
  one or two hops.

Do not create `log.md` by default. Page `updated` fields and the surrounding
revision system carry routine change history. Preserve an existing `log.md`,
but update it only when the user has chosen to maintain a human-readable audit
log.

## Page language

- Determine a source's dominant natural language from its prose, ignoring code
  blocks, quotations, citations, and embedded identifiers.
- Write a new page in the dominant language of the evidence that primarily
  supports it.
- When updating an existing page, preserve its established language and
  integrate the new evidence coherently in that language. Do not alternate
  languages paragraph by paragraph.
- When evidence uses several languages and none is clearly dominant, use the
  existing page language; for a new page, use the language of the primary or
  earliest authoritative source.
- Keep proper names and important technical terms in their established form.
  Add useful translations, abbreviations, former names, and alternate spellings
  to `aliases`.
- Do not create duplicate pages per language unless the user explicitly asks
  for a multilingual edition.
- Keep `SCHEMA.md`, `index.md`, and existing views in their established
  language. A first ingest uses the dominant language of the first source.

## Page conventions

- Use stable lowercase hyphenated filenames and paths, such as
  `topics/attention-mechanism.md`. A filename need not be translated when the
  page language later changes.
- Every generated knowledge page begins with YAML frontmatter:

  ```yaml
  ---
  title: Human-readable title
  aliases: [abbreviation, alternate spelling, translated name]
  updated: YYYY-MM-DD
  tags: [controlled, tags]
  sourceIds: [stable-source-id]
  ---
  ```

- Keep frontmatter minimal. The containing directory expresses the broad page
  role. Do not add page-wide confidence or contested fields when individual
  claims have different evidence quality.
- `aliases` is part of retrieval. Include useful abbreviations, alternate
  spellings, former names, and common translations, but not speculative or
  irrelevant keyword stuffing.
- `sourceIds` records page-level provenance only. Never copy whole source
  documents into the generated wiki. Cite the relevant source id close to a
  specific claim when its origin, date, confidence, or conflict matters.
- Use a small number of controlled topic tags from `SCHEMA.md`. Add a durable
  new tag or alias there when a later source introduces a genuinely new topic;
  do not force it into the first source's taxonomy.
- Place a concise, self-contained summary paragraph immediately after the H1.
  In one to three sentences, state what the subject is and why it matters. Use
  the canonical name and important terminology so lexical search can find it.
- After the summary, use descriptive headings appropriate to the page's
  language. Prefer focused sections for key knowledge, relationships, and
  evidence or conflicts when those sections are relevant.
- On entity pages, keep durable identity or definition near the beginning.
  Put volatile facts under dated status, history, observation, or evidence
  sections as appropriate to the page's language and subject.
- In relationship sections, prefer short labeled links or sentences that state
  how subjects relate. Do not encode relationships as opaque IDs or require a
  separate graph representation.
- Link related pages using standard relative Markdown links, for example
  `[Attention](../topics/attention.md)`. Index links are relative to the root,
  for example `[Attention](topics/attention.md)`.
- New or meaningfully updated pages should link to related pages when such pages
  exist. Do not invent links merely to satisfy a quota.
- Keep pages focused and scannable. Split a page that grows beyond roughly 200
  lines into focused pages and update the links and index.

## Editorial policy

- Create a page when a topic is central to one source or recurs across sources;
  otherwise merge the useful detail into an existing page.
- Before creating a page, search titles, paths, aliases, tags, and related pages
  for an existing canonical home. Update that page instead of creating a
  synonym or translated duplicate.
- A later source may introduce a completely unrelated subject. Add a new topic
  hub or index section when it has durable value; do not invent a relationship
  to existing topics.
- Keep one canonical page for a subject that spans multiple topics. Use tags
  and links to represent every meaningful membership.
- Update an existing page when new evidence confirms, refines, contradicts, or
  supersedes it. Update the `updated` date and provenance.
- Never silently erase a material conflict. Describe the conflicting claims
  together with dates and source document ids in the page body. Keep confidence
  and conflict notes close to the claims they qualify.
- Preserve useful user-authored pages and structure. Do not regenerate, rename,
  or translate unrelated pages because the selected skill changed.
