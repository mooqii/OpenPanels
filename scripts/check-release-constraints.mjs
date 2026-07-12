import { readFileSync } from "node:fs"

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
const studioVersion = readJson("apps/studio/package.json").version
const rustVersion = readCargoVersion("crates/myopenpanels/Cargo.toml")
const tag =
  process.env.GITHUB_REF_NAME || process.env.RELEASE_TAG || `v${rootVersion}`
const tagVersion = tag.startsWith("v") ? tag.slice(1) : tag

assert(
  rootVersion === rustVersion,
  `Root package version ${rootVersion} does not match Rust CLI version ${rustVersion}.`
)
assert(
  rootVersion === studioVersion,
  `Root package version ${rootVersion} does not match Studio version ${studioVersion}.`
)

const entrySkill = readFileSync(
  new URL("skills/myopenpanels/SKILL.md", ROOT),
  "utf8"
)
const entrySkillVersion = entrySkill.match(
  /^\s+version:\s*["']([^"']+)["']/m
)?.[1]
const cliSource = readFileSync(
  new URL("crates/myopenpanels/src/cli.rs", ROOT),
  "utf8"
)
assert(
  !cliSource.includes('"  agent context'),
  "Protocol v1 agent context must not return to the public CLI surface."
)
assert(
  entrySkillVersion,
  "MyOpenPanels entry skill must declare metadata.version."
)
for (const required of [
  "install-myopenpanels.sh",
  "install-myopenpanels.ps1",
  "agent bootstrap",
  "drawing",
  "organizing or comparing materials",
  "writing",
  "open or launch MyOpenPanels",
  "打开面板",
]) {
  assert(
    entrySkill.includes(required),
    `MyOpenPanels entry skill must retain ${required}.`
  )
}
for (const forbidden of [
  "canvas selection",
  "canvas generation",
  "wiki generation",
  "--context-id",
  "--protocol-version",
  "minCliVersion",
  "Do not use package-manager",
  "Node-based fallback",
]) {
  assert(
    !entrySkill.includes(forbidden),
    `MyOpenPanels entry skill must not embed panel workflow detail: ${forbidden}.`
  )
}
assert(
  tag === `v${rootVersion}`,
  `Release tag must be v${rootVersion}; got ${tag}.`
)

const manifest = {
  schemaVersion: 1,
  name: "myopenpanels",
  version: tagVersion,
  channel: tagVersion.includes("-") ? "prerelease" : "stable",
  entrySkill: {
    id: "myopenpanels",
    version: entrySkillVersion,
    source: `https://github.com/mooqii/OpenPanels/tree/${tag}/skills/myopenpanels`,
  },
  assets: Object.fromEntries(
    RELEASE_TARGETS.map((target) => {
      const extension = target.includes("windows") ? "zip" : "tar.gz"
      const fileName = `myopenpanels-${target}.${extension}`
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

console.log(`Release constraints passed for MyOpenPanels ${tag}.`)
console.log(JSON.stringify(manifest, null, 2))
