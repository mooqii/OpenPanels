import { spawn } from "node:child_process"
import { createHash, randomUUID } from "node:crypto"
import { closeSync, openSync, realpathSync } from "node:fs"
import {
  access,
  copyFile,
  mkdir,
  readFile,
  rename,
  rm,
  stat,
  writeFile,
} from "node:fs/promises"
import { homedir } from "node:os"
import {
  basename,
  dirname,
  extname,
  join,
  relative,
  resolve,
  sep,
} from "node:path"
import {
  LocalOpenPanelsStorage,
  sanitizePathPart,
} from "@openpanels/local-storage"
import type {
  OpenPanelsPanel,
  OpenPanelsPanelKind,
  OpenPanelsSession,
} from "@openpanels/protocol"
import { OpenPanelsRuntime } from "@openpanels/runtime"

export interface OpenPanelsLocalPaths {
  contextDir: string
  contextId: string
  contextIdSource: string
  projectDir: string
  storageDir: string
}

export interface OpenPanelsLocalContextOptions {
  contextId?: string
  storageDir?: string
}

export interface OpenPanelsLocalContext {
  paths: OpenPanelsLocalPaths
  runtime: OpenPanelsRuntime
  storage: LocalOpenPanelsStorage
}

export interface CanvasBootstrap {
  contextDir: string
  contextId: string
  contextIdSource: string
  panel: OpenPanelsPanel
  panelDir: string
  session: OpenPanelsSession
  sessions: OpenPanelsSession[]
  state: unknown
  storageDir: string
}

export interface ProjectPanelSnapshot {
  panel: OpenPanelsPanel
  state: unknown
}

export interface ProjectBootstrap {
  activePanelId: string
  activePanelKind: OpenPanelsPanelKind
  contextDir: string
  contextId: string
  contextIdSource: string
  panel: OpenPanelsPanel
  panelDir: string
  panels: ProjectPanelSnapshot[]
  session: OpenPanelsSession
  sessions: OpenPanelsSession[]
  state: unknown
  storageDir: string
}

export type WikiTaskType =
  | "convert_document_to_markdown"
  | "ingest_markdown_into_wiki"
  | "lint_wiki"
  | "rebuild_wiki_index"

export type WikiTaskStatus =
  | "queued"
  | "claimed"
  | "running"
  | "failed"
  | "succeeded"
  | "stale"

export type WikiLanguage = "en" | "zh-CN"

export interface WikiRawDocument {
  conversion: {
    error: string | null
    status: "not_required" | "queued" | "converting" | "failed" | "ready"
    taskId: string | null
    updatedAt: string
  }
  createdAt: string
  id: string
  ingestionByWikiSpace: Record<
    string,
    {
      error: string | null
      markdownVersion: number
      status:
        | "not_started"
        | "queued"
        | "ingesting"
        | "failed"
        | "ingested"
        | "stale"
      taskId: string | null
      updatedAt: string
    }
  >
  markdownRef: string | null
  markdownVersion: number
  mimeType: string
  originalFileName: string
  originalRef: string
  sha256: string
  sizeBytes: number
  source: "agent" | "user"
  title: string
  updatedAt: string
}

export interface WikiRuleSet {
  builtIn: boolean
  createdAt: string
  description: string
  id: string
  rulesRef: string
  title: string
  updatedAt: string
  version: number
}

export interface WikiPageIndexItem {
  path: string
  sourceDocumentIds: string[]
  summary: string
  tags: string[]
  title: string
  type: string
  updatedAt: string
}

export interface WikiSpace {
  createdAt: string
  id: string
  pageIndex: WikiPageIndexItem[]
  rootRef: string
  ruleSetId: string
  ruleSetVersion: number
  title: string
  updatedAt: string
}

export interface WikiTask {
  claimedByProcessId: string | null
  createdAt: string
  documentId: string | null
  error: string | null
  id: string
  markdownVersion: number | null
  result?: Record<string, unknown> | null
  ruleSetId: string | null
  ruleSetVersion: number | null
  status: WikiTaskStatus
  targetId: string
  type: WikiTaskType
  updatedAt: string
  wikiSpaceId: string | null
}

export interface AgentProcessContext {
  agentHost: string
  id: string
  startedAt: string
  status: "failed" | "finished" | "idle" | "running"
  taskId: string | null
  threadId: string | null
  updatedAt: string
  wikiSpaceId: string
}

export interface AgentThreadTarget {
  contextId: string
  createdAt: string
  host: string
  id: string
  projectDir: string
  threadId: string
  updatedAt: string
  wakeUrl: string | null
}

export interface WikiStateV2 {
  activeRawDocumentId: string | null
  activeWikiPagePath: string | null
  activeWikiSpaceId: string | null
  agentProcesses: AgentProcessContext[]
  rawDocuments: WikiRawDocument[]
  ruleSets: WikiRuleSet[]
  schemaVersion: 2
  tasks: WikiTask[]
  wikiLanguage: WikiLanguage | null
  wikiSpaces: WikiSpace[]
}

export interface WikiBootstrap {
  panel: OpenPanelsPanel
  session: OpenPanelsSession
  state: WikiStateV2
}

export interface AddWikiRawDocumentInput {
  content?: Buffer | string
  contextId?: string
  fileName: string
  mimeType?: string
  projectDir?: string
  source?: "agent" | "user"
  sourcePath?: string
  storageDir?: string
  title?: string
  wikiSpaceId?: string | null
}

export interface WriteWikiMarkdownInput {
  content: string
  contextId?: string
  documentId: string
  expectedVersion?: number
  projectDir?: string
  storageDir?: string
  taskId?: string
}

export interface WriteWikiPageInput {
  content: string
  contextId?: string
  expectedUpdatedAt?: string
  pagePath: string
  projectDir?: string
  storageDir?: string
  taskId?: string
  title?: string
  wikiSpaceId: string
}

export interface DeleteSessionResult {
  activeSessionId: string
  deletedSessionId: string
  sessions: OpenPanelsSession[]
}

export interface SelectionResult {
  base64: string | null
  contextDir: string
  contextId: string
  contextIdSource: string
  mimeType: string | null
  selection: Record<string, unknown>
  selectionFile: string
}

export interface InsertImageInput {
  anchorShapeId?: string
  contextId?: string
  displayHeight?: number
  displayWidth?: number
  fileName?: string
  imagePath: string
  placement?: "below" | "left" | "right"
  projectDir?: string
  replaceShapeId?: string
  sessionId?: string
  storageDir?: string
}

export interface InsertPlaceholderInput {
  anchorShapeId?: string
  contextId?: string
  displayHeight?: number
  displayWidth?: number
  projectDir?: string
  sessionId?: string
  storageDir?: string
  text?: string
}

interface ImageDimensions {
  height: number
  width: number
}

interface Bounds {
  height: number
  width: number
  x: number
  y: number
}

interface OccupiedBounds {
  maxX: number
  maxY: number
  minX: number
  minY: number
}

const DEFAULT_CANVAS_GAP = 80
const DEFAULT_PLACEHOLDER_SIZE = 512
const DEFAULT_PANEL_KINDS: OpenPanelsPanelKind[] = ["wiki", "canvas"]
const DEFAULT_ACTIVE_PANEL_KIND: OpenPanelsPanelKind = "wiki"
const DEFAULT_WIKI_RULE_SET_ID = "rule:default"
const DEFAULT_WIKI_SPACE_ID = "wiki:default"
const MAX_POSITION_SCAN = 40
const CONTEXT_ENV_VARS = [
  "CODEX_THREAD_ID",
  "HERMES_THREAD_ID",
  "HERMES_CONVERSATION_ID",
  "HERMES_SESSION_ID",
]

export function resolveOpenPanelsPaths(
  projectDir?: string,
  options: OpenPanelsLocalContextOptions = {}
): OpenPanelsLocalPaths {
  const resolvedProjectDir = resolve(
    projectDir || process.env.OPENPANELS_PROJECT_DIR || process.cwd()
  )
  const storageDir = resolve(options.storageDir ?? defaultStorageDir())
  const { contextId, contextIdSource } = resolveContextId(options.contextId)
  const contextDir = resolve(storageDir, "contexts", contextId)
  assertInside(storageDir, contextDir)
  return {
    contextDir,
    contextId,
    contextIdSource,
    projectDir: resolvedProjectDir,
    storageDir,
  }
}

export function createOpenPanelsLocalContext(
  projectDir?: string,
  options: OpenPanelsLocalContextOptions = {}
): OpenPanelsLocalContext {
  const paths = resolveOpenPanelsPaths(projectDir, options)
  const storage = new LocalOpenPanelsStorage({
    projectDir: paths.projectDir,
    storageDir: paths.storageDir,
  })
  const runtime = new OpenPanelsRuntime({ storage })
  return { paths, runtime, storage }
}

export function createLocalOpenPanelsRuntime(
  projectDir: string,
  options: OpenPanelsLocalContextOptions = {}
) {
  return createOpenPanelsLocalContext(projectDir, options).runtime
}

export async function ensureCanvasBootstrap(
  context: OpenPanelsLocalContext,
  requestedSessionId?: string | null
): Promise<CanvasBootstrap> {
  const bootstrap = await ensureProjectBootstrap(context, {
    requestedPanelKind: "canvas",
    requestedSessionId,
  })
  const canvasPanel =
    bootstrap.panels.find(({ panel }) => panel.kind === "canvas") ??
    bootstrap.panels.find(({ panel }) => panel.id === bootstrap.panel.id)
  if (!canvasPanel) {
    throw new Error("Canvas panel not found")
  }
  return {
    contextDir: bootstrap.contextDir,
    contextId: bootstrap.contextId,
    contextIdSource: bootstrap.contextIdSource,
    session: bootstrap.session,
    panel: canvasPanel.panel,
    sessions: bootstrap.sessions,
    state: canvasPanel.state,
    storageDir: bootstrap.storageDir,
    panelDir: panelDir(context, bootstrap.session.id, canvasPanel.panel.id),
  }
}

export async function ensureProjectBootstrap(
  context: OpenPanelsLocalContext,
  options: {
    requestedPanelId?: string | null
    requestedPanelKind?: OpenPanelsPanelKind | null
    requestedSessionId?: string | null
  } = {}
): Promise<ProjectBootstrap> {
  const sessions = await context.runtime.listSessions()
  const activeSessionId = await readActiveSession(context)
  const session =
    (options.requestedSessionId
      ? await context.runtime.getSession(options.requestedSessionId)
      : null) ??
    (activeSessionId
      ? await context.runtime.getSession(activeSessionId)
      : null) ??
    (await context.runtime.createSession({ title: nextProjectTitle(sessions) }))
  const bootstrap = await ensureProjectForSession(context, session, options)
  await writeActiveSession(context, bootstrap.session.id)
  await writeActivePanel(context, bootstrap.panel)
  return bootstrap
}

export async function createCanvasProject(
  context: OpenPanelsLocalContext,
  title?: string
): Promise<ProjectBootstrap> {
  const nextTitle =
    title?.trim() || nextProjectTitle(await context.runtime.listSessions())
  const session = await context.runtime.createSession({ title: nextTitle })
  const bootstrap = await ensureProjectForSession(context, session, {
    requestedPanelKind: DEFAULT_ACTIVE_PANEL_KIND,
  })
  await writeActiveSession(context, bootstrap.session.id)
  await writeActivePanel(context, bootstrap.panel)
  return bootstrap
}

export async function getCanvasState(input: {
  contextId?: string
  projectDir?: string
  sessionId?: string | null
  storageDir?: string
}): Promise<CanvasBootstrap> {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  return ensureCanvasBootstrap(context, input.sessionId)
}

export async function readActiveSession(
  context: OpenPanelsLocalContext
): Promise<string | null> {
  const active = await readJsonObjectOrNull(activeSessionPath(context))
  return typeof active?.sessionId === "string" ? active.sessionId : null
}

export async function writeActiveSession(
  context: OpenPanelsLocalContext,
  sessionId: string
): Promise<void> {
  const filePath = activeSessionPath(context)
  await mkdir(dirname(filePath), { recursive: true })
  await writeJson(filePath, {
    sessionId,
    updatedAt: new Date().toISOString(),
  })
}

