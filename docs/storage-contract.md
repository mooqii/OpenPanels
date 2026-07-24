# MyOpenPanels 1.0 Storage Contract

## Status and scope

This document defines the persistence ownership, version semantics, commit
ordering, and filesystem classes required for the first stable MyOpenPanels
release. It refines the authority rules in
[`core-concepts.md`](core-concepts.md) and is normative for every database,
content, recovery, and local-layout change made while converging on 1.0.

The current `0001_initial.sql` schema and parts of the current filesystem
implementation are a 0.x baseline. They do not yet satisfy every rule below.
Known differences are listed at the end of this document so target
architecture is not mistaken for current runtime behavior.

The contract freezes semantic ownership now. Exact schema changes and the new
content encoding become immutable when their ordered migrations land. Because
`0001_initial.sql` has already shipped in tagged 0.x builds, it is historical
input to those migrations and must not be rewritten.

## Authority rules

1. One persisted fact has one authoritative owner.
2. SQLite owns application-resource identity, relationships, lifecycle, mutable
   domain state, and the active immutable-content revision selected for a
   resource. Installed Skill package content is the explicit filesystem-owned
   exception described below.
3. Immutable content directories own the exact bytes and manifest of a content
   revision. They do not decide which revision is active.
4. `active.json` is a rebuildable projection of the active revision recorded in
   SQLite. It is never an independent source of truth.
5. Materialized directories, selection exports, Task snapshots, caches, and
   process records never own business data.
6. A flexible JSON column stores only facts not represented by canonical
   columns in the same model. API response JSON may compose canonical facts,
   but that response shape is not written back as a second authority.
7. Domain identifiers and logical content paths are opaque values. Code must
   not reconstruct either value from a sanitized physical path.
8. Every business mutation has one commit implementation shared by Studio,
   CLI, Task, and Direct Operation entry points.
9. A known older storage version is migrated without resetting user data. An
   unknown shape is preserved and rejected with recovery details rather than
   guessed or deleted.

## Version and hash vocabulary

The following counters and identifiers are independent and must not be
substituted for one another:

| Term | Meaning | Authority |
| --- | --- | --- |
| Schema version | Highest ordered database migration committed | SQLite `PRAGMA user_version` and migration history |
| Global revision | Monotonic revision for storage-wide change discovery | `storage_meta.global_revision` |
| Resource revision | Monotonic domain revision used for conflicts and live sync | `resources.revision` |
| UI state revision | Revision of one Panel's UI-only state | `panels.ui_state_revision` |
| Selection revision | Revision of one Panel's explicit selection | `panel_selections.revision` |
| Canvas state revision | Revision of the canonical Canvas snapshot | `canvas_documents.state_revision` |
| Content version | Per-resource sequence advanced only when active content changes | `resources.content_version` |
| Content revision ID | Immutable identity of one prepared content revision | `resources.active_content_revision_id` and revision manifest |
| Manifest hash | Hash of the canonical serialized revision manifest | `resources.content_manifest_hash` and immutable revision |
| Content hash | Hash of the exact active payload, or canonical aggregate of a multi-file payload | `resources.content_hash` and immutable revision |
| Metadata hash | Optional hash of mutable domain metadata | Owning domain row |

Changing a title, selection, filter, Skill association, or other metadata must
not advance `content_version`. Changing immutable bytes must advance
`content_version` exactly once in the same business commit that selects the new
content revision.

## Persisted authority matrix

### Platform records

| Fact | Authoritative owner | Non-authoritative projections |
| --- | --- | --- |
| Database identity, global revision, and content/directory format versions | `storage_meta` | Diagnostics |
| Applied migration identity and checksum | `schema_migrations` plus `PRAGMA user_version` | Backup metadata and diagnostics |
| Project identity, title, root, timestamps | `projects` | Bootstrap responses, focus files |
| Panel identity, kind, order, UI-only state | `panels` | Studio client state |
| Explicit Panel selection | `panel_selections` | Selection materializations |
| User settings and overrides | `settings` | Generated effective configuration |
| Stable resource identity, Project ownership, title, lifecycle, domain revision | `resources` | Panel-shaped API responses |
| Live-sync scope revision | `change_scopes` | Studio subscription state |
| Task lifecycle, dependency, lease, result, retry, and execution summaries | `tasks` | Panel status and dispatch phases |
| Task-to-resource roles and captured versions | `task_resources` | Task detail responses |
| Direct Operation lifecycle and target binding | Canonical columns in `direct_operations` | Procedure responses and operation artifacts |

`direct_operations` JSON is limited to intent-specific input, result, and
error payloads. It must not duplicate canonical status, Project, Panel, target,
base revision, or timestamps.

### Module resources

