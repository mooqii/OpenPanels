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
workflows. Bootstrap also publishes the required entry Skill version and its
canonical update source. On Studio startup, the Agent updates a missing or older
Skill through its own Skill installer; equal or newer Skills are left alone.
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
success, run `myopenpanels studio open-system-browser --local-only`. The CLI
reports `data.opened: true` only after the operating-system launcher succeeds; on
failure it returns `browser_open_failed` and the URL for manual recovery. An
open-only request is complete only after an opener succeeds. Bootstrap is needed
only for subsequent panel work.

Agents can then use project-backed CLI commands:

```bash
myopenpanels agent bootstrap --project-dir /path/to/project --format json
myopenpanels agent capability list --format json
myopenpanels agent capability list --scope wiki --format json
myopenpanels agent capability read --intent wiki.page.search --format json
myopenpanels agent guide list --project-dir /path/to/project
myopenpanels agent skill list --project-dir /path/to/project
myopenpanels agent skill read --skill-id canvas-panel --project-dir /path/to/project
myopenpanels panel list --project-dir /path/to/project --format json
myopenpanels panel activate --project-dir /path/to/project --panel-kind wiki --format json
myopenpanels panel context read --project-dir /path/to/project --format json
myopenpanels panel state read --project-dir /path/to/project --format json
myopenpanels panel selection read --project-dir /path/to/project --format json
myopenpanels canvas selection export --project-dir /path/to/project --output-file /tmp/selection.png --format json
myopenpanels canvas image insert --project-dir /path/to/project --image-file /tmp/result.png --placement right --expect-focus-revision <revision> --format json
```

## v0.1 Scope

- Local workflow for generic shell agents
- Rust CLI/server/storage with a React Studio frontend
- Multi-panel project workspace with wiki and canvas panels
- Image artifacts and editable canvas image shapes
- Project-local `.myopenpanels/` persistence
