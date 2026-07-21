# Read A Generated Wiki Document

Use this reference when the user asks to inspect a standalone generated
document rather than generated Wiki pages or a raw source.

1. Prefer an explicitly selected generated document.
2. When the user names a document and selection does not resolve it, list
   generated documents and match the title or id without guessing between
   ambiguous candidates.
3. Read the document through `wiki.document.read`; use verified local access
   only for oversized content or file-oriented tools.
4. Keep the operation read-only unless the user separately requests revision,
   publication, or deletion.
