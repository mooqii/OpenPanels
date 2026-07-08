import { readFileSync } from "node:fs"
import { basename } from "node:path"

const ROOT = new URL("..", import.meta.url)
const RELEASE_TARGETS = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "x86_64-unknown-linux-gnu",
  "aarch64-unknown-linux-gnu",
  "x86_64-pc-windows-msvc",
]

function readJson(path) {
  return JSON.parse(readFileSync(new URL(path, ROOT), "utf8"))
}

function readCargoVersion(path) {
  const toml = readFileSync(new URL(path, ROOT), "utf8")
  const match = toml.match(/^version\s*=\s*"([^"]+)"/m)
  if (!match) throw new Error(`Missing package version in ${path}`)
  return match[1]
}

function assert(condition, message) {
  if (!condition) throw new Error(message)
}

const rootVersion = readJson("package.json").version
const rustVersion = readCargoVersion("crates/openpanels-local/Cargo.toml")
const legacyNpmVersion = readJson("packages/local-cli/package.json").version
const tag =
  process.env.GITHUB_REF_NAME || process.env.RELEASE_TAG || `v${rootVersion}`
const tagVersion = tag.startsWith("v") ? tag.slice(1) : tag

assert(
  rootVersion === rustVersion,
  `Root package version ${rootVersion} does not match Rust CLI version ${rustVersion}.`
)
assert(
  rootVersion === legacyNpmVersion,
  `Root package version ${rootVersion} does not match legacy npm wrapper version ${legacyNpmVersion}.`
)
assert(
  tag === `v${rootVersion}`,
  `Release tag must be v${rootVersion}; got ${tag}.`
)

const manifest = {
  schemaVersion: 1,
  name: "openpanels-local",
  version: tagVersion,
  channel: tagVersion.includes("-") ? "prerelease" : "stable",
  assets: Object.fromEntries(
    RELEASE_TARGETS.map((target) => {
      const extension = target.includes("windows") ? "zip" : "tar.gz"
      const fileName = `openpanels-local-${target}.${extension}`
      return [
        target,
        {
          fileName,
          url: `https://github.com/mooqii/OpenPanels/releases/download/${tag}/${fileName}`,
          sha256: "<filled-by-release-workflow>",
          size: 0,
        },
      ]
    })
  ),
}

console.log(`Release constraints passed for ${basename(ROOT.pathname)} ${tag}.`)
console.log(JSON.stringify(manifest, null, 2))
