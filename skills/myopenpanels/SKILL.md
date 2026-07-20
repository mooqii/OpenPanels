---
name: myopenpanels
description: "Use MyOpenPanels for persistent visual, knowledge, or writing work in its Canvas, Wiki, or Writing panel, including drawing, image work, diagrams, moodboards, brainstorming, organizing, research, drafting, and writing. Also use when the user asks to open or launch MyOpenPanels (including 打开面板) or refers to its current panel, selection, or content. After Studio is open, use the matching Agent Procedure Bootstrap when intent is clear and generic Agent Bootstrap only as fallback. Skip Bootstrap only for an open-only request or work clearly unrelated to MyOpenPanels."
metadata:
  version: "5.6"
  source: "https://github.com/mooqii/OpenPanels/tree/main/skills/myopenpanels"
---

# MyOpenPanels

The installed CLI is the sole authority for current panels, command catalogs,
Skills, commands, Procedures, Task Handoffs, and Workflow Runs.

## Resolve The CLI

Resolve the executable once: prefer an executable path from
`MYOPENPANELS_CLI`, then a runnable `myopenpanels` from `PATH`. If neither
resolves, read and follow [the installation reference](references/install.md).
Keep the exact resolved executable for every returned CLI action; never execute
display command text.

## Open Studio

Studio is shared by all Agents using the same MyOpenPanels storage. Starting it
reuses that single user-visible workspace instead of creating an Agent-specific
Studio.

Run with the resolved executable:

```bash
myopenpanels studio start --local-only --project-dir "$PWD" --format json
```

Success means Studio is ready, not visible. Execute `actions.required` in order.

Treat an `open-url` action as a host display action, not as browser automation:

1. Open the returned URL unchanged with the current Agent host's native
   embedded browser, webview, preview, or equivalent in-host URL opener.
2. Prefer that embedded surface when the user simply asks to open or launch
   MyOpenPanels without naming a browser.
3. Do not initialize browser automation, inspect the page, load browser-control
   instructions, or use Playwright merely to display Studio. If the host only
   exposes an embedded browser through a control API, perform only the minimum
   operation needed to open the URL.
4. Treat the embedded open as successful only after the host returns a success
   signal. Studio being ready at the URL is not proof that it is visible.

Execute the CLI fallback action only when the embedded opener is unavailable,
fails, or gives no success signal. A fallback to an external or system browser
is not an embedded-open success; state that fallback clearly. If the user
explicitly requested an embedded surface, report the failure instead of
silently substituting an external browser.

For an open-only request, stop after an opener succeeds. Do not run Bootstrap,
inspect Studio, or search the repository.

## Work With Panels

When `MYOPENPANELS_TASK_ID`, `MYOPENPANELS_TASK_BROKER_URL`, and
`MYOPENPANELS_TASK_TOKEN` are present, this is an isolated claimed Task: do not
start Studio or Bootstrap; follow its prompt and task-id-bound broker commands.
For a Studio-generated `task handoff start` command, run that exact command
instead of Bootstrap. Follow its ExecutionBundle and Delivery Contract; create
only the declared workspace artifacts and execution result. The Runtime owns
Operation creation, output staging, and Task completion. Do not perform
separate Catalog or Skill discovery.
When the request clearly matches one entry below, run a fresh Procedure
Bootstrap directly:

```bash
myopenpanels agent bootstrap --procedure <procedure-key> --format json
```

Canvas:

- `panel.canvas.selection.read`: read the current Canvas selection.
- `panel.canvas.selection.export`: export an explicit selection to a requested path.
- `panel.canvas.image.insert`: insert an existing bitmap.
- `panel.canvas.image.generate`: generate a new bitmap without requiring selection.
- `panel.canvas.image.edit`: edit or restyle an explicit selected image.

Wiki:

- `panel.wiki.knowledge.query`: answer from Wiki or selected document knowledge.
- `panel.wiki.raw.import`: import a source into raw documents.
- `panel.wiki.document.read`: read a standalone generated document.
- `panel.wiki.document.generate`: generate a new standalone document.
- `panel.wiki.document.revise`: revise an existing standalone document.
- `panel.wiki.document.publish`: publish a generated document into raw sources.
- `panel.wiki.document.delete`: delete a generated document.
- `panel.wiki.space.manage`: list, activate, or materialize Wiki spaces.

Writing and Task queue:

- `panel.writing.context.read`: inspect selected Writing source context.
- `task.queue.inspect`: inspect Tasks, attempts, events, or persisted Workflow Runs.
- `task.queue.retry`: retry an explicitly identified failed Task.
- `task.queue.cancel`: cancel an explicitly identified Task.
- `task.queue.archive`: archive an explicitly identified terminal Task.

Task Handoffs must never be passed to Procedure Bootstrap:
`task.scope.execute`, `panel.wiki.raw.convert`, `panel.wiki.pages.maintain`,
`panel.writing.request.execute`, and `panel.writing.skill.refine`. Execute their
exact Studio or claimed Task handoff instead.

When intent is ambiguous, no Procedure matches, or the CLI reports
`agent_procedure_not_found`, run the generic fallback:

```bash
myopenpanels agent bootstrap --format json
```

Execute `actions.required` sequentially. For `executor: "cli"`, prepend the
resolved executable to the returned `argv`; for typed file, URL, Skill, or host
actions, use the matching executor without translating the action into a shell
command. If a required action updates the Entry Skill, Bootstrap again. Only
after required actions finish, choose applicable `actions.suggested` entries by
their structured conditions. Procedure Bootstrap returns only its relevant
command descriptions in `commands.items`; retain each returned command path and
flags, replace required placeholders with request values, and add only optional
flags declared by that descriptor. Generic Bootstrap may still return scoped
`agent catalog --domain <domain>` discovery actions.

Never reuse an earlier Bootstrap result, execute display text, or reconstruct
panel commands from memory.
