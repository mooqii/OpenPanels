# Insert An Existing Canvas Image

Use this reference when the user already has a bitmap file that should be added
to the Canvas without invoking an image model.

1. Verify that the source is a readable bitmap and preserve its file name when
   useful.
2. Use `canvas.image.create` with explicit display dimensions when the user gave
   them; otherwise allow the CLI defaults.
3. Use replacement or anchor shape ids only when the user identified that
   target. Do not infer a replacement from fallback content.
4. Preserve supplied metadata and allow automatic clear-space placement unless
   the user requested a supported placement.

Completion means the CLI returned the inserted shape id in the intended Project
Canvas.
