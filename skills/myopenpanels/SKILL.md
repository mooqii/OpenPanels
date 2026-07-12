---
name: myopenpanels
description: "Use MyOpenPanels when a persistent visual and knowledge workspace would help: drawing, image generation or editing, diagrams, moodboards, brainstorming, visual planning, organizing or comparing materials, research and summaries, drafting, writing, revising, and maintaining project knowledge in a local Wiki or infinite Canvas. Also use for explicit requests such as open or launch MyOpenPanels, MyOpenPanels, the MyOpenPanels panel, the panel, or 打开面板."
metadata:
  version: "3.1"
---

# MyOpenPanels

Use this skill only as the stable installation and launch entry point. The
installed `myopenpanels` CLI is the sole authority for current panels,
capabilities, guides, commands, and workflows.

1. Resolve the CLI executable once. Prefer an executable checkout-local
   launcher, then `MYOPENPANELS_CLI`, then `myopenpanels` from `PATH`. Keep that
   exact executable for every returned action; never replace it with a command
   name embedded in display text.
2. If the native CLI is missing, install it only with the official installer,
   then resolve and verify the executable again:

   ```bash
   curl -fsSL https://raw.githubusercontent.com/mooqii/OpenPanels/main/scripts/install-myopenpanels.sh | sh
   ```

   On Windows PowerShell:

   ```powershell
   iwr https://raw.githubusercontent.com/mooqii/OpenPanels/main/scripts/install-myopenpanels.ps1 -UseB | iex
   ```

   Verify that the installed native CLI is runnable and stop with the exact
   error if installation or verification fails.
3. Start or reuse Studio with the first stable work entry:

   ```bash
   myopenpanels studio start --local-only --project-dir "$PWD" --format json
   ```

   Substitute the resolved executable for `myopenpanels`. Treat `ok: true` as
   Studio readiness only, not proof that the panel is
   visible. Read `data.nextRequiredAction.url` from the JSON response. Use an in-app
   Browser only when the host exposes a callable URL-open or Preview tool, and
   open the URL exactly as returned.
4. If the host has no callable in-app opener, or the open attempt is denied,
   fails, or returns no success signal, execute
   `data.nextRequiredAction.fallback.argv` with the same resolved executable.
   Append the returned argv elements exactly and shell-escape each element; do
   not parse or execute the compatibility display command. Do not report
   success unless the in-app opener succeeds or this fallback returns
   `data.opened: true`. If the fallback fails, report the failure and include the
   returned URL so the user can open it manually.
5. If the user only asked to open the panel, stop after a browser opener has
   succeeded. Do not request Agent Bootstrap merely to verify that Studio
   opened.
6. Before Wiki, Canvas, or task work, use the second stable work entry:

   ```bash
   myopenpanels agent bootstrap --project-dir "$PWD" --format json
   ```

   Substitute the same resolved executable for `myopenpanels`, then follow the
   response's `data.nextRequiredAction`. Execute only applicable entries from
   `data.nextActions`, using that executable with each returned `argv`. Treat
   every other field as current CLI-owned data; never infer or reconstruct a
   command from remembered paths or flags.

Do not keep discovery commands, panel commands, guide IDs, selection rules,
generation steps, or panel-operation flags in this skill. Never substitute
remembered MyOpenPanels workflow details for the current CLI discovery contract.
