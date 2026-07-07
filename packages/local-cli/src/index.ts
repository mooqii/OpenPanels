import { spawn } from "node:child_process"
import {
  closeSync,
  existsSync,
  openSync,
  realpathSync,
  writeFileSync,
} from "node:fs"
import { mkdir, readFile, rm, writeFile } from "node:fs/promises"
import { createServer } from "node:net"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import {
  getCanvasState,
  getSelection,
  insertImage,
  insertPlaceholder,
  readSelectionAsset,
  resolveOpenPanelsPaths,
} from "@openpanels/local-control"
import { createLocalOpenPanelsServer } from "@openpanels/local-server"
import packageJson from "../package.json"

interface CliIo {
  stderr: NodeJS.WritableStream
  stdout: NodeJS.WritableStream
}

interface ParsedArgs {
  flags: Record<string, string | true>
  positionals: string[]
}

interface StudioSession {
  logPath: string
  pid: number
  port: number
  projectDir: string
  serverUrl: string
  startedAt: string
  storageDir: string
}

const DEFAULT_WAIT_TIMEOUT_MS = 10_000
const CLI_VERSION = packageJson.version

export async function runOpenPanelsCli(
  argv = process.argv.slice(2),
  io: CliIo = { stdout: process.stdout, stderr: process.stderr }
): Promise<number> {
  try {
    const parsed = parseArgs(argv)
    const [command, subcommand] = parsed.positionals

    if (parsed.flags.version || command === "version") {
      writeResult(io, parsed, { version: CLI_VERSION }, CLI_VERSION)
      return 0
    }

    if (!command || command === "help" || parsed.flags.help) {
      writeText(io.stdout, helpText())
      return 0
    }

    if (command === "__serve-studio") {
      await serveStudio(parsed)
      return 0
    }

    if (command === "studio") {
      return await runStudioCommand(subcommand, parsed, io)
    }

    if (command === "canvas-state") {
      const result = await getCanvasState({
        projectDir: stringFlag(parsed, "project"),
      })
      writeResult(io, parsed, result, `Canvas ready at ${result.storageDir}`)
      return 0
    }

    if (command === "selection") {
      const result = await getSelection({
        projectDir: stringFlag(parsed, "project"),
        includeImageBase64: Boolean(parsed.flags["include-image-base64"]),
      })
      writeResult(
        io,
        parsed,
        result,
        `Selection contains ${selectedShapeCount(result.selection)} shape(s)`
      )
      return 0
    }

    if (command === "read-selection-asset") {
      const result = await readSelectionAsset({
        projectDir: stringFlag(parsed, "project"),
      })
      const outputPath = stringFlag(parsed, "output")
      if (!outputPath) {
        throw new Error("Missing required --output <path>.")
      }
      const resolvedOutputPath = resolve(outputPath)
      await mkdir(dirname(resolvedOutputPath), { recursive: true })
      await writeFile(resolvedOutputPath, Buffer.from(result.base64, "base64"))
      const payload = {
        assetRef: result.assetRef,
        mimeType: result.mimeType,
        outputPath: resolvedOutputPath,
        bytes: Buffer.byteLength(result.base64, "base64"),
      }
      writeResult(io, parsed, payload, `Wrote ${resolvedOutputPath}`)
      return 0
    }

    if (command === "insert-image") {
      const imagePath = stringFlag(parsed, "image")
      if (!imagePath) throw new Error("Missing required --image <path>.")
      const result = await insertImage({
        projectDir: stringFlag(parsed, "project"),
        imagePath,
        placement: placementFlag(parsed),
        anchorShapeId: stringFlag(parsed, "anchor-shape-id"),
        replaceShapeId: stringFlag(parsed, "replace-shape-id"),
        fileName: stringFlag(parsed, "file-name"),
        displayWidth: numberFlag(parsed, "display-width"),
        displayHeight: numberFlag(parsed, "display-height"),
      })
      writeResult(io, parsed, result, `Inserted image shape ${result.shapeId}`)
      return 0
    }

    if (command === "insert-placeholder") {
      const result = await insertPlaceholder({
        projectDir: stringFlag(parsed, "project"),
        anchorShapeId: stringFlag(parsed, "anchor-shape-id"),
        displayWidth: numberFlag(parsed, "display-width"),
        displayHeight: numberFlag(parsed, "display-height"),
        text: stringFlag(parsed, "text"),
      })
      writeResult(
        io,
        parsed,
        result,
        `Inserted placeholder shape ${result.shapeId}`
      )
      return 0
    }

    throw new Error(`Unknown command: ${command}`)
  } catch (error) {
    return writeError(io, parseArgs(argv), error)
  }
}

