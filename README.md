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
Please install the MyOpenPanels skill from GitHub repo `mooqii/OpenPanels`,
using the skill at `skills/myopenpanels`.
```

The entry skill keeps itself small and stable. It uses the latest
`@openpanels/local-cli` package, then asks the CLI for `agent context`, which is
the source of truth for wiki, canvas, and future panel workflows. The compact
context includes state and the full command capability set; longer workflow
guides live in top-level `agent-guides/` markdown files and load on demand.

## Development

```bash
pnpm install
pnpm dev
```

The local studio runs from `apps/local-studio`. The publishable agent CLI lives
in `packages/local-cli`.

## Install

Install the CLI globally:

```bash
npm install -g @openpanels/local-cli
```

Or use npx without a global install:

```bash
npx -y @openpanels/local-cli@latest studio start --project /path/to/project --format json
```

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
openpanels-local agent context --project /path/to/project
openpanels-local agent guides --project /path/to/project
openpanels-local agent guide canvas.image-generation --project /path/to/project
openpanels-local panels --project /path/to/project --format json
openpanels-local active-panel --project /path/to/project --kind wiki --format json
openpanels-local panel-state --project /path/to/project --kind wiki --format json
openpanels-local canvas-state --project /path/to/project --format json
openpanels-local selection --project /path/to/project --format json
openpanels-local selection --project /path/to/project --include-image-base64 --format json
openpanels-local read-selection-asset --project /path/to/project --output /tmp/selection.png --format json
openpanels-local insert-image --project /path/to/project --image /tmp/result.png --placement right --format json
```

## v0.1 Scope

- Local workflow for generic shell agents
- Panel protocol, runtime, React host, SDK, local storage, and local server packages
- Multi-panel project workspace with wiki and canvas panels
- Image artifacts and editable canvas image shapes
- Project-local `.myopenpanels/` persistence
