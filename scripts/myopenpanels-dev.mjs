#!/usr/bin/env node
import { spawnSync } from "node:child_process"
import { existsSync, readdirSync, statSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const args = process.argv.slice(2)
const rustBin = join(repoRoot, "target", "debug", exeName("myopenpanels"))
const configuredStorageDir = process.env.MYOPENPANELS_STORAGE_DIR?.trim()
const devStorageDir = configuredStorageDir || join(repoRoot, ".myopenpanels")
ensureRustCli()
run(rustBin, args)

function ensureRustCli() {
  const sourcePaths = [
    join(repoRoot, "Cargo.toml"),
    join(repoRoot, "Cargo.lock"),
    join(repoRoot, "crates", "myopenpanels", "Cargo.toml"),
    join(repoRoot, "crates", "myopenpanels", "src"),
    join(repoRoot, "crates", "myopenpanels", "build.rs"),
    join(repoRoot, "skills", "myopenpanels"),
    join(repoRoot, "agent-resources"),
    join(repoRoot, "apps", "studio", "dist"),
  ]
  if (isOutdated(rustBin, sourcePaths)) {
    const cargo = findCargo()
    runChecked(cargo, ["build", "-p", "myopenpanels"])
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
  const homeCargo = join(
    process.env.HOME ?? "",
    ".cargo",
    "bin",
    exeName("cargo")
  )
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
      MYOPENPANELS_CLI: join(repoRoot, "scripts", "myopenpanels-dev"),
      MYOPENPANELS_STORAGE_DIR: devStorageDir,
    },
    stdio: "inherit",
  })
  if (result.error) {
    console.error(result.error.message)
    process.exit(1)
  }
  process.exit(result.status ?? 0)
}
