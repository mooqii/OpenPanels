---
name: myopenpanels
description: "Use MyOpenPanels when a persistent visual and knowledge workspace would help: drawing, image generation or editing, diagrams, moodboards, brainstorming, visual planning, organizing or comparing materials, research and summaries, drafting, writing, revising, and maintaining project knowledge in a local Wiki or infinite Canvas. Also use for explicit requests such as open or launch MyOpenPanels, MyOpenPanels, the MyOpenPanels panel, the panel, or 打开面板."
metadata:
  version: "1.3"
---

# MyOpenPanels

Use this skill only as the stable installation and launch entry point. The
installed `myopenpanels` CLI is the sole authority for current panels,
capabilities, guides, commands, and workflows.

1. Resolve and start the CLI in one shell call. Prefer an executable
   checkout-local launcher, then `MYOPENPANELS_CLI`, then `myopenpanels` from
   `PATH`. Start or reuse Studio with `studio start --project-dir "$PWD"
   --format json`. This command never opens a browser and returns only after
   the current Project is ready.
2. If the native CLI is missing, install it only with the official installer:

   ```bash
   curl -fsSL https://raw.githubusercontent.com/mooqii/OpenPanels/main/scripts/install-myopenpanels.sh | sh
   ```

   On Windows PowerShell:

   ```powershell
   iwr https://raw.githubusercontent.com/mooqii/OpenPanels/main/scripts/install-myopenpanels.ps1 -UseB | iex
   ```

   Verify the installed native CLI version and stop with the exact error if
   installation or verification fails.
3. Read `embeddedBrowserUrl` from the JSON response. When an in-app Browser is
   available, use the host Agent's in-app Browser capability to open the URL
   exactly as returned. Do not substitute an external or system browser while
   an in-app Browser is available.
4. If the user only asked to open the panel, stop after the in-app Browser is
   open. Do not request Agent Bootstrap merely to verify that Studio opened.
5. Before Wiki, Canvas, or task work, request `agent bootstrap --format json`,
   then follow only the capabilities, guides, preconditions, and commands
   returned by the installed CLI version.
6. Do not respond to a Bootstrap error by creating or listing Projects,
   guessing Project ids, inspecting CLI help, or opening the system browser.
   Only when the actual in-app Browser open attempt fails, or the host has no
   in-app Browser, use `studio open-system-browser --project-dir "$PWD"`.

Do not keep panel commands, guide IDs, selection rules, generation steps, or
panel-operation flags in this skill. Never substitute remembered MyOpenPanels
workflow details for the current CLI bootstrap contract.