export async function readActivePanel(
  context: OpenPanelsLocalContext
): Promise<{
  kind?: OpenPanelsPanelKind
  panelId?: string
  sessionId?: string
} | null> {
  const active = await readJsonObjectOrNull(activePanelPath(context))
  if (!active) return null
  return {
    kind: isPanelKind(active.kind) ? active.kind : undefined,
    panelId: typeof active.panelId === "string" ? active.panelId : undefined,
    sessionId:
      typeof active.sessionId === "string" ? active.sessionId : undefined,
  }
}

export async function writeActivePanel(
  context: OpenPanelsLocalContext,
  panel: OpenPanelsPanel
): Promise<void> {
  const filePath = activePanelPath(context)
  await mkdir(dirname(filePath), { recursive: true })
  await writeJson(filePath, {
    sessionId: panel.sessionId,
    panelId: panel.id,
    kind: panel.kind,
    updatedAt: new Date().toISOString(),
  })
}

export async function setActivePanel(input: {
  contextId?: string
  kind?: OpenPanelsPanelKind
  panelId?: string
  projectDir?: string
  sessionId?: string | null
  storageDir?: string
}): Promise<ProjectBootstrap> {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  return ensureProjectBootstrap(context, {
    requestedPanelId: input.panelId,
    requestedPanelKind: input.kind,
    requestedSessionId: input.sessionId,
  })
}

export async function getProjectBootstrap(input: {
  contextId?: string
  panelId?: string | null
  panelKind?: OpenPanelsPanelKind | null
  projectDir?: string
  sessionId?: string | null
  storageDir?: string
}): Promise<ProjectBootstrap> {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  return ensureProjectBootstrap(context, {
    requestedPanelId: input.panelId,
    requestedPanelKind: input.panelKind,
    requestedSessionId: input.sessionId,
  })
}

export async function getWikiBootstrap(input: {
  contextId?: string
  projectDir?: string
  storageDir?: string
}): Promise<WikiBootstrap> {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const bootstrap = await ensureProjectBootstrap(context, {
    requestedPanelKind: "wiki",
  })
  const state = await ensureWikiState(
    context,
    bootstrap.session,
    bootstrap.panel
  )
  return { session: bootstrap.session, panel: bootstrap.panel, state }
}

export async function addWikiRawDocument(input: AddWikiRawDocumentInput) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const { session, panel, state } = await getWikiBootstrap({
    projectDir: input.projectDir,
    storageDir: input.storageDir,
    contextId: input.contextId,
  })
  const now = new Date().toISOString()
  const wikiSpace = resolveWikiSpace(state, input.wikiSpaceId)
  const fileName = safePart(input.fileName)
  const content =
    input.content !== undefined
      ? Buffer.isBuffer(input.content)
        ? input.content
        : Buffer.from(input.content, "utf8")
      : input.sourcePath
        ? await readFile(resolve(input.sourcePath))
        : Buffer.alloc(0)
  const documentId = createId("raw")
  const rawDir = wikiPanelPath(context, session.id, panel.id, "raw", documentId)
  const originalPath = wikiPanelPath(
    context,
    session.id,
    panel.id,
    "raw",
    documentId,
    "original",
    fileName
  )
  await mkdir(dirname(originalPath), { recursive: true })
  if (input.sourcePath) {
    await copyFile(resolve(input.sourcePath), originalPath)
  } else {
    await writeFile(originalPath, content)
  }

  const originalRef = wikiRef("raw", documentId, "original", fileName)
  const markdownRef = wikiRef("raw", documentId, "source.md")
  const isText = isPlainTextFile(fileName, input.mimeType)
  const markdownContent = isText ? content.toString("utf8") : ""
  if (isText) {
    await writeFile(
      wikiPanelPath(context, session.id, panel.id, ...markdownRef.split("/")),
      markdownContent,
      "utf8"
    )
  }

  const conversionTask = isText
    ? null
    : createWikiTask(state, {
        documentId,
        markdownVersion: 0,
        targetId: documentId,
        type: "convert_document_to_markdown",
        wikiSpaceId: wikiSpace.id,
      })
  const document: WikiRawDocument = {
    id: documentId,
    title: input.title?.trim() || titleFromFileName(fileName),
    originalFileName: fileName,
    mimeType: input.mimeType || mimeTypeForFile(fileName),
    sizeBytes: content.byteLength,
    sha256: createHash("sha256").update(content).digest("hex"),
    source: input.source ?? "user",
    originalRef,
    markdownRef: isText ? markdownRef : null,
    markdownVersion: isText ? 1 : 0,
    conversion: {
      status: isText ? "not_required" : "queued",
      taskId: conversionTask?.id ?? null,
      error: null,
      updatedAt: now,
    },
    ingestionByWikiSpace: {
      [wikiSpace.id]: isText
        ? createIngestionState(
            createWikiTask(state, {
              documentId,
              markdownVersion: 1,
              targetId: documentId,
              type: "ingest_markdown_into_wiki",
              wikiSpaceId: wikiSpace.id,
            }),
            1
          )
        : {
            status: "not_started",
            taskId: null,
            markdownVersion: 0,
            error: null,
            updatedAt: now,
          },
    },
    createdAt: now,
    updatedAt: now,
  }
  state.rawDocuments.unshift(document)
  state.activeRawDocumentId = document.id
  await writeFile(
    join(rawDir, "meta.json"),
    `${JSON.stringify(document, null, 2)}\n`
  )
  await saveWikiState(context, session.id, panel.id, state)
  await wakeQueuedWikiTasks(context, session, panel, state)
  return { document, state }
}

export async function listWikiTasks(input: {
  contextId?: string
  projectDir?: string
  status?: WikiTaskStatus
  storageDir?: string
}) {
  const { state } = await getWikiBootstrap(input)
  return {
    tasks: input.status
      ? state.tasks.filter((task) => task.status === input.status)
      : state.tasks,
  }
}

export async function nextWikiTask(input: {
  contextId?: string
  projectDir?: string
  storageDir?: string
}) {
  const { state } = await getWikiBootstrap(input)
  return (
    state.tasks.find((task) => task.status === "queued") ??
    state.tasks.find((task) => task.status === "failed") ??
    null
  )
}

export async function claimWikiTask(input: {
  agentHost?: string
  contextId?: string
  projectDir?: string
  storageDir?: string
  taskId: string
  threadId?: string | null
}) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const { session, panel, state } = await getWikiBootstrap(input)
  const task = findWikiTask(state, input.taskId)
  if (task.status !== "queued" && task.status !== "failed") {
    throw new Error(`Wiki task is not claimable: ${task.id}`)
  }
  const processId = createId("process")
  const now = new Date().toISOString()
  const processContext: AgentProcessContext = {
    id: processId,
    agentHost: input.agentHost ?? "unknown",
    threadId: input.threadId ?? null,
    taskId: task.id,
    wikiSpaceId:
      task.wikiSpaceId ?? state.activeWikiSpaceId ?? DEFAULT_WIKI_SPACE_ID,
    status: "running",
    startedAt: now,
    updatedAt: now,
  }
  task.status =
    task.type === "convert_document_to_markdown" ? "running" : "claimed"
  task.claimedByProcessId = processId
  task.updatedAt = now
  if (task.type === "convert_document_to_markdown" && task.documentId) {
    const document = findWikiDocument(state, task.documentId)
    document.conversion.status = "converting"
    document.conversion.updatedAt = now
    document.updatedAt = now
  }
  if (task.type === "ingest_markdown_into_wiki" && task.documentId) {
    const document = findWikiDocument(state, task.documentId)
    const wikiSpaceId =
      task.wikiSpaceId ?? state.activeWikiSpaceId ?? DEFAULT_WIKI_SPACE_ID
    document.ingestionByWikiSpace[wikiSpaceId] = {
      ...(document.ingestionByWikiSpace[wikiSpaceId] ?? {
        error: null,
        markdownVersion: task.markdownVersion ?? document.markdownVersion,
        taskId: task.id,
      }),
      error: null,
      markdownVersion: task.markdownVersion ?? document.markdownVersion,
      status: "ingesting",
      taskId: task.id,
      updatedAt: now,
    }
    document.updatedAt = now
  }
  state.agentProcesses.unshift(processContext)
  await saveProcess(context, session.id, panel.id, processContext)
  await saveWikiTask(context, session.id, panel.id, task)
  await saveWikiState(context, session.id, panel.id, state)
  return { process: processContext, task, state }
}

export async function completeWikiTask(input: {
  contextId?: string
  projectDir?: string
  result?: Record<string, unknown> | null
  storageDir?: string
  taskId: string
}) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const { session, panel, state } = await getWikiBootstrap(input)
  const task = findWikiTask(state, input.taskId)
  const now = new Date().toISOString()
  task.status = "succeeded"
  task.error = null
  task.result = input.result ?? null
  task.updatedAt = now
  const queuedFollowUpTaskIds: string[] = []
  const process = task.claimedByProcessId
    ? state.agentProcesses.find((item) => item.id === task.claimedByProcessId)
    : null
  if (process) {
    process.status = "finished"
    process.updatedAt = now
    await saveProcess(context, session.id, panel.id, process)
  }
  if (task.type === "convert_document_to_markdown" && task.documentId) {
    const document = findWikiDocument(state, task.documentId)
    document.conversion.status = "ready"
    document.conversion.error = null
    document.conversion.updatedAt = now
    const ingestTask = createWikiTask(state, {
      documentId: document.id,
      markdownVersion: document.markdownVersion,
      targetId: document.id,
      type: "ingest_markdown_into_wiki",
      wikiSpaceId: task.wikiSpaceId,
    })
    document.ingestionByWikiSpace[
      ingestTask.wikiSpaceId ?? DEFAULT_WIKI_SPACE_ID
    ] = createIngestionState(ingestTask, document.markdownVersion)
    document.updatedAt = now
    queuedFollowUpTaskIds.push(ingestTask.id)
  }
  if (task.type === "ingest_markdown_into_wiki" && task.documentId) {
    const document = findWikiDocument(state, task.documentId)
    const wikiSpaceId = task.wikiSpaceId ?? state.activeWikiSpaceId
    if (wikiSpaceId && document.ingestionByWikiSpace[wikiSpaceId]) {
      document.ingestionByWikiSpace[wikiSpaceId] = {
        ...document.ingestionByWikiSpace[wikiSpaceId],
        status: "ingested",
        error: null,
        updatedAt: now,
      }
    }
    document.updatedAt = now
  }
  await saveWikiTask(context, session.id, panel.id, task)
  await saveWikiState(context, session.id, panel.id, state)
  await wakeQueuedWikiTasks(context, session, panel, state, {
    allowLocalWorkerFromAgentWorker: queuedFollowUpTaskIds.length > 0,
    taskIds: queuedFollowUpTaskIds,
  })
  return { task, state }
}

export async function failWikiTask(input: {
  contextId?: string
  error: string
  projectDir?: string
  storageDir?: string
  taskId: string
}) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const { session, panel, state } = await getWikiBootstrap(input)
  const task = findWikiTask(state, input.taskId)
  const now = new Date().toISOString()
  task.status = "failed"
  task.error = input.error
  task.updatedAt = now
  if (task.type === "convert_document_to_markdown" && task.documentId) {
    const document = findWikiDocument(state, task.documentId)
    document.conversion = {
      ...document.conversion,
      status: "failed",
      error: input.error,
      updatedAt: now,
    }
    document.updatedAt = now
  }
  if (task.type === "ingest_markdown_into_wiki" && task.documentId) {
    const document = findWikiDocument(state, task.documentId)
    const wikiSpaceId = task.wikiSpaceId ?? state.activeWikiSpaceId
    if (wikiSpaceId) {
      document.ingestionByWikiSpace[wikiSpaceId] = {
        ...(document.ingestionByWikiSpace[wikiSpaceId] ??
          createIngestionState(
            task,
            task.markdownVersion ?? document.markdownVersion
          )),
        status: "failed",
        error: input.error,
        updatedAt: now,
      }
    }
    document.updatedAt = now
  }
  await saveWikiTask(context, session.id, panel.id, task)
  await saveWikiState(context, session.id, panel.id, state)
  return { task, state }
}

