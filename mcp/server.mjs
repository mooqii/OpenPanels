import { spawn } from "node:child_process"
import { readFileSync } from "node:fs"
import { copyFile, mkdir, readFile, stat, writeFile } from "node:fs/promises"
import { createServer } from "node:net"
import { basename, extname, join, relative, resolve, sep } from "node:path"
import { fileURLToPath } from "node:url"
import { registerAppTool } from "@modelcontextprotocol/ext-apps/server"
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js"
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js"
import { z } from "zod"
import {
  openPanelsWidgetHtml,
  registerWidgetResource,
} from "./lib/widget-resource.mjs"

const TOOL_RENDER_WIDGET = "render_myopenpanels_widget"
const TOOL_START_STUDIO = "start_myopenpanels_studio"
const TOOL_GET_SESSION = "get_myopenpanels_session"
const TOOL_OPEN_PANEL = "open_myopenpanels_panel"
const TOOL_INSERT_ARTIFACT = "insert_myopenpanels_artifact"
const TOOL_SAVE_PANEL_STATE = "save_myopenpanels_panel_state"
const TOOL_READ_ASSET = "read_myopenpanels_panel_asset"
const TOOL_WRITE_ASSET = "write_myopenpanels_panel_asset"
const TOOL_GET_CANVAS_STATE = "get_myopenpanels_canvas_state"
const TOOL_GET_SELECTION = "get_myopenpanels_selection"
const TOOL_READ_SELECTION_ASSET = "read_myopenpanels_selection_asset"
const TOOL_INSERT_IMAGE = "insert_myopenpanels_image"
const OPENPANELS_WIDGET_URI = "ui://widget/myopenpanels/index.html"
const OPENPANELS_CONNECT_DOMAINS = [
  "http://127.0.0.1:*",
  "http://localhost:*",
  "ws://127.0.0.1:*",
  "ws://localhost:*",
]
const OPENPANELS_RESOURCE_DOMAINS = [
  "http://127.0.0.1:*",
  "http://localhost:*",
  "data:",
  "blob:",
]
const localStudioServers = new Map()

const pluginManifest = JSON.parse(
  readFileSync(new URL("../.codex-plugin/plugin.json", import.meta.url), "utf8")
)

const server = new McpServer(
  {
    name: pluginManifest.name,
    version: pluginManifest.version,
  },
  {
    instructions:
      "MyOpenPanels renders a local agent design canvas. In Codex or other clients that support native app resources, use render_myopenpanels_widget. In generic MCP clients such as Claude Desktop or Hermes, use start_myopenpanels_studio and open the returned serverUrl in a browser. Use get_myopenpanels_selection to read the user's current canvas selection, read_myopenpanels_selection_asset for the exported PNG selection, and insert_myopenpanels_image to place generated images back onto the canvas.",
  }
)

const panelKinds = z.enum(["canvas"])
const projectArgs = {
  projectDir: z.string().trim().optional(),
}
const currentCanvasArgs = {
  ...projectArgs,
  sessionId: z.string().trim().optional(),
}

registerWidgetTool()
registerStateTools()
registerShutdownCleanup()

const transport = new StdioServerTransport()
await server.connect(transport)

function registerShutdownCleanup() {
  const cleanup = () => {
    for (const serverInfo of localStudioServers.values()) {
      stopLocalStudio(serverInfo)
    }
    localStudioServers.clear()
  }
  process.once("exit", cleanup)
  process.once("SIGINT", () => {
    cleanup()
    process.exit(130)
  })
  process.once("SIGTERM", () => {
    cleanup()
    process.exit(143)
  })
}

