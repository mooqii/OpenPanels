# Revise A My Document

Use this reference when the user asks to revise an existing My Document.

1. Resolve exactly one target from explicit selection, document id, or an
   unambiguous title.
2. Read the current document before drafting.
3. Begin `my-document.revise` against the existing document id so the
   Operation captures its base content version.
4. Produce the complete replacement document and complete the Operation with
   the result file.
5. Stop on `content_conflict`; never overwrite a document that changed after the
   Operation began. Explicitly fail or cancel abandoned Operations.
