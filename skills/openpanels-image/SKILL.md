---
name: openpanels-image
description: Work with the OpenPanels image/canvas panel: read the current selection, use its exported PNG, and insert image results back into the canvas.
---

Use this skill when the user wants to use, edit, generate from, or place images in the OpenPanels canvas/image panel.

Workflow:

1. If the user refers to the current canvas selection, call `get_openpanels_selection` with the active project directory.
2. If the task needs visual pixels, call `read_openpanels_selection_asset` or call `get_openpanels_selection` with `includeImageBase64: true`.
3. Use `selection.selectedShapes` for IDs, bounds, type, and image metadata. If no object is selected, `get_openpanels_selection` falls back to the latest image shape and returns `selection.fallback: "last-image"` when available.
4. To place a generated or local bitmap into OpenPanels, prefer `insert_openpanels_image`.
5. For generic image/canvas artifacts, use `write_openpanels_panel_asset` and `insert_openpanels_artifact`.

Do not infer selection from screenshots or hand-write `.openpanels/` files unless the MCP tools are unavailable.
