#!/usr/bin/env node
import { spawnSync } from "node:child_process"
import { createHash } from "node:crypto"
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs"
import { join, resolve } from "node:path"

const args = parseArgs(process.argv.slice(2))
const target = required("target")
const binary = resolve(required("binary"))
const outDir = resolve(args["out-dir"] ?? "dist/release")
const version =
  args.version ?? process.env.RELEASE_VERSION ?? readCargoVersion()
const binaryName = target.includes("windows")
  ? "myopenpanels.exe"
  : "myopenpanels"
const extension = target.includes("windows") ? "zip" : "tar.gz"
const archiveName = `myopenpanels-${target}.${extension}`
const archivePath = join(outDir, archiveName)
const stagingDir = join(outDir, `.staging-${target}`)

if (!existsSync(binary)) {
  throw new Error(`Binary does not exist: ${binary}`)
}

rmSync(stagingDir, { recursive: true, force: true })
mkdirSync(stagingDir, { recursive: true })
mkdirSync(outDir, { recursive: true })
copyFileSync(binary, join(stagingDir, binaryName))

if (target.includes("windows")) {
  rmSync(archivePath, { force: true })
  if (process.platform === "win32") {
    run("powershell.exe", [
      "-NoProfile",
      "-Command",
      "Compress-Archive",
      "-LiteralPath",
      join(stagingDir, binaryName),
      "-DestinationPath",
      archivePath,
      "-Force",
    ])
  } else {
    run("zip", ["-q", "-j", archivePath, join(stagingDir, binaryName)])
  }
} else {
  run("tar", ["-C", stagingDir, "-czf", archivePath, binaryName])
}

const bytes = await import("node:fs/promises").then(({ readFile }) =>
  readFile(archivePath)
)
const sha256 = createHash("sha256").update(bytes).digest("hex")
const metadata = {
  archiveName,
  archivePath,
  binaryName,
  sha256,
  size: statSync(archivePath).size,
  target,
  version,
}
writeFileSync(
  join(outDir, `${archiveName}.json`),
  `${JSON.stringify(metadata, null, 2)}\n`
)
rmSync(stagingDir, { recursive: true, force: true })
console.log(`${archiveName} ${sha256}`)

function required(name) {
  const value =
    args[name] ??
    process.env[`MYOPENPANELS_${name.toUpperCase().replaceAll("-", "_")}`]
  if (!value) throw new Error(`Missing --${name}`)
  return value
}

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

function run(command, commandArgs) {
  const result = spawnSync(command, commandArgs, { stdio: "inherit" })
  if (result.error) throw result.error
  if (result.status !== 0) {
    throw new Error(`${command} ${commandArgs.join(" ")} failed`)
  }
}

function readCargoVersion() {
  const cargoToml = new URL(
    "../crates/myopenpanels/Cargo.toml",
    import.meta.url
  )
  const content = readFileSync(cargoToml, "utf8")
  const match = content.match(/^version\s*=\s*"([^"]+)"/m)
  if (!match) throw new Error("Missing Rust CLI version in Cargo.toml")
  return match[1]
}
