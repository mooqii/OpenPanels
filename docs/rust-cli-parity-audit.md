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
| Updates | `update`, `update check`, `update download` | partial | Rust updater exists; needs release workflow and studio update API integration. |
| Studio process | `studio start/status/open/wait/stop`, `__serve-studio` | partial | Rust can start/stop a server and track `studio-session.json`; LAN URLs, full restart behavior, and full API parity remain pending. |
| Project read | `panels`, `active-panel`, `panel-state`, `canvas-state` | complete | Backed by Rust storage/bootstrap and covered by Rust tests. |
| Agent context | `agent context`, `agent-context`, `agent capabilities`, `agent guides`, `agent guide <id>` | partial | Rust now emits context, guide metadata, guide markdown, and capability intents with tests. Capability arg metadata is still simplified versus Node. |
| Canvas read | `selection`, `read-selection-asset` | partial | Rust can read selection and write selection assets; selection still uses direct SQLite reads and needs bootstrap parity cleanup. |
| Canvas write | `insert-placeholder`, `insert-image` | partial | Rust supports placeholder/image insertion, asset refs, placement near anchors, replacement, and tests. Needs broader cross-version canvas snapshot parity before marking complete. |
| Wiki | `wiki context`, `agent-target`, `raw`, `markdown`, `tasks`, `spaces`, `pages` | partial | `wiki context` is routed through Rust agent context. Write/read wiki workflows remain Node-only. |

## Local HTTP API

| Area | Node API | Rust status | Notes |
| --- | --- | --- | --- |
| Bootstrap | `GET /api/bootstrap` | complete | Rust server returns bootstrap and lazily creates session/wiki/canvas panels. |
| Projects | `POST /api/projects`, session list/read/write/delete/rename | partial | Rust supports project creation, session list, and active-session switching. Session read/delete/rename remain pending. |
| Panels | active panel, panel state, selection, asset upload/read | partial | Rust supports active panel switching plus panel state, selection, and asset upload/read APIs needed by the studio canvas path. |
| Wiki | raw documents, markdown, tasks, agent targets, spaces, pages, language | pending | Required before Node server can be retired. |
| Updates | `/api/update/status`, `/api/update/download`, `/api/update/install-restart` | pending | Rust updater must back these routes. |
| Static files | embedded `apps/local-studio/dist` + SPA fallback | partial | Rust embeds and serves `dist`; `OPENPANELS_STUDIO_STATIC_DIR`/`--static-dir` override exists. Full API parity still blocks Node server removal. |

## Storage And Lifecycle

| Area | Required parity | Rust status | Notes |
| --- | --- | --- | --- |
| SQLite schema | `main.sqlite3`, migrations table, WAL, foreign keys, existing tables | partial | Rust creates compatible tables and can read/write sessions, panels, states; artifacts/wiki task sync pending. |
| Path resolution | `--project`, `OPENPANELS_PROJECT_DIR`, `--storage-dir`, `OPENPANELS_STORAGE_DIR`, context env vars | partial | Rust path resolver exists; sanitize/fallback must match Node exactly. |
| Bootstrap | first command creates session, wiki panel, canvas panel, default wiki files | partial | Rust creates sessions and panels; default wiki files are not created yet. |
| Active state | `contexts/<contextId>/active-session.json` and `active-panel.json` | complete | Rust writes the current JSON shape and preserves context isolation. |
| Assets | filesystem asset refs under `sessions/<session>/panels/<panel>/assets` | partial | Rust can write/read HTTP assets and read selection assets; insert-image/placeholder write workflows remain pending. |

## Required Test Gates

Before removing the Node CLI implementation:

1. Rust unit tests pass with `mise x rust@1.85.0 -- cargo test`.
2. Current Node contract tests pass with `pnpm --filter @openpanels/local-cli test`.
3. Rust has equivalent tests for Node CLI scenarios in `packages/local-cli/src/index.test.ts`.
4. A Rust-started studio opens the existing Vite-built UI and passes smoke API checks.
5. A Node-created `.myopenpanels/main.sqlite3` can be read by Rust, and a Rust-created database can be opened by the existing studio during migration.