function registerWidgetTool() {
  registerWidgetResource(server, {
    name: "myopenpanels-widget",
    uri: OPENPANELS_WIDGET_URI,
    title: "MyOpenPanels",
    description:
      "A native widget that opens the project-backed MyOpenPanels local studio.",
    connectDomains: OPENPANELS_CONNECT_DOMAINS,
    resourceDomains: OPENPANELS_RESOURCE_DOMAINS,
    html: () => openPanelsWidgetHtml({ initialDisplayMode: "fullscreen" }),
  })

  registerAppTool(
    server,
    TOOL_RENDER_WIDGET,
    {
      title: "Open MyOpenPanels",
      description: "Open the MyOpenPanels local widget for the active project.",
      inputSchema: {
        ...projectArgs,
        displayMode: z.enum(["fullscreen", "inline"]).optional(),
      },
      _meta: {
        ui: {
          resourceUri: OPENPANELS_WIDGET_URI,
          visibility: ["model", "app"],
        },
        "ui/resourceUri": OPENPANELS_WIDGET_URI,
        "openai/outputTemplate": OPENPANELS_WIDGET_URI,
        "openai/widgetAccessible": true,
        "openai/toolInvocation/invoking": "Opening MyOpenPanels...",
        "openai/toolInvocation/invoked": "MyOpenPanels ready",
      },
    },
    async ({ projectDir, displayMode = "fullscreen" }) => {
      const paths = resolvePaths(projectDir)
      await ensureSession(paths, "Agent Session")
      const localStudio = await ensureLocalStudioServer(paths.projectDir)
      return {
        content: [
          {
            type: "text",
            text: `Opened MyOpenPanels for ${paths.projectDir}`,
          },
        ],
        structuredContent: {
          version: 1,
          widget: "myopenpanels-widget",
          rendering: "local-studio",
          projectDir: paths.projectDir,
          storageDir: paths.storageDir,
          serverUrl: localStudio.url,
          port: localStudio.port,
          displayMode,
        },
        _meta: {
          "openai/outputTemplate": OPENPANELS_WIDGET_URI,
          widgetData: {
            version: 1,
            widget: "myopenpanels-widget",
            rendering: "local-studio",
            projectDir: paths.projectDir,
            storageDir: paths.storageDir,
            serverUrl: localStudio.url,
            port: localStudio.port,
            displayMode,
          },
        },
      }
    }
  )

  server.registerTool(
    TOOL_START_STUDIO,
    {
      title: "Start MyOpenPanels Studio",
      description:
        "Start the browser-based MyOpenPanels local studio and return a localhost URL for generic MCP clients.",
      inputSchema: projectArgs,
    },
    async ({ projectDir }) => {
      const paths = resolvePaths(projectDir)
      await ensureSession(paths, "Agent Session")
      const localStudio = await ensureLocalStudioServer(paths.projectDir)
      return jsonText({
        version: 1,
        mode: "generic-mcp",
        projectDir: paths.projectDir,
        storageDir: paths.storageDir,
        serverUrl: localStudio.url,
        port: localStudio.port,
        instructions:
          "Open serverUrl in a browser. Keep this MCP session running while using the studio.",
      })
    }
  )
}

