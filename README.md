# MyOpenPanels

MyOpenPanels is a local panel system for shell-capable AI agents. It lets agents
open interactive panels, insert artifacts, and persist local panel state under
the active project's `.myopenpanels/` directory through the `myopenpanels`
CLI.

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
its setup workflow, install or verify the `myopenpanels` CLI, and open the
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
`agent bootstrap`, which is the source of truth for wiki, canvas, and future panel
workflows. Bootstrap also publishes the Entry Skill identity and canonical
source as diagnostic metadata. The Skill does not compare CLI,
Protocol, Command Catalog, or Skill versions at runtime; the installed CLI is
always authoritative for current capabilities and returned actions.
Protocol v3 keeps the complete Bootstrap envelope under 8192 UTF-8 bytes. It
returns bounded Panel context and discovery references; command descriptors,
Skills, Guides, Tasks, Operations, and selection details load on demand. Longer
built-in agent resources live under `agent-resources/`. Wiki generation uses the
`karpathy-llm-wiki` skill, which the CLI
syncs into `.myopenpanels/skills/` at runtime.

## Development

```bash
pnpm install
pnpm dev
```

The MyOpenPanels Studio runs from `apps/studio`. The publishable agent CLI is the
Rust binary in `crates/myopenpanels`.

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

`update install` also returns an advisory Agent-host reminder to compare the
loaded MyOpenPanels Entry Skill with the version pinned by the installed release.
The reminder is not emitted by Bootstrap and never blocks CLI installation.

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

If the host has no callable opener, or the attempt fails or cannot report
success, execute `data.nextRequiredAction.fallback.argv` with the same resolved
CLI executable. The CLI reports `data.opened: true` only after the
operating-system launcher succeeds. An open-only request is complete only after
an opener succeeds. Bootstrap is needed only for subsequent panel work.

Agents can then use project-backed CLI commands:

```bash
myopenpanels agent bootstrap --project-dir /path/to/project --format json
```

These are the only stable Agent work entries. Follow each response's
`data.nextRequiredAction` and execute applicable `data.nextActions` entries with
the same resolved executable. Business command paths remain CLI-owned data and
are not hardcoded into the Entry Skill.

## v0.1 Scope

- Local workflow for generic shell agents
- Rust CLI/server/storage with a React Studio frontend
- Multi-panel project workspace with wiki and canvas panels
- Image artifacts and editable canvas image shapes
- Project-local `.myopenpanels/` persistence