export async function readWikiMarkdown(input: {
  contextId?: string
  documentId: string
  projectDir?: string
  storageDir?: string
}) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const { session, panel, state } = await getWikiBootstrap(input)
  const document = findWikiDocument(state, input.documentId)
  if (!document.markdownRef) return { document, markdown: "" }
  const markdown = await readFile(
    wikiPanelPath(
      context,
      session.id,
      panel.id,
      ...document.markdownRef.split("/")
    ),
    "utf8"
  )
  return { document, markdown }
}

export async function readWikiRawDocumentOriginal(input: {
  contextId?: string
  documentId: string
  projectDir?: string
  storageDir?: string
}) {
  const resolved = await resolveWikiRawDocumentOriginal(input)
  return {
    document: resolved.document,
    filePath: resolved.filePath,
    mimeType: resolved.document.mimeType || mimeTypeForFile(resolved.filePath),
    sizeBytes: resolved.stats.size,
  }
}

export async function revealWikiRawDocumentOriginal(
  input: {
    contextId?: string
    documentId: string
    projectDir?: string
    storageDir?: string
  },
  options: {
    platform?: NodeJS.Platform
    spawnCommand?: typeof spawn
  } = {}
) {
  const resolved = await resolveWikiRawDocumentOriginal(input)
  const platform = options.platform ?? process.platform
  const spawnCommand = options.spawnCommand ?? spawn
  const command =
    platform === "darwin"
      ? { file: "open", args: ["-R", resolved.filePath] }
      : platform === "win32"
        ? { file: "explorer.exe", args: [`/select,${resolved.filePath}`] }
        : platform === "linux"
          ? { file: "xdg-open", args: [dirname(resolved.filePath)] }
          : null

  if (!command) {
    throw new Error(`Reveal in file manager is not supported on ${platform}`)
  }

  await new Promise<void>((resolvePromise, reject) => {
    const child = spawnCommand(command.file, command.args, {
      detached: true,
      stdio: "ignore",
    })
    child.once("error", reject)
    child.once("spawn", () => {
      child.unref()
      resolvePromise()
    })
  })

  return {
    document: resolved.document,
    filePath: resolved.filePath,
    revealed: true,
  }
}

export async function deleteWikiRawDocument(input: {
  contextId?: string
  documentId: string
  projectDir?: string
  storageDir?: string
  wikiSpaceId?: string | null
}) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const { session, panel, state } = await getWikiBootstrap(input)
  const wikiSpace = resolveWikiSpace(state, input.wikiSpaceId)
  const documentIndex = state.rawDocuments.findIndex(
    (item) => item.id === input.documentId
  )
  if (documentIndex === -1) {
    throw new Error(`Wiki raw document not found: ${input.documentId}`)
  }
  const document = state.rawDocuments[documentIndex]
  if (!document) {
    throw new Error(`Wiki raw document not found: ${input.documentId}`)
  }
  state.rawDocuments.splice(documentIndex, 1)
  const now = new Date().toISOString()
  for (const task of state.tasks) {
    if (task.documentId === document.id || task.targetId === document.id) {
      task.status = "stale"
      task.error = "Source document deleted"
      task.updatedAt = now
    }
  }
  const task = createWikiRebuildIndexTask(state, wikiSpace.id)
  if (state.activeRawDocumentId === document.id) {
    state.activeRawDocumentId = state.rawDocuments[0]?.id ?? null
  }
  await rm(wikiPanelPath(context, session.id, panel.id, "raw", document.id), {
    force: true,
    recursive: true,
  })
  await saveWikiState(context, session.id, panel.id, state)
  await wakeQueuedWikiTasks(context, session, panel, state)
  return { document, task, state }
}

export async function extractWikiRawDocumentMarkdown(input: {
  contextId?: string
  documentId: string
  projectDir?: string
  storageDir?: string
  wikiSpaceId?: string | null
}) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const { session, panel, state } = await getWikiBootstrap(input)
  const document = findWikiDocument(state, input.documentId)
  const wikiSpace = resolveWikiSpace(state, input.wikiSpaceId)
  const now = new Date().toISOString()
  for (const task of state.tasks) {
    if (
      task.documentId === document.id &&
      task.type === "convert_document_to_markdown" &&
      (task.status === "queued" ||
        task.status === "claimed" ||
        task.status === "running" ||
        task.status === "failed")
    ) {
      task.status = "stale"
      task.error = "Superseded by a new extraction request"
      task.updatedAt = now
    }
  }
  const task = createWikiTask(state, {
    documentId: document.id,
    markdownVersion: document.markdownVersion,
    targetId: document.id,
    type: "convert_document_to_markdown",
    wikiSpaceId: wikiSpace.id,
  })
  document.conversion = {
    status: "queued",
    taskId: task.id,
    error: null,
    updatedAt: task.updatedAt,
  }
  document.updatedAt = now
  await saveWikiState(context, session.id, panel.id, state)
  await wakeQueuedWikiTasks(context, session, panel, state)
  return { document, task, state }
}

export async function writeWikiMarkdown(input: WriteWikiMarkdownInput) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const { session, panel, state } = await getWikiBootstrap(input)
  const document = findWikiDocument(state, input.documentId)
  if (
    input.expectedVersion !== undefined &&
    input.expectedVersion !== document.markdownVersion
  ) {
    throw new Error("Source Markdown has changed. Reload before saving.")
  }
  const now = new Date().toISOString()
  const markdownRef =
    document.markdownRef ?? wikiRef("raw", document.id, "source.md")
  await writeFile(
    wikiPanelPath(context, session.id, panel.id, ...markdownRef.split("/")),
    input.content,
    "utf8"
  )
  document.markdownRef = markdownRef
  document.markdownVersion += 1
  document.conversion = {
    ...document.conversion,
    status:
      document.conversion.status === "not_required" ? "not_required" : "ready",
    error: null,
    updatedAt: now,
  }
  const parentTask = input.taskId ? findWikiTask(state, input.taskId) : null
  const wikiSpace = resolveWikiSpace(state, parentTask?.wikiSpaceId)
  const shouldQueueIngest = parentTask?.type !== "convert_document_to_markdown"
  const task = shouldQueueIngest
    ? createWikiTask(state, {
        documentId: document.id,
        markdownVersion: document.markdownVersion,
        targetId: document.id,
        type: "ingest_markdown_into_wiki",
        wikiSpaceId: wikiSpace.id,
      })
    : null
  if (task) {
    document.ingestionByWikiSpace[wikiSpace.id] = createIngestionState(
      task,
      document.markdownVersion
    )
  }
  const rebuildTask = shouldQueueIngest
    ? createWikiRebuildIndexTask(state, wikiSpace.id)
    : null
  document.updatedAt = now
  await saveWikiState(context, session.id, panel.id, state)
  if (task || rebuildTask) {
    await wakeQueuedWikiTasks(context, session, panel, state)
  }
  return { document, rebuildTask, task, state }
}

export async function reindexWikiRawDocument(input: {
  contextId?: string
  documentId: string
  projectDir?: string
  storageDir?: string
  wikiSpaceId?: string | null
}) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const { session, panel, state } = await getWikiBootstrap(input)
  const document = findWikiDocument(state, input.documentId)
  if (!document.markdownRef) {
    throw new Error("Source Markdown is required before indexing.")
  }
  const wikiSpace = resolveWikiSpace(state, input.wikiSpaceId)
  const task = createWikiTask(state, {
    documentId: document.id,
    markdownVersion: document.markdownVersion,
    targetId: document.id,
    type: "ingest_markdown_into_wiki",
    wikiSpaceId: wikiSpace.id,
  })
  document.ingestionByWikiSpace[wikiSpace.id] = createIngestionState(
    task,
    document.markdownVersion
  )
  document.updatedAt = task.updatedAt
  await saveWikiState(context, session.id, panel.id, state)
  await wakeQueuedWikiTasks(context, session, panel, state)
  return { document, task, state }
}

export async function reindexWikiSpace(input: {
  contextId?: string
  projectDir?: string
  storageDir?: string
  wikiSpaceId?: string | null
}) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const { session, panel, state } = await getWikiBootstrap(input)
  const wikiSpace = resolveWikiSpace(state, input.wikiSpaceId)
  const task = createWikiRebuildIndexTask(state, wikiSpace.id)
  await saveWikiState(context, session.id, panel.id, state)
  await wakeQueuedWikiTasks(context, session, panel, state)
  return { task, state, wikiSpace }
}

export async function registerWikiAgentTarget(input: {
  contextId?: string
  host: string
  projectDir?: string
  storageDir?: string
  threadId: string
  wakeUrl?: string | null
}) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const now = new Date().toISOString()
  const target: AgentThreadTarget = {
    id: createId("target"),
    host: input.host,
    threadId: input.threadId,
    projectDir: context.paths.projectDir,
    contextId: context.paths.contextId,
    wakeUrl: input.wakeUrl ?? null,
    createdAt: now,
    updatedAt: now,
  }
  const targets = await listAgentTargets(context)
  const nextTargets = [
    target,
    ...targets.filter(
      (item) =>
        !(item.host === target.host && item.threadId === target.threadId)
    ),
  ]
  await writeJson(agentTargetsPath(context), nextTargets)
  return { target, targets: nextTargets }
}

export async function listWikiAgentTargets(input: {
  contextId?: string
  projectDir?: string
  storageDir?: string
}) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  return { targets: await listAgentTargets(context) }
}

export async function readWikiPage(input: {
  contextId?: string
  pagePath: string
  projectDir?: string
  storageDir?: string
  wikiSpaceId: string
}) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const { session, panel, state } = await getWikiBootstrap(input)
  const wikiSpace = resolveWikiSpace(state, input.wikiSpaceId)
  const filePath = wikiPagePath(
    context,
    session.id,
    panel.id,
    wikiSpace.id,
    input.pagePath
  )
  return {
    pagePath: input.pagePath,
    wikiSpace,
    markdown: await readFile(filePath, "utf8"),
  }
}

export async function writeWikiPage(input: WriteWikiPageInput) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const { session, panel, state } = await getWikiBootstrap(input)
  const wikiSpace = resolveWikiSpace(state, input.wikiSpaceId)
  const filePath = wikiPagePath(
    context,
    session.id,
    panel.id,
    wikiSpace.id,
    input.pagePath
  )
  const now = new Date().toISOString()
  await mkdir(dirname(filePath), { recursive: true })
  await writeFile(filePath, input.content, "utf8")
  upsertPageIndex(wikiSpace, input.pagePath, input.content, now, input.title)
  const parentTask = input.taskId ? findWikiTask(state, input.taskId) : null
  const task = parentTask
    ? null
    : createWikiTask(state, {
        documentId: null,
        markdownVersion: null,
        targetId: input.pagePath,
        type: "rebuild_wiki_index",
        wikiSpaceId: wikiSpace.id,
      })
  state.activeWikiSpaceId = wikiSpace.id
  state.activeWikiPagePath = input.pagePath
  wikiSpace.updatedAt = now
  await saveWikiState(context, session.id, panel.id, state)
  if (task) {
    await wakeQueuedWikiTasks(context, session, panel, state)
  }
  return { pagePath: input.pagePath, task, wikiSpace, state }
}

export async function setActiveWikiSpace(input: {
  contextId?: string
  projectDir?: string
  storageDir?: string
  wikiSpaceId: string
}) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const { session, panel, state } = await getWikiBootstrap(input)
  const wikiSpace = resolveWikiSpace(state, input.wikiSpaceId)
  state.activeWikiSpaceId = wikiSpace.id
  await saveWikiState(context, session.id, panel.id, state)
  return { wikiSpace, state }
}

export async function setWikiLanguage(input: {
  contextId?: string
  language: WikiLanguage
  projectDir?: string
  storageDir?: string
}) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const { session, panel, state } = await getWikiBootstrap(input)
  state.wikiLanguage = normalizeWikiLanguage(input.language)
  await saveWikiState(context, session.id, panel.id, state)
  return { language: state.wikiLanguage, state }
}

