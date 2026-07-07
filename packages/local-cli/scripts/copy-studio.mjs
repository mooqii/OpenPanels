import { spawn } from "node:child_process"
import { chmod, cp, mkdir, rm } from "node:fs/promises"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const CLI_DIR = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const ROOT_DIR = resolve(CLI_DIR, "../..")
const STUDIO_DIST = join(ROOT_DIR, "apps", "local-studio", "dist")
const TARGET_DIR = join(CLI_DIR, "dist", "studio")
const BIN_PATH = join(CLI_DIR, "dist", "openpanels-local.mjs")

await ensureStudioDist()
await rm(TARGET_DIR, { force: true, recursive: true })
await mkdir(TARGET_DIR, { recursive: true })
await cp(STUDIO_DIST, TARGET_DIR, { recursive: true })
await chmod(BIN_PATH, 0o755)

async function ensureStudioDist() {
  await run("pnpm", ["--filter", "@openpanels/local-studio", "build"], {
    cwd: ROOT_DIR,
  })
}

function run(command, args, options) {
  return new Promise((resolveRun, reject) => {
    const child = spawn(command, args, {
      ...options,
      stdio: "inherit",
      shell: process.platform === "win32",
    })
    child.once("error", reject)
    child.once("exit", (code, signal) => {
      if (code === 0) {
        resolveRun()
        return
      }
      reject(
        new Error(
          `${command} ${args.join(" ")} failed with ${signal || `exit ${code}`}`
        )
      )
    })
  })
}