function registerStateTools() {
  server.registerTool(
    TOOL_GET_CANVAS_STATE,
    {
      title: "Get MyOpenPanels Canvas State",
      description:
        "Read the current project-backed MyOpenPanels canvas session, panel, state, and storage paths.",
      inputSchema: currentCanvasArgs,
    },
    async ({ projectDir, sessionId }) => {
      const paths = resolvePaths(projectDir)
      const target = await ensureCanvasPanel(paths, sessionId)
      const state =
        (await readJson(
          panelFile(paths, target.session.id, target.panel.id, "state.json")
        )) ?? emptyCanvasSnapshot()
      return jsonText({
        session: target.session,
        panel: target.panel,
        state,
        storageDir: paths.storageDir,
        panelDir: panelDir(paths, target.session.id, target.panel.id),
      })
    }
  )

  server.registerTool(
    TOOL_GET_SELECTION,
    {
      title: "Get MyOpenPanels Selection",
      description:
        "Return the currently selected MyOpenPanels canvas shapes and optional exported PNG selection data.",
      inputSchema: {
        ...currentCanvasArgs,
        includeImageBase64: z.boolean().optional(),
      },
    },
    async ({ projectDir, sessionId, includeImageBase64 = false }) => {
      const paths = resolvePaths(projectDir)
      const target = await ensureCanvasPanel(paths, sessionId)
      const state =
        (await readJson(
          panelFile(paths, target.session.id, target.panel.id, "state.json")
        )) ?? emptyCanvasSnapshot()
      const rawSelection =
        (await readJson(
          panelFile(paths, target.session.id, target.panel.id, "selection.json")
        )) ?? emptySelection(target.session.id, target.panel.id)
      const selection = withLastImageFallback(rawSelection, state)
      let base64 = null
      if (includeImageBase64 && selection.assetRef) {
        const filePath = resolve(
          paths.storageDir,
          ...selection.assetRef.split("/").map(safePart)
        )
        assertInside(paths.storageDir, filePath)
        base64 = (await readFile(filePath)).toString("base64")
      }
      const selectedShapes = selection.selectedShapes ?? []
      const summary =
        selectedShapes.length === 0
          ? "No MyOpenPanels shapes are currently selected and no image fallback is available."
          : selectedShapes
              .map(
                (shape) =>
                  `${shape.id ?? "unknown"} [${shape.type ?? "unknown"}]${shape.asset?.name ? ` (${shape.asset.name})` : ""}`
              )
              .join("\n")
      return {
        content: [{ type: "text", text: summary }],
        structuredContent: {
          selection,
          selectionFile: panelFile(
            paths,
            target.session.id,
            target.panel.id,
            "selection.json"
          ),
          base64,
          mimeType: selection.assetRef
            ? mimeTypeForFile(selection.assetRef)
            : null,
        },
      }
    }
  )

  server.registerTool(
    TOOL_READ_SELECTION_ASSET,
    {
      title: "Read MyOpenPanels Selection Asset",
      description:
        "Read the PNG exported from the current MyOpenPanels canvas selection.",
      inputSchema: currentCanvasArgs,
    },
    async ({ projectDir, sessionId }) => {
      const paths = resolvePaths(projectDir)
      const target = await ensureCanvasPanel(paths, sessionId)
      const state =
        (await readJson(
          panelFile(paths, target.session.id, target.panel.id, "state.json")
        )) ?? emptyCanvasSnapshot()
      const rawSelection = await readJson(
        panelFile(paths, target.session.id, target.panel.id, "selection.json")
      )
      const selection = withLastImageFallback(rawSelection, state)
      if (!selection?.assetRef)
        throw new Error("No MyOpenPanels selection asset is available.")
      const filePath = resolve(
        paths.storageDir,
        ...selection.assetRef.split("/").map(safePart)
      )
      assertInside(paths.storageDir, filePath)
      const data = await readFile(filePath)
      return jsonText({
        assetRef: selection.assetRef,
        mimeType: mimeTypeForFile(selection.assetRef),
        base64: data.toString("base64"),
      })
    }
  )

  server.registerTool(
    TOOL_INSERT_IMAGE,
    {
      title: "Insert MyOpenPanels Image",
      description:
        "Copy a local image into MyOpenPanels assets and create an image shape in the current canvas.",
      inputSchema: {
        ...currentCanvasArgs,
        imagePath: z.string().trim(),
        anchorShapeId: z.string().trim().optional(),
        placement: z.enum(["right", "left", "below"]).optional(),
        fileName: z.string().trim().optional(),
        displayWidth: z.number().positive().optional(),
        displayHeight: z.number().positive().optional(),
      },
    },
    async ({
      projectDir,
      sessionId,
      imagePath,
      anchorShapeId,
      placement = "right",
      fileName,
      displayWidth,
      displayHeight,
    }) => {
      const paths = resolvePaths(projectDir)
      const target = await ensureCanvasPanel(paths, sessionId)
      const result = await insertOpenPanelsImage(paths, target, {
        imagePath,
        anchorShapeId,
        placement,
        fileName,
        displayWidth,
        displayHeight,
      })
      return jsonText(result)
    }
  )

  server.registerTool(
    TOOL_GET_SESSION,
    {
      title: "Get MyOpenPanels Session",
      description:
        "Read the current MyOpenPanels session from .openpanels storage.",
      inputSchema: currentCanvasArgs,
    },
    async ({ projectDir, sessionId }) => {
      const paths = resolvePaths(projectDir)
      const session = await ensureSession(paths, "Agent Session", sessionId)
      return jsonText({ session })
    }
  )

  server.registerTool(
    TOOL_OPEN_PANEL,
    {
      title: "Open MyOpenPanels Panel",
      description: "Create a panel in the current MyOpenPanels session.",
      inputSchema: {
        ...currentCanvasArgs,
        kind: panelKinds,
        title: z.string().trim().optional(),
      },
    },
    async ({ projectDir, sessionId, kind, title }) => {
      const paths = resolvePaths(projectDir)
      const session = await ensureSession(paths, "Agent Session", sessionId)
      const panel = await createPanel(paths, session, kind, title)
      await writeActiveSession(paths, session.id)
      return jsonText({ sessionId: session.id, panel })
    }
  )

  server.registerTool(
    TOOL_INSERT_ARTIFACT,
    {
      title: "Insert MyOpenPanels Artifact",
      description: "Insert an image or canvas artifact into the design canvas.",
      inputSchema: {
        ...currentCanvasArgs,
        panelId: z.string().trim().optional(),
        kind: z.enum(["image", "canvas"]),
        title: z.string().trim().optional(),
        assetRef: z.string().optional(),
        mimeType: z.string().optional(),
        snapshot: z.unknown().optional(),
      },
    },
    async ({
      projectDir,
      sessionId,
      panelId,
      kind,
      title,
      assetRef,
      mimeType,
      snapshot,
    }) => {
      const paths = resolvePaths(projectDir)
      const session = await ensureSession(paths, "Agent Session", sessionId)
      const targetPanel = panelId
        ? await readPanel(paths, session.id, panelId)
        : (await ensureCanvasPanel(paths, session.id)).panel
      if (!targetPanel) throw new Error(`Panel not found: ${panelId}`)
      const artifact = {
        id: createId("artifact"),
        panelId: targetPanel.id,
        kind,
        title,
        createdAt: new Date().toISOString(),
        ...(kind === "image"
          ? {
              assetRef: assetRef ?? "",
              mimeType: mimeType ?? "application/octet-stream",
            }
          : {}),
        ...(kind === "canvas" ? { snapshot: snapshot ?? {} } : {}),
      }
      await appendArtifact(paths, session.id, artifact)
      await writeActiveSession(paths, session.id)
      return jsonText({ artifact, panel: targetPanel })
    }
  )

  server.registerTool(
    TOOL_SAVE_PANEL_STATE,
    {
      title: "Save MyOpenPanels Panel State",
      description: "Persist panel state under .openpanels.",
      inputSchema: {
        ...projectArgs,
        sessionId: z.string(),
        panelId: z.string(),
        state: z.unknown(),
      },
    },
    async ({ projectDir, sessionId, panelId, state }) => {
      const paths = resolvePaths(projectDir)
      await writeJson(panelFile(paths, sessionId, panelId, "state.json"), state)
      await writeActiveSession(paths, sessionId)
      return jsonText({ saved: true, sessionId, panelId })
    }
  )

  server.registerTool(
    TOOL_WRITE_ASSET,
    {
      title: "Write MyOpenPanels Panel Asset",
      description: "Copy an existing local file into a panel asset directory.",
      inputSchema: {
        ...projectArgs,
        sessionId: z.string(),
        panelId: z.string(),
        sourcePath: z.string(),
        requestedName: z.string().optional(),
      },
    },
    async ({ projectDir, sessionId, panelId, sourcePath, requestedName }) => {
      const paths = resolvePaths(projectDir)
      const source = resolve(sourcePath)
      await stat(source)
      const assetDir = panelFile(paths, sessionId, panelId, "assets")
      await mkdir(assetDir, { recursive: true })
      const fileName = await uniqueFileName(
        assetDir,
        requestedName ?? basename(source)
      )
      const filePath = join(assetDir, fileName)
      assertInside(paths.storageDir, filePath)
      await copyFile(source, filePath)
      const assetRef = [
        "sessions",
        safePart(sessionId),
        "panels",
        safePart(panelId),
        "assets",
        fileName,
      ].join("/")
      return jsonText({ assetRef, fileName })
    }
  )

  server.registerTool(
    TOOL_READ_ASSET,
    {
      title: "Read MyOpenPanels Panel Asset",
      description: "Read a panel asset from .openpanels.",
      inputSchema: {
        ...projectArgs,
        assetRef: z.string(),
      },
    },
    async ({ projectDir, assetRef }) => {
      const paths = resolvePaths(projectDir)
      const filePath = resolve(
        paths.storageDir,
        ...assetRef.split("/").map(safePart)
      )
      assertInside(paths.storageDir, filePath)
      const data = await readFile(filePath)
      return {
        content: [
          {
            type: "text",
            text: data.toString("base64"),
          },
        ],
        structuredContent: {
          assetRef,
          base64: data.toString("base64"),
        },
      }
    }
  )
}