export async function renameSession(
  context: OpenPanelsLocalContext,
  sessionId: string,
  title: string | undefined
): Promise<OpenPanelsSession> {
  const session = await context.storage.readSession(sessionId)
  if (!session) throw new Error(`OpenPanels session not found: ${sessionId}`)
  const nextTitle = title?.trim()
  if (!nextTitle) throw new Error("Project title is required")
  const updated = {
    ...session,
    title: nextTitle,
    updatedAt: new Date().toISOString(),
  }
  await context.storage.writeSession(updated)
  return updated
}

export async function deleteSession(
  context: OpenPanelsLocalContext,
  sessionId: string
): Promise<DeleteSessionResult> {
  const sessions = await context.runtime.listSessions()
  if (sessions.length <= 1) {
    throw new Error("At least one project must remain")
  }
  if (!sessions.some((session) => session.id === sessionId)) {
    throw new Error(`OpenPanels session not found: ${sessionId}`)
  }

  await context.storage.deleteSession(sessionId)
  const remainingSessions = await context.runtime.listSessions()
  const currentActiveSessionId = await readActiveSession(context)
  const nextActiveSession =
    remainingSessions.find(
      (session) => session.id === currentActiveSessionId
    ) ?? remainingSessions[0]
  if (!nextActiveSession) {
    throw new Error("At least one project must remain")
  }

  await writeActiveSession(context, nextActiveSession.id)
  return {
    activeSessionId: nextActiveSession.id,
    deletedSessionId: sessionId,
    sessions: remainingSessions,
  }
}

export async function savePanelState(input: {
  contextId?: string
  panelId: string
  projectDir?: string
  sessionId: string
  state: unknown
  storageDir?: string
}): Promise<{ panelId: string; saved: true; sessionId: string }> {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  await context.runtime.savePanelState(
    input.sessionId,
    input.panelId,
    input.state
  )
  await writeActiveSession(context, input.sessionId)
  return { saved: true, sessionId: input.sessionId, panelId: input.panelId }
}

export async function saveSelectionState(input: {
  contextId?: string
  imageDataUrl?: string | null
  panelId: string
  projectDir?: string
  selection?: Record<string, unknown> | null
  sessionId: string
  storageDir?: string
}): Promise<{ saved: true; selection: Record<string, unknown> }> {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  let assetRef =
    typeof input.selection?.assetRef === "string"
      ? input.selection.assetRef
      : null

  if (input.imageDataUrl) {
    const image = dataUrlToBuffer(input.imageDataUrl)
    const written = await context.storage.writeAssetFromBuffer({
      sessionId: input.sessionId,
      panelId: input.panelId,
      buffer: image.buffer,
      requestedName: "__selection/current.png",
      overwrite: true,
    })
    assetRef = written.assetRef
  }

  const selection = {
    sessionId: input.sessionId,
    panelId: input.panelId,
    selectedShapeIds: Array.isArray(input.selection?.selectedShapeIds)
      ? input.selection?.selectedShapeIds
      : [],
    selectedShapes: Array.isArray(input.selection?.selectedShapes)
      ? input.selection?.selectedShapes
      : [],
    assetRef,
    updatedAt: new Date().toISOString(),
  }
  await context.storage.writePanelSelection(selection)
  await writeActiveSession(context, input.sessionId)
  return { saved: true, selection }
}

export async function getSelection(input: {
  contextId?: string
  includeImageBase64?: boolean
  projectDir?: string
  sessionId?: string | null
  storageDir?: string
}): Promise<SelectionResult> {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const bootstrap = await ensureCanvasBootstrap(context, input.sessionId)
  const rawState =
    (await context.storage.readPanelState(
      bootstrap.session.id,
      bootstrap.panel.id
    )) ?? emptyCanvasSnapshot()
  const rawSelection =
    (await context.storage.readPanelSelection(
      bootstrap.session.id,
      bootstrap.panel.id
    )) ?? emptySelection(bootstrap.session.id, bootstrap.panel.id)
  const selection = withLastImageFallback(rawSelection, rawState)
  let base64: string | null = null
  const assetRef =
    typeof selection.assetRef === "string" ? selection.assetRef : null
  if (input.includeImageBase64 && assetRef) {
    base64 = (await context.storage.readAsset(assetRef)).toString("base64")
  }
  return {
    selection: selection as Record<string, unknown>,
    selectionFile: panelFile(
      context,
      bootstrap.session.id,
      bootstrap.panel.id,
      "selection.json"
    ),
    base64,
    contextDir: context.paths.contextDir,
    contextId: context.paths.contextId,
    contextIdSource: context.paths.contextIdSource,
    mimeType: assetRef ? mimeTypeForFile(assetRef) : null,
  }
}

export async function readPanelAsset(input: {
  assetRef: string
  contextId?: string
  projectDir?: string
  storageDir?: string
}): Promise<{ assetRef: string; base64: string; mimeType: string }> {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const data = await context.storage.readAsset(input.assetRef)
  return {
    assetRef: input.assetRef,
    base64: data.toString("base64"),
    mimeType: mimeTypeForFile(input.assetRef),
  }
}

export async function readSelectionAsset(input: {
  contextId?: string
  projectDir?: string
  sessionId?: string | null
  storageDir?: string
}): Promise<{
  assetRef: string
  base64: string
  filePath: string
  mimeType: string
}> {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const selection = await getSelection({
    projectDir: input.projectDir,
    sessionId: input.sessionId,
    contextId: input.contextId,
    includeImageBase64: true,
    storageDir: input.storageDir,
  })
  const assetRef =
    typeof selection.selection.assetRef === "string"
      ? selection.selection.assetRef
      : null
  if (!(assetRef && selection.base64)) {
    throw new Error("No MyOpenPanels selection asset is available.")
  }
  return {
    assetRef,
    base64: selection.base64,
    filePath: context.storage.assetPath(assetRef),
    mimeType: selection.mimeType ?? mimeTypeForFile(assetRef),
  }
}

export async function writePanelAsset(input: {
  contextId?: string
  panelId: string
  projectDir?: string
  requestedName?: string
  sessionId: string
  sourcePath: string
  storageDir?: string
}) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  return context.storage.writeAssetFromFile({
    sessionId: input.sessionId,
    panelId: input.panelId,
    sourcePath: input.sourcePath,
    requestedName: input.requestedName,
  })
}

export async function insertImage(input: InsertImageInput) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const bootstrap = await ensureCanvasBootstrap(context, input.sessionId)
  const source = resolve(input.imagePath)
  await stat(source)
  const imageBuffer = await readFile(source)
  const dimensions: Partial<ImageDimensions> =
    readImageDimensions(imageBuffer) ?? {}
  const written = await context.storage.writeAssetFromFile({
    sessionId: bootstrap.session.id,
    panelId: bootstrap.panel.id,
    sourcePath: source,
    requestedName: input.fileName ?? basename(source),
  })

  const state =
    ((await context.storage.readPanelState(
      bootstrap.session.id,
      bootstrap.panel.id
    )) as Record<string, any> | null) ?? emptyCanvasSnapshot()
  const store =
    state.store && typeof state.store === "object" ? state.store : {}
  const pageId = state.currentPageId || findFirstPageId(store) || "page:main"
  if (!store[pageId]) {
    store[pageId] = { id: pageId, typeName: "page", name: "Page 1", index: 1 }
  }

  const replaceShape = input.replaceShapeId ? store[input.replaceShapeId] : null
  const replaceBounds =
    replaceShape?.typeName === "shape" ? shapeBounds(replaceShape) : null
  const width =
    input.displayWidth ??
    (replaceBounds ? replaceBounds.width : undefined) ??
    dimensions.width ??
    512
  const height =
    input.displayHeight ??
    (replaceBounds ? replaceBounds.height : undefined) ??
    (dimensions.width && dimensions.height && input.displayWidth
      ? Math.round((input.displayWidth * dimensions.height) / dimensions.width)
      : (dimensions.height ?? 512))
  const anchor = input.anchorShapeId ? store[input.anchorShapeId] : null
  const anchorBounds = anchor?.typeName === "shape" ? shapeBounds(anchor) : null
  const position = replaceBounds
    ? { x: replaceBounds.x, y: replaceBounds.y }
    : placeImage(anchorBounds, width, height, input.placement)
  const assetId = createId("asset")
  const shapeId = createId("shape")
  const assetUrl = `/api/panels/${encodeURIComponent(bootstrap.session.id)}/${encodeURIComponent(bootstrap.panel.id)}/assets/${encodeURIComponent(written.fileName)}`
  const mimeType = mimeTypeForFile(written.fileName)
  const parentId = replaceShape?.parentId || anchor?.parentId || pageId

  store[assetId] = {
    id: assetId,
    typeName: "asset",
    type: "image",
    props: {
      name: written.fileName,
      src: assetUrl,
      w: dimensions.width ?? width,
      h: dimensions.height ?? height,
      mimeType,
      isAnimated: false,
    },
    meta: { assetRef: written.assetRef },
  }
  store[shapeId] = {
    id: shapeId,
    typeName: "shape",
    type: "image",
    parentId,
    index: nextShapeIndex(store, pageId),
    props: {
      x: position.x,
      y: position.y,
      width,
      height,
      assetId,
    },
  }
  if (replaceShape?.typeName === "shape" && input.replaceShapeId) {
    delete store[input.replaceShapeId]
  }
  state.store = store
  state.currentPageId = pageId
  state.selectedShapeIds = [shapeId]
  await context.storage.writePanelState(
    bootstrap.session.id,
    bootstrap.panel.id,
    state
  )
  await writeActiveSession(context, bootstrap.session.id)
  return {
    sessionId: bootstrap.session.id,
    panelId: bootstrap.panel.id,
    assetId,
    shapeId,
    assetRef: written.assetRef,
    assetFile: written.filePath,
    assetUrl,
    replacedShapeId:
      replaceShape?.typeName === "shape" ? input.replaceShapeId : null,
    bounds: { x: position.x, y: position.y, width, height },
  }
}

export async function insertPlaceholder(input: InsertPlaceholderInput) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const bootstrap = await ensureCanvasBootstrap(context, input.sessionId)
  const state =
    ((await context.storage.readPanelState(
      bootstrap.session.id,
      bootstrap.panel.id
    )) as Record<string, any> | null) ?? emptyCanvasSnapshot()
  const store =
    state.store && typeof state.store === "object" ? state.store : {}
  const pageId = state.currentPageId || findFirstPageId(store) || "page:main"
  if (!store[pageId]) {
    store[pageId] = { id: pageId, typeName: "page", name: "Page 1", index: 1 }
  }

  const width = input.displayWidth ?? DEFAULT_PLACEHOLDER_SIZE
  const height = input.displayHeight ?? DEFAULT_PLACEHOLDER_SIZE
  const anchor = input.anchorShapeId ? store[input.anchorShapeId] : null
  const anchorBounds = anchor?.typeName === "shape" ? shapeBounds(anchor) : null
  const position = findCanvasPlacementPosition({
    anchorBounds,
    height,
    preferredPosition: { x: 160, y: 160 },
    store,
    width,
  })
  const shapeId = createId("shape")

  store[shapeId] = {
    id: shapeId,
    typeName: "shape",
    type: "placeholder",
    parentId: anchor?.parentId || pageId,
    index: nextShapeIndex(store, pageId),
    props: {
      cornerRadius: 0,
      height,
      text: input.text ?? "正在生成图片",
      width,
      x: position.x,
      y: position.y,
    },
    meta: {
      openpanelsGenerationPlaceholder: true,
      createdAt: new Date().toISOString(),
    },
  }
  state.store = store
  state.currentPageId = pageId
  state.selectedShapeIds = [shapeId]
  await context.storage.writePanelState(
    bootstrap.session.id,
    bootstrap.panel.id,
    state
  )
  await writeActiveSession(context, bootstrap.session.id)
  return {
    sessionId: bootstrap.session.id,
    panelId: bootstrap.panel.id,
    shapeId,
    bounds: { x: position.x, y: position.y, width, height },
  }
}