async function runStudioCommand(
  subcommand: string | undefined,
  parsed: ParsedArgs,
  io: CliIo
) {
  switch (subcommand) {
    case "start": {
      const result = await startStudio(stringFlag(parsed, "project"))
      writeResult(io, parsed, { ok: true, ...result }, result.serverUrl)
      return 0
    }
    case "status": {
      const result = await studioStatus(stringFlag(parsed, "project"))
      writeResult(io, parsed, { ok: true, ...result }, studioStatusText(result))
      return 0
    }
    case "open": {
      const result = await startStudio(stringFlag(parsed, "project"))
      openBrowser(result.serverUrl)
      writeResult(
        io,
        parsed,
        { ok: true, opened: true, ...result },
        `Opened ${result.serverUrl}`
      )
      return 0
    }
    case "wait": {
      const timeoutMs = Math.max(0, numberFlag(parsed, "timeout") ?? 10) * 1000
      const result = await waitForExistingStudio(
        stringFlag(parsed, "project"),
        timeoutMs
      )
      writeResult(io, parsed, { ok: true, ...result }, result.serverUrl)
      return 0
    }
    case "stop": {
      const result = await stopStudio(stringFlag(parsed, "project"))
      writeResult(io, parsed, { ok: true, ...result }, "Stopped MyOpenPanels")
      return 0
    }
    default:
      throw new Error(
        "Expected studio subcommand: start, status, open, wait, or stop."
      )
  }
}

async function startStudio(projectDir?: string): Promise<StudioSession> {
  const paths = resolveOpenPanelsPaths(projectDir)
  const current = await studioStatus(paths.projectDir)
  if (current.server === "running" && current.session) {
    return current.session
  }
  if (current.session?.pid && processExists(current.session.pid)) {
    await terminateProcess(current.session.pid)
  }
  await rm(studioSessionPath(paths.storageDir), { force: true })

  await mkdir(paths.storageDir, { recursive: true })
  const port = await findOpenPort()
  const serverUrl = `http://127.0.0.1:${port}`
  const logPath = join(paths.storageDir, "studio.log")
  const session: StudioSession = {
    projectDir: paths.projectDir,
    storageDir: paths.storageDir,
    serverUrl,
    port,
    pid: 0,
    logPath,
    startedAt: new Date().toISOString(),
  }
  const logFd = openSync(logPath, "a")
  const child = spawn(
    process.execPath,
    [
      cliEntryPath(),
      "__serve-studio",
      "--project",
      paths.projectDir,
      "--port",
      String(port),
      "--static-dir",
      studioStaticDir(),
    ],
    {
      detached: true,
      env: { ...process.env, FORCE_COLOR: "0" },
      stdio: ["ignore", logFd, logFd],
    }
  )
  closeSync(logFd)
  child.unref()
  if (!child.pid)
    throw new Error("Failed to start MyOpenPanels studio process.")

  session.pid = child.pid
  await writeStudioSession(paths.storageDir, session)
  try {
    await waitForStudio(serverUrl, DEFAULT_WAIT_TIMEOUT_MS)
  } catch (error) {
    await terminateProcess(child.pid)
    await rm(studioSessionPath(paths.storageDir), { force: true })
    throw error
  }
  return session
}

async function studioStatus(projectDir?: string): Promise<{
  logPath: string
  projectDir: string
  server: "missing" | "running" | "stale" | "unavailable"
  session: StudioSession | null
  storageDir: string
}> {
  const paths = resolveOpenPanelsPaths(projectDir)
  const logPath = join(paths.storageDir, "studio.log")
  const session = await readStudioSession(paths.storageDir)
  if (!session) {
    return {
      projectDir: paths.projectDir,
      storageDir: paths.storageDir,
      logPath,
      server: "missing",
      session: null,
    }
  }
  if (!processExists(session.pid)) {
    return {
      projectDir: paths.projectDir,
      storageDir: paths.storageDir,
      logPath,
      server: "stale",
      session,
    }
  }
  if (await isStudioHealthy(session.serverUrl)) {
    return {
      projectDir: paths.projectDir,
      storageDir: paths.storageDir,
      logPath,
      server: "running",
      session,
    }
  }
  return {
    projectDir: paths.projectDir,
    storageDir: paths.storageDir,
    logPath,
    server: "unavailable",
    session,
  }
}

async function waitForExistingStudio(
  projectDir: string | undefined,
  timeoutMs: number
) {
  const status = await studioStatus(projectDir)
  if (!status.session) {
    throw new Error("MyOpenPanels studio is not running.")
  }
  await waitForStudio(status.session.serverUrl, timeoutMs)
  return status.session
}

