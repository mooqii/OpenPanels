# OpenPanels Node-to-Rust CLI Parity Audit

This checklist is the working migration contract. The Rust CLI should not be
treated as a replacement for the Node CLI until every required row is either
`complete` or intentionally deferred by a new product decision.

## Status Legend

- `complete`: Rust behavior is implemented and covered by tests.
- `partial`: Rust has some implementation, but output or lifecycle behavior is
  not yet equivalent.
- `pending`: Node behavior still needs to be ported.

## CLI Commands

| Area | Node command / behavior | Rust status | Notes |
| --- | --- | --- | --- |
| Core | `version`, `--version`, `help`, text/json errors | complete | Covered by Rust unit tests. |
| Updates | `update`, `update check`, `update download` | partial | Rust updater and GitHub Releases workflow exist; needs first real release smoke before marking complete. |
| Studio process | `studio start/status/open/wait/stop`, `__serve-studio` | partial | Rust can start/stop a server and track `studio-session.json`; LAN URLs, full restart behavior, and full API parity remain pending. |
| Project read | `panels`, `active-panel`, `panel-state`, `canvas-state` | complete | Backed by Rust storage/bootstrap and covered by Rust tests. |
| Agent context | `agent context`, `agent-context`, `agent capabilities`, `agent guides`, `agent guide <id>` | partial | Rust now emits context, guide metadata, guide markdown, and capability intents with tests. Capability arg metadata is still simplified versus Node. |
| Canvas read | `selection`, `read-selection-asset` | partial | Rust can read selection and write selection assets; selection still uses direct SQLite reads and needs bootstrap parity cleanup. |
| Canvas write | `insert-placeholder`, `insert-image` | partial | Rust supports placeholder/image insertion, asset refs, placement near anchors, replacement, and tests. Needs broader cross-version canvas snapshot parity before marking complete. |
| Wiki | `wiki context`, `agent-target`, `raw`, `markdown`, `tasks`, `spaces`, `pages` | partial | Rust supports core wiki CLI flows, task side effects, wakeup files, remote wake URLs, local worker spawning, and SQLite task indexing. Remaining parity gaps are broader golden fixture coverage and any product decisions around unimplemented/non-core wiki commands. |

## Local HTTP API

| Area | Node API | Rust status | Notes |
| --- | --- | --- | --- |
| Bootstrap | `GET /api/bootstrap` | complete | Rust server returns bootstrap and lazily creates session/wiki/canvas panels. |
| Projects | `POST /api/projects`, session list/create/delete/rename, active session | partial | Rust supports project creation, session list/create/rename/delete, and active-session switching. Single session read and deeper runtime event parity remain pending. |
| Panels | active panel, panel state, selection, asset upload/read | partial | Rust supports active panel switching plus panel state, selection, and asset upload/read APIs needed by the studio canvas path. |
| Wiki | raw documents, markdown, tasks, agent targets, spaces, pages, language | partial | Rust server supports core markdown/task/page/language/agent-target APIs plus original streaming, reveal, delete, extract, raw reindex, space reindex, wakeups, local worker spawning, and task side effects. Needs broader HTTP golden coverage before marking complete. |
| Updates | `/api/update/status`, `/api/update/download`, `/api/update/install-restart` | partial | Rust backs status/download/install-restart. Install-restart replaces the binary, spawns a delayed studio process on the same host/port/context, writes the new session file, then exits the current server. Needs real release smoke before marking complete. |
| Static files | embedded `apps/local-studio/dist` + SPA fallback | partial | Rust embeds and serves `dist`; `OPENPANELS_STUDIO_STATIC_DIR`/`--static-dir` override exists. Full API parity still blocks Node server removal. |

## Release Package

| Area | Required parity | Rust status | Notes |
| --- | --- | --- | --- |
| GitHub Releases | package target archives, checksums, manifest | partial | `.github/workflows/release-openpanels-local.yml` builds target archives and publishes manifest/checksums. Workflow has local script smoke coverage but has not yet run on GitHub. |
| install scripts | download release asset, verify checksum, place binary on PATH | pending | npm install support has been removed from scope. Native install scripts should become the primary install path. |

## Storage And Lifecycle

| Area | Required parity | Rust status | Notes |
| --- | --- | --- | --- |
| SQLite schema | `main.sqlite3`, migrations table, WAL, foreign keys, existing tables | partial | Rust creates compatible tables and can read/write sessions, panels, states; artifacts/wiki task sync pending. |
| Path resolution | `--project`, `OPENPANELS_PROJECT_DIR`, `--storage-dir`, `OPENPANELS_STORAGE_DIR`, context env vars | partial | Rust path resolver exists; sanitize/fallback must match Node exactly. |
| Bootstrap | first command creates session, wiki panel, canvas panel, default wiki files | partial | Rust creates sessions and panels and ensures default wiki files when wiki APIs run; bootstrap-time file parity still needs golden checks. |
| Active state | `contexts/<contextId>/active-session.json` and `active-panel.json` | complete | Rust writes the current JSON shape and preserves context isolation. |
| Assets | filesystem asset refs under `sessions/<session>/panels/<panel>/assets` | partial | Rust can write/read HTTP assets, read selection assets, and create image assets from `insert-image`; broader cross-version asset parity still needs fixtures. |

## Required Test Gates

Before treating the Rust CLI migration as complete:

1. Rust unit tests pass with `mise x rust@1.85.0 -- cargo test`.
2. The Rust CLI crate owns former Node CLI contract coverage in `crates/openpanels-local/src/cli.rs`.
3. A Rust-started studio opens the existing Vite-built UI and passes smoke API checks.
4. A Node-created `.myopenpanels/main.sqlite3` can be read by Rust, and a Rust-created database can be opened by the existing studio during migration.
5. The native install script path is smoke-tested against a real GitHub Release asset.
