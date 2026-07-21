# Canvas Panel Contract

Use this contract for every Procedure that targets a Canvas panel.

- CLI selection data and Canvas state are authoritative; do not infer selection
  from a screenshot.
- Use a selected image's returned `localPath` directly. Do not export a
  selection as routine preparation.
- Selection export is only a user-facing copy operation requested for an
  explicit destination path.
- Never treat fallback content as an explicit selection.
- Capture Canvas placement before invoking an image model. Preserve prompt,
  model, references, source asset, and shape metadata in the result.
- Do not intentionally overlap existing images or placeholders. If a captured
  placeholder disappears, allow the CLI to choose clear space.

Canvas completion means the intended shape exists and any owning Operation has
been completed, failed, or cancelled explicitly.
