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

1. Ensure the native CLI is installed and available:

   ```bash
   OPENPANELS_LOCAL_CLI="${OPENPANELS_LOCAL_CLI:-openpanels-local}"
   if ! command -v "$OPENPANELS_LOCAL_CLI" >/dev/null 2>&1; then
     curl -fsSL https://raw.githubusercontent.com/mooqii/OpenPanels/main/scripts/install-openpanels-local.sh | sh
     if ! command -v "$OPENPANELS_LOCAL_CLI" >/dev/null 2>&1 && [ -x "$HOME/.local/bin/openpanels-local" ]; then
       OPENPANELS_LOCAL_CLI="$HOME/.local/bin/openpanels-local"
     fi
   fi
   "$OPENPANELS_LOCAL_CLI" --version
   ```

   On Windows PowerShell, use:

   ```powershell
   if (-not $env:OPENPANELS_LOCAL_CLI) { $env:OPENPANELS_LOCAL_CLI = "openpanels-local" }
   if (-not (Get-Command $env:OPENPANELS_LOCAL_CLI -ErrorAction SilentlyContinue)) {
     iwr https://raw.githubusercontent.com/mooqii/OpenPanels/main/scripts/install-openpanels-local.ps1 -UseB | iex
     $defaultCli = Join-Path $HOME ".local\bin\openpanels-local.exe"
     if (-not (Get-Command $env:OPENPANELS_LOCAL_CLI -ErrorAction SilentlyContinue) -and (Test-Path $defaultCli)) {
       $env:OPENPANELS_LOCAL_CLI = $defaultCli
     }
   }
   & $env:OPENPANELS_LOCAL_CLI --version
   ```

   Do not use package-manager or Node-based fallback installers. If
   installation or version verification fails, stop and report the exact error
   to the user.

2. Start or reuse the local studio:

   ```bash
   STUDIO_JSON="$("$OPENPANELS_LOCAL_CLI" studio start --project "$PWD" --format json --no-open)"
   OPENPANELS_CONTEXT_ID="$(printf '%s\n' "$STUDIO_JSON" | node -e 'let s="";process.stdin.on("data",d=>s+=d);process.stdin.on("end",()=>process.stdout.write(JSON.parse(s).contextId))')"
   BROWSER_URL="$(printf '%s\n' "$STUDIO_JSON" | node -e 'let s="";process.stdin.on("data",d=>s+=d);process.stdin.on("end",()=>process.stdout.write(JSON.parse(s).browserUrl || JSON.parse(s).serverUrl))')"
   ```

   Open the returned `browserUrl` (`$BROWSER_URL`) in the agent's in-app
   Browser side panel when available. Use the system browser only when no
   in-app Browser is available or the user explicitly asks for it.

   The `contextId` returned by `studio start` is the effective context for this
   studio. Save it as `OPENPANELS_CONTEXT_ID` and include
   `--context-id "$OPENPANELS_CONTEXT_ID"` in every later
   `openpanels-local` command for this task. This is required because
   `studio start` may reuse an already-running service owned by another agent
   context.

   If `studio start` returns an existing service, do not call `studio serve` or
   start another process. Use the returned `browserUrl` and `contextId`. Only use
   a foreground `studio serve` fallback if `studio start` fails and the error or
   project instructions explicitly request a foreground fallback.

3. Before interacting with any panel, fetch the current CLI-provided context:

   ```bash
   "$OPENPANELS_LOCAL_CLI" agent context --project "$PWD" --context-id "$OPENPANELS_CONTEXT_ID"
   ```

4. Follow the returned state summary and capability commands. For complex
   workflows, list and load the relevant guide:

   ```bash
   "$OPENPANELS_LOCAL_CLI" agent guides --project "$PWD" --context-id "$OPENPANELS_CONTEXT_ID"
   "$OPENPANELS_LOCAL_CLI" agent guide <guide-id> --project "$PWD" --context-id "$OPENPANELS_CONTEXT_ID"
   ```

   Refresh `agent context` when the user switches panels or when a task depends
   on current project/panel state.

Do not rely on stale panel-specific skill text when it conflicts with
`agent context` or loaded guides; the latest CLI output wins.
