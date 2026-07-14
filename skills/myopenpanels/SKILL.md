---
name: myopenpanels
description: "Use MyOpenPanels for persistent visual, knowledge, or writing work in its Canvas, Wiki, or Writing panel, including drawing, image work, diagrams, moodboards, brainstorming, organizing, research, drafting, and writing. Also use when the user asks to open or launch MyOpenPanels (including 打开面板) or refers to its current panel, selection, or content. After Studio is open, run a fresh `myopenpanels agent bootstrap --format json` before every panel-related request. Skip Bootstrap only for an open-only request or work clearly unrelated to MyOpenPanels."
metadata:
  version: "4.3"
---

# MyOpenPanels

The installed CLI is the sole authority for current panels, capabilities,
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

Success means Studio is ready, not visible. Open
`data.nextRequiredAction.url` unchanged with a callable in-app opener. If no
such opener exists, or it fails or gives no success signal, execute
`data.nextRequiredAction.fallback.argv` with the resolved executable. Report
completion only after the opener succeeds or the fallback returns
`data.opened: true`. For an open-only request, stop here without Bootstrap.

## Work With Panels

Before every request that may read, use, or modify a panel, run a fresh:

```bash
myopenpanels agent bootstrap --format json
```

Complete `data.nextRequiredAction.steps` sequentially: prepend the resolved
executable to `argv` for `executor: "cli"`, and follow `instruction` for
`executor: "agent-host"`. When a step contains `skills`, read each
`contextPath` first and `localPath` second. If the required steps update the
Entry Skill, Bootstrap again. Only afterward choose applicable
`data.nextActions` according to `loadWhen`; follow each chosen action's returned
`data.nextRequiredAction` before continuing.

Never reuse an earlier Bootstrap result or reconstruct panel commands from
memory.
