---
id: canvas.image-generation
title: Generate And Place Canvas Images
source: builtin
appliesTo:
  - canvas
taskTypes:
requiresCapabilities:
  - canvas.generation.begin
  - canvas.generation.complete
loadWhen:
  - User asks to generate, redraw, restyle, or edit an image on the canvas.
tokens: medium
---

Use this guide when the user wants an image result placed back onto the
OpenPanels canvas.

Workflow:

1. Start a Canvas generation operation before calling the image model. Declare
   whether the current explicit selection is a reference and provide the intended
   display dimensions. The CLI exports references, captures the original target,
   and creates the placeholder atomically.
2. If reference generation reports `explicit_selection_required`, stop and ask
   the user to select the intended Canvas item. Never use fallback images.
3. Generate or edit the bitmap with the current agent's image tool. Do not
   replace this step with hand-written Pillow, SVG, or canvas drawing unless the
   user explicitly asks for manual/vector rendering.
4. If the image tool does not clearly return an output file, locate the generated
   image file or report the image tool failure. Do not switch to a manual drawing
   fallback.
5. Write a small metadata JSON file for the generated image. Store the exact
   prompt sent to the image model under `generateOptions.prompt`, the model id
   under `generateOptions.model` when known, and every reference image under
   `generateOptions.referenceImages`. For local reference images, include the
   local file path. For canvas references, include any available `shapeId`,
   `assetRef`, and exported local path from `canvas selection export`.
6. Complete the captured operation with the exact bitmap and metadata file. The
   CLI replaces the placeholder in the original Canvas even if the user switched
   panels or Projects. If the placeholder was removed, it inserts into clear space.
7. On model failure, mark the operation failed. On user cancellation, cancel it.
8. Do not reload the browser after completion; the studio syncs project state into
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
- The operation is completed and its placeholder is replaced when it still exists; otherwise the generated
  image is inserted into clear canvas space.
- The image asset metadata includes the generation prompt and reference-image
  records when a generated/edited bitmap used a prompt or references.
- The final answer mentions the inserted result, not just the local file path.
