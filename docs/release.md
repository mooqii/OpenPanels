# OpenPanels CLI Release Contract

OpenPanels is a local-first project. The CLI updater only talks to GitHub
Releases, and it never depends on an OpenPanels cloud service.

## Version Source

- The Rust CLI version is the source of truth at
  `crates/openpanels-local/Cargo.toml`.
- The root `package.json` version and the compatibility npm wrapper version must
  match the Rust CLI version while those files remain in the repository.
- Release tags must be `v<version>`, for example `v0.1.9`.
- `openpanels-local --version` must print the same version without the leading
  `v`.

Run this before publishing:

```bash
pnpm run check:release
```

## GitHub Release Assets

Every published release must include:

```text
openpanels-local-aarch64-apple-darwin.tar.gz
openpanels-local-x86_64-apple-darwin.tar.gz
openpanels-local-x86_64-unknown-linux-gnu.tar.gz
openpanels-local-aarch64-unknown-linux-gnu.tar.gz
openpanels-local-x86_64-pc-windows-msvc.zip
openpanels-local-manifest.json
checksums.txt
```

Each archive must contain exactly one executable named `openpanels-local` or
`openpanels-local.exe`.

## Update Manifest

The updater reads:

```text
https://github.com/mooqii/OpenPanels/releases/latest/download/openpanels-local-manifest.json
```

The manifest schema is:

```json
{
  "schemaVersion": 1,
  "name": "openpanels-local",
  "version": "0.1.9",
  "channel": "stable",
  "assets": {
    "aarch64-apple-darwin": {
      "url": "https://github.com/mooqii/OpenPanels/releases/download/v0.1.9/openpanels-local-aarch64-apple-darwin.tar.gz",
      "sha256": "...",
      "size": 1234567
    }
  }
}
```

The manifest `version` must not include the leading `v`.

## Updater Behavior

- `openpanels-local update check` checks GitHub Releases and caches the result.
- `openpanels-local update download` downloads the matching asset into the local
  update cache after checking SHA-256.
- `openpanels-local update` reuses the cached asset when possible, verifies the
  downloaded binary with `--version`, then replaces the current executable.
- Normal text-mode commands may perform an opportunistic update check at most
  once every 24 hours. The check writes only a short stderr notice when an
  update exists.
- JSON output mode does not perform opportunistic checks.
- Failed opportunistic checks are silent and must never block local CLI work.
- The studio may show an update prompt when an update is available. It may
  pre-download the update, but installation and studio restart require an
  explicit user click or explicit user instruction to an agent.

Studio update API contract:

```text
GET  /api/update/status
POST /api/update/download
POST /api/update/install-restart
```

`status` drives the lower-right update prompt. `download` caches the latest
asset without replacing the running binary. `install-restart` is only invoked
after user confirmation; it installs the cached update when possible and then
restarts the studio process.

Environment controls:

```text
OPENPANELS_UPDATE_MANIFEST_URL   Override the release manifest URL.
OPENPANELS_UPDATE_CACHE_DIR      Override the update cache directory.
OPENPANELS_DISABLE_UPDATE_CHECK  Disable opportunistic 24-hour checks.
OPENPANELS_ALLOW_DEV_SELF_UPDATE Allow replacing target/debug or target/release builds.
```

The updater refuses to replace development builds by default and refuses
Homebrew-managed binaries. Package-manager installs should be updated through
their package manager.