function resolvePaths(projectDir) {
  const resolvedProjectDir = resolve(
    projectDir || process.env.OPENPANELS_PROJECT_DIR || process.cwd()
  )
  const storageDir = resolve(resolvedProjectDir, ".openpanels")
  assertInside(resolvedProjectDir, storageDir)
  return { projectDir: resolvedProjectDir, storageDir }
}

async function ensureLocalStudioServer(projectDir) {
  const existing = localStudioServers.get(projectDir)
  if (existing?.process && !existing.process.killed) {
    try {
      await waitForLocalStudio(existing.url, 500)
      return existing
    } catch (_error) {
      stopLocalStudio(existing)
      localStudioServers.delete(projectDir)
    }
  }

  const port = await findOpenPort()
  const url = `http://127.0.0.1:${port}`
  const child = spawn(
    process.execPath,
    [
      viteCliPath(),
      "--host",
      "127.0.0.1",
      "--port",
      String(port),
      "--strictPort",
    ],
    {
      cwd: localStudioDir(),
      env: {
        ...process.env,
        FORCE_COLOR: "0",
        OPENPANELS_PROJECT_DIR: projectDir,
      },
      stdio: ["pipe", "pipe", "pipe"],
      shell: process.platform === "win32",
    }
  )

  child.stdout?.on("data", (chunk) =>
    process.stderr.write(`[openpanels-studio] ${chunk}`)
  )
  child.stderr?.on("data", (chunk) =>
    process.stderr.write(`[openpanels-studio] ${chunk}`)
  )
  let startupError = null
  child.once("error", (error) => {
    startupError = error
  })
  child.once("exit", (code, signal) => {
    if (!startupError && code !== 0) {
      startupError = new Error(
        `OpenPanels local studio exited during startup (code ${code}, signal ${
          signal ?? "none"
        }).`
      )
    }
    const current = localStudioServers.get(projectDir)
    if (current?.process === child) localStudioServers.delete(projectDir)
  })

  const serverInfo = { projectDir, url, port, process: child }
  localStudioServers.set(projectDir, serverInfo)

  try {
    await waitForLocalStudio(url, 20_000, () => startupError)
  } catch (error) {
    stopLocalStudio(serverInfo)
    localStudioServers.delete(projectDir)
    throw error
  }

  return serverInfo
}