async function stopStudio(projectDir?: string) {
  const paths = resolveOpenPanelsPaths(projectDir)
  const session = await readStudioSession(paths.storageDir)
  if (session?.pid && processExists(session.pid)) {
    await terminateProcess(session.pid)
  }
  await rm(studioSessionPath(paths.storageDir), { force: true })
  return {
    projectDir: paths.projectDir,
    storageDir: paths.storageDir,
    stopped: true,
  }
}

async function serveStudio(parsed: ParsedArgs) {
  const projectDir = stringFlag(parsed, "project")
  const port = numberFlag(parsed, "port")
  const staticDir = stringFlag(parsed, "static-dir")
  if (!projectDir)
    throw new Error("Missing --project for internal studio server.")
  if (!port) throw new Error("Missing --port for internal studio server.")
  if (!staticDir)
    throw new Error("Missing --static-dir for internal studio server.")
  if (!existsSync(join(staticDir, "index.html"))) {
    throw new Error(`MyOpenPanels studio static files not found: ${staticDir}`)
  }

  const server = createLocalOpenPanelsServer({ projectDir, staticDir })
  await new Promise<void>((resolveListen) => {
    server.listen(port, "127.0.0.1", resolveListen)
  })
  process.once("SIGTERM", () => {
    server.close(() => process.exit(0))
  })
  process.once("SIGINT", () => {
    server.close(() => process.exit(130))
  })
}

function parseArgs(argv: string[]): ParsedArgs {
  const flags: Record<string, string | true> = {}
  const positionals: string[] = []
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index]
    if (!arg.startsWith("--")) {
      positionals.push(arg)
      continue
    }
    const raw = arg.slice(2)
    const equalsIndex = raw.indexOf("=")
    if (equalsIndex !== -1) {
      flags[raw.slice(0, equalsIndex)] = raw.slice(equalsIndex + 1)
      continue
    }
    const next = argv[index + 1]
    if (next && !next.startsWith("--")) {
      flags[raw] = next
      index += 1
    } else {
      flags[raw] = true
    }
  }
  return { flags, positionals }
}

function stringFlag(parsed: ParsedArgs, name: string): string | undefined {
  const value = parsed.flags[name]
  return typeof value === "string" ? value : undefined
}

function numberFlag(parsed: ParsedArgs, name: string): number | undefined {
  const value = stringFlag(parsed, name)
  if (value === undefined) return undefined
  const number = Number(value)
  if (!Number.isFinite(number)) {
    throw new Error(`Expected --${name} to be a number.`)
  }
  return number
}

function placementFlag(parsed: ParsedArgs): "below" | "left" | "right" {
  const placement = stringFlag(parsed, "placement") ?? "right"
  if (["below", "left", "right"].includes(placement)) {
    return placement as "below" | "left" | "right"
  }
  throw new Error("Expected --placement to be one of: right, left, below.")
}

function formatFlag(parsed: ParsedArgs) {
  return stringFlag(parsed, "format") ?? "text"
}

function writeResult(
  io: CliIo,
  parsed: ParsedArgs,
  payload: unknown,
  text: string
) {
  if (formatFlag(parsed) === "json") {
    writeText(io.stdout, `${JSON.stringify(payload, null, 2)}\n`)
    return
  }
  writeText(io.stdout, `${text}\n`)
}

function writeError(io: CliIo, parsed: ParsedArgs, error: unknown) {
  const message = error instanceof Error ? error.message : String(error)
  if (formatFlag(parsed) === "json") {
    writeText(
      io.stdout,
      `${JSON.stringify({ ok: false, error: message }, null, 2)}\n`
    )
  } else {
    writeText(io.stderr, `Error: ${message}\n`)
  }
  return 1
}

function writeText(stream: NodeJS.WritableStream, text: string) {
  stream.write(text)
}

function selectedShapeCount(selection: Record<string, unknown>) {
  return Array.isArray(selection.selectedShapes)
    ? selection.selectedShapes.length
    : 0
}

async function readStudioSession(
  storageDir: string
): Promise<StudioSession | null> {
  try {
    const parsed = JSON.parse(
      await readFile(studioSessionPath(storageDir), "utf8")
    ) as StudioSession
    if (
      typeof parsed.serverUrl === "string" &&
      typeof parsed.pid === "number"
    ) {
      return parsed
    }
    return null
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return null
    throw error
  }
}

