# OpenPanels

OpenPanels is a local panel system for shell-capable AI agents. It lets agents
open interactive panels, insert artifacts, and persist local panel state under
the active project's `.myopenpanels/` directory through the `openpanels-local`
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
its setup workflow, install or verify the `openpanels-local` CLI, and open the
MyOpenPanels panel.
```

This gives the agent the exact skill directory URL instead of only a repository
and path. If your agent only accepts repo/path syntax, use repository
`mooqii/OpenPanels` with path `skills/myopenpanels`.
Installing the skill only adds the agent instructions; the first MyOpenPanels
skill run installs or verifies the native CLI from GitHub Releases, starts the
local studio, and opens the MyOpenPanels panel URL returned by the CLI.

The entry skill keeps itself small and stable. It uses the Rust-native
`openpanels-local` CLI from GitHub Releases, then asks the CLI for
`agent bootstrap`, which is the source of truth for wiki, canvas, and future panel
workflows. Bootstrap also publishes the required entry Skill version and its
canonical update source. On Studio startup, the Agent updates a missing or older
Skill through its own Skill installer; equal or newer Skills are left alone.
The compact context includes state and the full command capability
set; longer built-in agent resources live under `agent-resources/` and load on
demand. Wiki generation now uses the `karpathy-llm-wiki` skill, which the CLI
syncs into `.myopenpanels/skills/` at runtime.

## Development

```bash
pnpm install
pnpm dev
```

The local studio runs from `apps/local-studio`. The publishable agent CLI is the
Rust binary in `crates/openpanels-local`.

## Install

Install the Rust-native CLI from GitHub Releases, then verify it:

macOS/Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/mooqii/OpenPanels/main/scripts/install-openpanels-local.sh | sh
```

Windows PowerShell:

```powershell
iwr https://raw.githubusercontent.com/mooqii/OpenPanels/main/scripts/install-openpanels-local.ps1 -UseB | iex
```

```bash
openpanels-local --version
```

Check for and install release updates:

```bash
openpanels-local update check
openpanels-local update
```

GitHub Releases are the update source. Release constraints and manifest
requirements live in [docs/release.md](docs/release.md).

## Use with Shell Agents

MyOpenPanels works in any local agent that can run shell commands. Start the
studio and open the returned `browserUrl` in the agent's in-app Browser side
panel. `serverUrl` remains the localhost URL for direct use on the same
computer; `browserUrl` may use a LAN address so a browser on another device can
reach the same agent host:

```bash
openpanels-local studio start --project /path/to/project --format json
```

Use `openpanels-local studio open` only when you explicitly want the system
browser instead of the agent side panel.

Agents can then use project-backed CLI commands:

```bash
openpanels-local agent bootstrap --project /path/to/project --format json
openpanels-local agent guides --project /path/to/project
openpanels-local agent guide canvas.image-generation --project /path/to/project
openpanels-local panel list --project /path/to/project --format json
openpanels-local panel switch --project /path/to/project --kind wiki --format json
openpanels-local wiki context --project /path/to/project --format json
openpanels-local canvas state --project /path/to/project --format json
openpanels-local canvas selection read --project /path/to/project --format json
openpanels-local canvas selection read --project /path/to/project --include-image-base64 --format json
openpanels-local canvas selection export --project /path/to/project --output /tmp/selection.png --format json
openpanels-local canvas image insert --project /path/to/project --image /tmp/result.png --placement right --format json
```

## v0.1 Scope

- Local workflow for generic shell agents
- Rust local CLI/server/storage with a React local-studio frontend
- Multi-panel project workspace with wiki and canvas panels
- Image artifacts and editable canvas image shapes
- Project-local `.myopenpanels/` persistence
