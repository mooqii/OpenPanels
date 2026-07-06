---
name: openpanels-open
description: Open OpenPanels for the active project in Codex or a generic MCP host.
---

Use this skill when the user asks to open, view, or work in OpenPanels.

In Codex, if OpenPanels MCP tools are not already visible in the active tool
list, use tool discovery/search for `render_openpanels_widget` or
`start_openpanels_studio` before concluding that OpenPanels is unavailable.

In Codex, use the `render_openpanels_widget` MCP tool so the native widget opens inline.

In a generic MCP host that cannot render native app resources, use `start_openpanels_studio` and tell the user to open the returned `serverUrl` in a browser.

Do not manually create `.openpanels/` files unless the MCP tools are unavailable.

The widget stores local state in the active project's `.openpanels/` directory.
The local studio also syncs the current canvas selection to `.openpanels/` so agents can read it later with `get_openpanels_selection`.
