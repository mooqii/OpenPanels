---
name: myopenpanels-image
description: Work with the MyOpenPanels image/canvas panel: read the current selection, use its exported PNG, and insert image results back into the canvas.
---

Use this skill when the user wants to use, edit, generate from, or place images in the MyOpenPanels canvas/image panel.

Workflow:

0. Use the `openpanels-local` CLI. If `command -v openpanels-local` fails, use
   `npx -y @openpanels/local-cli@latest` in place of `openpanels-local`.
1. If the user refers to the current canvas selection, run
   `openpanels-local selection --project "$PWD" --format json`.
2. If the task needs visual pixels, run
   `openpanels-local selection --project "$PWD" --include-image-base64 --format json`
   or `openpanels-local read-selection-asset --project "$PWD" --output <path>
   --format json`.
3. Use `selection.selectedShapes` for IDs, bounds, type, and image metadata. If no object is selected, `openpanels-local selection` falls back to the latest image shape and returns `selection.fallback: "last-image"` when available.
4. To place a generated or local bitmap into MyOpenPanels, run
   `openpanels-local insert-image --project "$PWD" --image <path> --placement right
   --format json`.
5. Run `openpanels-local studio start --project "$PWD" --format json` if the user
   needs to view the canvas.

Do not infer selection from screenshots or hand-write `.myopenpanels/` files.
