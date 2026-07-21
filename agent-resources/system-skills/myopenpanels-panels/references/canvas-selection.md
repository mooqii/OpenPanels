# Use Canvas Selection As Reference

Use this reference when selected Canvas content is relevant to the task.

Execution Steps:

1. Read selection metadata from the CLI.
2. Use returned shapes, bounds, shape ids, and image metadata. Do not infer
   selection from screenshots.
3. If `isExplicitSelection` is false, stop when the request requires an explicit
   reference. Do not substitute the reported fallback.
4. Use `selection.image.localPath` directly when the selection reader returns
   it. A simple image points to its persistent source asset; a rendered
   selection points to an immutable, short-lived composite.
5. Do not load or invoke selection export during normal Canvas work. Only when
   the user explicitly requests a copy at a particular path, discover
   `canvas.selection.export` through the current CLI capability catalog and use
   its returned action.
6. Carry available source information into generated image metadata, including
   shape id, asset ref, returned local path, and existing generation metadata.

Selection composites are rendered lazily when detailed selection data is read.
Do not copy, rename, overwrite, or delete the returned file. Starting a Canvas
generation with `--use-selection` copies the reference into the Operation so it
remains valid if the user changes selection.

A fallback such as the latest image may be useful context, but it is not a
user-confirmed selection.
