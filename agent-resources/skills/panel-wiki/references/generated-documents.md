# Store Generated Documents In Wiki

Use the generated-documents module for standalone document deliverables such as
reports, plans, proposals, research summaries, and specifications.

- Write document deliverables as UTF-8 Markdown.
- Do not register ordinary code changes, temporary notes, chat explanations, or
  non-document outputs.
- Begin a Wiki generation operation before writing the deliverable. For a
  revision, begin against the existing generated document id instead of creating
  a duplicate.
- Complete the captured operation with the local UTF-8 document file. The
  operation remains bound to its original Project and Wiki panel if the user
  switches elsewhere.
- Stop on `content_conflict` rather than overwriting a document that changed
  after generation began.
- Mark model or tool failures as failed and user-requested stops as cancelled.
- Publishing into raw Wiki sources remains a separate explicit user action.

Completion criteria:

- The generated document is visible in the captured Wiki panel.
- The operation is completed, failed, or cancelled explicitly.
