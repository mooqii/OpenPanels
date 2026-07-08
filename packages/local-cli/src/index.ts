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
import { networkInterfaces } from "node:os"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import {
  addWikiRawDocument,
  claimWikiTask,
  completeWikiTask,
  failWikiTask,
  getCanvasState,
  getProjectBootstrap,
  getSelection,
  insertImage,
  insertPlaceholder,
  listWikiAgentTargets,
  listWikiTasks,
  nextWikiTask,
  readSelectionAsset,
  readWikiMarkdown,
  readWikiPage,
  registerWikiAgentTarget,
  resolveOpenPanelsPaths,
  setActivePanel,
  setActiveWikiSpace,
  writeWikiMarkdown,
  writeWikiPage,
} from "@openpanels/local-control"
import { createLocalOpenPanelsServer } from "@openpanels/local-server"
import packageJson from "../package.json"
import { AGENT_CAPABILITIES } from "./agent-capabilities"
import { agentContextMarkdown, agentContextPayload } from "./agent-context"
import {
  listAgentGuides,
  readAgentGuide,
  renderAgentGuidesMarkdown,
} from "./agent-guides"

type OpenPanelsPanelKind =
  | "wiki"
  | "canvas"
  | "image"
  | "diff"
  | "preview"
  | "files"

interface CliIo {
  stderr: NodeJS.WritableStream
  stdout: NodeJS.WritableStream
}

interface ParsedArgs {
  flags: Record<string, string | true>
  positionals: string[]
}

interface CliLocalOptions {
  contextId?: string
  host?: string
  localOnly?: boolean
  projectDir?: string
  storageDir?: string
}

interface StudioSession {
  browserUrl: string
  contextDir: string
  contextId: string
  contextIdSource: string
  host: string
  lanServerUrls: string[]
  localServerUrl: string
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

    if (command === "agent") {
      return await runAgentCommand(parsed.positionals.slice(1), parsed, io)
    }

    if (command === "wiki") {
      return await runWikiCommand(parsed.positionals.slice(1), parsed, io)
    }

    if (command === "canvas-state") {
      const result = await getCanvasState(localOptions(parsed))
      writeResult(io, parsed, result, `Canvas ready at ${result.storageDir}`)
      return 0
    }

    if (command === "agent-context") {
      await writeAgentContext(parsed, io)
      return 0
    }

    if (command === "panels") {
      const result = await getProjectBootstrap(localOptions(parsed))
      const payload = {
        activePanelId: result.activePanelId,
        activePanelKind: result.activePanelKind,
        panels: result.panels.map(({ panel }) => panel),
        project: result.session,
      }
      writeResult(
        io,
        parsed,
        payload,
        result.panels
          .map(({ panel }) =>
            panel.id === result.activePanelId
              ? `* ${panel.kind}: ${panel.title}`
              : `  ${panel.kind}: ${panel.title}`
          )
          .join("\n")
      )
      return 0
    }

    if (command === "active-panel") {
      const kind = panelKindFlag(parsed)
      const panelId = stringFlag(parsed, "panel-id")
      const result =
        kind || panelId
          ? await setActivePanel({
              ...localOptions(parsed),
              kind,
              panelId,
              sessionId: stringFlag(parsed, "session-id"),
            })
          : await getProjectBootstrap(localOptions(parsed))
      const payload = {
        activePanelId: result.activePanelId,
        activePanelKind: result.activePanelKind,
        panel: result.panel,
        project: result.session,
      }
      writeResult(
        io,
        parsed,
        payload,
        `${result.activePanelKind}: ${result.panel.title}`
      )
      return 0
    }

    if (command === "panel-state") {
      const result = await getProjectBootstrap({
        ...localOptions(parsed),
        panelKind: panelKindFlag(parsed),
        panelId: stringFlag(parsed, "panel-id"),
        sessionId: stringFlag(parsed, "session-id"),
      })
      const payload = {
        activePanelId: result.activePanelId,
        activePanelKind: result.activePanelKind,
        panel: result.panel,
        project: result.session,
        state: result.state,
      }
      writeResult(io, parsed, payload, `${result.activePanelKind} state ready`)
      return 0
    }