export function emptyCanvasSnapshot(): Record<string, any> {
  return {
    schema: {
      schemaVersion: 1,
      recordVersions: { page: 1, shape: 1, asset: 1 },
    },
    camera: { x: 0, y: 0, zoom: 1 },
    currentPageId: "page:main",
    openedGroupId: null,
    selectedShapeIds: [],
    store: {
      "page:main": {
        id: "page:main",
        typeName: "page",
        name: "Page 1",
        index: 1,
      },
    },
  }
}

export function normalizeSerializableSnapshot(value: unknown): unknown {
  if (
    value &&
    typeof value === "object" &&
    "selectedShapeIds" in value &&
    !Array.isArray((value as { selectedShapeIds?: unknown }).selectedShapeIds)
  ) {
    return {
      ...(value as Record<string, unknown>),
      selectedShapeIds:
        (value as { selectedShapeIds?: unknown }).selectedShapeIds instanceof
        Set
          ? [...(value as { selectedShapeIds: Set<string> }).selectedShapeIds]
          : [],
    }
  }
  return value
}

export function dataUrlToBuffer(dataUrl: string): {
  buffer: Buffer
  mimeType: string
} {
  const match = dataUrl.match(/^data:([^;,]+)?(;base64)?,(.*)$/)
  if (!match) throw new Error("Expected a data URL")
  const mimeType = match[1] || "application/octet-stream"
  const isBase64 = Boolean(match[2])
  const data = match[3] || ""
  return {
    mimeType,
    buffer: isBase64
      ? Buffer.from(data, "base64")
      : Buffer.from(decodeURIComponent(data), "utf8"),
  }
}

export function mimeTypeForFile(fileName: string): string {
  switch (extname(fileName).toLowerCase()) {
    case ".aac":
      return "audio/aac"
    case ".avi":
      return "video/x-msvideo"
    case ".csv":
      return "text/csv"
    case ".doc":
      return "application/msword"
    case ".docx":
      return "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
    case ".gif":
      return "image/gif"
    case ".htm":
    case ".html":
      return "text/html"
    case ".jpg":
    case ".jpeg":
      return "image/jpeg"
    case ".json":
      return "application/json"
    case ".m4a":
      return "audio/mp4"
    case ".md":
    case ".markdown":
      return "text/markdown"
    case ".mov":
      return "video/quicktime"
    case ".mp3":
      return "audio/mpeg"
    case ".mp4":
      return "video/mp4"
    case ".pdf":
      return "application/pdf"
    case ".png":
      return "image/png"
    case ".svg":
      return "image/svg+xml"
    case ".txt":
      return "text/plain"
    case ".wav":
      return "audio/wav"
    case ".webp":
      return "image/webp"
    case ".xls":
      return "application/vnd.ms-excel"
    case ".xlsx":
      return "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    case ".zip":
      return "application/zip"
    default:
      return "application/octet-stream"
  }
}

async function ensureProjectForSession(
  context: OpenPanelsLocalContext,
  session: OpenPanelsSession,
  options: {
    requestedPanelId?: string | null
    requestedPanelKind?: OpenPanelsPanelKind | null
  } = {}
): Promise<ProjectBootstrap> {
  let currentSession = session
  for (const kind of DEFAULT_PANEL_KINDS) {
    const ensured = await ensurePanelForSession(context, currentSession, kind)
    currentSession = ensured.session
  }

  const panels = await readPanelSnapshots(context, currentSession)
  const active = await readActivePanel(context)
  const preferredKind =
    options.requestedPanelKind ??
    (active?.sessionId === currentSession.id ? active.kind : null) ??
    active?.kind ??
    DEFAULT_ACTIVE_PANEL_KIND
  const panelSnapshot =
    (options.requestedPanelId
      ? panels.find(({ panel }) => panel.id === options.requestedPanelId)
      : null) ??
    (preferredKind
      ? panels.find(({ panel }) => panel.kind === preferredKind)
      : null) ??
    panels.find(({ panel }) => panel.kind === DEFAULT_ACTIVE_PANEL_KIND) ??
    panels[0]

  if (!panelSnapshot) {
    throw new Error(`OpenPanels project has no panels: ${currentSession.id}`)
  }

  return {
    activePanelId: panelSnapshot.panel.id,
    activePanelKind: panelSnapshot.panel.kind,
    contextDir: context.paths.contextDir,
    contextId: context.paths.contextId,
    contextIdSource: context.paths.contextIdSource,
    session: currentSession,
    panel: panelSnapshot.panel,
    panels,
    sessions: await context.runtime.listSessions(),
    state: panelSnapshot.state,
    storageDir: context.paths.storageDir,
    panelDir: panelDir(context, currentSession.id, panelSnapshot.panel.id),
  }
}

async function ensurePanelForSession(
  context: OpenPanelsLocalContext,
  session: OpenPanelsSession,
  kind: OpenPanelsPanelKind
): Promise<{ panel: OpenPanelsPanel; session: OpenPanelsSession }> {
  for (const panelId of session.panelIds) {
    const candidate = await context.runtime.getPanel(session.id, panelId)
    if (candidate?.kind === kind) {
      return { panel: candidate, session }
    }
  }

  const panel = await context.runtime.openPanel({
    sessionId: session.id,
    kind,
    title: initialPanelTitle(kind),
    initialState: initialPanelState(kind),
  })
  return {
    panel,
    session: (await context.runtime.getSession(session.id)) ?? session,
  }
}

async function readPanelSnapshots(
  context: OpenPanelsLocalContext,
  session: OpenPanelsSession
): Promise<ProjectPanelSnapshot[]> {
  const snapshots: ProjectPanelSnapshot[] = []
  for (const panelId of session.panelIds) {
    const panel = await context.runtime.getPanel(session.id, panelId)
    if (!panel) continue
    const rawState = await context.runtime.readPanelState(session.id, panel.id)
    snapshots.push({
      panel,
      state: normalizePanelState(panel.kind, rawState),
    })
  }
  return snapshots.sort((left, right) => {
    const leftIndex = DEFAULT_PANEL_KINDS.indexOf(left.panel.kind)
    const rightIndex = DEFAULT_PANEL_KINDS.indexOf(right.panel.kind)
    if (leftIndex === -1 && rightIndex === -1) {
      return left.panel.createdAt.localeCompare(right.panel.createdAt)
    }
    if (leftIndex === -1) return 1
    if (rightIndex === -1) return -1
    return leftIndex - rightIndex
  })
}

function normalizePanelState(
  kind: OpenPanelsPanelKind,
  state: unknown
): unknown {
  if (kind === "canvas") {
    return normalizeSerializableSnapshot(state ?? emptyCanvasSnapshot())
  }
  if (kind === "wiki") {
    return normalizeWikiState(state)
  }
  return state ?? {}
}

function initialPanelState(kind: OpenPanelsPanelKind): unknown {
  if (kind === "canvas") return emptyCanvasSnapshot()
  if (kind === "wiki") return emptyWikiState()
  return {}
}

function initialPanelTitle(kind: OpenPanelsPanelKind): string {
  if (kind === "wiki") return "文档库"
  if (kind === "canvas") return "Design canvas"
  switch (kind) {
    case "image":
      return "Images"
    case "diff":
      return "Diff"
    case "preview":
      return "Preview"
    case "files":
      return "Files"
    default:
      return kind
  }
}

function emptyWikiState() {
  const now = new Date().toISOString()
  const ruleSet: WikiRuleSet = {
    id: DEFAULT_WIKI_RULE_SET_ID,
    title: "Default LLM Wiki",
    description: "Default agent-friendly structured wiki rules.",
    builtIn: true,
    version: 1,
    rulesRef: wikiRef("rules", "default", "rules.md"),
    createdAt: now,
    updatedAt: now,
  }
  const wikiSpace: WikiSpace = {
    id: DEFAULT_WIKI_SPACE_ID,
    title: "Default Wiki",
    ruleSetId: ruleSet.id,
    ruleSetVersion: ruleSet.version,
    rootRef: wikiRef("wikis", DEFAULT_WIKI_SPACE_ID),
    pageIndex: [],
    createdAt: now,
    updatedAt: now,
  }
  return {
    schemaVersion: 2,
    rawDocuments: [],
    ruleSets: [ruleSet],
    wikiSpaces: [wikiSpace],
    activeRawDocumentId: null,
    activeWikiSpaceId: wikiSpace.id,
    activeWikiPagePath: "index.md",
    agentProcesses: [],
    tasks: [],
    wikiLanguage: null,
  } satisfies WikiStateV2
}

async function ensureWikiState(
  context: OpenPanelsLocalContext,
  session: OpenPanelsSession,
  panel: OpenPanelsPanel
): Promise<WikiStateV2> {
  const rawState = await context.runtime.readPanelState(session.id, panel.id)
  const state = normalizeWikiState(rawState)
  await ensureDefaultWikiFiles(context, session.id, panel.id, state)
  await saveWikiState(context, session.id, panel.id, state)
  return state
}

function normalizeWikiState(state: unknown): WikiStateV2 {
  if (isWikiStateV2(state)) {
    return withWikiDefaults(state)
  }
  return emptyWikiState()
}

function withWikiDefaults(state: WikiStateV2): WikiStateV2 {
  const defaults = emptyWikiState()
  const ruleSets = state.ruleSets.length ? state.ruleSets : defaults.ruleSets
  const wikiSpaces = state.wikiSpaces.length
    ? state.wikiSpaces
    : defaults.wikiSpaces
  return {
    ...state,
    ruleSets,
    wikiSpaces,
    activeWikiSpaceId:
      state.activeWikiSpaceId ?? wikiSpaces[0]?.id ?? DEFAULT_WIKI_SPACE_ID,
    activeWikiPagePath: state.activeWikiPagePath ?? "index.md",
    agentProcesses: state.agentProcesses ?? [],
    tasks: state.tasks ?? [],
    wikiLanguage: normalizeWikiLanguage(state.wikiLanguage),
  }
}

function isWikiStateV2(state: unknown): state is WikiStateV2 {
  return (
    typeof state === "object" &&
    state !== null &&
    (state as { schemaVersion?: unknown }).schemaVersion === 2 &&
    Array.isArray((state as { rawDocuments?: unknown }).rawDocuments) &&
    Array.isArray((state as { ruleSets?: unknown }).ruleSets) &&
    Array.isArray((state as { wikiSpaces?: unknown }).wikiSpaces) &&
    Array.isArray((state as { tasks?: unknown }).tasks)
  )
}

async function saveWikiState(
  context: OpenPanelsLocalContext,
  sessionId: string,
  panelId: string,
  state: WikiStateV2
) {
  await context.runtime.savePanelState(sessionId, panelId, state)
}

async function ensureDefaultWikiFiles(
  context: OpenPanelsLocalContext,
  sessionId: string,
  panelId: string,
  state: WikiStateV2
) {
  const defaultRulePath = wikiPanelPath(
    context,
    sessionId,
    panelId,
    "rules",
    "default",
    "rules.md"
  )
  await writeFileIfMissing(defaultRulePath, defaultRulesMarkdown())
  for (const wikiSpace of state.wikiSpaces) {
    const pagesDir = wikiPanelPath(
      context,
      sessionId,
      panelId,
      "wikis",
      wikiSpace.id,
      "pages"
    )
    await mkdir(pagesDir, { recursive: true })
    await writeFileIfMissing(
      join(pagesDir, "index.md"),
      `---\ntitle: "Index"\ntype: "overview"\nsummary: "Structured wiki index."\ntags: []\nsourceDocumentIds: []\nupdatedAt: "${new Date().toISOString()}"\n---\n\n# Index\n\nNo pages yet.\n`
    )
    await writeFileIfMissing(
      join(pagesDir, "log.md"),
      `---\ntitle: "Log"\ntype: "log"\nsummary: "Agent and user wiki changes."\ntags: []\nsourceDocumentIds: []\nupdatedAt: "${new Date().toISOString()}"\n---\n\n# Log\n`
    )
  }
}

