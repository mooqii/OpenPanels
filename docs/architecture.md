# MyOpenPanels Architecture

The conceptual contract for Tasks, Operations, Procedures, CLI commands,
panels, module capabilities, and Skill layers is defined in
[`core-concepts.md`](core-concepts.md). Persistence ownership, version
semantics, commit ordering, and local file classes are defined in
[`storage-contract.md`](storage-contract.md). Planned convergence work is
tracked in [`core-optimization-plan.md`](core-optimization-plan.md).

This document describes the 1.0 target architecture. The storage contract lists
the known places where the current 0.x runtime has not converged yet.

## Product boundary

MyOpenPanels is a local, single-user application with one Studio process per
storage directory. The CLI release contains the complete panel behavior, Task
Handler Registry, Agent CLI adapters, scheduling policy, and validation logic.
SQLite is a persistence mechanism, not a configurable workflow engine.

The database stores only data that must survive a restart, participate in a
transaction, or be queried efficiently. Large and immutable content lives in
the project content directory. Studio focus and process ownership remain small
filesystem records outside the application database.

## Panels and modules

Panels are Studio workspaces. They own only UI state, ordering, focus, and
selection. They do not own business records and are not Agent capability
boundaries.

Modules own stable data and behavior:

| Module | Durable resources | Consuming panels |
| --- | --- | --- |
| My Document | `documents(document_kind = 'my_document')` | Wiki, Writing, Typesetting |
| Wiki Source | `documents(document_kind = 'wiki_source')` | Wiki, Writing |
| Wiki Space | `wiki_spaces`, `wiki_source_ingestions` | Wiki, Writing |
| Canvas Document | `canvas_documents` | Canvas |
| Asset | `assets` | Canvas, Typesetting, Publishing |
| Publication | `publications` | Typesetting, Publishing |
| Release | `releases` | Publishing |
| Writing and Skill | Task inputs and user Skill settings | Writing and other consumers |
| Task | `tasks`, `task_resources` | Every panel |

The same Publication can therefore be edited in Typesetting and released from
Publishing without copying it into either panel. Agent capabilities, CLI
commands, handler keys, Task queues, and HTTP resources use module names such
as `publication`, `release`, `asset`, `wiki-source`, and `my-document`. Panel
names appear only where the operation actually concerns panel UI or selection.

## Database model

The ordered schema currently contains 18 application tables.
`0001_initial.sql` introduced the 17 domain tables and
`0002_migration_registry.sql` added migration history,
`0003_canonical_content_authority.sql` established canonical content pointers
and removed misleading domain pointer columns, and
`0004_content_objects.sql` records content format 2 while its journaled hook
migrates text revisions to opaque objects.
`0005_asset_objects.sql` applies the same resumable manifest and opaque-object
layout to binary Assets. `0006_same_project_relationships.sql` adds explicit
Project ownership to relationship tables, enforces same-Project foreign keys,
and prevents Task dependency cycles. `0007_release_snapshots.sql` normalizes
Release identity and ownership facts. `0008_stable_directory_keys.sql` records
directory layout 2 while its journaled hook moves Project and Resource
directories to 19-character bounded hash keys with exact logical-ID
descriptors:

| Table | Responsibility |
| --- | --- |
| `storage_meta` | Database identity, global data revision, and persistent content/directory format versions |
| `schema_migrations` | Applied migration names, versions, checksums, and timestamps |
| `projects` | Project identity, title, root path, and timestamps |
| `panels` | Panel identity, ordering, and UI-only state |
| `panel_selections` | Selection JSON and its independent revision |
| `settings` | Generic `key + value_json` user overrides |
| `change_scopes` | Catalog, UI, resource, Task, and settings revisions used by Studio live synchronization |
| `direct_operations` | Target-bound direct Agent interactions and their four-state lifecycle |
| `tasks` | Durable work, dependency, lease, fencing, result, and execution summaries |
| `resources` | Stable identity, ownership, lifecycle, domain revision, and canonical active content pointer for every durable domain resource |
| `documents` | Wiki sources, My Documents, drafts, and articles |
| `task_resources` | Explicit Task-to-resource roles and captured resource versions |
| `wiki_spaces` | Wiki aggregate identity, root ref, Skill selection, and configuration |
| `wiki_source_ingestions` | Last source and Wiki versions known to be indexed together, plus disposition |
| `canvas_documents` | Versioned opaque Canvas snapshots, independent of panel UI state |
| `assets` | Project-level binary asset metadata and active file ref |
| `publications` | Publication structured content, source links, title selection, and config version |
| `releases` | Publication link, platform, captured snapshot payload, and attempt results |

Content revision IDs, versions, manifest hashes, and aggregate content hashes
have one database owner on `resources`. Domain tables store only module facts
such as logical refs, root refs, config versions, and metadata.

