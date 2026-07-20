# Edit A Selected Canvas Image

Use this reference for redraw, restyle, retouch, or other bitmap edits based on
an explicitly selected Canvas image.

1. Require the Canvas panel to be active and an explicit image-capable
   selection to be present.
2. Begin `canvas.image.generate` with selection enabled before invoking the
   image model. This captures an immutable reference and the target placement.
3. Load the selected Canvas workflow Skill when one is advertised, then perform
   the edit with the current Agent image tool.
4. Preserve the exact prompt, model id when known, source asset, shape ids, and
   reference path in result metadata.
5. Complete the captured Operation with the resulting bitmap, or explicitly
   fail or cancel it.

Never replace a missing explicit selection with Canvas fallback content.
