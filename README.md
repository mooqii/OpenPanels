# OpenPanels

OpenPanels is a local panel system for shell-capable AI agents. It lets agents
open interactive panels, insert artifacts, and persist local panel state under
the active project's `.myopenpanels/` directory through the `openpanels-local`
CLI.

## Skills

MyOpenPanels is distributed as a portable skill instruction file:
`skills/myopenpanels-canvas/SKILL.md`.

Paste this into your agent to install and trigger the skill:

```text
Please install and activate the MyOpenPanels skill in this agent: create a local
skill named `myopenpanels-canvas` from `skills/myopenpanels-canvas/SKILL.md` in
this repository, then trigger it whenever I say "open MyOpenPanels", "use the
MyOpenPanels canvas", "put this on the canvas", or mention a canvas, board,
selected image, or reference image for visual work.
```

The skill file contains the environment setup and CLI workflow the agent needs.

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

## Native Codex Plugin

This repo includes a local Codex plugin at `plugins/myopenpanels` and a
repo-scoped marketplace at `.agents/plugins/marketplace.json`. After restarting
Codex, install **MyOpenPanels** from that local marketplace to expose the
`render_myopenpanels_panel` tool. Agents should use that tool first for native
panel/widget opening, then fall back to the browser workflow below when native
widgets are unavailable.

## Use with Shell Agents

MyOpenPanels works in any local agent that can run shell commands. Agents that
support native panels/widgets should open MyOpenPanels through their native
OpenPanels surface first. If the current agent does not support native panels,
start the studio and open the returned `serverUrl` in the agent's in-app Browser
side panel:

```bash
openpanels-local studio start --project /path/to/project --format json
```

Use `openpanels-local studio open` only when you explicitly want the system
browser instead of the agent side panel.

Agents can then use project-backed CLI commands:

```bash
openpanels-local canvas-state --project /path/to/project --format json
openpanels-local selection --project /path/to/project --format json
openpanels-local selection --project /path/to/project --include-image-base64 --format json
openpanels-local read-selection-asset --project /path/to/project --output /tmp/selection.png --format json
openpanels-local insert-image --project /path/to/project --image /tmp/result.png --placement right --format json
```

## v0.1 Scope

- Local workflow for generic shell agents
- Panel protocol, runtime, React host, SDK, local storage, and local server packages
- Canvas-first design workspace prepared for the Moodbook canvas migration
- Image artifacts and editable canvas image shapes
- Project-local `.myopenpanels/` persistence

See [docs/specs/openpanels-v0.1-spec.md](docs/specs/openpanels-v0.1-spec.md).
