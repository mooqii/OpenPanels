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

## Use with Shell Agents

MyOpenPanels works in any local agent that can run shell commands. Start the
studio and open the returned `serverUrl`:

```bash
openpanels-local studio start --project /path/to/project --format json
openpanels-local studio open --project /path/to/project --format json
```

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

## v0.1 Scope

- Local workflow for generic shell agents
- Panel protocol, runtime, React host, SDK, local storage, and local server packages
- Canvas-first design workspace prepared for the Moodbook canvas migration
- Image artifacts and editable canvas image shapes
- Project-local `.myopenpanels/` persistence

See [docs/specs/openpanels-v0.1-spec.md](docs/specs/openpanels-v0.1-spec.md).