function stopLocalStudio(serverInfo) {
  if (serverInfo?.process && !serverInfo.process.killed) {
    serverInfo.process.kill()
  }
}

function pluginRootDir() {
  return resolve(fileURLToPath(new URL("..", import.meta.url)))
}

function localStudioDir() {
  return join(pluginRootDir(), "apps", "local-studio")
}

function viteCliPath() {
  return join(pluginRootDir(), "node_modules", "vite", "bin", "vite.js")
}

async function findOpenPort() {
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

async function waitForLocalStudio(url, timeoutMs, getStartupError = () => null) {
  const startedAt = Date.now()
  let lastError
  while (Date.now() - startedAt < timeoutMs) {
    const startupError = getStartupError()
    if (startupError) throw startupError
    try {
      const response = await fetch(`${url}/api/bootstrap`)
      if (response.ok) return
      lastError = new Error(
        `OpenPanels local studio responded with ${response.status}.`
      )
    } catch (error) {
      lastError = error
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 250))
  }
  throw new Error(
    `OpenPanels local studio did not become ready at ${url}: ${
      lastError instanceof Error ? lastError.message : String(lastError)
    }`
  )
}

async function ensureSession(paths, title, requestedSessionId) {
  await mkdir(join(paths.storageDir, "sessions"), { recursive: true })
  const indexPath = join(paths.storageDir, "index.json")
  const index = (await readJson(indexPath)) ?? {
    schemaVersion: 1,
    sessions: [],
  }
  if (requestedSessionId) {
    const session = await readSession(paths, requestedSessionId)
    if (!session) {
      throw new Error(`OpenPanels session not found: ${requestedSessionId}`)
    }
    return session
  }
  const activeSessionId = await readActiveSession(paths)
  const existing =
    (activeSessionId
      ? index.sessions?.find((session) => session.id === activeSessionId)
      : null) ?? index.sessions?.[0]
  if (existing) {
    const session = await readSession(paths, existing.id)
    if (session) return session
  }
  const now = new Date().toISOString()
  const session = {
    id: createId("session"),
    title,
    createdAt: now,
    updatedAt: now,
    panelIds: [],
  }
  await writeSession(paths, session)
  await writeActiveSession(paths, session.id)
  return session
}

