---
name: openpanels-open
description: Open the OpenPanels Codex widget for the active project.
---

Use this skill when the user asks to open, view, or work in OpenPanels.

Always use the `render_openpanels_widget` MCP tool. Do not manually create `.openpanels/` files unless the tool is unavailable.

The widget stores local state in the active project's `.openpanels/` directory.
The local studio also syncs the current canvas selection to `.openpanels/` so agents can read it later with `get_openpanels_selection`.
