---
name: myopenpanels
description: "Use MyOpenPanels for persistent visual, knowledge, or writing work in its Canvas, Wiki, or Writing panel, including drawing, image work, diagrams, moodboards, brainstorming, organizing, research, drafting, and writing. Also use when the user asks to open or launch MyOpenPanels (including 打开面板) or refers to its current panel, selection, or content. After Studio is open, run a fresh `myopenpanels agent bootstrap --format json` before every panel-related request. Skip Bootstrap only for an open-only request or work clearly unrelated to MyOpenPanels."
metadata:
  version: "5.1"
---

# MyOpenPanels

The installed CLI is the sole authority for current panels, command catalogs,
Skills, commands, and workflows.

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
Before every request that may read, use, or modify a panel, run a fresh:

```bash
myopenpanels agent bootstrap --format json
```

Execute `actions.required` sequentially. For `executor: "cli"`, prepend the
resolved executable to the returned `argv`; for typed file, URL, Skill, or host
actions, use the matching executor without translating the action into a shell
command. If a required action updates the Entry Skill, Bootstrap again. Only
after required actions finish, choose applicable `actions.suggested` entries by
their structured conditions. Use `agent catalog --domain <domain>` actions to
load complete command descriptions for the domains needed by the selected
Skills.

Never reuse an earlier Bootstrap result, execute display text, or reconstruct
panel commands from memory.
