# MyOpenPanels Architecture

## Runtime boundaries

MyOpenPanels has five supported panels: Wiki, Writing, Canvas, Typesetting, and Publishing. Every Project exposes
them in that fixed order. Panel state and structured runtime data live in
SQLite; large assets and Wiki Markdown remain on disk below the Project panel
directory.

The Rust CLI and local Studio server are transports over the same domain
services. Transport modules parse and serialize data but do not own SQL.
Storage repositories own SQL and transaction boundaries. A business mutation
and its `change_scopes` revision must commit in the same transaction.

## Data ownership

- `projects`, `panels`, and `panel_states` own Project and Panel state.
- `tasks`, `agent_targets`, and delivery tables are the authority for Task
  lifecycle, leases, retries, assignment, results, and errors.
- Wiki panel state owns documents, spaces, rules, page indexes, and ingestion
  projections. It does not persist Agent process records.
- Project Writing Skills live below the Project storage directory and are
  loaded only for that Project; each generated Skill has a task-bound manifest
  and a self-contained `SKILL.md`.
- Typesetting panel state owns publication projects, ordered cover references,
  and Tiptap JSON content. Imported images are copied into the Typesetting
  panel so they do not depend on the source Canvas asset lifecycle.
- Publishing panel state is reserved for the publishing workflow scaffold.
- `agent_operations` owns persistent Canvas and Wiki generation operations.
- `studio/instance.json` owns the storage-wide Studio process binding, while
  `studio/focus/` owns the single user-visible Project and Panel focus.
- Agent context files contain only Agent-private loader and lifecycle state;
  they never select or own a Studio process.

## Compatibility

The permanent compatibility surface is `studio start`, `agent bootstrap`, the
CLI JSON envelope, released database migrations, and self-update behavior.
Business commands are discovered from the installed Command Registry. Studio
HTTP APIs support only the currently installed package; queue-specific legacy
Wiki Task routes are not supported.

Published migrations `0001` through `0004` are immutable. Later migrations
upgrade Session storage to Project storage, preserve Wiki and Canvas data, and
back up unsupported historical Panel records before removing them.

| Surface | Status | Authority |
| --- | --- | --- |
| `studio start`, `agent bootstrap` | Stable | Clap command and CLI envelope |
| Task lifecycle | Stable | `tasks` and `/api/tasks/*` |
| Agent targets | Stable | `agent_targets` and `/api/agent/targets/*` |
| Agent guidance | Skill-only | `agent skill list/read` |
| Panel kinds | Wiki, Writing, Canvas, Typesetting, and Publishing | `panels.kind` constraint |
| CLI self-update | Release-critical | Previously installed CLI updater |
| Wiki Task/target routes | Removed | No compatibility handler |

## Transaction rules

Repository write methods begin the transaction, apply the domain mutation,
record the matching `change_scopes` revision, and commit once. Transport and
domain service code never records a second revision. A Task lease, delivery,
target heartbeat, or lifecycle transition is therefore visible together with
the revision that announces it.

Wiki document ingestion fields are projections of canonical Task state. They
may be updated by the Wiki adapter as part of Task completion, but they never
hydrate or replace rows in `tasks`. Context JSON is not a Task or Agent process
store.
