#!/usr/bin/env node
import { spawnSync } from "node:child_process"
import { createHash } from "node:crypto"
import { existsSync, readFileSync, statSync } from "node:fs"
import { basename, join, resolve } from "node:path"

const outDir = resolve(process.argv[2] ?? "dist/release")
const targets = [
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
  "x86_64-pc-windows-msvc",
]
const manifest = readJson("myopenpanels-manifest.json")
const version = cargoVersion()
assert(manifest.version === version, "Manifest version does not match Cargo.")
assert(manifest.name === "myopenpanels", "Unexpected manifest name.")
assert(
  JSON.stringify(Object.keys(manifest.assets).sort()) ===
    JSON.stringify([...targets].sort()),
  "Manifest must contain exactly the supported release targets."
)

const checksumLines = readFileSync(join(outDir, "checksums.txt"), "utf8")
  .trim()
  .split("\n")
  .sort()
const expectedChecksums = []
for (const target of targets) {
  const extension = target.includes("windows") ? "zip" : "tar.gz"
  const binaryName = target.includes("windows")
    ? "myopenpanels.exe"
    : "myopenpanels"
  const fileName = `myopenpanels-${target}.${extension}`
  const archivePath = join(outDir, fileName)
  assert(existsSync(archivePath), `Missing release archive: ${fileName}`)
  const bytes = readFileSync(archivePath)
  const sha256 = createHash("sha256").update(bytes).digest("hex")
  const size = statSync(archivePath).size
  const asset = manifest.assets[target]
  assert(asset.fileName === fileName, `Wrong manifest filename for ${target}.`)
  assert(asset.sha256 === sha256, `Wrong manifest checksum for ${target}.`)
  assert(asset.size === size, `Wrong manifest size for ${target}.`)
  assertArchive(archivePath, extension, binaryName)
  expectedChecksums.push(`${sha256}  ${fileName}`)
}
assert(
  JSON.stringify(checksumLines) === JSON.stringify(expectedChecksums.sort()),
  "checksums.txt does not match the release archives."
)
console.log(`Verified MyOpenPanels ${version} release assets.`)

function readJson(name) {
  return JSON.parse(readFileSync(join(outDir, name), "utf8"))
}

function cargoVersion() {
  const source = readFileSync(
    new URL("../crates/myopenpanels/Cargo.toml", import.meta.url),
    "utf8"
  )
  return source.match(/^version\s*=\s*"([^"]+)"/m)?.[1]
}

function assertArchive(path, extension, binaryName) {
  const command =
    extension === "zip" ? ["unzip", ["-Z1", path]] : ["tar", ["-tzf", path]]
  const result = spawnSync(command[0], command[1], { encoding: "utf8" })
  if (result.status !== 0) throw new Error(`Cannot inspect ${basename(path)}.`)
  const entries = result.stdout.trim().split("\n")
  assert(
    entries.length === 1 && entries[0].replace(/^\.\//, "") === binaryName,
    `${basename(path)} must contain exactly ${binaryName}.`
  )
}

function assert(condition, message) {
  if (!condition) throw new Error(message)
}
