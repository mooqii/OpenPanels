# MyOpenPanels Repository Agent Notes

- When working inside this repository, prefer the checkout-local CLI wrapper:
  `scripts/myopenpanels-dev`.
- Set `MYOPENPANELS_CLI="$PWD/scripts/myopenpanels-dev"` when the script
  exists and is executable; otherwise use `myopenpanels` from `PATH`.
- When asked to open the MyOpenPanels panel, do not inspect the repository first.
  Start the MyOpenPanels Studio directly with:
  `MYOPENPANELS_CLI="$PWD/scripts/myopenpanels-dev" scripts/myopenpanels-dev studio start --project "$PWD" --format json --no-open`
  Then append `myopenpanels-view=embedded` as a query parameter to the returned
  `browserUrl` and open that URL in the in-app browser. If that command
  fails or reports a stale server, run the foreground fallback:
  `MYOPENPANELS_CLI="$PWD/scripts/myopenpanels-dev" scripts/myopenpanels-dev studio serve --project "$PWD" --local-only --format json`
- Do not run broad `rg` searches before starting MyOpenPanels unless the CLI
  command fails.
- For MyOpenPanels work after the Studio is running, use only the current
  agent-facing commands advertised by
  `scripts/myopenpanels-dev agent capabilities --format json`.
- Canvas, panel, wiki, and task commands target the current user-visible
  Project automatically. Do not pass context/session/panel ids unless you are
  explicitly debugging the CLI internals.
- Do not create a Project unless the user explicitly asks. Project creation is
  done with `scripts/myopenpanels-dev project create`.
- After changing anything that affects the local CLI or bundled Studio server
  (Rust CLI/server code, studio UI assets, or embedded guides/assets),
  rebuild the latest checkout-local CLI with `scripts/myopenpanels-dev`,
  then stop and restart any running development Studio service before checking
  the browser. Do not assume an already-running service picked up the new CLI.
- If `studio start --format json` returns `"reusedExisting": true` after local
  Studio/CLI changes, the browser is still pointed at an older detached server
  process. Stop that exact session before opening it: use
  `scripts/myopenpanels-dev studio stop --project "$PWD" --context-id <contextId>`
  from the JSON payload (or terminate the returned `pid` if it is a borrowed
  session), then run `corepack pnpm --dir apps/studio build`
  when UI assets changed, rerun `scripts/myopenpanels-dev` to rebuild the
  embedded CLI, and start Studio again. A healthy reused server is not evidence
  that it contains the latest checkout-local code.
- Before publishing a new CLI release, follow this workflow:
  bump the version in `crates/myopenpanels/Cargo.toml`, `Cargo.lock`,
  root `package.json`, and `apps/studio/package.json`; run
  `corepack pnpm run check:release`, `corepack pnpm run lint`,
  `corepack pnpm run typecheck`, `corepack pnpm run test`, and
  `corepack pnpm run build`; rebuild the checkout-local CLI with
  `scripts/myopenpanels-dev`; commit the release changes; create and push
  the `v<version>` tag so `.github/workflows/release-myopenpanels.yml`
  publishes the GitHub Release assets.
- When changing Studio live sync, storage events, tasks, or canvas selection
  persistence, do not let non-`panel_state` changes rebuild the canvas editor,
  clear canvas selection, or bump the canvas `snapshotVersion`. Only a real
  canvas panel revision increase, project/session switch, panel switch, or
  explicit reload should reload the canvas snapshot; add/update regression
  tests for this behavior.
