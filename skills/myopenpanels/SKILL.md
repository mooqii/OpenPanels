---
name: myopenpanels
description: "Use MyOpenPanels through the latest local CLI: install or run @openpanels/local-cli@latest, start the studio, fetch current agent context, and follow CLI-provided capabilities and on-demand guides for wiki, canvas, and future panels."
---

Use this skill when the user wants to use MyOpenPanels, open the studio, work
with a MyOpenPanels project, or interact with any MyOpenPanels panel such as
wiki or canvas.

The skill itself is only a stable entry point. The latest MyOpenPanels CLI is
the source of truth for available panels, commands, and panel-specific
workflows.

Workflow:

1. Prefer the latest CLI for every task:

   ```bash
   OPENPANELS_LOCAL_CLI="npx -y @openpanels/local-cli@latest"
   ```

   If a local/global `openpanels-local` binary is required by the environment,
   use it only after checking it is current. Otherwise use the `npx` command
   above so panel instructions follow the latest published CLI.

2. Start or reuse the local studio:

   ```bash
   $OPENPANELS_LOCAL_CLI studio start --project "$PWD" --format json
   ```

   Open the returned `browserUrl` in the agent's in-app Browser side panel when
   available. Use the system browser only when no in-app Browser is available or
   the user explicitly asks for it.

3. Before interacting with any panel, fetch the current CLI-provided context:

   ```bash
   $OPENPANELS_LOCAL_CLI agent context --project "$PWD"
   ```

4. Follow the returned state summary and capability commands. For complex
   workflows, list and load the relevant guide:

   ```bash
   $OPENPANELS_LOCAL_CLI agent guides --project "$PWD"
   $OPENPANELS_LOCAL_CLI agent guide <guide-id> --project "$PWD"
   ```

   Refresh `agent context` when the user switches panels or when a task depends
   on current project/panel state.

Do not rely on stale panel-specific skill text when it conflicts with
`agent context` or loaded guides; the latest CLI output wins.