async function writeFileIfMissing(filePath: string, content: string) {
  try {
    await stat(filePath)
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error
    await mkdir(dirname(filePath), { recursive: true })
    await writeFile(filePath, content, "utf8")
  }
}

function defaultRulesMarkdown() {
  return `# Default LLM Wiki Rules

- Keep \`index.md\` as the primary agent-readable map.
- Keep \`log.md\` as an append-only change log.
- Create source, topic, entity, category, and contradiction pages when useful.
- Preserve source document references in page frontmatter.
- Prefer concise summaries and relative Markdown links.
`
}

function normalizeWikiLanguage(language: unknown): WikiLanguage | null {
  return language === "en" || language === "zh-CN" ? language : null
}

function wikiLanguageLabel(language: WikiLanguage | null | undefined) {
  switch (language) {
    case "zh-CN":
      return "Simplified Chinese (zh-CN)"
    case "en":
      return "English"
    default:
      return "the wiki panel language selected by the user"
  }
}

function createIngestionState(task: WikiTask, markdownVersion: number) {
  return {
    status: "queued" as const,
    taskId: task.id,
    markdownVersion,
    error: null,
    updatedAt: task.updatedAt,
  }
}

function resolveWikiSpace(
  state: WikiStateV2,
  wikiSpaceId?: string | null
): WikiSpace {
  const requested = wikiSpaceId ?? state.activeWikiSpaceId
  const wikiSpace =
    state.wikiSpaces.find((space) => space.id === requested) ??
    state.wikiSpaces[0]
  if (!wikiSpace) throw new Error("Wiki space not found")
  return wikiSpace
}

function findWikiDocument(
  state: WikiStateV2,
  documentId: string
): WikiRawDocument {
  const document = state.rawDocuments.find((item) => item.id === documentId)
  if (!document) throw new Error(`Wiki raw document not found: ${documentId}`)
  return document
}

async function resolveWikiRawDocumentOriginal(input: {
  contextId?: string
  documentId: string
  projectDir?: string
  storageDir?: string
}) {
  const context = createOpenPanelsLocalContext(input.projectDir, input)
  const { session, panel, state } = await getWikiBootstrap(input)
  const document = findWikiDocument(state, input.documentId)
  const filePath = wikiPanelPath(
    context,
    session.id,
    panel.id,
    ...document.originalRef.split("/")
  )
  assertInside(panelDir(context, session.id, panel.id), filePath)
  const stats = await stat(filePath)
  if (!stats.isFile()) {
    throw new Error(`Wiki raw document original is not a file: ${document.id}`)
  }
  return { document, filePath, stats }
}

function findWikiTask(state: WikiStateV2, taskId: string): WikiTask {
  const task = state.tasks.find((item) => item.id === taskId)
  if (!task) throw new Error(`Wiki task not found: ${taskId}`)
  return task
}

function createWikiTask(
  state: WikiStateV2,
  input: {
    documentId: string | null
    markdownVersion: number | null
    targetId: string
    type: WikiTaskType
    wikiSpaceId?: string | null
  }
): WikiTask {
  const now = new Date().toISOString()
  const wikiSpace = resolveWikiSpace(state, input.wikiSpaceId)
  const task: WikiTask = {
    id: createId("task"),
    type: input.type,
    status: "queued",
    targetId: input.targetId,
    documentId: input.documentId,
    wikiSpaceId: wikiSpace.id,
    ruleSetId: wikiSpace.ruleSetId,
    ruleSetVersion: wikiSpace.ruleSetVersion,
    markdownVersion: input.markdownVersion,
    claimedByProcessId: null,
    error: null,
    result: null,
    createdAt: now,
    updatedAt: now,
  }
  state.tasks.unshift(task)
  return task
}

function createWikiRebuildIndexTask(
  state: WikiStateV2,
  wikiSpaceId?: string | null
) {
  const wikiSpace = resolveWikiSpace(state, wikiSpaceId)
  return createWikiTask(state, {
    documentId: null,
    markdownVersion: null,
    targetId: "index.md",
    type: "rebuild_wiki_index",
    wikiSpaceId: wikiSpace.id,
  })
}

async function saveWikiTask(
  context: OpenPanelsLocalContext,
  sessionId: string,
  panelId: string,
  task: WikiTask
) {
  await writeJson(
    wikiPanelPath(
      context,
      sessionId,
      panelId,
      "tasks",
      `${safePart(task.id)}.json`
    ),
    task
  )
}

async function saveProcess(
  context: OpenPanelsLocalContext,
  sessionId: string,
  panelId: string,
  processContext: AgentProcessContext
) {
  await writeJson(
    wikiPanelPath(
      context,
      sessionId,
      panelId,
      "processes",
      `${safePart(processContext.id)}.json`
    ),
    processContext
  )
}

interface WakeQueuedWikiTasksOptions {
  allowLocalWorkerFromAgentWorker?: boolean
  taskIds?: string[]
}

async function wakeQueuedWikiTasks(
  context: OpenPanelsLocalContext,
  session: OpenPanelsSession,
  panel: OpenPanelsPanel,
  state: WikiStateV2,
  options: WakeQueuedWikiTasksOptions = {}
) {
  const targets = await listAgentTargets(context)
  const taskIdSet = options.taskIds ? new Set(options.taskIds) : null
  const queuedTasks = state.tasks.filter(
    (task) => task.status === "queued" && (!taskIdSet || taskIdSet.has(task.id))
  )
  if (queuedTasks.length === 0) return
  const wakeupsDir = join(context.paths.contextDir, "wakeups")
  const runsDir = join(context.paths.contextDir, "agent-runs")
  await mkdir(wakeupsDir, { recursive: true })
  for (const task of queuedTasks) {
    const wakeupPath = join(wakeupsDir, `${safePart(task.id)}.json`)
    const runPath = join(runsDir, `${safePart(task.id)}.json`)
    const alreadyWoken = await pathExists(wakeupPath)
    const localRunExists = await pathExists(runPath)
    const message = {
      projectDir: context.paths.projectDir,
      storageDir: context.paths.storageDir,
      contextId: context.paths.contextId,
      sessionId: session.id,
      wikiPanelId: panel.id,
      taskId: task.id,
      taskType: task.type,
      targetId: task.targetId,
      documentId: task.documentId,
      wikiSpaceId: task.wikiSpaceId,
      wikiLanguage: state.wikiLanguage,
      wikiLanguageLabel: wikiLanguageLabel(state.wikiLanguage),
      createdAt: new Date().toISOString(),
      originalFilePath: originalFilePathForTask(
        context,
        session,
        panel,
        state,
        task
      ),
    }
    await writeJson(wakeupPath, message)
    let sentToTarget = false
    for (const target of targets) {
      if (target.wakeUrl) {
        try {
          await fetch(target.wakeUrl, {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ ...message, target }),
          })
          sentToTarget = true
        } catch (_error) {
          // The durable queued task remains the source of truth.
        }
      }
    }
    if (!(sentToTarget || (alreadyWoken && localRunExists))) {
      await wakeLocalAgentWorker(context, message, options)
    }
  }
}

interface LocalAgentWorker {
  args: string[]
  cwd?: string
  executable: string
  host: string
  stdin?: string | null
}

async function wakeLocalAgentWorker(
  context: OpenPanelsLocalContext,
  message: Record<string, unknown>,
  options: WakeQueuedWikiTasksOptions = {}
) {
  const worker = resolveLocalAgentWorker(context, message, options)
  if (!worker) return
  const runsDir = join(context.paths.contextDir, "agent-runs")
  await mkdir(runsDir, { recursive: true })
  const taskId = String(message.taskId ?? "unknown")
  const runPath = join(runsDir, `${safePart(taskId)}.json`)
  try {
    await writeFile(
      runPath,
      `${JSON.stringify(
        {
          host: worker.host,
          status: "spawning",
          message,
          createdAt: new Date().toISOString(),
        },
        null,
        2
      )}\n`,
      { flag: "wx" }
    )
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "EEXIST") return
    throw error
  }

  try {
    const logPath = join(runsDir, `${safePart(taskId)}.log`)
    const logFd = openSync(logPath, "a")
    const child = spawn(worker.executable, worker.args, {
      cwd: worker.cwd,
      detached: true,
      env: {
        ...process.env,
        OPENPANELS_AGENT_WORKER: "1",
        OPENPANELS_PROJECT_DIR: context.paths.projectDir,
        OPENPANELS_STORAGE_DIR: context.paths.storageDir,
      },
      stdio: ["pipe", logFd, logFd],
    })
    child.stdin?.end(worker.stdin ?? "")
    let logClosed = false
    const closeLog = () => {
      if (logClosed) return
      logClosed = true
      closeSync(logFd)
    }
    child.on("error", (error) => {
      closeLog()
      writeFile(
        runPath,
        `${JSON.stringify(
          {
            host: worker.host,
            status: "spawn_failed",
            logPath,
            message,
            error: error instanceof Error ? error.message : String(error),
            updatedAt: new Date().toISOString(),
          },
          null,
          2
        )}\n`
      ).catch(() => undefined)
    })
    child.on("exit", (code, signal) => {
      closeLog()
      writeFile(
        runPath,
        `${JSON.stringify(
          {
            host: worker.host,
            status: code === 0 ? "finished" : "exited",
            code,
            signal,
            logPath,
            message,
            updatedAt: new Date().toISOString(),
          },
          null,
          2
        )}\n`
      ).catch(() => undefined)
    })
    child.unref()
  } catch (error) {
    await writeFile(
      runPath,
      `${JSON.stringify(
        {
          host: worker.host,
          status: "spawn_failed",
          message,
          error: error instanceof Error ? error.message : String(error),
          updatedAt: new Date().toISOString(),
        },
        null,
        2
      )}\n`
    )
  }
}

function resolveLocalAgentWorker(
  context: OpenPanelsLocalContext,
  message: Record<string, unknown>,
  options: WakeQueuedWikiTasksOptions = {}
): LocalAgentWorker | null {
  if (!shouldWakeLocalAgentWorker(options)) return null
  const requestedHost = process.env.OPENPANELS_LOCAL_AGENT_HOST
  const codex = findExecutable("codex", process.env.OPENPANELS_CODEX_EXECUTABLE)
  const hermes = findExecutable(
    "hermes",
    process.env.OPENPANELS_HERMES_EXECUTABLE
  )
  const hostPreference =
    requestedHost ??
    (hasCodexAgentEnvironment()
      ? "codex"
      : hasHermesAgentEnvironment()
        ? "hermes"
        : null)

  if (hostPreference === "hermes" && hermes) {
    return hermesWorker(context, message, hermes)
  }
  if (hostPreference === "codex" && codex) {
    return codexWorker(context, message, codex)
  }
  if (codex) return codexWorker(context, message, codex)
  if (hermes) return hermesWorker(context, message, hermes)
  return null
}

function codexWorker(
  context: OpenPanelsLocalContext,
  message: Record<string, unknown>,
  executable: string
): LocalAgentWorker {
  const args = [
    "exec",
    "--cd",
    context.paths.projectDir,
    "--add-dir",
    context.paths.storageDir,
    "--dangerously-bypass-approvals-and-sandbox",
    "-",
  ]
  const originalFilePath =
    typeof message.originalFilePath === "string"
      ? message.originalFilePath
      : null
  if (originalFilePath && isImageFile(originalFilePath)) {
    args.splice(1, 0, "--image", originalFilePath)
  }
  return {
    args,
    executable,
    host: "codex-cli",
    stdin: localAgentWorkerPrompt(context, message, "codex-cli"),
  }
}

function hermesWorker(
  context: OpenPanelsLocalContext,
  message: Record<string, unknown>,
  executable: string
): LocalAgentWorker {
  return {
    args: [
      "--yolo",
      "--accept-hooks",
      "--pass-session-id",
      "--oneshot",
      localAgentWorkerPrompt(context, message, "hermes"),
    ],
    cwd: context.paths.projectDir,
    executable,
    host: "hermes",
    stdin: null,
  }
}