    if (command === "selection") {
      const result = await getSelection({
        ...localOptions(parsed),
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
        ...localOptions(parsed),
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
        ...localOptions(parsed),
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
        ...localOptions(parsed),
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
      const result = await startStudio(localOptions(parsed))
      writeResult(io, parsed, { ok: true, ...result }, result.serverUrl)
      return 0
    }
    case "status": {
      const result = await studioStatus(localOptions(parsed))
      writeResult(io, parsed, { ok: true, ...result }, studioStatusText(result))
      return 0
    }
    case "open": {
      const result = await startStudio(localOptions(parsed))
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
        localOptions(parsed),
        timeoutMs
      )
      writeResult(io, parsed, { ok: true, ...result }, result.serverUrl)
      return 0
    }
    case "stop": {
      const result = await stopStudio(localOptions(parsed))
      writeResult(io, parsed, { ok: true, ...result }, "Stopped MyOpenPanels")
      return 0
    }
    default:
      throw new Error(
        "Expected studio subcommand: start, status, open, wait, or stop."
      )
  }
}

async function runAgentCommand(
  positionals: string[],
  parsed: ParsedArgs,
  io: CliIo
) {
  const [scope, guideId] = positionals
  if (!scope || scope === "context") {
    await writeAgentContext(parsed, io)
    return 0
  }

  if (scope === "capabilities") {
    writeResult(
      io,
      parsed,
      { capabilities: AGENT_CAPABILITIES },
      renderCapabilitiesSummary()
    )
    return 0
  }

  if (scope === "guides") {
    const guides = await listAgentGuides()
    writeResult(io, parsed, { guides }, renderAgentGuidesMarkdown(guides))
    return 0
  }

  if (scope === "guide") {
    if (!guideId) throw new Error("Missing guide id.")
    const result = await readAgentGuide(guideId, {
      ...localOptions(parsed),
      taskId: stringFlag(parsed, "task-id"),
    })
    writeResult(
      io,
      parsed,
      { guide: result.guide, markdown: result.markdown },
      result.markdown
    )
    return 0
  }

  throw new Error(
    "Expected agent subcommand: context, capabilities, guides, or guide."
  )
}

