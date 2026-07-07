import { readFile } from "node:fs/promises"
import {
  createServer,
  type IncomingMessage,
  type ServerResponse,
} from "node:http"
import { extname, relative, resolve, sep } from "node:path"
import {
  createCanvasProject,
  createOpenPanelsLocalContext,
  createLocalOpenPanelsRuntime as createRuntime,
  dataUrlToBuffer,
  deleteSession,
  ensureCanvasBootstrap,
  readActiveSession,
  renameSession,
  savePanelState,
  saveSelectionState,
  writeActiveSession,
} from "@openpanels/local-control"
import type {
  CreateSessionInput,
  InsertArtifactInput,
  OpenPanelInput,
} from "@openpanels/protocol"

export interface CreateLocalServerOptions {
  projectDir: string
  staticDir?: string
}

export function createLocalOpenPanelsRuntime(projectDir: string) {
  return createRuntime(projectDir)
}

export function createLocalOpenPanelsServer(options: CreateLocalServerOptions) {
  const context = createOpenPanelsLocalContext(options.projectDir)

  return createServer(async (request, response) => {
    try {
      await routeRequest(request, response, context, options.staticDir)
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
  context: ReturnType<typeof createOpenPanelsLocalContext>,
  staticDir?: string
) {
  const url = new URL(request.url ?? "/", "http://localhost")
  setCorsHeaders(response)

  if (request.method === "OPTIONS") {
    response.statusCode = 204
    response.end()
    return
  }

  if (request.method === "GET" && url.pathname === "/api/bootstrap") {
    const bootstrap = await ensureCanvasBootstrap(
      context,
      url.searchParams.get("sessionId")
    )
    return json(response, bootstrap)
  }

  if (request.method === "POST" && url.pathname === "/api/projects") {
    const body = (await readBody(request)) as { title?: string }
    const bootstrap = await createCanvasProject(context, body.title)
    return json(response, bootstrap)
  }

  if (request.method === "GET" && url.pathname === "/api/sessions") {
    return json(response, await context.runtime.listSessions())
  }

  if (request.method === "GET" && url.pathname === "/api/active-session") {
    return json(response, { sessionId: await readActiveSession(context) })
  }

  if (request.method === "PUT" && url.pathname === "/api/active-session") {
    const body = (await readBody(request)) as { sessionId?: string }
    const sessionId = body.sessionId?.trim()
    if (!(sessionId && (await context.runtime.getSession(sessionId)))) {
      throw new Error(`OpenPanels session not found: ${sessionId}`)
    }
    await writeActiveSession(context, sessionId)
    return json(response, { sessionId })
  }

  const sessionMatch = url.pathname.match(/^\/api\/sessions\/([^/]+)$/)
  if (sessionMatch) {
    const sessionId = decodeURIComponent(sessionMatch[1])
    if (request.method === "PATCH") {
      const body = (await readBody(request)) as { title?: string }
      const updated = await renameSession(context, sessionId, body.title)
      return json(response, { session: updated })
    }
    if (request.method === "DELETE") {
      return json(response, await deleteSession(context, sessionId))
    }
  }

  if (request.method === "POST" && url.pathname === "/api/sessions") {
    return json(
      response,
      await context.runtime.createSession(
        (await readBody(request)) as CreateSessionInput
      )
    )
  }

  if (request.method === "POST" && url.pathname === "/api/panels") {
    return json(
      response,
      await context.runtime.openPanel(
        (await readBody(request)) as OpenPanelInput
      )
    )
  }

  if (request.method === "POST" && url.pathname === "/api/artifacts") {
    return json(
      response,
      await context.runtime.insertArtifact(
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
      await savePanelState({
        projectDir: context.paths.projectDir,
        sessionId,
        panelId,
        state: body,
      })
      return json(response, { saved: true, sessionId, panelId })
    }

    if (request.method === "PUT" && tail === "selection") {
      const body = (await readBody(request)) as {
        imageDataUrl?: string | null
        selection?: Record<string, unknown>
      }
      const saved = await saveSelectionState({
        projectDir: context.paths.projectDir,
        sessionId,
        panelId,
        selection: body.selection,
        imageDataUrl: body.imageDataUrl,
      })
      return json(response, saved)
    }

    if (request.method === "POST" && tail === "assets") {
      const body = (await readBody(request)) as {
        dataUrl: string
        fileName?: string
        mimeType?: string
      }
      const image = dataUrlToBuffer(body.dataUrl)
      const written = await context.storage.writeAssetFromBuffer({
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
      const file = await context.storage.readAsset(assetRef)
      response.statusCode = 200
      response.setHeader("content-type", contentType(assetName))
      response.end(file)
      return
    }
  }

  if (staticDir && request.method === "GET") {
    const filePath = staticFilePath(staticDir, url.pathname)
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
  const context = createOpenPanelsLocalContext(projectDir)
  return async (
    request: IncomingMessage,
    response: ServerResponse,
    next: (error?: unknown) => void
  ) => {
    if (!request.url?.startsWith("/api/")) return next()
    try {
      await routeRequest(request, response, context)
    } catch (error) {
      next(error)
    }
  }
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

function setCorsHeaders(response: ServerResponse): void {
  response.setHeader("access-control-allow-origin", "*")
  response.setHeader(
    "access-control-allow-methods",
    "GET,POST,PUT,PATCH,DELETE,OPTIONS"
  )
  response.setHeader("access-control-allow-headers", "content-type")
}

function jsonReplacer(_key: string, value: unknown) {
  return value instanceof Set ? [...value] : value
}

function staticFilePath(staticDir: string, pathname: string): string {
  const root = resolve(staticDir)
  const decodedPath = decodeURIComponent(pathname)
  const relativePath =
    decodedPath === "/" ? "index.html" : decodedPath.replace(/^\/+/, "")
  const filePath = resolve(root, relativePath)
  assertInside(root, filePath)
  return filePath
}

function assertInside(parent: string, child: string) {
  const rel = relative(parent, child)
  if (rel.startsWith("..") || rel.includes(`..${sep}`)) {
    throw new Error(`Path escapes OpenPanels static root: ${child}`)
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
