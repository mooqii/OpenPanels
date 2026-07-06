# OpenPanels

OpenPanels is a local panel system for AI agents. It lets Codex, Claude,
Hermes, and other MCP-capable agents open interactive panels, insert artifacts,
and persist local panel state under the active project's `.openpanels/`
directory.

## Development

```bash
pnpm install
pnpm dev
```

The local studio runs from `apps/local-studio`.

## Install in Codex

MyOpenPanels can be installed from this repository as a Codex plugin marketplace:

```bash
codex plugin marketplace add mooqii/OpenPanels
```

To pin a stable release, install a tagged ref:

```bash
codex plugin marketplace add mooqii/OpenPanels --ref v0.1.1
```

After adding the marketplace, restart Codex, open the plugin directory, choose
the MyOpenPanels marketplace, and install the MyOpenPanels plugin. Start a new
thread after installation so Codex can load the bundled skills and MCP tools.

## Use with Generic MCP Agents

MyOpenPanels also works with MCP clients that do not support Codex plugins or
native Codex widgets. Configure the MCP server with this repository as its
working directory:

```json
{
  "mcpServers": {
    "myopenpanels": {
      "command": "node",
      "args": ["/absolute/path/to/OpenPanels/scripts/start-mcp.mjs"],
      "cwd": "/absolute/path/to/OpenPanels"
    }
  }
}
```

For Claude Desktop, put that server entry in the app's MCP configuration. For
Hermes or another generic MCP host, add the same stdio server command wherever
that host accepts MCP servers.

Generic MCP hosts should call `start_myopenpanels_studio` first. It starts the
local studio and returns a `serverUrl` such as `http://127.0.0.1:49231`. Open
that URL in a browser and keep the MCP session running while using the canvas.
Agents can then use the same project-backed tools:

- `get_myopenpanels_selection`
- `read_myopenpanels_selection_asset`
- `insert_myopenpanels_image`
- `write_myopenpanels_panel_asset`
- `insert_myopenpanels_artifact`

## Codex Plugin

MyOpenPanels is packaged as a Codex plugin for the best Codex experience. The plugin installs both parts that Codex agents need:

- `skills/` tells Codex when and how to use MyOpenPanels.
- `.mcp.json` registers the MCP server that provides the actual tools and widget.

The repository includes `.agents/plugins/marketplace.json`, which exposes this
repo root as an installable Codex plugin source.

The plugin manifest lives at `.codex-plugin/plugin.json`. When installed in Codex, it exposes the `myopenpanels` MCP server through `scripts/start-mcp.mjs`, which prepares dependencies when needed, starts the MCP server, and opens the local studio in a native widget through `render_myopenpanels_widget`.

For agents that do not support plugins, configure the MCP server directly as
shown above. Installing only a skill is not enough because the workflow depends
on MyOpenPanels MCP tools such as `start_myopenpanels_studio`,
`get_myopenpanels_selection`, and `insert_myopenpanels_image`.

## v0.1 Scope

- Local workflow for Codex and generic MCP agents
- Panel protocol, runtime, React host, SDK, local storage, and local server packages
- Canvas-first design workspace prepared for the Moodbook canvas migration
- Image artifacts and editable canvas image shapes
- Project-local `.openpanels/` persistence

See [docs/specs/openpanels-v0.1-spec.md](docs/specs/openpanels-v0.1-spec.md).