async function runWikiCommand(
  positionals: string[],
  parsed: ParsedArgs,
  io: CliIo
) {
  const [scope, action] = positionals
  if (!scope || scope === "context") {
    await writeAgentContext(parsed, io, "wiki")
    return 0
  }

  if (scope === "agent-target" && action === "register") {
    const result = await registerWikiAgentTarget({
      ...localOptions(parsed),
      host: stringFlag(parsed, "host") ?? "unknown",
      threadId: stringFlag(parsed, "thread-id") ?? "default",
      wakeUrl: stringFlag(parsed, "wake-url"),
    })
    writeResult(io, parsed, result, `Registered ${result.target.host}`)
    return 0
  }

  if (scope === "agent-target" && (!action || action === "list")) {
    const result = await listWikiAgentTargets(localOptions(parsed))
    writeResult(
      io,
      parsed,
      result,
      result.targets
        .map((target) => `${target.host}:${target.threadId}`)
        .join("\n")
    )
    return 0
  }

  if (scope === "raw" && action === "add") {
    const filePath = stringFlag(parsed, "file")
    if (!filePath) throw new Error("Missing required --file <path>.")
    const result = await addWikiRawDocument({
      ...localOptions(parsed),
      sourcePath: filePath,
      fileName:
        stringFlag(parsed, "file-name") ??
        filePath.split(/[\\/]/).pop() ??
        "document",
      mimeType: stringFlag(parsed, "mime-type"),
      title: stringFlag(parsed, "title"),
      source: "agent",
      wikiSpaceId: stringFlag(parsed, "wiki-space-id"),
    })
    writeResult(io, parsed, result, `Added raw document ${result.document.id}`)
    return 0
  }

  if (scope === "raw" && (action === "new-markdown" || action === "add-text")) {
    const title = stringFlag(parsed, "title") ?? "Untitled"
    const fileName = stringFlag(parsed, "file-name") ?? `${title}.md`
    const filePath = stringFlag(parsed, "file")
    const content = filePath
      ? await readFile(resolve(filePath), "utf8")
      : (stringFlag(parsed, "content") ?? "")
    const result = await addWikiRawDocument({
      ...localOptions(parsed),
      content,
      fileName,
      mimeType: "text/markdown",
      title,
      source: "agent",
      wikiSpaceId: stringFlag(parsed, "wiki-space-id"),
    })
    writeResult(io, parsed, result, `Created markdown ${result.document.id}`)
    return 0
  }

  if (scope === "raw" && (!action || action === "list")) {
    const result = await getProjectBootstrap({
      ...localOptions(parsed),
      panelKind: "wiki",
    })
    const state = result.state as { rawDocuments?: unknown[] }
    const documents = Array.isArray(state.rawDocuments)
      ? state.rawDocuments
      : []
    writeResult(io, parsed, { documents }, `${documents.length} document(s)`)
    return 0
  }

  if (scope === "markdown" && action === "read") {
    const documentId = requiredFlag(parsed, "document-id")
    const result = await readWikiMarkdown({
      ...localOptions(parsed),
      documentId,
    })
    writeResult(io, parsed, result, result.markdown)
    return 0
  }

  if (scope === "markdown" && action === "write") {
    const documentId = requiredFlag(parsed, "document-id")
    const filePath = requiredFlag(parsed, "file")
    const result = await writeWikiMarkdown({
      ...localOptions(parsed),
      documentId,
      content: await readFile(resolve(filePath), "utf8"),
      taskId: stringFlag(parsed, "task-id"),
    })
    writeResult(io, parsed, result, `Wrote markdown ${documentId}`)
    return 0
  }

  if (scope === "tasks") {
    if (!action || action === "list") {
      const result = await listWikiTasks({
        ...localOptions(parsed),
        status: wikiTaskStatusFlag(parsed),
      })
      writeResult(io, parsed, result, `${result.tasks.length} task(s)`)
      return 0
    }
    if (action === "next") {
      const task = await nextWikiTask(localOptions(parsed))
      writeResult(io, parsed, { task }, task ? task.id : "No queued task")
      return 0
    }
    const taskId = requiredFlag(parsed, "task-id")
    if (action === "claim") {
      const result = await claimWikiTask({
        ...localOptions(parsed),
        taskId,
        agentHost: stringFlag(parsed, "agent-host"),
        threadId: stringFlag(parsed, "thread-id"),
      })
      writeResult(io, parsed, result, `Claimed ${taskId}`)
      return 0
    }
    if (action === "complete") {
      const result = await completeWikiTask({ ...localOptions(parsed), taskId })
      writeResult(io, parsed, result, `Completed ${taskId}`)
      return 0
    }
    if (action === "fail") {
      const result = await failWikiTask({
        ...localOptions(parsed),
        taskId,
        error: stringFlag(parsed, "message") ?? "Wiki task failed",
      })
      writeResult(io, parsed, result, `Failed ${taskId}`)
      return 0
    }
  }

  if (scope === "spaces" && action === "active") {
    const wikiSpaceId = requiredFlag(parsed, "wiki-space-id")
    const result = await setActiveWikiSpace({
      ...localOptions(parsed),
      wikiSpaceId,
    })
    writeResult(io, parsed, result, `Active wiki space ${wikiSpaceId}`)
    return 0
  }

  if (scope === "spaces" && (!action || action === "list")) {
    const result = await getProjectBootstrap({
      ...localOptions(parsed),
      panelKind: "wiki",
    })
    const state = result.state as { wikiSpaces?: unknown[] }
    const spaces = Array.isArray(state.wikiSpaces) ? state.wikiSpaces : []
    writeResult(io, parsed, { spaces }, `${spaces.length} wiki space(s)`)
    return 0
  }

  if (scope === "pages") {
    const wikiSpaceId = requiredFlag(parsed, "wiki-space-id")
    if (action === "read") {
      const pagePath = requiredFlag(parsed, "path")
      const result = await readWikiPage({
        ...localOptions(parsed),
        wikiSpaceId,
        pagePath,
      })
      writeResult(io, parsed, result, result.markdown)
      return 0
    }
    if (action === "create" || action === "write") {
      const pagePath = requiredFlag(parsed, "path")
      const filePath = requiredFlag(parsed, "file")
      const result = await writeWikiPage({
        ...localOptions(parsed),
        wikiSpaceId,
        pagePath,
        content: await readFile(resolve(filePath), "utf8"),
        taskId: stringFlag(parsed, "task-id"),
        title: stringFlag(parsed, "title"),
      })
      writeResult(io, parsed, result, `Wrote page ${pagePath}`)
      return 0
    }
    if (!action || action === "list") {
      const result = await getProjectBootstrap({
        ...localOptions(parsed),
        panelKind: "wiki",
      })
      const state = result.state as {
        wikiSpaces?: Array<{ id: string; pageIndex?: unknown[] }>
      }
      const space = state.wikiSpaces?.find((item) => item.id === wikiSpaceId)
      const pages = Array.isArray(space?.pageIndex) ? space.pageIndex : []
      writeResult(io, parsed, { pages }, `${pages.length} page(s)`)
      return 0
    }
  }

  throw new Error("Unknown wiki command.")
}