| Resource | Mutable domain authority | Immutable content authority | Active content authority |
| --- | --- | --- | --- |
| My Document | `resources` plus `documents` metadata | Original and active document revisions | Canonical content fields on `resources` |
| Wiki Source | `resources` plus `documents` metadata | Original and converted Markdown revisions | Canonical content fields on `resources` |
| Wiki Space | `resources`, `wiki_spaces`, and `wiki_source_ingestions` | One complete multi-file Wiki revision | Canonical content fields on `resources` |
| Canvas Document | `resources` plus `canvas_documents.state_json` | None; referenced images are Assets | `canvas_documents.state_revision` and `state_hash` |
| Asset | `resources` plus `assets` metadata | One immutable binary revision | Canonical content fields on `resources` |
| Publication | `resources` plus bounded structured Publication data | None in 1.0; referenced Documents and Assets retain their own content | `resources.revision` |
| Release | `resources` plus a captured Publication snapshot and publish result in `releases` | Referenced Assets and captured Skill content retain their own owners | `resources.revision`; the Release snapshot is immutable after creation except for attempt results |
| Custom or imported Skill | Filesystem Skill package and its manifest | The package files themselves | The installed package directory selected by Skill ID |
| Built-in System or Preset Skill | CLI-embedded package | Runtime projection under local Skill access paths | The running CLI version |

Publication structured content is transactional domain data. The unused
Publication content-pointer columns in the 0.x schema are not part of the 1.0
contract. A future exported Publication artifact must be introduced as a
separate immutable resource or by an explicit migration; it must not overload
the Publication's domain revision.

A Release captures its source content rather than continuing to read a mutable
Publication during execution. Media references resolve to immutable Asset
revisions. Task status remains authoritative for scheduled publishing work;
Release attempt summaries do not implement a second Task lifecycle.

## JSON ownership

Persisted JSON is allowed where the data is bounded, transactional, and not
usefully normalized. Each JSON column must have an explicit field allowlist or
typed representation at its write boundary.

The following rules apply:

- `panels.ui_state_json` contains interaction state only.
- `canvas_documents.state_json` is the canonical bounded Canvas document.
- Publication structured content may use a dedicated JSON column, but identity,
  ownership, links, title, lifecycle, and revision remain canonical columns.
- Release snapshot, platform request, and remote result payloads may use
  dedicated JSON columns; normalized identifiers and timestamps are not copied
  into those payloads.
- Task input, source, result, error, and attempt summaries are Task-owned
  payloads. Domain resources do not persist Task status.
- JSON read models assembled for Studio are never persisted wholesale.
- Promoting a JSON fact to a canonical column requires a migration that removes
  or ignores the old JSON copy in the same release.

## Immutable content contract

Every immutable content revision contains:

```text
manifest.json
objects/<opaque-object-key>
```

The manifest records:

- content format version;
- content revision ID and parent revision ID;
- content version;
- creation time;
- each original UTF-8 logical path;
- the matching object reference, MIME type, size, and SHA-256.

The canonical manifest hash is calculated over the complete serialized
manifest and stored in SQLite plus the rebuildable `active.json` projection; it
is not duplicated inside the manifest it hashes.

Logical paths are normalized to `/` separators and validated against absolute
paths, empty components, `.` components, `..` traversal, NUL, and control
characters. Unicode, spaces, punctuation, and platform-reserved names are not
converted to `_` in authoritative storage.

Physical object names are opaque and portable. A logical path is always read
from the manifest, never reconstructed by walking the object directory.
Materializers may use a reversible platform-safe name for a path unsupported by
the host filesystem, but must retain a sidecar mapping to the authoritative
logical path.

Project, resource, revision, Task, Operation, Context, and Skill identifiers
use one centralized physical-key function. The function produces bounded
lowercase ASCII, detects an existing-key identity mismatch, and never serves as
a decoder. Domain identity always comes from SQLite or an owning manifest.

## Content commit ordering

Task, Direct Operation, and immediate user mutations use the same state
machine:

```text
validate captured base
-> write and fsync a prepared immutable revision
-> validate manifest and object hashes
-> commit domain mutation and exact revision identity in SQLite
-> atomically publish the derived active.json pointer
-> remove staging and eventually collect unreferenced revisions
```

An immediate write for a resource that does not exist yet publishes a
recoverable `pending.json` pointer rather than an active pointer. The resource
row, domain row, and exact pending content revision are then selected in one
SQLite transaction. Only after that transaction commits is `active.json`
published and `pending.json` removed. Startup recovery republishes the active
pointer from SQLite and collects an abandoned pending revision that has no
canonical resource.

If preparation fails, neither SQLite nor the active pointer changes. If the
SQLite transaction fails, the prepared revision is an unreferenced orphan. If
the process exits after the SQLite commit, startup recovery republishes
`active.json` from the database record. Recovery never changes SQLite merely
because a different filesystem pointer exists. It validates the authoritative
manifest, objects, version, and hashes before repairing the pointer; corrupt
authoritative content is preserved and reported as an integrity failure.

Deletion and archival are separate:

- deletion first commits resource lifecycle and Task fencing in SQLite, then
  archives or collects immutable content;
- Task archival affects Task-list visibility only;
- physical garbage collection removes content only after proving that no
  canonical database row references it.

## Local filesystem classes

The 1.0 layout keeps current top-level boundaries unless a migration has a
specific reason to change them:

