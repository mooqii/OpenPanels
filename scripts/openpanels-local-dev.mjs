#!/usr/bin/env node
import { spawnSync } from "node:child_process"
import { existsSync, readdirSync, statSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const args = process.argv.slice(2)
const rustBin = join(repoRoot, "target", "debug", exeName("openpanels-local"))
const nodeCli = join(
  repoRoot,
  "packages",
  "local-cli",
  "dist",
  "openpanels-local.mjs"
)

const command = commandName(args)
const shouldUseRust =
  args.includes("--version") ||
  command === "version" ||
  command === "help" ||
  command === "update" ||
  command === "selection"

if (shouldUseRust) {
  ensureRustCli()
  run(rustBin, args)
} else {
  ensureNodeCli()
  run(process.execPath, [nodeCli, ...args])
}

function commandName(argv) {
  for (const arg of argv) {
    if (!arg.startsWith("-")) return arg
  }
  return null
}

function ensureRustCli() {
  const sourcePaths = [
    join(repoRoot, "Cargo.toml"),
    join(repoRoot, "Cargo.lock"),
    join(repoRoot, "crates", "openpanels-local", "Cargo.toml"),
    join(repoRoot, "crates", "openpanels-local", "src"),
  ]
  if (isOutdated(rustBin, sourcePaths)) {
    const cargo = findCargo()
    runChecked(cargo, ["build", "-p", "openpanels-local"])
  }
}

function ensureNodeCli() {
  const sourcePaths = [
    join(repoRoot, "package.json"),
    join(repoRoot, "pnpm-lock.yaml"),
    join(repoRoot, "apps", "local-studio", "src"),
    join(repoRoot, "packages", "canvas", "src"),
    join(repoRoot, "packages", "local-cli", "src"),
    join(repoRoot, "packages", "local-control", "src"),
    join(repoRoot, "packages", "local-server", "src"),
    join(repoRoot, "packages", "local-storage", "src"),
    join(repoRoot, "packages", "protocol", "src"),
    join(repoRoot, "packages", "runtime", "src"),
  ]
  if (isOutdated(nodeCli, sourcePaths)) {
    runChecked("pnpm", ["--filter", "@openpanels/local-cli", "build"])
  }
}

function isOutdated(outputPath, inputPaths) {
  if (!existsSync(outputPath)) return true
  const outputMtime = statSync(outputPath).mtimeMs
  return newestMtime(inputPaths) > outputMtime
}

function newestMtime(paths) {
  let newest = 0
  for (const path of paths) {
    newest = Math.max(newest, pathMtime(path))
  }
  return newest
}

function pathMtime(path) {
  if (!existsSync(path)) return 0
  const stat = statSync(path)
  if (!stat.isDirectory()) return stat.mtimeMs
  let newest = stat.mtimeMs
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    if (
      entry.name === "node_modules" ||
      entry.name === "dist" ||
      entry.name === "target"
    ) {
      continue
    }
    newest = Math.max(newest, pathMtime(join(path, entry.name)))
  }
  return newest
}

function findCargo() {
  if (process.env.CARGO) return process.env.CARGO
  const homeCargo = join(process.env.HOME ?? "", ".cargo", "bin", exeName("cargo"))
  if (existsSync(homeCargo)) return homeCargo
  return "cargo"
}

function exeName(name) {
  return process.platform === "win32" ? `${name}.exe` : name
}

function runChecked(command, commandArgs) {
  const result = spawnSync(command, commandArgs, {
    cwd: repoRoot,
    env: process.env,
    encoding: "utf8",
    stdio: ["inherit", "pipe", "pipe"],
  })
  if (result.stdout) process.stderr.write(result.stdout)
  if (result.stderr) process.stderr.write(result.stderr)
  if (result.error) {
    console.error(result.error.message)
    process.exit(1)
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1)
  }
}

function run(command, commandArgs) {
  const result = spawnSync(command, commandArgs, {
    cwd: process.cwd(),
    env: {
      ...process.env,
      OPENPANELS_LOCAL_CLI: join(repoRoot, "scripts", "openpanels-local-dev"),
    },
    stdio: "inherit",
  })
  if (result.error) {
    console.error(result.error.message)
    process.exit(1)
  }
  process.exit(result.status ?? 0)
}