function shouldWakeLocalAgentWorker(options: WakeQueuedWikiTasksOptions = {}) {
  if (process.env.OPENPANELS_DISABLE_LOCAL_AGENT === "1") return false
  if (process.env.VITEST || process.env.NODE_ENV === "test") return false
  if (
    process.env.OPENPANELS_AGENT_WORKER === "1" &&
    !options.allowLocalWorkerFromAgentWorker
  ) {
    return false
  }
  return Boolean(
    process.env.OPENPANELS_ENABLE_LOCAL_AGENT === "1" ||
      hasCodexAgentEnvironment() ||
      hasHermesAgentEnvironment()
  )
}

function hasCodexAgentEnvironment() {
  return Boolean(
    process.env.CODEX_THREAD_ID ||
      process.env.CODEX_SHELL ||
      process.env.CODEX_INTERNAL_ORIGINATOR_OVERRIDE
  )
}

function hasHermesAgentEnvironment() {
  return Boolean(
    process.env.HERMES_THREAD_ID ||
      process.env.HERMES_CONVERSATION_ID ||
      process.env.HERMES_SESSION_ID ||
      process.env.HERMES_PROFILE ||
      process.env.HERMES_HOME
  )
}

function findExecutable(name: string, override?: string) {
  if (override && fileLooksAvailableSync(override)) return override
  for (const directory of (process.env.PATH ?? "").split(sep)) {
    const candidate = join(directory, name)
    if (fileLooksAvailableSync(candidate)) return candidate
  }
  const homeCandidate = join(homedir(), ".local", "bin", name)
  if (fileLooksAvailableSync(homeCandidate)) return homeCandidate
  return null
}

function localAgentThreadId(agentHost: string) {
  if (agentHost === "hermes") {
    return (
      process.env.HERMES_THREAD_ID ??
      process.env.HERMES_CONVERSATION_ID ??
      process.env.HERMES_SESSION_ID ??
      "hermes-oneshot"
    )
  }
  return process.env.CODEX_THREAD_ID ?? "codex-exec"
}

function localAgentWorkerPrompt(
  context: OpenPanelsLocalContext,
  message: Record<string, unknown>,
  agentHost: string
) {
  const cli = preferredLocalCliCommand(context)
  const taskType = String(message.taskType ?? "")
  const documentId = message.documentId ? String(message.documentId) : ""
  const originalPath = message.originalFilePath
    ? String(message.originalFilePath)
    : ""
  const wikiSpaceId = String(message.wikiSpaceId ?? DEFAULT_WIKI_SPACE_ID)
  const wikiLanguage = String(
    message.wikiLanguageLabel ??
      wikiLanguageLabel(normalizeWikiLanguage(message.wikiLanguage))
  )
  const taskId = String(message.taskId ?? "")
  const commonFlags = `--project ${shellQuote(context.paths.projectDir)} --storage-dir ${shellQuote(
    context.paths.storageDir
  )} --context-id ${shellQuote(context.paths.contextId)} --format json`

  return `You are a MyOpenPanels wiki agent worker. Process exactly one queued wiki task, then stop.

Do not modify application source code. Only read project files as needed and write MyOpenPanels wiki data through the CLI/API.

Task:
${JSON.stringify(message, null, 2)}

Use this CLI command prefix:
${cli} ${commonFlags}

Required workflow:
1. Claim the task:
   ${cli} ${commonFlags} wiki tasks claim --task-id ${shellQuote(taskId)} --agent-host ${shellQuote(agentHost)} --thread-id ${shellQuote(localAgentThreadId(agentHost))}
2. Process according to taskType.
   - Use ${wikiLanguage} for any newly generated structured wiki pages, index entries, summaries, and log text. Do not rewrite already-generated wiki content solely to translate it.
3. On success, call:
   ${cli} ${commonFlags} wiki tasks complete --task-id ${shellQuote(taskId)}
4. If the task cannot be completed reliably, call:
   ${cli} ${commonFlags} wiki tasks fail --task-id ${shellQuote(taskId)} --message <short reason>

For taskType=${taskType}:
- convert_document_to_markdown:
  - Source document id: ${documentId}
  - Original file path: ${originalPath || "(read it from raw document metadata)"}
  - Convert the original file into clean Markdown. Preserve titles, headings, lists, tables, quoted text, and useful image/file placeholders.
  - Write the Markdown to a temporary .md file, then run:
    ${cli} ${commonFlags} wiki markdown write --document-id ${shellQuote(documentId)} --file <temporary-md-file> --task-id ${shellQuote(taskId)}
  - Complete the conversion task. Completing it will enqueue the follow-up structured wiki ingest task automatically.
- ingest_markdown_into_wiki:
  - Read source Markdown:
    ${cli} ${commonFlags} wiki markdown read --document-id ${shellQuote(documentId)}
  - Update the target wiki space ${wikiSpaceId}: create/update a source page under sources/, update relevant topic/category pages when useful, update index.md, and append log.md.
  - Use page writes with --task-id ${shellQuote(taskId)} so your own wiki edits do not enqueue redundant rebuild tasks:
    ${cli} ${commonFlags} wiki pages write --wiki-space-id ${shellQuote(wikiSpaceId)} --path <page-path> --file <md-file> --task-id ${shellQuote(taskId)}
  - Complete the task.
- rebuild_wiki_index:
  - Read current pages for wiki space ${wikiSpaceId}, rebuild index.md and append log.md so deleted/edited sources are reflected.
  - Use wiki pages write with --task-id ${shellQuote(taskId)}.
  - Complete the task.

Keep the final response brief.`
}

function preferredLocalCliCommand(context: OpenPanelsLocalContext) {
  if (process.env.OPENPANELS_LOCAL_CLI) {
    return shellQuote(process.env.OPENPANELS_LOCAL_CLI)
  }
  const repoCli = join(
    context.paths.projectDir,
    "packages",
    "local-cli",
    "dist",
    "openpanels-local.mjs"
  )
  if (fileLooksAvailableSync(repoCli)) {
    return `node ${shellQuote(repoCli)}`
  }
  const argvEntry = process.argv[1]
  if (argvEntry && basename(argvEntry).includes("openpanels-local")) {
    return `node ${shellQuote(argvEntry)}`
  }
  return "openpanels-local"
}

function originalFilePathForTask(
  context: OpenPanelsLocalContext,
  session: OpenPanelsSession,
  panel: OpenPanelsPanel,
  state: WikiStateV2,
  task: WikiTask
) {
  if (!task.documentId) return null
  const document = state.rawDocuments.find(
    (item) => item.id === task.documentId
  )
  if (!document) return null
  return wikiPanelPath(
    context,
    session.id,
    panel.id,
    ...document.originalRef.split("/")
  )
}

async function pathExists(path: string) {
  try {
    await access(path)
    return true
  } catch {
    return false
  }
}

function fileLooksAvailableSync(path: string) {
  try {
    return Boolean(realpathSync(path))
  } catch {
    return false
  }
}

function isImageFile(path: string) {
  const extension = extname(path).toLowerCase()
  return [".png", ".jpg", ".jpeg", ".gif", ".webp"].includes(extension)
}

function shellQuote(value: string) {
  return `'${value.replaceAll("'", "'\\''")}'`
}

async function listAgentTargets(
  context: OpenPanelsLocalContext
): Promise<AgentThreadTarget[]> {
  try {
    const parsed = JSON.parse(await readFile(agentTargetsPath(context), "utf8"))
    return Array.isArray(parsed) ? (parsed as AgentThreadTarget[]) : []
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return []
    throw error
  }
}

function agentTargetsPath(context: OpenPanelsLocalContext) {
  return join(context.paths.contextDir, "agent-targets.json")
}

function wikiRef(...parts: string[]) {
  return parts.map(safePart).join("/")
}

function wikiPanelPath(
  context: OpenPanelsLocalContext,
  sessionId: string,
  panelId: string,
  ...parts: string[]
) {
  const target = join(
    panelDir(context, sessionId, panelId),
    ...parts.map(safePart)
  )
  assertInside(panelDir(context, sessionId, panelId), target)
  return target
}

function wikiPagePath(
  context: OpenPanelsLocalContext,
  sessionId: string,
  panelId: string,
  wikiSpaceId: string,
  pagePath: string
) {
  const pagesDir = wikiPanelPath(
    context,
    sessionId,
    panelId,
    "wikis",
    wikiSpaceId,
    "pages"
  )
  const target = join(pagesDir, ...pagePath.split("/").map(safePart))
  assertInside(pagesDir, target)
  return target
}

function isPlainTextFile(fileName: string, mimeType?: string): boolean {
  const extension = extname(fileName).toLowerCase()
  return (
    extension === ".md" ||
    extension === ".markdown" ||
    extension === ".txt" ||
    extension === ".text" ||
    mimeType?.startsWith("text/") ||
    mimeType === "application/json"
  )
}

function titleFromFileName(fileName: string) {
  const extension = extname(fileName)
  return (
    (extension ? fileName.slice(0, -extension.length) : fileName) || fileName
  )
}

function upsertPageIndex(
  wikiSpace: WikiSpace,
  pagePath: string,
  markdown: string,
  updatedAt: string,
  title?: string
) {
  const frontmatterTitle = markdown.match(
    /^---[\s\S]*?\ntitle:\s*"?([^"\n]+)"?/m
  )
  const headingTitle = markdown.match(/^#\s+(.+)$/m)
  const nextItem: WikiPageIndexItem = {
    path: pagePath,
    title:
      title?.trim() ||
      frontmatterTitle?.[1]?.trim() ||
      headingTitle?.[1]?.trim() ||
      titleFromFileName(pagePath),
    type: pagePath === "index.md" ? "overview" : "page",
    summary: firstMarkdownParagraph(markdown),
    tags: [],
    sourceDocumentIds: [],
    updatedAt,
  }
  const existingIndex = wikiSpace.pageIndex.findIndex(
    (item) => item.path === pagePath
  )
  if (existingIndex === -1) {
    wikiSpace.pageIndex.push(nextItem)
  } else {
    wikiSpace.pageIndex[existingIndex] = nextItem
  }
}

function firstMarkdownParagraph(markdown: string) {
  return (
    markdown
      .split(/\n{2,}/)
      .map((part) => part.trim())
      .find((part) => part && !part.startsWith("---") && !part.startsWith("#"))
      ?.replace(/\s+/g, " ")
      .slice(0, 180) ?? ""
  )
}

function isPanelKind(value: unknown): value is OpenPanelsPanelKind {
  return (
    value === "wiki" ||
    value === "canvas" ||
    value === "image" ||
    value === "diff" ||
    value === "preview" ||
    value === "files"
  )
}

function activeSessionPath(context: OpenPanelsLocalContext): string {
  return join(context.paths.contextDir, "active-session.json")
}

function activePanelPath(context: OpenPanelsLocalContext): string {
  return join(context.paths.contextDir, "active-panel.json")
}

function defaultStorageDir(): string {
  const configured = process.env.OPENPANELS_STORAGE_DIR?.trim()
  if (configured) return resolve(configured)
  if (process.platform === "darwin") {
    return join(
      homedir(),
      "Library",
      "Application Support",
      "MyOpenPanels",
      ".myopenpanels"
    )
  }
  if (process.platform === "win32") {
    return join(
      process.env.APPDATA || join(homedir(), "AppData", "Roaming"),
      "MyOpenPanels",
      ".myopenpanels"
    )
  }
  return join(
    process.env.XDG_DATA_HOME || join(homedir(), ".local", "share"),
    "myopenpanels",
    ".myopenpanels"
  )
}

function resolveContextId(explicitContextId?: string): {
  contextId: string
  contextIdSource: string
} {
  if (explicitContextId?.trim()) {
    return {
      contextId: safeContextId(explicitContextId),
      contextIdSource: "explicit",
    }
  }
  for (const envName of CONTEXT_ENV_VARS) {
    const value = process.env[envName]?.trim()
    if (value) {
      return {
        contextId: safeContextId(value),
        contextIdSource: envName,
      }
    }
  }
  return { contextId: "default", contextIdSource: "default" }
}