async function writeAgentContext(
  parsed: ParsedArgs,
  io: CliIo,
  panelKind?: OpenPanelsPanelKind
) {
  const options = localOptions(parsed)
  const [result, guides, selection] = await Promise.all([
    getProjectBootstrap({
      ...options,
      panelKind,
    }),
    listAgentGuides(),
    safeGetSelection(options),
  ])
  writeResult(
    io,
    parsed,
    agentContextPayload(result, {
      cliVersion: CLI_VERSION,
      guides,
      selectionResult: selection,
    }),
    agentContextMarkdown(result, {
      cliVersion: CLI_VERSION,
      guides,
      selectionResult: selection,
    })
  )
}

async function safeGetSelection(options: CliLocalOptions) {
  try {
    return await getSelection(options)
  } catch (_error) {
    return null
  }
}

function renderCapabilitiesSummary() {
  return `# OpenPanels Agent Capabilities

| Intent | Command |
| --- | --- |
${AGENT_CAPABILITIES.map(
  (capability) => `| \`${capability.intent}\` | \`${capability.command}\` |`
).join("\n")}
`
}

async function startStudio(options: CliLocalOptions): Promise<StudioSession> {
  const paths = resolveOpenPanelsPaths(options.projectDir, options)
  const host = studioHost(options)
  const current = await studioStatus(options)
  if (
    current.server === "running" &&
    current.session &&
    studioSessionMatchesHost(current.session, host)
  ) {
    return current.session
  }
  if (current.session?.pid && processExists(current.session.pid)) {
    await terminateProcess(current.session.pid)
  }
  await rm(studioSessionPath(paths.contextDir), { force: true })

  await mkdir(paths.contextDir, { recursive: true })
  const port = await findOpenPort(host)
  const serverUrl = `http://127.0.0.1:${port}`
  const lanServerUrls = studioLanServerUrls(host, port)
  const logPath = join(paths.contextDir, "studio.log")
  const session: StudioSession = {
    browserUrl: lanServerUrls[0] ?? serverUrl,
    contextDir: paths.contextDir,
    contextId: paths.contextId,
    contextIdSource: paths.contextIdSource,
    host,
    lanServerUrls,
    localServerUrl: serverUrl,
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
      "--storage-dir",
      paths.storageDir,
      "--context-id",
      paths.contextId,
      "--port",
      String(port),
      "--host",
      host,
      "--static-dir",
      studioStaticDir(),
    ],
    {
      detached: true,
      env: {
        ...process.env,
        FORCE_COLOR: "0",
        OPENPANELS_STORAGE_DIR: paths.storageDir,
      },
      stdio: ["ignore", logFd, logFd],
    }
  )
  closeSync(logFd)
  child.unref()
  if (!child.pid)
    throw new Error("Failed to start MyOpenPanels studio process.")

  session.pid = child.pid
  await writeStudioSession(paths.contextDir, session)
  try {
    await waitForStudio(serverUrl, DEFAULT_WAIT_TIMEOUT_MS)
  } catch (error) {
    await terminateProcess(child.pid)
    await rm(studioSessionPath(paths.contextDir), { force: true })
    throw error
  }
  return session
}

