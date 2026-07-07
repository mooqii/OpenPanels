---
name: myopenpanels-image
description: Work with the MyOpenPanels image/canvas panel: read the current selection, use its exported PNG, and insert image results back into the canvas.
---

Use this skill when the user wants to use, edit, generate from, or place images in the MyOpenPanels canvas/image panel.

Workflow:

0. In Codex, if MyOpenPanels MCP tools are not already visible in the active tool
   list, use tool discovery/search for `get_myopenpanels_selection`,
   `read_myopenpanels_selection_asset`, or `insert_myopenpanels_image` before
   concluding that MyOpenPanels is unavailable.
1. If the user refers to the current canvas selection, call `get_myopenpanels_selection` with the active project directory.
2. If the task needs visual pixels, call `read_myopenpanels_selection_asset` or call `get_myopenpanels_selection` with `includeImageBase64: true`.
3. Use `selection.selectedShapes` for IDs, bounds, type, and image metadata. If no object is selected, `get_myopenpanels_selection` falls back to the latest image shape and returns `selection.fallback: "last-image"` when available.
4. To place a generated or local bitmap into MyOpenPanels, prefer `insert_myopenpanels_image`.
5. For generic image/canvas artifacts, use `write_myopenpanels_panel_asset` and `insert_myopenpanels_artifact`.

Do not infer selection from screenshots or hand-write `.myopenpanels/` files unless the MCP tools are unavailable.
