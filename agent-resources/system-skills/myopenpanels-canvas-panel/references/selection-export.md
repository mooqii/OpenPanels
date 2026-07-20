# Export A Canvas Selection

Use this reference only when the user explicitly asks for a copy of the current
Canvas selection at a particular output path.

1. Require the Canvas panel to be active and verify that the selection is
   explicit.
2. Ask for an output path only when the user did not provide one.
3. Run the advertised `canvas.selection.export` command once.
4. Report the returned output path. Do not use the exported file as routine
   preparation for generation or editing.

Stop on an empty or fallback selection. Never silently export the latest Canvas
image in place of an explicit selection.
