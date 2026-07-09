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
2. If the user referred to selected canvas content and `isExplicitSelection` is
   false, stop and ask the user to select the intended canvas item. Do not use
   fallback images as references unless the user explicitly asks for fallback.
3. If selected pixels are useful as reference context, export them with
   `canvas selection export`.
4. Create a placeholder before calling the image model. Use the intended aspect
   ratio and place it near the selected/reference item.
5. Generate or edit the bitmap with the current agent's image tool. Do not
   replace this step with hand-written Pillow, SVG, or canvas drawing unless the
   user explicitly asks for manual/vector rendering.
6. If the image tool does not clearly return an output file, locate the generated
   image file or report the image tool failure. Do not switch to a manual drawing
   fallback.
7. Write a small metadata JSON file for the generated image. Store the exact
   prompt sent to the image model under `generateOptions.prompt`, the model id
   under `generateOptions.model` when known, and every reference image under
   `generateOptions.referenceImages`. For local reference images, include the
   local file path. For canvas references, include any available `shapeId`,
   `assetRef`, and exported local path from `canvas selection export`.
8. Insert the exact generated bitmap with `--replace-shape-id` using the
   placeholder id.
   Pass the metadata file with `--metadata-file <metadata.json>`.
   If the placeholder no longer exists, the CLI will still insert the image into
   clear canvas space instead of failing the task.
9. Verify once that `canvas image insert` succeeded and the latest `canvas state`
   contains the returned shape and asset ids.
10. Do not reload the browser after insertion; the studio syncs project state into
   the open canvas.

Recommended metadata shape:

```json
{
  "generateOptions": {
    "prompt": "exact prompt used for generation",
    "model": "image-model-id-if-known",
    "referenceImages": [
      {
        "source": "local_path",
        "path": "/absolute/path/to/reference.png",
        "role": "reference"
      }
    ]
  },
  "generatedBy": "agent"
}
```

Placement rules:

- Prefer clear space immediately to the right of the selected/reference image.
- Otherwise place below the existing image or placeholder group.
- Keep about an 80 canvas unit gap.
- Never intentionally overlap existing images or placeholders.

Completion criteria:

- The generated bitmap is inserted into OpenPanels.
- The placeholder is replaced when it still exists; otherwise the generated
  image is inserted into clear canvas space.
- The image asset metadata includes the generation prompt and reference-image
  records when a generated/edited bitmap used a prompt or references.
- The final answer mentions the inserted result, not just the local file path.
