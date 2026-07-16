# Route Wiki Authoring To The Selected Skill

Use this reference for `ingest_markdown_into_wiki` and `maintain_wiki` tasks.

The Wiki panel skill owns MyOpenPanels context and task lifecycle. The selected
authoring skill owns language, document conversion, page structure, synthesis,
index, log, provenance, and editorial rules.

Workflow:

1. Confirm the current task id, task type, raw document id when present, and Wiki
   space id from CLI task context.
2. Read the selected authoring skill id from `state.wiki.agentSkillId` in Agent
   Bootstrap or the task-specific loader context.
3. Load it with `agent skill <skill-id> --task-id <task-id>`, then read its local
   `SKILL.md` and only the references it routes to for this task type.
4. Claim the task before writing unless the task bridge already owns lifecycle.
5. Perform all Markdown and Wiki page writes through the CLI with the current
   task id.
6. Complete the task on success. Fail it with an actionable error when it cannot
   be completed reliably.

Do not replace the selected authoring skill with rules remembered from another
Wiki style. Do not duplicate its writing method in this panel skill.
