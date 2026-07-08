# CLI Agent Setup

MyOpenPanels can run in any local agent that can execute shell commands. Agents
use the `openpanels-local` CLI to start the local studio, inspect the active
project and panel, read panel state, and use panel-specific commands such as
canvas selection and image insertion.

## Install

Install the CLI globally:

```bash
npm install -g @openpanels/local-cli
```

Or use npx without a global install:

```bash
npx -y @openpanels/local-cli@latest studio start --project /absolute/path/to/project --format json
```

The recommended agent skill uses `npx -y @openpanels/local-cli@latest` as the
stable entry point. Panel-specific instructions are returned by the CLI through
`agent context`, so users do not need to keep separate canvas/wiki skills
manually updated.

The compact context renderer lives in `packages/local-cli/src/agent-context.ts`,
the command capability manifest lives in
`packages/local-cli/src/agent-capabilities.ts`, and longer workflow guides live
in top-level `agent-guides/*.md` files.

If you do not pass `--project`, OpenPanels uses `OPENPANELS_PROJECT_DIR` or the
current working directory for project metadata. Canvas data is stored in the
global MyOpenPanels data directory so agents and projects can share the same
boards and assets.

The current project and studio process are isolated per agent conversation when
the agent exposes a thread/session environment variable such as
`CODEX_THREAD_ID` or a Hermes conversation id. A new conversation creates a new
MyOpenPanels Project on first use, while still allowing the user to switch to
any existing Project in the studio.

## Agent Workflow

1. Run `openpanels-local studio start --project <project> --format json`.
2. Open the returned `browserUrl` in the agent's in-app Browser side panel.
   `serverUrl` is kept as the localhost URL for same-computer use; `browserUrl`
   may use a LAN address when another device is viewing the agent.
3. Run `openpanels-local agent context --project <project>` before
   panel-specific work. The returned context lists the current project, active
   panel, available panels, current state, and full command capabilities.
4. Run `openpanels-local panels --project <project> --format json` or
   `openpanels-local active-panel --project <project> --kind wiki --format json`
   to inspect or switch panels.
5. For complex workflows, run `openpanels-local agent guides --project
   <project>` and then `openpanels-local agent guide <guide-id> --project
   <project>` to load task-specific instructions.
6. For canvas work, run `openpanels-local selection --project <project>
   --format json` to inspect the current canvas selection.
7. Run `openpanels-local selection --project <project> --include-image-base64
   --format json` or `openpanels-local read-selection-asset --project <project>
   --output <path> --format json` when selected pixels are needed.
8. Run `openpanels-local insert-image --project <project> --image <path> --placement
   right --format json` to place a generated local image into the canvas.

## Command Map

- `openpanels-local studio start`: start or reuse the local studio.
- `openpanels-local studio status`: show the conversation-local studio process status.
- `openpanels-local studio open`: open the studio URL in the system browser.
- `openpanels-local studio wait`: wait for the studio HTTP server to become ready.
- `openpanels-local studio stop`: stop the conversation-local studio process.
- `openpanels-local agent context`: print compact agent context, state, and
  full command capabilities.
- `openpanels-local agent guides`: list loadable built-in guides.
- `openpanels-local agent guide <id>`: print one full workflow guide.
- `openpanels-local agent-context`: compatibility alias for `agent context`.
- `openpanels-local panels`: list panels in the current Project.
- `openpanels-local active-panel`: read or switch the active Project panel.
- `openpanels-local panel-state`: read state for the active or requested panel.
- `openpanels-local canvas-state`: read the current canvas session, panel, and state.
- `openpanels-local selection`: read selected shapes and optional PNG data.
- `openpanels-local read-selection-asset`: write the exported selection PNG to a file.
- `openpanels-local insert-image`: add a local image file as a canvas image shape.