There are no persisted Workflow Runs, dependency graphs, Task Attempts, Task
Events, Agent Routes, Model Gateway connections, content objects, or staging
sessions. Direct Operations are small SQLite records because their lifecycle
must commit atomically with the placeholder, pending document, or completed
resource projection they own. They remain multi-command direct interaction
sessions, not scheduled Task entities. A Task may have one
`depends_on_task_id`; mutation ordering is derived from `mutation_key` and Task
creation order. Up to three execution summaries are embedded in the Task row.

`panels.ui_state_json` contains only presentation and interaction state such as
the active resource, filters, draft form values, and selected Skill. Documents,
Wiki spaces, Canvas snapshots, publications, releases, and Task status are not
stored in that JSON. Domain API responses compose those rows into the
panel-shaped JSON consumed by Studio.

Panel selection writes update only `panel_selections` and its
`panel_selection` change scope. They never modify Canvas state revision,
Canvas `snapshotVersion`, or cause the Canvas editor to rebuild.

## Transaction rules

Every database mutation and its matching `change_scopes` revision commit in the
same SQLite transaction. Resource creation or deletion, Task-resource links,
and cancellation of work invalidated by a deletion also commit together.
Direct Operation begin and completion commit the Operation row and its panel or
resource mutation together after revalidating the bound target and revision.
Claiming uses an `IMMEDIATE` transaction so concurrent workers cannot claim the
same Task. A successful claim changes the Task to `running`, increments
`attempt_count`, advances `execution_generation`, and installs a hashed
lease/token fence atomically.

The only persisted Task states are `queued`, `running`, `succeeded`, `failed`,
`cancelled`, and `superseded`. Dependency waiting is derived from the
predecessor; backoff waiting is derived from `available_at`.

Task execution policy belongs to code:

- global concurrency is configurable from 1 through 4 and defaults to 2;
- the same mutation key is serialized while unrelated or read-only work may run
  concurrently;
- every transition into `running` consumes one of exactly three executions;
- Agent CLI fallback changes the runner, not the Task execution budget;
- restart and lease recovery invalidate the old generation and token;
- manual retry creates a new Task linked by `retry_of_task_id`.

The value `3` is a Rust constant and is returned by Task APIs as
`attemptLimit`. It is not a SQL default or constraint, so changing this policy
does not require a database migration.

## Content storage

Immutable content is stored below:

```text
.myopenpanels/projects/<project>/content/<kind>/<resource>/<revision>/
```

Each revision contains a manifest and immutable objects. SQLite records the
active revision. A small atomic `active.json` pointer is a rebuildable projection
of that database fact. Task output is first written under
`content/.staging/<task>/<generation>/`, validated against the captured base
revision, and promoted into its immutable revision directory. Exact logical
paths live in the manifest; object filenames are content hashes, so Unicode
paths and formerly colliding sanitized names remain distinct. The
Task result, domain resource revision, and exact content revision then commit in
SQLite before pointer publication. A conflict leaves the database and previous
active pointer unchanged. Wiki spaces are one resource, so all files in a Wiki
update move together.

Direct My Document Operations use the same immutable revision preparation
under their Operation artifact directory. The completed Operation and document
resource commit first; pointer publication follows, and startup recovery can
publish the exact revision named by that completed Operation. My Document
content fields have one module-owned transition shared by direct edits, Writing
Tasks, imported-document conversion, and Direct Operations. These commits use
the document content version rather than the containing Wiki Panel revision.
Direct Canvas assets are materialized before their asset row, Canvas snapshot,
and completed Operation commit in one database transaction.

Binary assets use the same project-scoped immutable-revision contract:

```text
.myopenpanels/projects/<project>/content/asset/<asset>/<revision>/
  manifest.json
  objects/<object>
```

The active asset revision and metadata live in SQLite. Panel directories are
not authoritative content storage, and new assets or documents must never be
written below `projects/<project>/panels/<panel>/`.

Immediate creation of a new Document or Wiki content resource first prepares
the immutable revision and records a recoverable `pending.json`. The relational
resource row and canonical content pointer then commit in one transaction.
`active.json` is published only after that commit; startup recovery completes a
committed publication or removes an abandoned pending revision.

Publishing a My Document into Wiki is one coordinated mutation. The immutable
Wiki source revision contains the original file and normalized `source.md`;
the Wiki source metadata, ingestion Tasks, Task-resource links, content pointer,
and source My Document publication history commit together. The My Document
version and Wiki Panel revision are checked before commit, so retry cannot
silently publish a stale version or split publication history from its source.

Creating or importing a My Document no longer persists a complete Wiki Panel
projection. It inserts the document resource, optional conversion Task, and
pending content pointer in one transaction. Rename, content update, and delete
are also resource-scoped. A stale Wiki projection therefore cannot delete a
newer project-level My Document.