async function studioStatus(options: CliLocalOptions): Promise<{
  contextDir: string
  contextId: string
  contextIdSource: string
  logPath: string
  projectDir: string
  server: "missing" | "running" | "stale" | "unavailable"
  session: StudioSession | null
  storageDir: string
}> {
  const paths = resolveOpenPanelsPaths(options.projectDir, options)
  const logPath = join(paths.contextDir, "studio.log")
  const session = await readStudioSession(paths.contextDir)
  if (!session) {
    return {
      contextDir: paths.contextDir,
      contextId: paths.contextId,
      contextIdSource: paths.contextIdSource,
      projectDir: paths.projectDir,
      storageDir: paths.storageDir,
      logPath,
      server: "missing",
      session: null,
    }
  }
  if (!processExists(session.pid)) {
    return {
      contextDir: paths.contextDir,
      contextId: paths.contextId,
      contextIdSource: paths.contextIdSource,
      projectDir: paths.projectDir,
      storageDir: paths.storageDir,
      logPath,
      server: "stale",
      session,
    }
  }
  if (await isStudioHealthy(studioHealthUrl(session))) {
    return {
      contextDir: paths.contextDir,
      contextId: paths.contextId,
      contextIdSource: paths.contextIdSource,
      projectDir: paths.projectDir,
      storageDir: paths.storageDir,
      logPath,
      server: "running",
      session,
    }
  }
  return {
    contextDir: paths.contextDir,
    contextId: paths.contextId,
    contextIdSource: paths.contextIdSource,
    projectDir: paths.projectDir,
    storageDir: paths.storageDir,
    logPath,
    server: "unavailable",
    session,
  }
}

async function waitForExistingStudio(
  options: CliLocalOptions,
  timeoutMs: number
) {
  const status = await studioStatus(options)
  if (!status.session) {
    throw new Error("MyOpenPanels studio is not running.")
  }
  await waitForStudio(studioHealthUrl(status.session), timeoutMs)
  return status.session
}

async function stopStudio(options: CliLocalOptions) {
  const paths = resolveOpenPanelsPaths(options.projectDir, options)
  const session = await readStudioSession(paths.contextDir)
  if (session?.pid && processExists(session.pid)) {
    await terminateProcess(session.pid)
  }
  await rm(studioSessionPath(paths.contextDir), { force: true })
  return {
    contextDir: paths.contextDir,
    contextId: paths.contextId,
    contextIdSource: paths.contextIdSource,
    projectDir: paths.projectDir,
    storageDir: paths.storageDir,
    stopped: true,
  }
}

