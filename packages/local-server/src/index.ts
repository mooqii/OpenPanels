import { readFile } from "node:fs/promises"
import {
  createServer,
  type IncomingMessage,
  type ServerResponse,
} from "node:http"
import { extname, join } from "node:path"
import { LocalOpenPanelsStorage } from "@openpanels/local-storage"
import type {
  CreateSessionInput,
  InsertArtifactInput,
  OpenPanelInput,
  OpenPanelsPanel,
  OpenPanelsSession,
} from "@openpanels/protocol"
import { OpenPanelsRuntime } from "@openpanels/runtime"

export interface CreateLocalServerOptions {
  projectDir: string
  staticDir?: string
}

export function createLocalOpenPanelsRuntime(projectDir: string) {
  return new OpenPanelsRuntime({
    storage: new LocalOpenPanelsStorage({ projectDir }),
  })
}

export function createLocalOpenPanelsServer(options: CreateLocalServerOptions) {
  const runtime = createLocalOpenPanelsRuntime(options.projectDir)
  const storage = new LocalOpenPanelsStorage({ projectDir: options.projectDir })

  return createServer(async (request, response) => {
    try {
      await routeRequest(request, response, runtime, storage, options.staticDir)
    } catch (error) {
      response.statusCode = 500
      response.setHeader("content-type", "application/json")
      response.end(
        JSON.stringify({
          error: error instanceof Error ? error.message : String(error),
        })
      )
    }
  })
}

async function routeRequest(
  request: IncomingMessage,
  response: ServerResponse,
  runtime: OpenPanelsRuntime,
  storage: LocalOpenPanelsStorage,
  staticDir?: string
) {
  const url = new URL(request.url ?? "/", "http://localhost")

  if (request.method === "GET" && url.pathname === "/api/bootstrap") {
    const bootstrap = await ensureCanvasBootstrap(
      runtime,
      url.searchParams.get("sessionId")
    )
    return json(response, bootstrap)
  }

  if (request.method === "POST" && url.pathname === "/api/projects") {
    const body = (await readBody(request)) as { title?: string }
    const bootstrap = await createCanvasProject(runtime, body.title)
    return json(response, bootstrap)
  }

  if (request.method === "GET" && url.pathname === "/api/sessions") {
    return json(response, await runtime.listSessions())
  }

  const sessionMatch = url.pathname.match(/^\/api\/sessions\/([^/]+)$/)
  if (sessionMatch) {
    const sessionId = decodeURIComponent(sessionMatch[1])
    if (request.method === "PATCH") {
      const body = (await readBody(request)) as { title?: string }
      const updated = await renameSession(storage, sessionId, body.title)
      return json(response, { session: updated })
    }
  }

  if (request.method === "POST" && url.pathname === "/api/sessions") {
    return json(
      response,
      await runtime.createSession(
        (await readBody(request)) as CreateSessionInput
      )
    )
  }

  if (request.method === "POST" && url.pathname === "/api/panels") {
    return json(
      response,
      await runtime.openPanel((await readBody(request)) as OpenPanelInput)
    )
  }

  if (request.method === "POST" && url.pathname === "/api/artifacts") {
    return json(
      response,
      await runtime.insertArtifact(
        (await readBody(request)) as InsertArtifactInput
      )
    )
  }

  const panelMatch = url.pathname.match(
    /^\/api\/panels\/([^/]+)\/([^/]+)\/(.+)$/
  )
  if (panelMatch) {
    const [, rawSessionId, rawPanelId, tail] = panelMatch
    const sessionId = decodeURIComponent(rawSessionId)
    const panelId = decodeURIComponent(rawPanelId)

    if (request.method === "PUT" && tail === "state") {
      const body = await readBody(request)
      await runtime.savePanelState(sessionId, panelId, body)
      return json(response, { saved: true, sessionId, panelId })
    }

    if (request.method === "PUT" && tail === "selection") {
      const body = (await readBody(request)) as {
        imageDataUrl?: string | null
        selection?: Record<string, unknown>
      }
      let assetRef =
        (body.selection?.assetRef as string | null | undefined) ?? null
      if (body.imageDataUrl) {
        const image = dataUrlToBuffer(body.imageDataUrl)
        const written = await storage.writeAssetFromBuffer({
          sessionId,
          panelId,
          buffer: image.buffer,
          requestedName: "__selection/current.png",
          overwrite: true,
        })
        assetRef = written.assetRef
      }
      const selection = {
        sessionId,
        panelId,
        selectedShapeIds: Array.isArray(body.selection?.selectedShapeIds)
          ? (body.selection?.selectedShapeIds as string[])
          : [],
        selectedShapes: Array.isArray(body.selection?.selectedShapes)
          ? (body.selection?.selectedShapes as unknown[])
          : [],
        assetRef,
        updatedAt: new Date().toISOString(),
      }
      await storage.writePanelSelection(selection)
      return json(response, { saved: true, selection })
    }

    if (request.method === "POST" && tail === "assets") {
      const body = (await readBody(request)) as {
        dataUrl: string
        fileName?: string
        mimeType?: string
      }
      const image = dataUrlToBuffer(body.dataUrl)
      const written = await storage.writeAssetFromBuffer({
        sessionId,
        panelId,
        buffer: image.buffer,
        requestedName: body.fileName || "asset.png",
      })
      return json(response, {
        ...written,
        meta: {
          assetRef: written.assetRef,
          fileName: written.fileName,
        },
        mimeType: body.mimeType || image.mimeType,
        src: `/api/panels/${encodeURIComponent(sessionId)}/${encodeURIComponent(panelId)}/assets/${written.fileName}`,
      })
    }

    if (request.method === "GET" && tail.startsWith("assets/")) {
      const assetName = tail.slice("assets/".length)
      const assetRef = [
        "sessions",
        sessionId,
        "panels",
        panelId,
        "assets",
        ...assetName.split("/"),
      ].join("/")
      const file = await storage.readAsset(assetRef)
      response.statusCode = 200
      response.setHeader("content-type", contentType(assetName))
      response.end(file)
      return
    }
  }

  if (staticDir && request.method === "GET") {
    const path = url.pathname === "/" ? "/index.html" : url.pathname
    const filePath = join(staticDir, path)
    const file = await readFile(filePath)
    response.statusCode = 200
    response.setHeader("content-type", contentType(filePath))
    response.end(file)
    return
  }

  response.statusCode = 404
  response.end("Not found")
}

