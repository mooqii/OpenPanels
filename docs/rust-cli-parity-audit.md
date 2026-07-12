# MyOpenPanels Node-to-Rust CLI Parity Audit

This checklist records the completed migration. The current Rust CLI help,
capability manifest, guides, and tests are the authoritative contract. The old
Node CLI and its compatibility aliases are not a parity target.

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
| Project read | `project current/list`, `panel list/current/switch`, `canvas state`, `wiki context` | complete | Backed by Rust storage/bootstrap and covered by Rust tests. |
| Agent protocol | `agent bootstrap`, scoped capability/Guide/Skill discovery | complete | Rust emits budgeted Protocol v3 Bootstrap references and progressively loads full command and workflow descriptors. |
| Canvas read | `canvas selection read/export` | complete | Rust reads explicit selection state and exports selection assets. |
| Canvas write | `canvas placeholder create`, `canvas image insert` | complete | Rust supports asset refs, placement near anchors, replacement, and tests. |
| Wiki | `wiki context/selection/documents/markdown/tasks/spaces/pages` | complete | Rust supports Wiki context, knowledge selection, raw documents, page search/read/write, task lifecycle, and agent workflows. |

## Local HTTP API

| Area | Node API | Rust status | Notes |
| --- | --- | --- | --- |
| Bootstrap | `GET /api/bootstrap` | complete | Rust server returns bootstrap and lazily creates session/wiki/canvas panels. |
| Projects | `POST /api/projects`, session list/create/delete/rename, active session | partial | Rust supports project creation, session list/create/rename/delete, and active-session switching. Single session read and deeper runtime event parity remain pending. |
| Panels | active panel, panel state, selection, asset upload/read | partial | Rust supports active panel switching plus panel state, selection, and asset upload/read APIs needed by the studio canvas path. |
| Wiki | raw documents, markdown, tasks, agent targets, spaces, pages, agent skill | partial | Rust server supports core markdown/task/page/agent-skill/agent-target APIs plus original streaming, reveal, delete, extract, raw reindex, space reindex, wakeups, local worker spawning, and task side effects. Needs broader HTTP golden coverage before marking complete. |
| Updates | `/api/update/status`, `/api/update/download`, `/api/update/install-restart` | partial | Rust backs status/download/install-restart. Install-restart replaces the binary, spawns a delayed studio process on the same host/port/context, writes the new session file, then exits the current server. Needs real release smoke before marking complete. |
| Static files | embedded `apps/studio/dist` + SPA fallback | partial | Rust embeds and serves `dist`; `MYOPENPANELS_STUDIO_STATIC_DIR`/`--static-dir` override exists. Full API parity still blocks Node server removal. |

## Release Package

| Area | Required parity | Rust status | Notes |
| --- | --- | --- | --- |
| GitHub Releases | package target archives, checksums, manifest | partial | `.github/workflows/release-myopenpanels.yml` builds target archives and publishes manifest/checksums. Workflow has local script smoke coverage but has not yet run on GitHub. |
| install scripts | download release asset, verify checksum, place binary on PATH | pending | Native install scripts are the primary install path. |

## Storage And Lifecycle

| Area | Required parity | Rust status | Notes |
| --- | --- | --- | --- |
| SQLite schema | `main.sqlite3`, migrations table, WAL, foreign keys, existing tables | partial | Rust creates compatible tables and can read/write sessions, panels, states; artifacts/wiki task sync pending. |
| Path resolution | `--project-dir`, `MYOPENPANELS_PROJECT_DIR`, `--storage-dir`, `MYOPENPANELS_STORAGE_DIR`, context env vars | partial | Rust path resolver exists; sanitize/fallback must match Node exactly. |
| Bootstrap | first command creates session, wiki panel, canvas panel, default wiki files | partial | Rust creates sessions and panels and ensures default wiki files when wiki APIs run; bootstrap-time file parity still needs golden checks. |
| Active state | `contexts/<contextId>/active-session.json` and `active-panel.json` | complete | Rust writes the current JSON shape and preserves context isolation. |
| Assets | filesystem asset refs under `sessions/<session>/panels/<panel>/assets` | partial | Rust can write/read HTTP assets, read selection assets, and create image assets from `canvas image insert`; broader cross-version asset parity still needs fixtures. |

## Required Test Gates

Before treating the Rust CLI migration as complete:

1. Rust unit tests pass with `mise x rust@1.85.0 -- cargo test`.
2. The Rust CLI crate owns former Node CLI contract coverage in `crates/myopenpanels/src/cli.rs`.
3. A Rust-started studio opens the existing Vite-built UI and passes smoke API checks.
4. A Node-created `.myopenpanels/main.sqlite3` can be read by Rust, and a Rust-created database can be opened by the existing studio during migration.
5. The native install script path is smoke-tested against a real GitHub Release asset.
