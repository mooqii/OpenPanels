# MyOpenPanels Architecture

The conceptual contract for Tasks, Operations, Procedures, CLI commands,
panels, module capabilities, and Skill layers is defined in
[`core-concepts.md`](core-concepts.md). Planned convergence work is tracked in
[`core-optimization-plan.md`](core-optimization-plan.md).

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

The current schema contains 17 application tables:

| Table | Responsibility |
| --- | --- |
| `storage_meta` | Database identity and the global data revision |
| `projects` | Project identity, title, root path, and timestamps |
| `panels` | Panel identity, ordering, and UI-only state |
| `panel_selections` | Selection JSON and its independent revision |
| `settings` | Generic `key + value_json` user overrides |
| `change_scopes` | Catalog, UI, resource, Task, and settings revisions used by Studio live synchronization |
| `direct_operations` | Target-bound direct Agent interactions and their four-state lifecycle |
| `tasks` | Durable work, dependency, lease, fencing, result, and execution summaries |
| `resources` | Stable identity, ownership, lifecycle, and revision for every durable domain resource |
| `documents` | Wiki sources, My Documents, drafts, and articles |
| `task_resources` | Explicit Task-to-resource roles and captured resource versions |
| `wiki_spaces` | Wiki aggregate identity, immutable revision pointer, and configuration |
| `wiki_source_ingestions` | Last source and Wiki versions known to be indexed together, plus disposition |
| `canvas_documents` | Versioned opaque Canvas snapshots, independent of panel UI state |
| `assets` | Project-level binary asset metadata and immutable content revision pointer |
| `publications` | Publication content, source links, title selection, and revision data |
| `releases` | Release snapshot, platform result, remote reference, and publication link |

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

Each revision contains a manifest and complete files. A small atomic
`active.json` pointer selects the current revision. Task output is first written
under `content/.staging/<task>/<generation>/`, validated against the captured
base revision, and atomically renamed into its immutable revision directory.
Only after validation does Task completion publish the new pointer and persist
the Task result and relevant domain resource revision. A conflict leaves the
previous active pointer unchanged. Wiki spaces are one resource, so all files
in a Wiki update move together.

Direct My Document Operations use the same immutable revision preparation
under their Operation artifact directory. The completed Operation and document
projection commit first; pointer publication follows, and startup recovery can
publish the exact revision named by that completed Operation. Direct Canvas
assets are materialized before their asset row, Canvas snapshot, and completed
Operation commit in one database transaction.

Binary assets use the same project-scoped content boundary with a compact
versioned layout:

```text
.myopenpanels/projects/<project>/content/asset/<asset>/<version>/<file>
```

The active asset reference and metadata live in SQLite. Panel directories are
not authoritative content storage, and new assets or documents must never be
written below `projects/<project>/panels/<panel>/`.

Startup recovery removes abandoned staging directories and may finish a
database-committed content publication. Unreferenced prepared revisions are
orphans and can be removed without changing active content.

## Resource and Task coordination

A file is not a Task and the relationship is not assumed to be one-to-one.
`resources` owns stable identity, while `documents`, `wiki_spaces`,
`canvas_documents`, `publications`, and `releases` own type-specific facts.
Immutable bytes and revisions live in the content directory.
`task_resources` explicitly associates any number of Tasks and resources using
`primary`, `input`, `output`, or `context` roles and records the captured
resource version.

`tasks.status` is authoritative for execution. Wiki source indexing is derived
from three facts: the source content version, the version recorded in
`wiki_source_ingestions`, and the latest related Task. There is no separately
persisted `pending`, `running`, or `indexed` document status. The domain
projection computes those labels when it composes Wiki state, so changing or
archiving a Task cannot leave a stale status string inside panel JSON.

Deleting a resource soft-deletes the `resources` row and cancels every linked
queued or running Task in the same transaction. Cancellation advances the
execution generation, clears leases and tokens, and fences already-running
executors from heartbeat, writes, and completion. Immutable content archival
and physical cleanup happen after the database commit and are recoverable.
Archiving is deliberately different: it is a Task-list visibility operation
allowed only for terminal Tasks, retains the Task and `task_resources` rows,
and does not alter the resource.

## Migrations

SQL migrations under `crates/myopenpanels/migrations` are the permanent data
upgrade history. `0001_initial.sql` is the complete clean-install baseline.
Every later persistent shape or JSON-format change adds the next immutable,
strictly consecutive file. The migration registry uses `include_str!`, so all
migration SQL ships inside the CLI binary.

`PRAGMA user_version` records only the highest migration that committed. It
does not replace migration files. Startup validates that the registry is
contiguous, rejects a database newer than the CLI, creates a consistent backup
with SQLite's Backup API, and applies each missing migration in its own
`IMMEDIATE` transaction. The schema/data conversion and `user_version` update
commit together. A failed migration rolls back that step and leaves both the
last valid database version and the pre-upgrade backup available.

Pre-1.0 storage, including the experimental seven-table database, is first
backed up as a complete directory. The importer creates the clean domain
baseline, extracts panel business arrays into domain rows, recreates
Task-resource links, fences previously running Tasks, and copies immutable
content into the rebuilt storage directory. An unknown 0.x shape is preserved
in the backup and skipped rather than guessed. Starting with the 1.0 baseline,
user data is always preserved by ordered migrations. Released migration files
are never edited; a correction is a new migration.

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