function safeContextId(value: string): string {
  try {
    return sanitizePathPart(value)
  } catch (_error) {
    return "default"
  }
}

function nextProjectTitle(sessions: OpenPanelsSession[]): string {
  let maxProjectNumber = 0
  for (const session of sessions) {
    const match = session.title.match(/^Project (\d+)$/)
    if (match) {
      maxProjectNumber = Math.max(maxProjectNumber, Number(match[1]))
    }
  }
  return `Project ${maxProjectNumber + 1}`
}

function emptySelection(sessionId: string, panelId: string) {
  return {
    sessionId,
    panelId,
    selectedShapeIds: [],
    selectedShapes: [],
    assetRef: null,
    updatedAt: new Date().toISOString(),
  }
}

function withLastImageFallback(selection: unknown, state: unknown) {
  const current =
    selection && typeof selection === "object"
      ? (selection as Record<string, any>)
      : {}
  const selectedShapes = Array.isArray(current.selectedShapes)
    ? current.selectedShapes
    : []
  if (selectedShapes.length > 0) return current
  const fallback = findLastImageSelectionShape(state)
  if (!fallback) return current
  return {
    ...current,
    selectedShapeIds: [fallback.id],
    selectedShapes: [fallback],
    assetRef: fallback.asset?.assetRef ?? null,
    fallback: "last-image",
  }
}

function findLastImageSelectionShape(state: unknown) {
  const snapshot =
    state && typeof state === "object" ? (state as Record<string, any>) : {}
  const store =
    snapshot.store && typeof snapshot.store === "object" ? snapshot.store : {}
  const images = Object.values(store)
    .filter(
      (record: any) => record?.typeName === "shape" && record.type === "image"
    )
    .sort((a: any, b: any) => {
      const indexDiff = (Number(b.index) || 0) - (Number(a.index) || 0)
      if (indexDiff !== 0) return indexDiff
      return String(b.id).localeCompare(String(a.id))
    })
  const shape = images[0]
  if (!shape) return null
  return summarizeShapeForAgent(shape, store)
}

function summarizeShapeForAgent(shape: any, store: Record<string, any>) {
  const asset = shape?.props?.assetId ? store[shape.props.assetId] : null
  const assetRef = assetRefFromAsset(asset)
  return {
    id: shape.id,
    type: shape.type,
    parentId: shape.parentId,
    props: shape.props ?? {},
    bounds: shapeBounds(shape),
    asset: asset
      ? {
          id: asset.id,
          type: asset.type,
          name: asset.props?.name ?? null,
          src: asset.props?.src ?? null,
          w: asset.props?.w ?? null,
          h: asset.props?.h ?? null,
          mimeType: asset.props?.mimeType ?? null,
          assetRef,
        }
      : null,
  }
}

function assetRefFromAsset(asset: any): string | null {
  if (!asset) return null
  if (typeof asset.meta?.assetRef === "string") return asset.meta.assetRef
  const src = asset.props?.src
  if (typeof src !== "string") return null
  const match = src.match(/^\/api\/panels\/([^/]+)\/([^/]+)\/assets\/(.+)$/)
  if (!match) return null
  const sessionId = decodeURIComponent(match[1])
  const panelId = decodeURIComponent(match[2])
  const assetPath = match[3].split("/").map(decodeURIComponent).join("/")
  return ["sessions", sessionId, "panels", panelId, "assets", assetPath].join(
    "/"
  )
}

function findFirstPageId(store: Record<string, any>) {
  return (
    Object.values(store).find((record: any) => record?.typeName === "page")
      ?.id ?? null
  )
}

function nextShapeIndex(store: Record<string, any>, pageId: string) {
  let max = 0
  for (const record of Object.values(store)) {
    if (
      record?.typeName === "shape" &&
      record.parentId === pageId &&
      Number.isFinite(record.index)
    ) {
      max = Math.max(max, record.index)
    }
  }
  return max + 1
}

function shapeBounds(shape: any): Bounds {
  const props = shape.props || {}
  return {
    x: Number(props.x) || 0,
    y: Number(props.y) || 0,
    width: Number(props.width || props.w) || 160,
    height: Number(props.height || props.h) || 120,
  }
}

function toOccupiedBounds(bounds: Bounds): OccupiedBounds {
  return {
    maxX: bounds.x + bounds.width,
    maxY: bounds.y + bounds.height,
    minX: bounds.x,
    minY: bounds.y,
  }
}

function intersectsWithPadding(
  left: OccupiedBounds,
  right: OccupiedBounds,
  padding: number
) {
  return !(
    left.maxX <= right.minX - padding ||
    left.minX >= right.maxX + padding ||
    left.maxY <= right.minY - padding ||
    left.minY >= right.maxY + padding
  )
}

function hasOverlap(
  target: OccupiedBounds,
  occupiedBounds: OccupiedBounds[],
  padding: number
) {
  return occupiedBounds.some((bounds) =>
    intersectsWithPadding(target, bounds, padding)
  )
}

function canvasOccupiedBounds(store: Record<string, any>): OccupiedBounds[] {
  return Object.values(store)
    .filter(
      (record: any) =>
        record?.typeName === "shape" &&
        (record.type === "image" || record.type === "placeholder")
    )
    .map((record: any) => toOccupiedBounds(shapeBounds(record)))
}

function overallBounds(bounds: OccupiedBounds[]): OccupiedBounds | null {
  const first = bounds[0]
  if (!first) return null
  return bounds.reduce(
    (current, bound) => ({
      maxX: Math.max(current.maxX, bound.maxX),
      maxY: Math.max(current.maxY, bound.maxY),
      minX: Math.min(current.minX, bound.minX),
      minY: Math.min(current.minY, bound.minY),
    }),
    first
  )
}

function placementBelowExistingImages(
  occupiedBounds: OccupiedBounds[],
  padding: number
): { x: number; y: number } | null {
  const overall = overallBounds(occupiedBounds)
  if (!overall) return null
  const bottomMost = occupiedBounds.reduce((current, bounds) => {
    if (bounds.maxY > current.maxY) return bounds
    if (bounds.maxY === current.maxY && bounds.minX < current.minX) {
      return bounds
    }
    return current
  }, occupiedBounds[0])
  return { x: bottomMost.minX, y: overall.maxY + padding }
}

function scanForAvailablePosition(input: {
  basePosition: { x: number; y: number }
  height: number
  occupiedBounds: OccupiedBounds[]
  padding: number
  width: number
}) {
  const initialCandidate = toOccupiedBounds({
    x: input.basePosition.x,
    y: input.basePosition.y,
    width: input.width,
    height: input.height,
  })
  if (!hasOverlap(initialCandidate, input.occupiedBounds, input.padding)) {
    return input.basePosition
  }

  const stepX = Math.max(input.width + input.padding, input.padding)
  const stepY = Math.max(input.height + input.padding, input.padding)
  for (let row = 0; row < MAX_POSITION_SCAN; row += 1) {
    for (let col = 0; col < MAX_POSITION_SCAN; col += 1) {
      const x = input.basePosition.x + col * stepX
      const y = input.basePosition.y + row * stepY
      const candidate = toOccupiedBounds({
        x,
        y,
        width: input.width,
        height: input.height,
      })
      if (!hasOverlap(candidate, input.occupiedBounds, input.padding)) {
        return { x, y }
      }
    }
  }

  const overall = overallBounds(input.occupiedBounds)
  return overall
    ? { x: overall.minX, y: overall.maxY + input.padding }
    : input.basePosition
}

function findCanvasPlacementPosition(input: {
  anchorBounds: Bounds | null
  height: number
  preferredPosition: { x: number; y: number }
  store: Record<string, any>
  width: number
}) {
  const occupiedBounds = canvasOccupiedBounds(input.store)
  if (occupiedBounds.length === 0) return input.preferredPosition

  if (input.anchorBounds) {
    const anchorPosition = {
      x: input.anchorBounds.x + input.anchorBounds.width + DEFAULT_CANVAS_GAP,
      y: input.anchorBounds.y,
    }
    const anchorCandidate = toOccupiedBounds({
      ...anchorPosition,
      width: input.width,
      height: input.height,
    })
    if (!hasOverlap(anchorCandidate, occupiedBounds, DEFAULT_CANVAS_GAP)) {
      return anchorPosition
    }
  }

  const basePosition =
    placementBelowExistingImages(occupiedBounds, DEFAULT_CANVAS_GAP) ??
    input.preferredPosition

  return scanForAvailablePosition({
    basePosition,
    height: input.height,
    occupiedBounds,
    padding: DEFAULT_CANVAS_GAP,
    width: input.width,
  })
}

function placeImage(
  anchorBounds: { height: number; width: number; x: number; y: number } | null,
  width: number,
  _height: number,
  placement: "below" | "left" | "right" = "right"
) {
  if (!anchorBounds) return { x: 160, y: 160 }
  const margin = 40
  switch (placement) {
    case "left":
      return { x: anchorBounds.x - width - margin, y: anchorBounds.y }
    case "below":
      return {
        x: anchorBounds.x,
        y: anchorBounds.y + anchorBounds.height + margin,
      }
    default:
      return {
        x: anchorBounds.x + anchorBounds.width + margin,
        y: anchorBounds.y,
      }
  }
}

function readImageDimensions(buffer: Buffer): ImageDimensions | null {
  if (
    buffer.length >= 24 &&
    buffer[0] === 0x89 &&
    buffer.toString("ascii", 1, 4) === "PNG"
  ) {
    return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) }
  }
  if (buffer.length >= 10 && buffer.toString("ascii", 0, 3) === "GIF") {
    return { width: buffer.readUInt16LE(6), height: buffer.readUInt16LE(8) }
  }
  if (buffer.length >= 4 && buffer[0] === 0xff && buffer[1] === 0xd8) {
    let offset = 2
    while (offset < buffer.length) {
      if (buffer[offset] !== 0xff) break
      const marker = buffer[offset + 1]
      const length = buffer.readUInt16BE(offset + 2)
      if (marker >= 0xc0 && marker <= 0xc3) {
        return {
          height: buffer.readUInt16BE(offset + 5),
          width: buffer.readUInt16BE(offset + 7),
        }
      }
      offset += 2 + length
    }
  }
  return null
}

function panelDir(
  context: OpenPanelsLocalContext,
  sessionId: string,
  panelId: string
) {
  const dir = join(
    context.paths.storageDir,
    "sessions",
    safePart(sessionId),
    "panels",
    safePart(panelId)
  )
  assertInside(context.paths.storageDir, dir)
  return dir
}

function panelFile(
  context: OpenPanelsLocalContext,
  sessionId: string,
  panelId: string,
  name: string
) {
  return join(panelDir(context, sessionId, panelId), safePart(name))
}

async function readJsonObjectOrNull(
  filePath: string
): Promise<Record<string, unknown> | null> {
  try {
    const raw = await readFile(filePath, "utf8")
    if (!raw.trim()) return null
    const parsed = JSON.parse(raw)
    if (!(parsed && typeof parsed === "object") || Array.isArray(parsed)) {
      return null
    }
    return parsed as Record<string, unknown>
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return null
    if (error instanceof SyntaxError) return null
    throw error
  }
}

async function writeJson(filePath: string, value: unknown) {
  await mkdir(dirname(filePath), { recursive: true })
  const tempPath = `${filePath}.${process.pid}.${randomUUID()}.tmp`
  await writeFile(tempPath, `${JSON.stringify(value, null, 2)}\n`, "utf8")
  await rename(tempPath, filePath)
}

function createId(prefix: string) {
  return `${prefix}:${randomUUID()}`
}

function safePart(value: string) {
  const safe = basename(String(value))
    .replace(/[^a-zA-Z0-9._:-]+/g, "-")
    .replace(/^-+|-+$/g, "")
  if (!safe || safe === "." || safe === "..") {
    throw new Error(`Unsafe path part: ${value}`)
  }
  return safe
}

function assertInside(parent: string, child: string) {
  const rel = relative(parent, child)
  if (rel.startsWith("..") || rel.includes(`..${sep}`)) {
    throw new Error(`Path escapes OpenPanels storage: ${child}`)
  }
}
