# Resolve Or Install The CLI

Use this only when `MYOPENPANELS_CLI` and the current `PATH` do not provide a
runnable native CLI.

Before installing, check the standard location. A failed `PATH` lookup does not
prove the CLI is absent.

## macOS

Check `${MYOPENPANELS_INSTALL_DIR:-$HOME/.local/bin}/myopenpanels`. If it is
executable and its `--version` succeeds, use that absolute path. Otherwise run:

```bash
curl -fsSL https://raw.githubusercontent.com/mooqii/OpenPanels/main/scripts/install-myopenpanels.sh | sh
```

Resolve the standard location again and verify `--version`.

## Windows PowerShell

When `$env:MYOPENPANELS_INSTALL_DIR` is set, check `myopenpanels.exe` there;
otherwise check `(Join-Path $HOME ".local\bin\myopenpanels.exe")`. If its
`--version` succeeds, use that absolute path. Otherwise run:

```powershell
iwr https://raw.githubusercontent.com/mooqii/OpenPanels/main/scripts/install-myopenpanels.ps1 -UseB | iex
```

Resolve the standard location again and verify `--version`. Stop with the exact
error if installation or verification fails.
