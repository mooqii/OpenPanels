---
name: myopenpanels-canvas-panel
description: Use before reading Canvas selections or generating, editing, arranging, or inserting visual content through a MyOpenPanels Canvas panel.
---

Use this skill as the required operating contract for an MyOpenPanels Canvas
panel. It defines safe Canvas selection, generation, and placement mechanics. It
does not define an artistic style or prompt-writing method.

Intent routing:

- When selected Canvas content or a visual reference matters, read
  `references/selection.md`.
- When generating, redrawing, restyling, or editing a bitmap for the Canvas,
  read `references/image-generation.md`.
- Before invoking an image model, read `references/workflow-skill-routing.md` to
  determine whether a separate drawing workflow skill was selected.

Core rules:

- Canvas reads and writes target the Project's Canvas panel directly; they do
  not require or change the active panel.
- CLI state and selection data are authoritative; do not infer selection from a
  screenshot.
- Selection is the exception to panel-kind targeting: read or use it only when
  Canvas is the active panel.
- Use the image `localPath` returned by selection read directly. Do not export
  the selection as a routine preparation step.
- Treat selection export as an exceptional user-facing copy operation. Discover
  `canvas.selection.export` only when the user explicitly requests a file at a
  particular path.
- Never treat fallback content as an explicit user selection.
- Begin target-bound generation before invoking an external image model.
- Complete against the captured Canvas target even if the user switches panels.
- Preserve generation prompt, model, references, and source asset metadata.
- Do not intentionally overlap existing images or placeholders.
- Use commands advertised by the current CLI instead of remembered syntax.

Completion criteria:

- The required references and any selected workflow skill were loaded.
- The result is inserted into the intended Canvas and the operation lifecycle is
  closed explicitly.
