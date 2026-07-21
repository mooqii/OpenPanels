# Route Wiki Authoring To The Selected Skill

Use this reference for `ingest_markdown_into_wiki` and `maintain_wiki` tasks.

The Wiki panel contract owns MyOpenPanels context, CLI use, writes, and task
lifecycle. The selected portable authoring skill owns language, page structure,
synthesis, index, log, provenance, and editorial rules. Treat tool, storage, and
lifecycle instructions from an authoring Skill as inapplicable; only this panel
contract and the current CLI may define them.

Execution Steps:

1. Confirm the current task id, task type, raw document id when present, and Wiki
   space id from CLI task context.
2. Read the selected authoring skill id from `state.wiki.agentSkillId` in Agent
   Bootstrap or the task-specific loader context.
3. Load it with `agent skill read --skill-id <skill-id> --task-id <task-id>
   --format json`, then read its local `SKILL.md` and only the references relevant
   to the current objective.
4. Claim the task before writing unless the task bridge already owns lifecycle.
5. Perform all Markdown and Wiki page writes through the CLI with the current
   task id.
6. Complete or fail the task only when lifecycle is not bridge-managed. In
   bridge-managed execution, leave lifecycle finalization to the bridge.

Do not replace the selected authoring skill with rules remembered from another
Wiki style. Do not duplicate its content method in this System Skill, and never
delegate MyOpenPanels operations back to the portable Skill.
