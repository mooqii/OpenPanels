# Create My Documents

Use the my-documents module for standalone document deliverables such as
reports, plans, proposals, research summaries, and specifications.

- Write document deliverables as UTF-8 Markdown.
- Do not register ordinary code changes, temporary notes, chat explanations, or
  non-document outputs.
- Begin `my-document.create` before writing a new deliverable. Begin
  `my-document.revise` against the existing My Document id for a revision.
- Complete the captured operation with the local UTF-8 document file. The
  operation remains bound to its original Project and Wiki panel if the user
  switches elsewhere.
- Stop on `content_conflict` rather than overwriting a document that changed
  after the Operation began.
- Mark model or tool failures as failed and user-requested stops as cancelled.
- Publishing into raw Wiki sources remains a separate explicit user action.

Completion criteria:

- The My Document is visible in the captured Wiki panel.
- The operation is completed, failed, or cancelled explicitly.