Release identity, Project ownership, Publication link, platform, title, and
timestamps are normalized columns. `releases.snapshot_json` contains only the
captured body, tags, and media payload; `result_json` contains attempt results.
Migration 0007 removes the former JSON copies and unused latest-attempt
columns while preserving the Studio read model.

Startup recovery removes abandoned staging directories and may finish a
database-committed content publication. Unreferenced prepared revisions are
orphans and can be removed without changing active content. Recovery validates
the authoritative manifest, objects, version, and hashes before repairing its
filesystem pointer; corrupt authoritative content stops startup with an
integrity error instead of being silently skipped or pruned.

## Resource and Task coordination

A file is not a Task and the relationship is not assumed to be one-to-one.
`resources` owns stable identity, while `documents`, `wiki_spaces`,
`canvas_documents`, `publications`, and `releases` own type-specific facts.
Immutable bytes and revisions live in the content directory.
`task_resources` explicitly associates any number of Tasks and resources using
`primary`, `input`, `output`, or `context` roles and records the captured
resource version. Its Project key and composite foreign keys guarantee that
both ends belong to the same Project. Publication document links, Release
publication links, Task predecessors, retry origins, origin panels, and
resource change scopes follow the same rule.

`tasks.status` is authoritative for execution. Wiki source indexing is derived
from three facts: the source content version, the version recorded in
`wiki_source_ingestions`, and the latest related Task. There is no separately
persisted `pending`, `running`, or `indexed` document status. The domain
projection computes those labels when it composes Wiki state, so changing or
archiving a Task cannot leave a stale status string inside panel JSON.

Direct Wiki page writes and staged Wiki Task output use the same domain
mutation for page-index metadata and the Wiki Space timestamp. The transport
paths remain different, but neither path owns an independent interpretation of
a written page.

Deleting a resource soft-deletes the `resources` row and cancels every linked
queued or running Task in the same transaction. A failed, cancelled, or
superseded prerequisite also terminates every queued or running descendant in
its dependency chain; dependency cycles are rejected by the database.
Termination advances the execution generation, clears leases and tokens, and
fences already-running executors from heartbeat, writes, and completion.
Immutable content archival and physical cleanup happen after the database
commit and are recoverable. Archiving is deliberately different: it is a
Task-list visibility operation allowed only for terminal Tasks, retains the
Task and `task_resources` rows, and does not alter the resource.

## Migrations

SQL migrations under `crates/myopenpanels/migrations` are the permanent data
upgrade history. `0001_initial.sql` is the shipped 0.x baseline and is now
immutable. Every later persistent shape or JSON-format change adds the next
immutable, strictly consecutive file. Clean installations and upgrades run the
same registry, and all migration SQL ships inside the CLI binary.

`PRAGMA user_version` records only the highest migration that committed. It
does not replace migration files or migration history. Startup validates a
contiguous checksummed registry, rejects a database newer than the CLI, creates
a consistent backup with SQLite's Backup API, and applies each missing
migration in its own `IMMEDIATE` transaction. The schema/data conversion,
migration-history row, and `user_version` update commit together. A failed
migration rolls back that step and leaves both the last valid database version
and the pre-upgrade backup available.

Known 0.x storage is migrated or imported without resetting user data.
Filesystem conversions use a resumable journal and do not delete source content
until database and content integrity checks pass. An unknown 0.x shape is
preserved in backup and rejected rather than guessed. Starting with 1.0, user
data is always preserved by ordered migrations. Released migration files are
never edited; a correction is a new migration.

Directory layout 2 uses a 96-bit SHA-256 prefix encoded as base64url for
19-character Project and Resource directory names. `project.json` and
`resource.json` preserve exact logical identity and ownership. Migration 0008
checks the old sanitized namespace for ambiguous Project or Resource IDs before
moving data.

Schema, constraints, indexes, and persisted JSON shape changes require a
migration. Retry limits, concurrency defaults, handler behavior, Agent CLI
priority, and other release-owned business policy do not.

## Runtime interfaces

The public Task surface supports list, read, next, cancel, archive, linked
retry, and internal Bridge or Handoff lifecycle operations. Task reads return
the embedded execution summaries directly.

Agent CLI definitions and default priority come from the release's adapter
registry. The `settings` table stores only user overrides. The Studio Worker
tries enabled, available CLIs in order while preserving the Task-wide
three-execution limit. Agent Handoff and Bridge use the same Handler Registry,
execution bundle, output validation, lease fencing, and finalization path.

There are no public Workflow Run, Task Event, Task Attempt, Agent Target, or
Agent Route commands or HTTP endpoints. Studio Trace displays the Task and its
embedded execution summaries rather than joining auxiliary lifecycle tables.
