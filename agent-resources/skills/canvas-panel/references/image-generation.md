# Generate And Place Canvas Images

Use this reference when an image result must be placed into MyOpenPanels Canvas.

Workflow:

1. Start a Canvas generation operation before calling the image model. Declare
   whether explicit selection is used and provide intended display dimensions.
2. Stop on `explicit_selection_required`; never replace an explicit reference
   with fallback content.
3. Load any selected workflow skill as described in
   `workflow-skill-routing.md`, then generate or edit the bitmap with the current
   agent image tool.
4. Do not replace a failed image-model call with hand-written Pillow, SVG, or
   Canvas drawing unless the user explicitly requests manual or vector output.
5. Write metadata containing the exact prompt, model id when known, and all
   reference images. Preserve local paths, shape ids, and asset refs when
   available.
6. Complete the captured operation with the exact bitmap and metadata file. If
   its placeholder was removed, allow the CLI to insert the result into clear
   space.
7. Mark model failures as failed and user-requested stops as cancelled.
8. Do not reload the Studio after completion; live sync updates the Canvas.

Prefer clear space to the right of a selected reference, otherwise below it,
with roughly 80 Canvas units of separation. Never intentionally overlap
existing content.
