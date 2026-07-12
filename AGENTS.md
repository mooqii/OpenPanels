# MyOpenPanels Repository Agent Notes

- During solution discussions, do not agree with or follow the user's proposed
  approach merely to be accommodating. Treat it as input that may be mistaken,
  use independent professional judgment, explain better alternatives in plain
  language, and recommend the approach that best serves the user's actual
  goals. Follow a proposed approach as a requirement only when the user
  explicitly makes it one.
- When working inside this repository, prefer the checkout-local CLI wrapper:
  `scripts/myopenpanels-dev`.
- Set `MYOPENPANELS_CLI="$PWD/scripts/myopenpanels-dev"` when the script
  exists and is executable; otherwise use `myopenpanels` from `PATH`.
- When asked to open the MyOpenPanels panel, do not inspect the repository first.
  Start the MyOpenPanels Studio directly with:
  `MYOPENPANELS_CLI="$PWD/scripts/myopenpanels-dev" scripts/myopenpanels-dev studio start --local-only --project-dir "$PWD" --format json`
  Treat the successful response as Studio readiness, then follow
  `data.nextRequiredAction`: open its URL unchanged with a callable in-app browser
  tool. If no such tool exists, or the attempt fails or has no success signal,
  execute `data.nextRequiredAction.fallback.argv` with the same resolved CLI
  executable. Do not execute the compatibility display command.
  Do not report completion until an opener succeeds.
  If the start command fails or reports a stale server, run the foreground fallback:
  `MYOPENPANELS_CLI="$PWD/scripts/myopenpanels-dev" scripts/myopenpanels-dev studio serve --project-dir "$PWD" --local-only --format json`
- Do not run broad `rg` searches before starting MyOpenPanels unless the CLI
  command fails.
- For MyOpenPanels work after the Studio is running, call the second stable
  entry with `scripts/myopenpanels-dev agent bootstrap --format json`, then
  execute only the returned action
  `argv` arrays with the same resolved CLI executable.
- Canvas, panel, wiki, and task commands target the current user-visible
  Project automatically. Do not pass context/session/panel ids unless you are
  explicitly debugging the CLI internals.
- Do not create a Project unless the user explicitly asks. Project creation is
  done with `scripts/myopenpanels-dev project create`.
- For an open-only request, do not run Agent Bootstrap. Use Bootstrap only when
  subsequent Wiki, Canvas, or task work requires the Agent protocol.
- Use the returned fallback action only when the in-app browser open attempt
  itself fails or no in-app browser is available.
- After changing anything that affects the local CLI or bundled Studio server
  (Rust CLI/server code, studio UI assets, or embedded guides/assets),
  rebuild the latest checkout-local CLI with `scripts/myopenpanels-dev`,
  then stop and restart any running development Studio service before checking
  the browser. Do not assume an already-running service picked up the new CLI.
- If `studio start --format json` returns `"data.reusedExisting": true` after local
  Studio/CLI changes, the browser is still pointed at an older detached server
  process. Stop that exact session before opening it: use
  `scripts/myopenpanels-dev studio stop --project-dir "$PWD" --context-id <contextId>`
  from `data.contextId` in the JSON payload (or terminate the returned `data.pid` if it is a borrowed
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
- Treat CLI self-update as a release-critical compatibility contract. The
  installed old CLI performs the download, candidate `--version` check,
  executable replacement, and Studio restart, so later CLI, argument-parser,
  output-format, manifest, archive, or restart changes must not break that old
  updater. Keep plain `myopenpanels --version` output as the bare version plus
  a newline (for example, `0.4.2\n`) unless the updater contract and all
  supported upgrade paths are deliberately migrated together. The normal
  Studio "update now" flow must install and restart without depending on an
  Agent; an Agent command is recovery only, and recovery must install the CLI
  before starting Studio. Before publishing, run an end-to-end release smoke
  test from the latest previously published CLI through the real candidate
  manifest and platform archive, verifying download, checksum, candidate
  version, executable replacement, Studio restart, and browser reconnection.
  Unit tests or a successful build alone are not sufficient evidence that
  self-update still works.
- When changing Studio live sync, storage events, tasks, or canvas selection
  persistence, do not let non-`panel_state` changes rebuild the canvas editor,
  clear canvas selection, or bump the canvas `snapshotVersion`. Only a real
  canvas panel revision increase, project/session switch, panel switch, or
  explicit reload should reload the canvas snapshot; add/update regression
  tests for this behavior.
