# MyOpenPanels Repository Agent Notes

## Engineering

- Use independent professional judgment during solution discussions. Treat the
  user's proposed approach as input, recommend a better alternative when one
  exists, and follow their approach as a requirement only when explicitly asked.
- Build frontend pages with HeroUI's native components wherever possible. Create
  custom components only when HeroUI primitives or their composition cannot meet
  the product requirements.

## Local CLI and Studio

- Prefer the checkout-local CLI wrapper. Set
  `MYOPENPANELS_CLI="$PWD/scripts/myopenpanels-dev"` when it is executable;
  otherwise use `myopenpanels` from `PATH`.
- For a request that only opens MyOpenPanels, do not inspect or broadly search the
  repository and do not run Agent Bootstrap. Start Studio directly:
  `MYOPENPANELS_CLI="$PWD/scripts/myopenpanels-dev" scripts/myopenpanels-dev studio start --local-only --project-dir "$PWD" --format json`.
  Follow `data.nextRequiredAction` by opening its URL unchanged with an in-app
  browser. Only if that is unavailable or fails, execute
  `data.nextRequiredAction.fallback.argv` with the resolved CLI. Do not execute
  the compatibility display command or report completion before an opener
  succeeds. If start fails or reports a stale server, run:
  `MYOPENPANELS_CLI="$PWD/scripts/myopenpanels-dev" scripts/myopenpanels-dev studio serve --project-dir "$PWD" --local-only --format json`.
- For Wiki, Canvas, or task work after Studio is running, call
  `scripts/myopenpanels-dev agent bootstrap --format json`, then execute only its
  returned action `argv` arrays with the resolved CLI.
- Canvas, panel, Wiki, and task commands automatically target the current visible
  Project. Do not pass context, session, or panel IDs unless debugging CLI
  internals. Create a Project with `scripts/myopenpanels-dev project create` only
  when the user explicitly asks.
- After changing the CLI, server, Studio UI, or embedded assets, rebuild with
  `scripts/myopenpanels-dev`, stop any running development Studio, and restart it
  before browser verification. Run `corepack pnpm --dir apps/studio build` first
  when UI assets changed. Treat `"data.reusedExisting": true` after such changes
  as stale: run
  `scripts/myopenpanels-dev studio stop --project-dir "$PWD" --format json`, then
  rebuild and start Studio again.

## Releases

- Before a CLI release, bump the version in `crates/myopenpanels/Cargo.toml`,
  `Cargo.lock`, `package.json`, and `apps/studio/package.json`. Run
  `corepack pnpm run check:release`, `corepack pnpm run lint`,
  `corepack pnpm run typecheck`, `corepack pnpm run test`, and
  `corepack pnpm run build`;
  rebuild with `scripts/myopenpanels-dev`; commit; then create and push the
  `v<version>` tag for `.github/workflows/release-myopenpanels.yml`.
- Treat self-update as a compatibility contract with the previously released CLI,
  which performs the download, candidate version check, executable replacement,
  and Studio restart. Keep plain `myopenpanels --version` output as the bare
  version plus a newline unless all supported upgrade paths are deliberately
  migrated. The normal Studio update must install and restart without an Agent;
  Agent recovery must install the CLI before starting Studio.
- Before publishing, run an end-to-end smoke test from the latest published CLI
  through the real candidate manifest and platform archive. Verify download,
  checksum, candidate version, replacement, Studio restart, and browser
  reconnection; unit tests and builds alone are insufficient.

## Live Sync

- Changes to Studio live sync, storage events, tasks, or canvas selection
  persistence must not let non-`panel_state` updates rebuild the canvas editor,
  clear its selection, or bump `snapshotVersion`. Reload the snapshot only for a
  real canvas revision increase, project/session switch, panel switch, or explicit
  reload, and add or update regression tests for this behavior.
