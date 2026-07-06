import { spawnSync } from "node:child_process"
import { existsSync } from "node:fs"
import path from "node:path"
import { fileURLToPath, pathToFileURL } from "node:url"

const ROOT_DIR = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  ".."
)
const REQUIRED_DEPENDENCIES = [
  { packageName: "@modelcontextprotocol/ext-apps" },
  { packageName: "@modelcontextprotocol/sdk" },
  { packageName: "@vitejs/plugin-react" },
  { packageName: "vite" },
  { packageName: "zod" },
  { packageName: "react", workspacePath: "apps/local-studio" },
  { packageName: "react-dom", workspacePath: "apps/local-studio" },
]

function dependencyDir(dependency) {
  return path.join(
    ROOT_DIR,
    dependency.workspacePath ?? "",
    "node_modules",
    ...dependency.packageName.split("/")
  )
}

function missingDependencies() {
  return REQUIRED_DEPENDENCIES.filter(
    (dependency) => !existsSync(dependencyDir(dependency))
  )
}

function pnpmCommand() {
  if (process.env.OPENPANELS_PNPM_COMMAND) {
    return { command: process.env.OPENPANELS_PNPM_COMMAND, args: [] }
  }
  if (process.platform === "win32") {
    return { command: "cmd.exe", args: ["/d", "/s", "/c", "pnpm"] }
  }
  return { command: "pnpm", args: [] }
}

function runPnpmInstall() {
  const pnpm = pnpmCommand()
  const result = spawnSync(pnpm.command, [
    ...pnpm.args,
    "install",
    "--config.minimum-release-age=0",
    "--config.dangerously-allow-all-builds=true",
  ], {
    cwd: ROOT_DIR,
    env: {
      ...process.env,
      FORCE_COLOR: "0",
    },
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  })

  if (result.stdout) process.stderr.write(result.stdout)
  if (result.stderr) process.stderr.write(result.stderr)
  if (result.error) throw result.error
  if (result.status !== 0) {
    throw new Error(
      `pnpm install failed while preparing MyOpenPanels MCP (exit ${result.status}).`
    )
  }
}

if (missingDependencies().length > 0) {
  runPnpmInstall()
}

process.chdir(ROOT_DIR)
await import(pathToFileURL(path.join(ROOT_DIR, "mcp", "server.mjs")).href)
