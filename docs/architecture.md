# MyOpenPanels Architecture

## Runtime boundaries

MyOpenPanels has two supported panels: Wiki and Canvas. Every Project exposes
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
- `agent_operations` owns persistent Canvas and Wiki generation operations.
- Context files contain only current focus and Studio process bindings.

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
| Panel kinds | Wiki and Canvas only | `panels.kind` constraint |
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
