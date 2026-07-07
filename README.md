# OpenPanels

OpenPanels is a local panel system for shell-capable AI agents. It lets agents
open interactive panels, insert artifacts, and persist local panel state under
the active project's `.myopenpanels/` directory through the `openpanels-local`
CLI.

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

The agent-facing instructions live in `skills/`. Add those skill files to any
agent environment that supports local skills, or use the CLI commands directly
from a shell-capable agent.

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

## Skills

MyOpenPanels is distributed as portable skill instructions plus the
`openpanels-local` CLI. There is no plugin manifest or agent-specific runtime
registration required.

- `skills/myopenpanels-canvas/SKILL.md` teaches agents how to use the infinite
  canvas as visual context for image generation, start/open the local studio,
  read selections, and insert generated images.

### Install the Agent Skill

1. Make the CLI available to your local agent:

   ```bash
   npm install -g @openpanels/local-cli
   ```

   If you do not want a global install, the skill can use
   `npx -y @openpanels/local-cli@latest` as its fallback command.
2. If your agent supports local skills, copy
   `skills/myopenpanels-canvas/` into the agent's skills directory and keep the
   skill name `myopenpanels-canvas`.
3. If your agent does not support local skill installation, paste this into the
   agent instead:

   ```text
   Use MyOpenPanels when I ask for an infinite canvas, visual workspace, image
   generation or editing with canvas context, reference gathering, layout
   exploration, or placing generated results onto a board.

   Use the `openpanels-local` CLI. If `command -v openpanels-local` fails, use
   `npx -y @openpanels/local-cli@latest` in place of `openpanels-local`.

   To open the canvas, prefer any native OpenPanels/MyOpenPanels panel or widget
   surface. If native panels are unavailable, run:
   `openpanels-local studio start --project "$PWD" --format json`
   Then open the returned `serverUrl` in the agent's in-app Browser side panel.

   Read the current selection with:
   `openpanels-local selection --project "$PWD" --format json`

   Read selected pixels when needed with:
   `openpanels-local selection --project "$PWD" --include-image-base64 --format json`
   or:
   `openpanels-local read-selection-asset --project "$PWD" --output /tmp/selection.png --format json`

   Insert generated or local images back onto the canvas with:
   `openpanels-local insert-image --project "$PWD" --image <path> --placement right --format json`

   Do not hand-write `.myopenpanels/` files.
   ```

Trigger the skill by asking your agent to "open MyOpenPanels", "use the
MyOpenPanels canvas", "put this on the canvas", or by naming the installed
`myopenpanels-canvas` skill directly. Visual tasks that mention a canvas,
selected image, board, reference image, or placing generated results should also
trigger it.

## v0.1 Scope

- Local workflow for generic shell agents
- Panel protocol, runtime, React host, SDK, local storage, and local server packages
- Canvas-first design workspace prepared for the Moodbook canvas migration
- Image artifacts and editable canvas image shapes
- Project-local `.myopenpanels/` persistence

See [docs/specs/openpanels-v0.1-spec.md](docs/specs/openpanels-v0.1-spec.md).
