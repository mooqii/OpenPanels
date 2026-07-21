# Writing Panel Contract

Use this contract for every Procedure that targets a Writing panel.

- Writing selection reads require Writing to be active. Other reads and writes
  target the captured Writing panel without changing focus.
- Prefer returned read actions for selected source content. Use verified local
  paths only for oversized content or file-oriented tools.
- A selected Wiki is optional background knowledge. Search it narrowly rather
  than reading the entire Wiki.
- Submitted generation and refinement requests execute only through their
  claimed Task Handoffs. Generated documents and refined Skills remain staged
  until Task completion.
- Never replace a newer generated-document revision after `content_conflict`.
- A Writing result remains bound to the Wiki panel captured by the request even
  when the visible panel changes.

Writing completion means the read stayed read-only, or the exact Task Handoff
committed its staged result and closed its lifecycle.