async function serveStudio(parsed: ParsedArgs) {
  const projectDir = stringFlag(parsed, "project")
  const storageDir = stringFlag(parsed, "storage-dir")
  const contextId = stringFlag(parsed, "context-id")
  const port = numberFlag(parsed, "port")
  const host = stringFlag(parsed, "host") ?? "127.0.0.1"
  const staticDir = stringFlag(parsed, "static-dir")
  if (!projectDir)
    throw new Error("Missing --project for internal studio server.")
  if (!storageDir)
    throw new Error("Missing --storage-dir for internal studio server.")
  if (!contextId)
    throw new Error("Missing --context-id for internal studio server.")
  if (!port) throw new Error("Missing --port for internal studio server.")
  if (!staticDir)
    throw new Error("Missing --static-dir for internal studio server.")
  if (!existsSync(join(staticDir, "index.html"))) {
    throw new Error(`MyOpenPanels studio static files not found: ${staticDir}`)
  }

  const server = createLocalOpenPanelsServer({
    buildInfo: {
      channel: "release",
      label: `v${CLI_VERSION}`,
      version: CLI_VERSION,
    },
    projectDir,
    staticDir,
    storageDir,
    contextId,
  })
  await new Promise<void>((resolveListen) => {
    server.listen(port, host, resolveListen)
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

function requiredFlag(parsed: ParsedArgs, name: string): string {
  const value = stringFlag(parsed, name)
  if (!value) throw new Error(`Missing required --${name} <value>.`)
  return value
}

function localOptions(parsed: ParsedArgs): CliLocalOptions {
  return {
    projectDir: stringFlag(parsed, "project"),
    storageDir: stringFlag(parsed, "storage-dir"),
    contextId: stringFlag(parsed, "context-id"),
    host: stringFlag(parsed, "host"),
    localOnly: Boolean(parsed.flags["local-only"]),
  }
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

function panelKindFlag(parsed: ParsedArgs): OpenPanelsPanelKind | undefined {
  const kind = stringFlag(parsed, "kind")
  if (kind === undefined) return undefined
  if (
    kind === "wiki" ||
    kind === "canvas" ||
    kind === "image" ||
    kind === "diff" ||
    kind === "preview" ||
    kind === "files"
  ) {
    return kind
  }
  throw new Error(
    "Expected --kind to be one of: wiki, canvas, image, diff, preview, files."
  )
}

function wikiTaskStatusFlag(
  parsed: ParsedArgs
):
  | "queued"
  | "claimed"
  | "running"
  | "failed"
  | "succeeded"
  | "stale"
  | undefined {
  const status = stringFlag(parsed, "status")
  if (status === undefined) return undefined
  if (
    status === "queued" ||
    status === "claimed" ||
    status === "running" ||
    status === "failed" ||
    status === "succeeded" ||
    status === "stale"
  ) {
    return status
  }
  throw new Error(
    "Expected --status to be one of: queued, claimed, running, failed, succeeded, stale."
  )
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
  contextDir: string
): Promise<StudioSession | null> {
  try {
    const parsed = JSON.parse(
      await readFile(studioSessionPath(contextDir), "utf8")
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

async function writeStudioSession(contextDir: string, session: StudioSession) {
  await mkdir(contextDir, { recursive: true })
  writeFileSync(
    studioSessionPath(contextDir),
    `${JSON.stringify(session, null, 2)}\n`,
    "utf8"
  )
}

function studioSessionPath(contextDir: string) {
  return join(contextDir, "studio-session.json")
}

async function findOpenPort(host: string): Promise<number> {
  return new Promise((resolvePort, reject) => {
    const server = createServer()
    server.unref()
    server.on("error", reject)
    server.listen(0, host, () => {
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

function studioHost(options: CliLocalOptions): string {
  if (options.localOnly) return "127.0.0.1"
  const explicitHost = options.host ?? process.env.OPENPANELS_STUDIO_HOST
  if (explicitHost) return explicitHost
  return "0.0.0.0"
}

function studioSessionMatchesHost(
  session: StudioSession,
  host: string
): boolean {
  return (session.host ?? "127.0.0.1") === host
}

function studioLanServerUrls(host: string, port: number): string[] {
  if (isLoopbackHost(host)) return []
  if (host !== "0.0.0.0" && host !== "::") {
    return [`http://${formatUrlHost(host)}:${port}`]
  }
  return lanAddresses().map(
    (address) => `http://${formatUrlHost(address)}:${port}`
  )
}

function lanAddresses(): string[] {
  const candidates = Object.values(networkInterfaces())
    .flatMap((interfaces) => interfaces ?? [])
    .filter(
      (entry) =>
        entry.family === "IPv4" &&
        !entry.internal &&
        isUsableLanAddress(entry.address)
    )
    .map((entry) => entry.address)

  return [...new Set(candidates)].sort(lanAddressSort)
}

function lanAddressSort(left: string, right: string): number {
  return (
    lanAddressRank(left) - lanAddressRank(right) || left.localeCompare(right)
  )
}

function lanAddressRank(address: string): number {
  if (address.startsWith("192.168.")) return 0
  if (address.startsWith("10.")) return 1
  const secondOctet = Number(address.split(".")[1])
  if (
    address.startsWith("172.") &&
    Number.isInteger(secondOctet) &&
    secondOctet >= 16 &&
    secondOctet <= 31
  ) {
    return 2
  }
  return 3
}

function isUsableLanAddress(address: string): boolean {
  return address !== "0.0.0.0" && !address.startsWith("169.254.")
}

function isLoopbackHost(host: string): boolean {
  return ["127.0.0.1", "localhost", "::1"].includes(host)
}

function formatUrlHost(host: string): string {
  return host.includes(":") && !host.startsWith("[") ? `[${host}]` : host
}

function studioHealthUrl(session: StudioSession): string {
  return session.localServerUrl ?? session.serverUrl
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
  agent context             Print compact agent context with capabilities
  agent capabilities        Print the agent capability manifest
  agent guides              List loadable agent guides
  agent guide <id>          Print one full agent guide
  agent-context             Print current project, panels, and agent instructions
  panels                    List panels in the current project
  active-panel              Read or switch the active project panel
  panel-state               Read state for the active or requested panel
  canvas-state              Read the current canvas state
  selection                 Read the current canvas selection
  read-selection-asset      Write the exported selection asset to a file
  insert-placeholder        Insert a generation placeholder into a clear area
  insert-image              Insert a local image into the canvas

Options:
  --project <dir>           Project directory (default: cwd or OPENPANELS_PROJECT_DIR; data is global)
  --host <host>             Studio bind host (default: 0.0.0.0; set 127.0.0.1 for local-only)
  --local-only              Bind the studio to 127.0.0.1
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
