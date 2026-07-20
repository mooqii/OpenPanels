---
name: myopenpanels-writing-panel
description: Use before reading or executing submitted writing requests and placing their results in the shared Wiki generated-document module.
---

Use this Skill as the operating contract for the MyOpenPanels Writing panel.
Writing reads and writes target the Project's Writing panel directly without
requiring or changing the active panel. Only current-selection reads depend on
which panel the user has active.

During a claimed v3 Task, Writing commands use Task Broker. Generated documents
and refined Skills remain staged until Task completion; an Operation marked
`prepared` is not yet a visible successful result. Never access shared storage
or SQLite directly.

Outside a claimed Task, selected source metadata includes verified local paths
and read actions. Prefer the read action because it returns content directly;
use the local path for oversized content or file-oriented tools. Read a Wiki
root only when its `localAccess.status` is `ready`, materializing it first when
the status is `on_demand`.

Intent routing:

- To inspect the source and Wiki context currently selected in Writing, read
  `references/knowledge-context.md`.
- To execute a claimed `generate_document` request, read
  `references/execute-writing-request.md`.
- To execute a claimed `refine_writing_skill` request, read
  `references/refine-writing-skill.md`.

Never replace a newer revision after `content_conflict`. Do not read the whole
Wiki when a targeted search is sufficient. The generated result must remain
bound to the Wiki panel captured by the task even if the visible panel changes.