```text
<storage>/
  main.sqlite3
  projects/<project-storage-key>/
    project.json
    content/
      <resource-kind>/<resource-storage-key>/
        resource.json
        active.json
        pending.json
        <revision-storage-key>/
          manifest.json
          objects/
          materialized/
      .staging/
  skills/<skill-storage-key>/
  operations/<operation-storage-key>/
  task-snapshots/
  selection-materializations/
  contexts/
  studio/
```

Project and Resource storage keys use a 19-character
`v2-<base64url(sha256(logical-id)[0..12])>` form. Their descriptors retain the
exact logical ID, Project ownership, and Resource kind. Runtime code resolves
physical paths from the logical ID and verifies descriptors; it never derives
an ID from a directory name.

| Class | Paths | Recovery rule |
| --- | --- | --- |
| Canonical durable | `main.sqlite3`, immutable revision manifests and objects, custom/imported Skill packages | Must be backed up and migrated; never recreated from a projection |
| Derived durable pointer | resource `active.json` | Rebuild from SQLite without changing domain state |
| Recoverable temporary | content `.staging`, pre-creation `pending.json`, active Operation artifacts | Retain only while referenced by an active canonical record; resume or collect on startup |
| Derived materialization | Revision `materialized`, `selection-materializations`, `task-snapshots` | May be deleted and rebuilt |
| Runtime/process | `contexts`, `studio`, logs, locks, focus pointers | May be expired or rebuilt; must not contain the only copy of business data |
| Backup | sibling `<storage-name>-backups/<backup-id>/` | Immutable recovery input; never opened as live storage implicitly |

Update downloads and package-manager caches are outside the live storage
contract. Deleting a cache must never affect Projects or resources.

## Migration and change control

Starting now:

1. `0001_initial.sql` is immutable because it exists in tagged 0.x builds.
2. Every schema, constraint, index, canonical JSON shape, or persistent file
   format change receives the next consecutive migration.
3. Clean installations apply the same ordered migration history as upgrades;
   there is no separate hand-maintained final schema.
4. Startup validates migration continuity and checksums before writing.
5. A database newer than the CLI is opened only far enough to report a
   structured incompatibility and is not modified.
6. A consistent database backup and any filesystem backup required by the
   migration complete before the first migration transaction.
7. Database migrations use one `IMMEDIATE` transaction per version. A version
   is recorded only in the transaction that completed it.
8. Filesystem migrations are journaled and resumable. Source content is not
   removed until the new database and file layout pass integrity checks.
   Journals are removed, together with the empty `.migrations` directory, after
   the corresponding database migration commits.
9. Known 0.x layouts are imported or migrated. Unknown layouts are backed up
   and rejected without destructive fallback.
10. A released migration is never edited. Corrections are new migrations.

Every persistence-changing pull request must include:

- the ordered migration or an explanation of why none is required;
- an updated authority matrix when ownership changes;
- old-version and clean-install fixtures;
- rollback or restart-recovery coverage;
- `PRAGMA integrity_check` and `PRAGMA foreign_key_check`;
- content count, identity, version, and hash assertions where files move.

## Pre-1.0 migration status

Migrations 0004 and 0005 move text and Asset revisions to manifest-owned
logical paths plus content-addressed opaque objects through resumable
filesystem journals without deleting legacy source files. A known 0.x Asset
row whose bytes were already removed is soft-deleted and recorded with its
former reference in the migration journal rather than blocking the rest of
storage. Migration 0008 moves Project, Resource, and Wiki materialization
directories to bounded hash keys, writes exact ownership descriptors, and
rejects ambiguous legacy sanitizer collisions before moving any content.

Canonical content authority, pointer recovery, Asset manifests, and atomic
new-resource content activation are now implemented migration inputs.
Migration 0006 makes Project ownership explicit on relationship tables,
repairs invalid 0.x links, enforces same-Project foreign keys, and prevents
Task dependency cycles. Terminal prerequisite state now propagates through all
queued or running descendants. Migration 0007 removes normalized Release facts
from JSON and drops unused latest-attempt columns. My Document catalog
creation, import, and rename now commit directly to the project-level document
resource; stale Wiki projections cannot delete those resources.

These historical differences are ordered migration inputs, not accepted
exceptions to the stable contract.

## 1.0 storage acceptance

The storage contract is ready for stable release only when:

- a clean install and every supported 0.x fixture converge on the same schema
  and file layout;
- no known upgrade path asks the user to reset storage;
- every active database content pointer resolves to a verified immutable
  revision;
- every active pointer can be deleted and rebuilt from SQLite;
- Unicode and collision-prone logical paths round-trip through write, read,
  rename, materialization, migration, and recovery;
- injected exits at each content commit boundary recover without data loss or
  split authority;
- all Project-scoped relationships are enforced as same-Project relationships;
- canonical row counts, IDs, versions, and hashes match before and after
  migration;
- the previous released CLI upgrades through the real candidate archive,
  restarts Studio, and reconnects without Agent-assisted data repair.
