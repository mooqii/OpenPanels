# Read A My Document

Use this reference when the user asks to inspect a My Document rather than
generated Wiki pages or a Wiki raw source.

1. Prefer an explicitly selected My Document.
2. When the user names a document and selection does not resolve it, list
   My Documents and match the title or id without guessing between
   ambiguous candidates.
3. Read the document through `my-document.read`; use verified local access
   only for oversized content or file-oriented tools.
4. Keep the operation read-only unless the user separately requests revision,
   publication, or deletion.