async function writeStudioSession(storageDir: string, session: StudioSession) {
  await mkdir(storageDir, { recursive: true })
  writeFileSync(
    studioSessionPath(storageDir),
    `${JSON.stringify(session, null, 2)}\n`,
    "utf8"
  )
}

function studioSessionPath(storageDir: string) {
  return join(storageDir, "studio-session.json")
}

async function findOpenPort(): Promise<number> {
  return new Promise((resolvePort, reject) => {
    const server = createServer()
    server.unref()
    server.on("error", reject)
    server.listen(0, "127.0.0.1", () => {
      const address = server.address()
      server.close(() => {
        if (address && typeof address === "object") {
          resolvePort(address.port)
        } else {
          reject(
            new Error("Could not allocate an OpenPanels local studio port.")
          )
        }
      })
    })
  })
}

async function waitForStudio(serverUrl: string, timeoutMs: number) {
  const startedAt = Date.now()
  let lastError: unknown
  while (Date.now() - startedAt < timeoutMs) {
    try {
      const response = await fetch(`${serverUrl}/api/bootstrap`)
      if (response.ok) return
      lastError = new Error(`Studio responded with ${response.status}.`)
    } catch (error) {
      lastError = error
    }
    await delay(250)
  }
  throw new Error(
    `MyOpenPanels studio did not become ready at ${serverUrl}: ${
      lastError instanceof Error ? lastError.message : String(lastError)
    }`
  )
}

async function isStudioHealthy(serverUrl: string) {
  try {
    const response = await fetch(`${serverUrl}/api/bootstrap`)
    return response.ok
  } catch (_error) {
    return false
  }
}

function processExists(pid: number) {
  try {
    process.kill(pid, 0)
    return true
  } catch (_error) {
    return false
  }
}

async function terminateProcess(pid: number) {
  try {
    process.kill(pid, "SIGTERM")
  } catch (_error) {
    return
  }
  const startedAt = Date.now()
  while (Date.now() - startedAt < 3000) {
    if (!processExists(pid)) return
    await delay(100)
  }
  try {
    process.kill(pid, "SIGKILL")
  } catch (_error) {
    // Process already exited.
  }
}

function openBrowser(url: string) {
  const command =
    process.platform === "darwin"
      ? "open"
      : process.platform === "win32"
        ? "cmd"
        : "xdg-open"
  const args = process.platform === "win32" ? ["/c", "start", "", url] : [url]
  const child = spawn(command, args, {
    detached: true,
    stdio: "ignore",
  })
  child.unref()
}

function studioStaticDir() {
  if (process.env.OPENPANELS_STUDIO_STATIC_DIR) {
    return resolve(process.env.OPENPANELS_STUDIO_STATIC_DIR)
  }
  const bundled = fileURLToPath(new URL("./studio", import.meta.url))
  if (existsSync(join(bundled, "index.html"))) return bundled
  const sourceTree = fileURLToPath(
    new URL("../../../apps/local-studio/dist", import.meta.url)
  )
  if (existsSync(join(sourceTree, "index.html"))) return sourceTree
  throw new Error(
    "MyOpenPanels studio static files are missing. Run `pnpm --filter @openpanels/local-cli build` first."
  )
}

function cliEntryPath() {
  return fileURLToPath(import.meta.url)
}

function studioStatusText(result: {
  server: string
  session: StudioSession | null
}) {
  if (result.server === "running" && result.session) {
    return `MyOpenPanels studio running at ${result.session.serverUrl}`
  }
  return `MyOpenPanels studio ${result.server}`
}

function delay(ms: number) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, ms))
}

function helpText() {
  return `openpanels-local <command> [options]

Commands:
  studio start              Start or reuse the local studio
  studio status             Show local studio status
  studio open               Open the local studio in a browser
  studio wait               Wait for the local studio to become ready
  studio stop               Stop the local studio
  canvas-state              Read the current canvas state
  selection                 Read the current canvas selection
  read-selection-asset      Write the exported selection asset to a file
  insert-placeholder        Insert a generation placeholder into a clear area
  insert-image              Insert a local image into the canvas

Options:
  --project <dir>           Project directory (default: cwd or OPENPANELS_PROJECT_DIR)
  --format json             Emit stable JSON output
  --version                 Print the CLI version
`
}

if (isCliEntrypoint()) {
  const exitCode = await runOpenPanelsCli()
  process.exitCode = exitCode
}

function isCliEntrypoint() {
  if (!process.argv[1]) return false
  const modulePath = fileURLToPath(import.meta.url)
  if (process.argv[1] === modulePath) return true
  try {
    return realpathSync(process.argv[1]) === modulePath
  } catch {
    return false
  }
}
