#!/usr/bin/env node
import { createHash } from "node:crypto"
import {
  existsSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs"
import { join, resolve } from "node:path"

const args = parseArgs(process.argv.slice(2))
const outDir = resolve(args["out-dir"] ?? "dist/release")
const version = (
  args.version ??
  process.env.RELEASE_VERSION ??
  readCargoVersion()
).replace(/^v/, "")
const tag =
  args.tag ??
  process.env.RELEASE_TAG ??
  (process.env.GITHUB_REF_TYPE === "tag"
    ? process.env.GITHUB_REF_NAME
    : undefined) ??
  `v${version}`
const repo = args.repo ?? process.env.GITHUB_REPOSITORY ?? "mooqii/OpenPanels"
const channel =
  args.channel ?? (version.includes("-") ? "prerelease" : "stable")
const entrySkill = readEntrySkillMetadata()
const archivePattern = /^myopenpanels-(.+)\.(tar\.gz|zip)$/

if (!existsSync(outDir))
  throw new Error(`Release output directory does not exist: ${outDir}`)

const assets = {}
const checksumLines = []
for (const fileName of readdirSync(outDir).sort()) {
  const match = fileName.match(archivePattern)
  if (!match) continue
  const target = match[1]
  const filePath = join(outDir, fileName)
  const bytes = readFileSync(filePath)
  const sha256 = createHash("sha256").update(bytes).digest("hex")
  const size = statSync(filePath).size
  assets[target] = {
    fileName,
    url: `https://github.com/${repo}/releases/download/${tag}/${fileName}`,
    sha256,
    size,
  }
  checksumLines.push(`${sha256}  ${fileName}`)
}

if (Object.keys(assets).length === 0) {
  throw new Error(`No myopenpanels release archives found in ${outDir}`)
}

const manifest = {
  schemaVersion: 1,
  name: "myopenpanels",
  version,
  channel,
  entrySkill: {
    id: "myopenpanels",
    version: entrySkill.version,
    source: entrySkill.source,
  },
  assets,
}

writeFileSync(
  join(outDir, "myopenpanels-manifest.json"),
  `${JSON.stringify(manifest, null, 2)}\n`
)
writeFileSync(join(outDir, "checksums.txt"), `${checksumLines.join("\n")}\n`)
console.log(`Generated manifest for ${Object.keys(assets).length} asset(s).`)

function parseArgs(argv) {
  const result = {}
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index]
    if (!arg.startsWith("--")) continue
    const name = arg.slice(2)
    const next = argv[index + 1]
    if (next && !next.startsWith("--")) {
      result[name] = next
      index += 1
    } else {
      result[name] = "1"
    }
  }
  return result
}

function readCargoVersion() {
  const content = readFileSync(
    new URL("../crates/myopenpanels/Cargo.toml", import.meta.url),
    "utf8"
  )
  const match = content.match(/^version\s*=\s*"([^"]+)"/m)
  if (!match) throw new Error("Missing Rust CLI version in Cargo.toml")
  return match[1]
}

function readEntrySkillMetadata() {
  const content = readFileSync(
    new URL("../skills/myopenpanels/SKILL.md", import.meta.url),
    "utf8"
  )
  const version = content.match(/^\s+version:\s*["']([^"']+)["']/m)?.[1]
  const source = content.match(/^\s+source:\s*["']([^"']+)["']/m)?.[1]
  if (!version) throw new Error("Missing MyOpenPanels entry Skill version")
  if (!source) throw new Error("Missing MyOpenPanels entry Skill source")
  return { source, version }
}
