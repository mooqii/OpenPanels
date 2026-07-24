# MyOpenPanels

MyOpenPanels is a local panel system for shell-capable AI agents. It lets agents
open interactive panels, insert artifacts, and persist local panel state through
the `myopenpanels` CLI.

## Skills

MyOpenPanels is distributed from the
[`mooqii/OpenPanels`](https://github.com/mooqii/OpenPanels) repository as a
portable entry skill:
`skills/myopenpanels/SKILL.md`.

Paste this into your agent to install the skill:

```text
Install the MyOpenPanels Agent Skill directly from this GitHub skill URL:
https://github.com/mooqii/OpenPanels/tree/main/skills/myopenpanels

Use your agent's skill installer for GitHub skill URLs if one is available.
Download only that skill directory. Do not clone or inspect the full repository
unless direct skill-directory installation fails.

After the skill is installed, invoke the MyOpenPanels skill once so it can run
its setup process, install or verify the `myopenpanels` CLI, and open the
MyOpenPanels panel.
```

This gives the agent the exact skill directory URL instead of only a repository
and path. If your agent only accepts repo/path syntax, use repository
`mooqii/OpenPanels` with path `skills/myopenpanels`.
Installing the skill only adds the agent instructions; the first MyOpenPanels
skill run installs or verifies the native CLI from GitHub Releases, starts the
MyOpenPanels Studio, and opens the MyOpenPanels panel URL returned by the CLI.

The entry skill keeps itself small and stable. It uses the Rust-native
`myopenpanels` CLI from GitHub Releases, then asks the CLI for
`agent bootstrap`, which is the source of truth for wiki, writing, canvas,
typesetting, publishing, and future panel Procedures. A normal Bootstrap
contains no Entry Skill update fields. After a CLI
release changes the Entry Skill requirement, Bootstrap delivers a one-time
Agent-host update check and keeps it pending until that Agent context
acknowledges the installed version. The installed CLI remains authoritative for
current command catalogs and returned actions.
Protocol v12 keeps the complete Bootstrap envelope under 8192 UTF-8 bytes.
Procedure Bootstrap returns only its required panel context, Skill references,
blockers, and Command Catalog v5 descriptors; generic Bootstrap keeps broader
discovery progressive. Longer
built-in Agent resources live under `agent-resources/` and are synced into the
MyOpenPanels data directory at runtime.

## Development

```bash
pnpm install
pnpm dev
```

The MyOpenPanels Studio runs from `apps/studio`. The publishable agent CLI is the
Rust binary in `crates/myopenpanels`.

The checkout-local `scripts/myopenpanels-dev` wrapper stores development data in
the repository's ignored `.myopenpanels/` directory. The installed CLI stores
release data in `~/.myopenpanels/`; this is intentionally a new, empty storage
location and does not migrate or delete data from the previous platform-specific
directory. Set `MYOPENPANELS_STORAGE_DIR` explicitly to override either location.

Core ownership is documented in
[`docs/core-concepts.md`](docs/core-concepts.md). The database, immutable
content, migration, and local-layout contract for 1.0 is documented in
[`docs/storage-contract.md`](docs/storage-contract.md).

## Install

Install the Rust-native CLI from GitHub Releases, then verify it:

macOS/Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/mooqii/OpenPanels/main/scripts/install-myopenpanels.sh | sh
```

Windows PowerShell:

```powershell
iwr https://raw.githubusercontent.com/mooqii/OpenPanels/main/scripts/install-myopenpanels.ps1 -UseB | iex
```

```bash
myopenpanels --version
```

Check for and install release updates:

```bash
myopenpanels update check
myopenpanels update install
```

`update install` also returns an immediate advisory Agent-host reminder when an
Agent invoked the update. Studio-initiated updates are covered by a persistent,
one-time control event delivered on the next Bootstrap; normal Bootstraps do not
carry the reminder.

GitHub Releases are the update source. Release constraints and manifest
requirements live in [docs/release.md](docs/release.md).

## Use with Shell Agents

MyOpenPanels works in any local agent that can run shell commands. `studio
start` prepares the current Project without opening a browser. Its
`data.nextRequiredAction` describes the separate, required open step. Open the
returned URL unchanged in an in-app Browser only when the host exposes a
callable URL opener:

```bash
myopenpanels studio start --local-only --project-dir /path/to/project --format json
```

Each storage directory owns exactly one Studio process and one user-visible
Project/Panel focus. Calls from other Agents or working directories reuse that
same service while retaining their own Agent context for private lifecycle data.

If the host has no callable opener, or the attempt fails or cannot report
success, execute the conditional CLI fallback in `actions.required` with the
same resolved CLI executable. The CLI reports `data.opened: true` only after the
operating-system launcher succeeds. An open-only request is complete only after
an opener succeeds. Bootstrap is needed only for subsequent panel work.

Agents can then use project-backed CLI commands:

```bash
myopenpanels agent bootstrap --procedure <procedure-key> --format json
myopenpanels agent bootstrap --format json
```

Use Procedure Bootstrap for a clear indexed intent and generic Bootstrap only
as fallback. Execute top-level `actions.required` in order, then applicable
`actions.suggested` entries with the same resolved executable. Business command
paths remain CLI-owned data and are not hardcoded into the Entry Skill.

## v0.1 Scope

- Local operation for generic shell agents
- Rust CLI/server/storage with a React Studio frontend
- Multi-panel project workspace with wiki, writing, canvas, typesetting, and publishing panels
- Image artifacts and editable canvas image shapes
- Platform-native persistence with checkout-local development isolation
