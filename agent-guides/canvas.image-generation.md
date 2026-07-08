---
id: canvas.image-generation
title: Generate And Place Canvas Images
source: builtin
appliesTo:
  - canvas
taskTypes:
requiresCapabilities:
  - canvas.selection.read
  - canvas.selection.asset.read
  - canvas.placeholder.create
  - canvas.image.insert
loadWhen:
  - User asks to generate, redraw, restyle, or edit an image on the canvas.
tokens: medium
---

Use this guide when the user wants an image result placed back onto the
OpenPanels canvas.

Workflow:

1. Read the current canvas selection.
2. If selected pixels are useful as reference context, export them with
   `read-selection-asset`.
3. Create a placeholder before calling the image model. Use the intended aspect
   ratio and place it near the selected/reference item.
4. Generate or edit the bitmap with the current agent's image tool.
5. Insert the exact generated bitmap with `--replace-shape-id` using the
   placeholder id.
6. Do not reload the browser after insertion; the studio syncs project state into
   the open canvas.

Placement rules:

- Prefer clear space immediately to the right of the selected/reference image.
- Otherwise place below the existing image or placeholder group.
- Keep about an 80 canvas unit gap.
- Never intentionally overlap existing images or placeholders.

Completion criteria:

- The generated bitmap is inserted into OpenPanels.
- The placeholder is replaced when one was created.
- The final answer mentions the inserted result, not just the local file path.
