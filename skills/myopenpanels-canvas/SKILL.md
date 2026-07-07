---
name: myopenpanels-canvas
description: "Open and use the MyOpenPanels infinite canvas for visual work, image generation, image editing, reference gathering, layout exploration, and agent handoff: start the canvas, read selected visual context, use exported PNGs as references, and place generated images back onto the board."
---

Use this skill when the user wants an infinite canvas for visual work, image generation, image editing, reference gathering, layout exploration, or placing generated results. It is especially useful when an agent is generating images and needs a shared board to inspect references, read the current canvas selection, use selected pixels as image context, and insert new image outputs beside the source material.

Workflow:

0. Use the `openpanels-local` CLI. If `command -v openpanels-local` fails, use
   `npx -y @openpanels/local-cli@latest` in place of `openpanels-local`.
1. If the user asks to open or activate the canvas, run
   `openpanels-local studio start --project "$PWD" --format json`, then open the
   returned `serverUrl` or run
   `openpanels-local studio open --project "$PWD" --format json`.
2. Use `openpanels-local studio status --project "$PWD" --format json` to inspect
   an existing session, and `openpanels-local studio wait --project "$PWD"
   --timeout 10 --format json` after startup if you need to verify readiness.
3. If the user refers to the current canvas selection, run
   `openpanels-local selection --project "$PWD" --format json`.
4. If the task needs visual pixels, run
   `openpanels-local selection --project "$PWD" --include-image-base64 --format json`
   or `openpanels-local read-selection-asset --project "$PWD" --output <path>
   --format json`.
5. Use `selection.selectedShapes` for IDs, bounds, type, and image metadata. If no object is selected, `openpanels-local selection` falls back to the latest image shape and returns `selection.fallback: "last-image"` when available.
6. To place a generated or local bitmap into MyOpenPanels, run
   `openpanels-local insert-image --project "$PWD" --image <path> --placement right
   --format json`.

Do not infer selection from screenshots or hand-write `.myopenpanels/` files.

The local studio stores state in the active project's `.myopenpanels/`
directory and syncs the current canvas selection there so agents can read it
later with `openpanels-local selection`.
