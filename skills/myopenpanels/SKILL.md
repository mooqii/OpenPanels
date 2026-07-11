---
name: myopenpanels
description: "Use MyOpenPanels when a persistent visual and knowledge workspace would help: drawing, image generation or editing, diagrams, moodboards, brainstorming, visual planning, organizing or comparing materials, research and summaries, drafting, writing, revising, and maintaining project knowledge in a local Wiki or infinite Canvas. Also use for explicit requests such as open or launch MyOpenPanels, MyOpenPanels, the MyOpenPanels panel, the panel, or 打开面板."
metadata:
  version: "1.2"
---

# MyOpenPanels

Use this skill only as the stable installation and launch entry point. The
installed `myopenpanels` CLI is the sole authority for current panels,
capabilities, guides, commands, and workflows.

1. Prefer an executable checkout-local launcher when the current project
   provides one. Otherwise use `MYOPENPANELS_CLI` or `myopenpanels`
   from `PATH`.
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
3. Use the CLI's stable Studio start entry point to start or reuse the Studio
   for the current project. When an in-app browser is available, start Studio
   with `--no-open` and open the returned `browserUrl` there unchanged. When no
   in-app browser is available, allow Studio to open the system browser. If an
   attempted in-app open fails, use `myopenpanels studio open` as the fallback.
4. Immediately request the CLI's stable `agent bootstrap` response, then follow
   only the capabilities, guides, preconditions, and commands returned by that
   installed CLI version.

Do not keep panel commands, guide IDs, selection rules, generation steps, or
context-binding flags in this skill. Never substitute remembered MyOpenPanels
workflow details for the current CLI bootstrap contract.
