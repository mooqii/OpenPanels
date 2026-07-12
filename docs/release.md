# MyOpenPanels CLI Release Contract

MyOpenPanels is a local-first project. The CLI updater only talks to GitHub
Releases, and it never depends on a MyOpenPanels cloud service.

## Version Source

- The Rust CLI version is the source of truth at
  `crates/myopenpanels/Cargo.toml`.
- The root `package.json` version must match the Rust CLI version while both
  files remain in the repository.
- Release tags must be `v<version>`, for example `v0.1.9`.
- `myopenpanels --version` must print the same version without the leading
  `v`.

Run this before publishing:

```bash
pnpm run check:release
```

GitHub tags matching `v*` run `.github/workflows/release-myopenpanels.yml`.
The workflow currently builds macOS Apple Silicon, macOS Intel, and Windows
x64 packages. Linux release packages are temporarily disabled. It packages the
archives, generates `myopenpanels-manifest.json`, and uploads all release assets
to the matching GitHub Release.

For local packaging smoke tests:

```bash
node scripts/package-myopenpanels.mjs \
  --target aarch64-apple-darwin \
  --binary target/debug/myopenpanels \
  --out-dir dist/release
node scripts/generate-myopenpanels-release-manifest.mjs --out-dir dist/release
```

## GitHub Release Assets

Every published release must include:

```text
myopenpanels-aarch64-apple-darwin.tar.gz
myopenpanels-x86_64-apple-darwin.tar.gz
myopenpanels-x86_64-pc-windows-msvc.zip
myopenpanels-manifest.json
checksums.txt
```

Each archive must contain exactly one executable named `myopenpanels` or
`myopenpanels.exe`.

## Install Scripts

The public install entry points are:

```bash
curl -fsSL https://raw.githubusercontent.com/mooqii/OpenPanels/main/scripts/install-myopenpanels.sh | sh
```

```powershell
iwr https://raw.githubusercontent.com/mooqii/OpenPanels/main/scripts/install-myopenpanels.ps1 -UseB | iex
```

The scripts read the latest release manifest, choose the current platform asset,
verify SHA-256, install to the user-local bin directory, and run
`myopenpanels --version`. They print PATH instructions when the install
directory is not already on PATH, but do not edit shell profiles.

Install-script environment controls:

```text
MYOPENPANELS_INSTALL_REPO          Override the GitHub repository.
MYOPENPANELS_INSTALL_MANIFEST_URL  Override the release manifest URL.
MYOPENPANELS_INSTALL_DIR           Override the install directory.
```

## Update Manifest

The updater reads:

```text
https://github.com/mooqii/OpenPanels/releases/latest/download/myopenpanels-manifest.json
```

The manifest schema is:

```json
{
  "schemaVersion": 1,
  "name": "myopenpanels",
  "version": "0.1.9",
  "channel": "stable",
  "entrySkill": {
    "id": "myopenpanels",
    "version": "3.1",
    "source": "https://github.com/mooqii/OpenPanels/tree/v0.1.9/skills/myopenpanels"
  },
  "assets": {
    "aarch64-apple-darwin": {
      "url": "https://github.com/mooqii/OpenPanels/releases/download/v0.1.9/myopenpanels-aarch64-apple-darwin.tar.gz",
      "sha256": "...",
      "size": 1234567
    }
  }
}
```

The manifest `version` must not include the leading `v`.

## Updater Behavior

- `myopenpanels update check` checks GitHub Releases and caches the result.
- `myopenpanels update download` downloads the matching asset into the local
  update cache after checking SHA-256.
- `myopenpanels update install` reuses the cached asset when possible, verifies the
  downloaded binary with `--version`, then replaces the current executable. Its
  response also carries an advisory Agent-host action asking the Agent to compare
  the currently loaded MyOpenPanels Entry Skill with the version pinned in the
  release manifest and consider updating it when older.
- The replacement CLI also latches its compiled Entry Skill requirement into
  local Agent control storage on the next Bootstrap. This closes the Studio
  manual-update path even though the old installed CLI performs replacement and
  restart. Only contexts that have not acknowledged that requirement receive
  the compact required update response; normal Bootstrap payloads carry no
  Entry Skill update reminder or version comparison.
- Normal text-mode commands may perform an opportunistic update check at most
  once every 24 hours. The check writes only a short stderr notice when an
  update exists.
- JSON output mode does not perform opportunistic checks.
- Failed opportunistic checks are silent and must never block CLI work.
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
spawns a delayed replacement studio process on the same host, port, project,
storage directory, context id, and static asset override. The new process
writes `studio-session.json` before the current server exits.

Environment controls:

```text
MYOPENPANELS_UPDATE_MANIFEST_URL   Override the release manifest URL.
MYOPENPANELS_UPDATE_CACHE_DIR      Override the update cache directory.
MYOPENPANELS_DISABLE_UPDATE_CHECK  Disable opportunistic 24-hour checks.
MYOPENPANELS_ALLOW_DEV_SELF_UPDATE Allow replacing target/debug or target/release builds.
```

The updater refuses to replace development builds by default and refuses
Homebrew-managed binaries. Package-manager installs should be updated through
their package manager.
