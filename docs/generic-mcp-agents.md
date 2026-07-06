# Generic MCP Agent Setup

OpenPanels can run in any local agent that can start a stdio MCP server. Codex
gets a native widget through the Codex plugin. Generic hosts such as Claude
Desktop or Hermes should use the browser-based local studio URL returned by the
MCP server.

## Server Configuration

Clone the repository and install dependencies:

```bash
git clone https://github.com/mooqii/OpenPanels.git
cd OpenPanels
pnpm install
```

Add OpenPanels as an MCP server in your agent:

```json
{
  "mcpServers": {
    "openpanels": {
      "command": "node",
      "args": ["/absolute/path/to/OpenPanels/scripts/start-mcp.mjs"],
      "cwd": "/absolute/path/to/OpenPanels"
    }
  }
}
```

If your host lets you set environment variables, set `OPENPANELS_PROJECT_DIR`
to the project whose `.openpanels/` state should be used:

```json
{
  "OPENPANELS_PROJECT_DIR": "/absolute/path/to/your/project"
}
```

If you do not set `OPENPANELS_PROJECT_DIR`, OpenPanels uses the MCP process
working directory or the `projectDir` argument passed to each tool.

## Agent Workflow

1. Call `start_openpanels_studio`.
2. Open the returned `serverUrl` in a browser.
3. Use `get_openpanels_selection` to inspect the current canvas selection.
4. Use `read_openpanels_selection_asset` when the selected pixels are needed.
5. Use `insert_openpanels_image` to place a generated local image into the
   canvas.

The Codex-only `render_openpanels_widget` tool is still available for clients
that support native app resources. Generic MCP clients should prefer
`start_openpanels_studio`.

## Tool Map

- `start_openpanels_studio`: start the local studio and return a browser URL.
- `get_openpanels_session`: read or create the active project session.
- `open_openpanels_panel`: create a canvas panel.
- `get_openpanels_canvas_state`: read the current canvas state.
- `get_openpanels_selection`: read selected shapes and optional PNG data.
- `read_openpanels_selection_asset`: read the exported selection PNG.
- `write_openpanels_panel_asset`: copy a local file into OpenPanels storage.
- `insert_openpanels_image`: add a local image file as a canvas image shape.
- `insert_openpanels_artifact`: add generic image or canvas artifacts.
