---
name: myopenpanels
description: "Use MyOpenPanels when a persistent visual and knowledge workspace would help: drawing, image generation or editing, diagrams, moodboards, brainstorming, visual planning, organizing or comparing materials, research and summaries, drafting, writing, revising, and maintaining project knowledge in a local Wiki or infinite Canvas. Also use for explicit requests such as open or launch MyOpenPanels, MyOpenPanels, the MyOpenPanels panel, the panel, or 打开面板."
metadata:
  version: "1.4"
---

# MyOpenPanels

Use this skill only as the stable installation and launch entry point. The
installed `myopenpanels` CLI is the sole authority for current panels,
capabilities, guides, commands, and workflows.

1. Resolve and start the CLI in one shell call. Prefer an executable
   checkout-local launcher, then `MYOPENPANELS_CLI`, then `myopenpanels` from
   `PATH`. Start or reuse Studio with `studio start --local-only --project-dir
   "$PWD" --format json`. This command never opens a browser and returns only
   after the current Project is ready.
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
3. Treat `ok: true` as Studio readiness only, not proof that the panel is
   visible. Read `nextRequiredAction.url` from the JSON response. Use an in-app
   Browser only when the host exposes a callable URL-open or Preview tool, and
   open the URL exactly as returned. The presence of a WorkBuddy Results Panel
   alone is not such a capability and does not require a separate Agent Browser
   Skill.
4. If the host has no callable in-app opener, or the open attempt is denied,
   fails, or returns no success signal, immediately run
   `studio open-system-browser --local-only --project-dir "$PWD" --format json`.
   Do not report success unless the in-app opener succeeds or this fallback
   returns `opened: true`. If the fallback fails, report the failure and include
   the returned URL so the user can open it manually.
5. If the user only asked to open the panel, stop after a browser opener has
   succeeded. Do not request Agent Bootstrap merely to verify that Studio
   opened.
6. Before Wiki, Canvas, or task work, request `agent bootstrap --format json`,
   then follow only the capabilities, guides, preconditions, and commands
   returned by the installed CLI version.

Do not keep panel commands, guide IDs, selection rules, generation steps, or
panel-operation flags in this skill. Never substitute remembered MyOpenPanels
workflow details for the current CLI bootstrap contract.