async function ensureCanvasPanel(paths, requestedSessionId) {
  const session = await ensureSession(
    paths,
    "Agent Session",
    requestedSessionId
  )
  for (const panelId of session.panelIds ?? []) {
    const panel = await readPanel(paths, session.id, panelId)
    if (panel?.kind === "canvas") return { session, panel }
  }
  const panel = await createPanel(paths, session, "canvas", "Design canvas")
  return { session, panel }
}

function emptySelection(sessionId, panelId) {
  return {
    sessionId,
    panelId,
    selectedShapeIds: [],
    selectedShapes: [],
    assetRef: null,
    updatedAt: new Date().toISOString(),
  }
}

function withLastImageFallback(selection, state) {
  const selectedShapes = selection?.selectedShapes ?? []
  if (selectedShapes.length > 0) return selection
  const fallback = findLastImageSelectionShape(state)
  if (!fallback) return selection
  return {
    ...selection,
    selectedShapeIds: [fallback.id],
    selectedShapes: [fallback],
    assetRef: fallback.asset?.assetRef ?? null,
    fallback: "last-image",
  }
}

function findLastImageSelectionShape(state) {
  const store =
    state?.store && typeof state.store === "object" ? state.store : {}
  const images = Object.values(store)
    .filter((record) => record?.typeName === "shape" && record.type === "image")
    .sort((a, b) => {
      const indexDiff = (Number(b.index) || 0) - (Number(a.index) || 0)
      if (indexDiff !== 0) return indexDiff
      return String(b.id).localeCompare(String(a.id))
    })
  const shape = images[0]
  if (!shape) return null
  return summarizeShapeForAgent(shape, store)
}

