---
name: myopenpanels
description: "Use MyOpenPanels when a persistent visual and knowledge workspace would help: drawing, image generation or editing, diagrams, moodboards, brainstorming, visual planning, organizing or comparing materials, research and summaries, drafting, writing, revising, and maintaining project knowledge in a local Wiki or infinite Canvas. Also use for explicit requests such as open or launch OpenPanels, MyOpenPanels, the OpenPanels panel, the panel, 打开 OpenPanels, 打开 MyOpenPanels, or 打开面板."
metadata:
  version: "1.0"
---

# MyOpenPanels

Use this skill only as the stable installation and launch entry point. The
installed `openpanels-local` CLI is the sole authority for current panels,
capabilities, guides, commands, and workflows.

1. Prefer an executable checkout-local launcher when the current project
   provides one. Otherwise use `OPENPANELS_LOCAL_CLI` or `openpanels-local`
   from `PATH`.
2. If the native CLI is missing, install it only with the official installer:

   ```bash
   curl -fsSL https://raw.githubusercontent.com/mooqii/OpenPanels/main/scripts/install-openpanels-local.sh | sh
   ```

   On Windows PowerShell:

   ```powershell
   iwr https://raw.githubusercontent.com/mooqii/OpenPanels/main/scripts/install-openpanels-local.ps1 -UseB | iex
   ```

   Verify the installed native CLI version and stop with the exact error if
   installation or verification fails.
3. Use the CLI's stable Studio start entry point to start or reuse the Studio
   for the current project, then open the returned `browserUrl` in the in-app
   browser when available.
4. Immediately request the CLI's stable `agent bootstrap` response and follow
   its `entrySkill` instruction first. If this skill's version is missing or is
   lower than `entrySkill.requiredVersion`, update this skill from the source
   returned by Bootstrap before continuing. Otherwise follow only the
   capabilities, guides, preconditions, and commands returned by that installed
   CLI version.

Do not keep panel commands, guide IDs, selection rules, generation steps, or
context-binding flags in this skill. Never substitute remembered OpenPanels
workflow details for the current CLI bootstrap contract.