export function createOpenPanelsApiMiddleware(projectDir: string) {
  const runtime = createLocalOpenPanelsRuntime(projectDir)
  const storage = new LocalOpenPanelsStorage({ projectDir })
  return async (
    request: IncomingMessage,
    response: ServerResponse,
    next: (error?: unknown) => void
  ) => {
    if (!request.url?.startsWith("/api/")) return next()
    try {
      await routeRequest(request, response, runtime, storage)
    } catch (error) {
      next(error)
    }
  }
}

async function ensureCanvasBootstrap(
  runtime: OpenPanelsRuntime,
  requestedSessionId?: string | null
) {
  const sessions = await runtime.listSessions()
  const session =
    (requestedSessionId
      ? await runtime.getSession(requestedSessionId)
      : null) ??
    sessions[0] ??
    (await runtime.createSession({ title: "Untitled" }))
  return ensureCanvasForSession(runtime, session)
}

async function createCanvasProject(
  runtime: OpenPanelsRuntime,
  title = "Untitled"
) {
  const session = await runtime.createSession({
    title: title.trim() || "Untitled",
  })
  return ensureCanvasForSession(runtime, session)
}

async function ensureCanvasForSession(
  runtime: OpenPanelsRuntime,
  session: OpenPanelsSession
) {
  let currentSession = session
  let panel: OpenPanelsPanel | null = null
  for (const panelId of currentSession.panelIds) {
    const candidate = await runtime.getPanel(currentSession.id, panelId)
    if (candidate?.kind === "canvas") {
      panel = candidate
      break
    }
  }
  if (!panel) {
    panel = await runtime.openPanel({
      sessionId: session.id,
      kind: "canvas",
      title: "Design canvas",
      initialState: emptyCanvasSnapshot(),
    })
    currentSession =
      (await runtime.getSession(currentSession.id)) ?? currentSession
  }
  const state =
    (await runtime.readPanelState(currentSession.id, panel.id)) ??
    emptyCanvasSnapshot()
  return {
    session: currentSession,
    panel,
    sessions: await runtime.listSessions(),
    state: normalizeSerializableSnapshot(state),
  }
}

async function renameSession(
  storage: LocalOpenPanelsStorage,
  sessionId: string,
  title: string | undefined
) {
  const session = await storage.readSession(sessionId)
  if (!session) throw new Error(`OpenPanels session not found: ${sessionId}`)
  const nextTitle = title?.trim()
  if (!nextTitle) throw new Error("Project title is required")
  const updated = {
    ...session,
    title: nextTitle,
    updatedAt: new Date().toISOString(),
  }
  await storage.writeSession(updated)
  return updated
}

async function readBody(request: IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = []
  for await (const chunk of request) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk))
  }
  if (chunks.length === 0) return {}
  return JSON.parse(Buffer.concat(chunks).toString("utf8"))
}

function json(response: ServerResponse, data: unknown): void {
  response.statusCode = 200
  response.setHeader("content-type", "application/json")
  response.end(JSON.stringify(data, jsonReplacer))
}

function jsonReplacer(_key: string, value: unknown) {
  return value instanceof Set ? [...value] : value
}

function normalizeSerializableSnapshot(value: unknown): unknown {
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

function dataUrlToBuffer(dataUrl: string): {
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

function contentType(filePath: string): string {
  switch (extname(filePath)) {
    case ".html":
      return "text/html; charset=utf-8"
    case ".js":
      return "text/javascript; charset=utf-8"
    case ".css":
      return "text/css; charset=utf-8"
    case ".gif":
      return "image/gif"
    case ".jpg":
    case ".jpeg":
      return "image/jpeg"
    case ".png":
      return "image/png"
    case ".webp":
      return "image/webp"
    default:
      return "application/octet-stream"
  }
}