function summarizeShapeForAgent(shape, store) {
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

function assetRefFromAsset(asset) {
  if (!asset) return null
  if (typeof asset.meta?.assetRef === "string") return asset.meta.assetRef
  const src = asset.props?.src
  if (typeof src !== "string") return null
  const match = src.match(/^\/api\/panels\/([^/]+)\/([^/]+)\/assets\/(.+)$/)
  if (!match) return null
  const sessionId = decodeURIComponent(match[1])
  const panelId = decodeURIComponent(match[2])
  const assetPath = match[3].split("/").map(decodeURIComponent).join("/")
  return [
    "sessions",
    sessionId,
    "panels",
    panelId,
    "assets",
    ...assetPath.split("/"),
  ].join("/")
}

async function insertOpenPanelsImage(paths, target, input) {
  const source = resolve(input.imagePath)
  await stat(source)
  const imageBuffer = await readFile(source)
  const dimensions = readImageDimensions(imageBuffer) ?? {}
  const assetDir = panelFile(
    paths,
    target.session.id,
    target.panel.id,
    "assets"
  )
  await mkdir(assetDir, { recursive: true })
  const fileName = await uniqueFileName(
    assetDir,
    input.fileName ?? basename(source)
  )
  const filePath = join(assetDir, fileName)
  assertInside(paths.storageDir, filePath)
  await copyFile(source, filePath)

  const statePath = panelFile(
    paths,
    target.session.id,
    target.panel.id,
    "state.json"
  )
  const state = (await readJson(statePath)) ?? emptyCanvasSnapshot()
  const store =
    state.store && typeof state.store === "object" ? state.store : {}
  const pageId = state.currentPageId || findFirstPageId(store) || "page:main"
  if (!store[pageId]) {
    store[pageId] = { id: pageId, typeName: "page", name: "Page 1", index: 1 }
  }
  const width = input.displayWidth ?? dimensions.width ?? 512
  const height =
    input.displayHeight ??
    (dimensions.width && dimensions.height && input.displayWidth
      ? Math.round((input.displayWidth * dimensions.height) / dimensions.width)
      : (dimensions.height ?? 512))
  const anchor = input.anchorShapeId ? store[input.anchorShapeId] : null
  const anchorBounds = anchor?.typeName === "shape" ? shapeBounds(anchor) : null
  const position = placeImage(anchorBounds, width, height, input.placement)
  const assetId = createId("asset")
  const shapeId = createId("shape")
  const assetRef = [
    "sessions",
    safePart(target.session.id),
    "panels",
    safePart(target.panel.id),
    "assets",
    fileName,
  ].join("/")
  const assetUrl = `/api/panels/${encodeURIComponent(target.session.id)}/${encodeURIComponent(target.panel.id)}/assets/${encodeURIComponent(fileName)}`
  const mimeType = mimeTypeForFile(fileName)

  store[assetId] = {
    id: assetId,
    typeName: "asset",
    type: "image",
    props: {
      name: fileName,
      src: assetUrl,
      w: dimensions.width ?? width,
      h: dimensions.height ?? height,
      mimeType,
      isAnimated: false,
    },
    meta: { assetRef },
  }
  store[shapeId] = {
    id: shapeId,
    typeName: "shape",
    type: "image",
    parentId: anchor?.parentId || pageId,
    index: nextShapeIndex(store, pageId),
    props: {
      x: position.x,
      y: position.y,
      width,
      height,
      assetId,
    },
  }
  state.store = store
  state.currentPageId = pageId
  state.selectedShapeIds = [shapeId]
  await writeJson(statePath, state)
  await writeActiveSession(paths, target.session.id)
  return {
    sessionId: target.session.id,
    panelId: target.panel.id,
    assetId,
    shapeId,
    assetRef,
    assetFile: filePath,
    assetUrl,
    bounds: { x: position.x, y: position.y, width, height },
  }
}

async function writeSession(paths, session) {
  const dir = join(paths.storageDir, "sessions", safePart(session.id))
  await mkdir(dir, { recursive: true })
  await writeJson(join(dir, "session.json"), session)
  const indexPath = join(paths.storageDir, "index.json")
  const index = (await readJson(indexPath)) ?? {
    schemaVersion: 1,
    sessions: [],
  }
  const existingSessions = Array.isArray(index.sessions) ? index.sessions : []
  const sessionsById = new Map(
    existingSessions.map((existing) => [existing.id, existing])
  )
  sessionsById.set(session.id, {
    id: session.id,
    title: session.title,
    updatedAt: session.updatedAt,
  })
  await writeJson(join(paths.storageDir, "index.json"), {
    schemaVersion: 1,
    sessions: [...sessionsById.values()].sort((a, b) =>
      String(b.updatedAt).localeCompare(String(a.updatedAt))
    ),
  })
}

async function createPanel(paths, session, kind, title) {
  const now = new Date().toISOString()
  const panel = {
    id: createId("panel"),
    sessionId: session.id,
    kind,
    title: title || titleForKind(kind),
    createdAt: now,
    updatedAt: now,
    stateRef: `sessions/${session.id}/panels/panel/state.json`,
  }
  const dir = panelDir(paths, session.id, panel.id)
  await mkdir(dir, { recursive: true })
  panel.stateRef = `sessions/${safePart(session.id)}/panels/${safePart(panel.id)}/state.json`
  await writeJson(join(dir, "panel.json"), panel)
  await writeJson(
    join(dir, "state.json"),
    kind === "canvas" ? emptyCanvasSnapshot() : {}
  )
  const nextSession = {
    ...session,
    updatedAt: now,
    panelIds: [...new Set([...(session.panelIds ?? []), panel.id])],
  }
  await writeSession(paths, nextSession)
  Object.assign(session, nextSession)
  return panel
}

async function readPanel(paths, sessionId, panelId) {
  return readJson(panelFile(paths, sessionId, panelId, "panel.json"))
}

async function readSession(paths, sessionId) {
  return readJson(
    join(paths.storageDir, "sessions", safePart(sessionId), "session.json")
  )
}

async function readActiveSession(paths) {
  const active = await readJson(activeSessionFile(paths))
  return typeof active?.sessionId === "string" ? active.sessionId : null
}

async function writeActiveSession(paths, sessionId) {
  await writeJson(activeSessionFile(paths), {
    sessionId,
    updatedAt: new Date().toISOString(),
  })
}

function activeSessionFile(paths) {
  return join(paths.storageDir, "active-session.json")
}

async function appendArtifact(paths, sessionId, artifact) {
  const filePath = join(
    paths.storageDir,
    "sessions",
    safePart(sessionId),
    "artifacts.json"
  )
  const artifacts = (await readJson(filePath)) ?? []
  await writeJson(filePath, [...artifacts, artifact])
}

function panelDir(paths, sessionId, panelId) {
  const dir = join(
    paths.storageDir,
    "sessions",
    safePart(sessionId),
    "panels",
    safePart(panelId)
  )
  assertInside(paths.storageDir, dir)
  return dir
}

function panelFile(paths, sessionId, panelId, name) {
  return join(panelDir(paths, sessionId, panelId), safePart(name))
}

async function readJson(filePath) {
  try {
    return JSON.parse(await readFile(filePath, "utf8"))
  } catch (error) {
    if (error?.code === "ENOENT") return null
    throw error
  }
}

async function writeJson(filePath, value) {
  await mkdir(resolve(filePath, ".."), { recursive: true })
  await writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`, "utf8")
}

function createId(prefix) {
  return `${prefix}:${crypto.randomUUID()}`
}

function titleForKind(kind) {
  return kind.charAt(0).toUpperCase() + kind.slice(1)
}

function emptyCanvasSnapshot() {
  return {
    schema: {
      schemaVersion: 1,
      recordVersions: { page: 1, shape: 1, asset: 1 },
    },
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

function findFirstPageId(store) {
  return (
    Object.values(store).find((record) => record?.typeName === "page")?.id ??
    null
  )
}

function nextShapeIndex(store, pageId) {
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

function shapeBounds(shape) {
  const props = shape.props || {}
  return {
    x: Number(props.x) || 0,
    y: Number(props.y) || 0,
    width: Number(props.width || props.w) || 160,
    height: Number(props.height || props.h) || 120,
  }
}

function placeImage(anchorBounds, width, _height, placement) {
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

function readImageDimensions(buffer) {
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

function mimeTypeForFile(fileName) {
  switch (extname(fileName).toLowerCase()) {
    case ".png":
      return "image/png"
    case ".jpg":
    case ".jpeg":
      return "image/jpeg"
    case ".gif":
      return "image/gif"
    case ".webp":
      return "image/webp"
    default:
      return "application/octet-stream"
  }
}

function safePart(value) {
  const safe = basename(String(value))
    .replace(/[^a-zA-Z0-9._:-]+/g, "-")
    .replace(/^-+|-+$/g, "")
  if (!safe || safe === "." || safe === "..")
    throw new Error(`Unsafe path part: ${value}`)
  return safe
}

async function uniqueFileName(dir, requestedName) {
  const raw = basename(requestedName || "asset.bin")
  const extension = extname(raw) || ".bin"
  const base =
    raw
      .slice(0, raw.length - extname(raw).length)
      .replace(/[^a-zA-Z0-9._-]+/g, "-") || "asset"
  let candidate = `${base}${extension}`
  let counter = 2
  while (true) {
    try {
      await stat(join(dir, candidate))
      candidate = `${base}-${counter}${extension}`
      counter += 1
    } catch (error) {
      if (error?.code === "ENOENT") return candidate
      throw error
    }
  }
}

function assertInside(parent, child) {
  const rel = relative(parent, child)
  if (rel.startsWith("..") || rel.includes(`..${sep}`)) {
    throw new Error(`Path escapes OpenPanels storage: ${child}`)
  }
}

function jsonText(value) {
  return {
    content: [
      {
        type: "text",
        text: JSON.stringify(value, null, 2),
      },
    ],
    structuredContent: value,
  }
}
