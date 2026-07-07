---
name: myopenpanels-canvas
description: "Open and use the MyOpenPanels infinite canvas for visual work, image generation, image editing, reference gathering, layout exploration, and agent handoff: start the canvas, read selected visual context, use exported PNGs as references, and place generated images back onto the board."
---

Use this skill when the user wants an infinite canvas for visual work, image generation, image editing, reference gathering, layout exploration, or placing generated results. It is especially useful when an agent is generating images and needs a shared board to inspect references, read the current canvas selection, use selected pixels as image context, and insert new image outputs beside the source material.

When the user asks to create, redraw, restyle, or edit an image, use the
current agent's available image generation or image editing model/tool for the
actual visual output. Use the canvas selection as reference context when
relevant. The generated bitmap must be inserted back into MyOpenPanels before
the task is considered complete. Do not
hand-draw production image results with ad hoc scripts, SVG, PIL/canvas code, or
other procedural approximations unless the user explicitly asks for that
implementation style.

For image generation tasks, create a temporary canvas placeholder before calling
the image model:

1. Read the current selection first. If a selected shape exists and is useful as
   the reference or anchor, keep its shape id.
2. Insert a placeholder with
   `openpanels-local insert-placeholder --project "$PWD" --display-width <w> --display-height <h> --anchor-shape-id <selected-shape-id> --format json`.
   Use the requested or intended output aspect ratio for `<w>` and `<h>`; when
   unspecified, use a practical square such as `1024 x 1024`.
3. The placeholder placement follows the Moodbook canvas rule: use an 80 canvas
   unit gap; prefer the clear space immediately to the right of the selected or
   referenced image; otherwise place below the existing image/placeholder group;
   scan right and downward in a grid using the output size plus the gap; never
   intentionally overlap existing image or placeholder shapes.
4. Generate the image with the current agent's image model/tool.
5. Resolve the exact local bitmap produced by this generation call. Do not use a
   stale file from a previous generation.
6. Replace the placeholder with the generated bitmap:
   `openpanels-local insert-image --project "$PWD" --image <generated-path> --replace-shape-id <placeholder-shape-id> --format json`.
   This replacement step is required; do not merely show the generated image in
   chat.
7. Do not reload the in-app Browser after inserting or replacing canvas content.
   The studio syncs project state into the existing canvas view; reloading loses
   the user's current zoom and pan position.

Workflow:

0. Use the `openpanels-local` CLI. If `command -v openpanels-local` fails, use
   `npx -y @openpanels/local-cli@latest` in place of `openpanels-local`.
1. If the user asks to open or activate the canvas, run
   `openpanels-local studio start --project "$PWD" --format json`, then open the
   returned `serverUrl` in the Codex in-app Browser side panel. Make the in-app
   Browser visible so the canvas appears on the agent's right side. Do not use
   `openpanels-local studio open` unless the user explicitly asks to open the
   canvas in their external/system browser.
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
6. To place a generated or local bitmap into MyOpenPanels without a placeholder,
   run
   `openpanels-local insert-image --project "$PWD" --image <path> --placement right
   --format json`.

Do not infer selection from screenshots or hand-write `.myopenpanels/` files.

The local studio stores state in the active project's `.myopenpanels/`
directory and syncs the current canvas selection there so agents can read it
later with `openpanels-local selection`.
