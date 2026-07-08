---
id: canvas.selection-reference
title: Use Canvas Selection As Reference
source: builtin
appliesTo:
  - canvas
taskTypes:
requiresCapabilities:
  - canvas.selection.read
  - canvas.selection.asset.read
loadWhen:
  - The user refers to selected canvas content or asks to use a visual reference.
tokens: short
---

Use this guide when the current canvas selection is relevant to the task.

Workflow:

1. Read selection metadata with `canvas.selection.read`.
2. Use `selectedShapes`, bounds, shape ids, and image metadata from the CLI
   result. Do not infer selection from screenshots.
3. If pixels are needed, use `canvas.selection.asset.read` or selection with
   image base64.
4. If there is no explicit selection, the CLI may return a fallback such as the
   latest image. Treat fallback context as useful but not user-confirmed
   selection.

Completion criteria:

- The agent used CLI selection data, not guessed screen state.
- Any generated or edited output is placed back onto the canvas when requested.
