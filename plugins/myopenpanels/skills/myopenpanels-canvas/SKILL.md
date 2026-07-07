---
name: myopenpanels-canvas
description: "Open and use the MyOpenPanels canvas as a native Codex widget when supported, with local-studio browser fallback for shell-capable agents."
---

Use this skill when the user wants to open or work with the MyOpenPanels
canvas.

Open workflow:

1. Prefer native mode. Call `render_myopenpanels_panel` with
   `projectDir: "$PWD"` so the project-backed canvas opens in a native Codex
   widget.
2. If native widgets or the native tool are unavailable, start browser fallback:
   `openpanels-local studio start --project "$PWD" --format json`.
3. Open the returned `serverUrl` in the agent's in-app Browser side panel. Do
   not use `openpanels-local studio open` unless the user explicitly asks for
   the external/system browser.

Use the project-local CLI commands for canvas state, selection, and image
insertion. Do not hand-write `.myopenpanels/` files.
