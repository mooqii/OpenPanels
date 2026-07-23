# Import A Wiki Raw Document

Use this reference when the user asks to add a source file or Markdown text to
the Wiki's raw-document layer.

1. Preserve the original bytes, file name, title, and MIME type when available.
2. Use `wiki.raw.create` against the intended Wiki space. Do not create a
   My Document or generated Wiki page instead.
3. Treat any conversion or Wiki authoring Tasks created by the import as
   asynchronous work owned by the Task system.
4. Report the returned raw document id and any created Task state.

Do not summarize, rewrite, or pre-convert the source during import.
