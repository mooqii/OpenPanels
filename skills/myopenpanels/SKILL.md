---
name: myopenpanels
description: "Use MyOpenPanels when the user wants a local wiki for preparing, organizing, and refining project documents, or an infinite canvas for drawing, visual planning, image work, and richer agent-user collaboration around diagrams and visual artifacts."
---

Use this skill when the user wants to use MyOpenPanels, prepare or organize
documents in a local wiki, open an infinite canvas, draw diagrams, work with
visual artifacts, or collaborate with an agent through wiki or canvas panels.

The skill itself is only a stable entry point. The installed `openpanels-local`
CLI is the source of truth for available panels, commands, and panel-specific
workflows.

Workflow:

1. Use the local CLI:

   ```bash
   if [ -x "$PWD/scripts/openpanels-local-dev" ]; then
     OPENPANELS_LOCAL_CLI="${OPENPANELS_LOCAL_CLI:-$PWD/scripts/openpanels-local-dev}"
   else
     OPENPANELS_LOCAL_CLI="${OPENPANELS_LOCAL_CLI:-openpanels-local}"
   fi
   $OPENPANELS_LOCAL_CLI --version
   ```

   In this repository, prefer `scripts/openpanels-local-dev` automatically so
   agents test the current checkout even when an older installed
   `openpanels-local` is on PATH.

   If `openpanels-local` is missing, ask the user to install the CLI for their
   platform before continuing. Do not fall back to the legacy npm wrapper unless
   the user explicitly asks for it.

2. Let the CLI manage update freshness:

   ```bash
   $OPENPANELS_LOCAL_CLI update check
   $OPENPANELS_LOCAL_CLI update
   ```

   The CLI may also perform an opportunistic update check at most once every
   24 hours on normal text-mode commands. Do not build your own polling loop.
   If `update check` reports an available update, do not install and restart
   unless the user explicitly asks you to. The studio may show an update button
   so the user can install and restart from the UI. If the binary is managed by
   a package manager and self-update is refused, follow the CLI's message or ask
   the user to update that package-manager install.

3. Start or reuse the local studio:

   ```bash
   $OPENPANELS_LOCAL_CLI studio start --project "$PWD" --format json
   ```

   Open the returned `browserUrl` in the agent's in-app Browser side panel when
   available. Use the system browser only when no in-app Browser is available or
   the user explicitly asks for it.

4. Before interacting with any panel, fetch the current CLI-provided context:

   ```bash
   $OPENPANELS_LOCAL_CLI agent context --project "$PWD"
   ```

5. Follow the returned state summary and capability commands. For complex
   workflows, list and load the relevant guide:

   ```bash
   $OPENPANELS_LOCAL_CLI agent guides --project "$PWD"
   $OPENPANELS_LOCAL_CLI agent guide <guide-id> --project "$PWD"
   ```

   Refresh `agent context` when the user switches panels or when a task depends
   on current project/panel state.

Do not rely on stale panel-specific skill text when it conflicts with
`agent context` or loaded guides; the latest CLI output wins.
