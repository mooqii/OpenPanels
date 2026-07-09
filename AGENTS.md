# OpenPanels Repository Agent Notes

- When working inside this repository, prefer the checkout-local CLI wrapper:
  `scripts/openpanels-local-dev`.
- Set `OPENPANELS_LOCAL_CLI="$PWD/scripts/openpanels-local-dev"` when the script
  exists and is executable; otherwise use `openpanels-local` from `PATH`.
- When asked to open the MyOpenPanels panel, do not inspect the repository first.
  Start the local studio directly with:
  `OPENPANELS_LOCAL_CLI="$PWD/scripts/openpanels-local-dev" scripts/openpanels-local-dev studio start --project "$PWD" --format json --no-open`
  Then open the returned `browserUrl` in the in-app browser. If that command
  fails or reports a stale server, run the foreground fallback:
  `OPENPANELS_LOCAL_CLI="$PWD/scripts/openpanels-local-dev" scripts/openpanels-local-dev studio serve --project "$PWD" --local-only --format json`
- Do not run broad `rg` searches before starting MyOpenPanels unless the CLI
  command fails.
