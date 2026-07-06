# OpenPanels

OpenPanels is a Codex-first local panel system for AI agents. It lets agents open interactive panels, insert artifacts, and persist local panel state under the active project's `.openpanels/` directory.

## Development

```bash
pnpm install
pnpm dev
```

The local studio runs from `apps/local-studio`.

## Install in Codex

OpenPanels can be installed from this repository as a Codex plugin marketplace:

```bash
codex plugin marketplace add mooqii/OpenPanels
```

To pin a stable release, install a tagged ref:

```bash
codex plugin marketplace add mooqii/OpenPanels --ref v0.1.0
```

After adding the marketplace, restart Codex, open the plugin directory, choose
the OpenPanels marketplace, and install the OpenPanels plugin. Start a new
thread after installation so Codex can load the bundled skills and MCP tools.

## Codex Plugin

OpenPanels is packaged as a Codex plugin, not as a standalone skill. The plugin installs both parts that agents need:

- `skills/` tells Codex when and how to use OpenPanels.
- `.mcp.json` registers the MCP server that provides the actual tools and widget.

The repository includes `.agents/plugins/marketplace.json`, which exposes this
repo root as an installable Codex plugin source.

The plugin manifest lives at `.codex-plugin/plugin.json`. When installed in Codex, it exposes the `openpanels` MCP server through `scripts/start-mcp.mjs`, which prepares dependencies when needed, starts the MCP server, and opens the local studio in a native widget through `render_openpanels_widget`.

For agents that do not support plugins, install both `.mcp.json` and the `skills/` directory together. Installing only a skill is not enough because the skill depends on OpenPanels MCP tools such as `render_openpanels_widget`, `get_openpanels_selection`, and `insert_openpanels_image`.

## v0.1 Scope

- Codex-first local workflow
- Panel protocol, runtime, React host, SDK, local storage, and local server packages
- Canvas-first design workspace prepared for the Moodbook canvas migration
- Image artifacts and editable canvas image shapes
- Project-local `.openpanels/` persistence

See [docs/specs/openpanels-v0.1-spec.md](docs/specs/openpanels-v0.1-spec.md).
