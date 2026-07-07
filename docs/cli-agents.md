# CLI Agent Setup

MyOpenPanels can run in any local agent that can execute shell commands. Agents
use the `openpanels-local` CLI to start the local studio, read canvas state, read the
current selection, and insert images.

## Install

Install the CLI globally:

```bash
npm install -g @openpanels/local-cli
```

Or use npx without a global install:

```bash
npx -y @openpanels/local-cli@latest studio start --project /absolute/path/to/project --format json
```

If you do not pass `--project`, OpenPanels uses `OPENPANELS_PROJECT_DIR` or the
current working directory.

## Agent Workflow

1. Prefer the agent's native OpenPanels/MyOpenPanels panel or widget tool when
   the current host exposes one. In Codex, install the repo-local
   **MyOpenPanels** plugin from `.agents/plugins/marketplace.json`, then call
   `render_myopenpanels_panel` with the project directory so the native panel
   uses the same `.myopenpanels/` storage as the CLI.
2. If native panels are unavailable or unsupported, run
   `openpanels-local studio start --project <project> --format json`.
3. Open the returned `serverUrl` in the agent's in-app Browser side panel.
4. Run `openpanels-local selection --project <project> --format json` to inspect the
   current canvas selection.
5. Run `openpanels-local selection --project <project> --include-image-base64
   --format json` or `openpanels-local read-selection-asset --project <project>
   --output <path> --format json` when selected pixels are needed.
6. Run `openpanels-local insert-image --project <project> --image <path> --placement
   right --format json` to place a generated local image into the canvas.

## Command Map

- `openpanels-local studio start`: start or reuse the local studio.
- `openpanels-local studio status`: show the project-local studio process status.
- `openpanels-local studio open`: open the studio URL in the system browser.
- `openpanels-local studio wait`: wait for the studio HTTP server to become ready.
- `openpanels-local studio stop`: stop the project-local studio process.
- `openpanels-local canvas-state`: read the current canvas session, panel, and state.
- `openpanels-local selection`: read selected shapes and optional PNG data.
- `openpanels-local read-selection-asset`: write the exported selection PNG to a file.
- `openpanels-local insert-image`: add a local image file as a canvas image shape.
