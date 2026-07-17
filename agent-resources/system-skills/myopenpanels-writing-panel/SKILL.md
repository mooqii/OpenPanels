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

Route `generate_document` tasks through the document workflow below. Route
`refine_writing_skill` tasks through the task-selected `提炼写作` Skill; those
tasks may read selected raw and generated documents but must never read the
selected Wiki.

Workflow:

1. Read the claimed task with `writing request read`. Treat its instruction,
   mode, target, selected Writing Skill, and captured context as immutable.
2. Load the task-selected Writing Skill using the returned required action and
   follow that Skill's authoring rules for the complete result.
3. Read every explicitly selected raw or generated document that is relevant.
   When the Wiki itself is selected, search it and read only relevant pages.
4. In revision mode, read the captured target document before drafting.
5. Begin the task-bound Writing generation Operation before producing the
   document. Derive a concise title from the instruction. Use Markdown unless
   the user explicitly requests plain text.
6. Write in the language requested by the user; otherwise follow the language
   of the submitted instruction.
7. Complete the Operation with the UTF-8 result file, then complete the Task.
   Fail the Task explicitly after model, file, target, or version conflicts.

Never replace a newer revision after `content_conflict`. Do not read the whole
Wiki when a targeted search is sufficient. The generated result must remain
bound to the Wiki panel captured by the task even if the visible panel changes.
