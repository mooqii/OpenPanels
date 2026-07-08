import { createReadStream } from "node:fs"
import { readFile } from "node:fs/promises"
import {
  createServer,
  type IncomingMessage,
  type ServerResponse,
} from "node:http"
import { basename, extname, relative, resolve, sep } from "node:path"
import type {
  OpenPanelsLocalContextOptions,
  WikiTaskStatus,
} from "@openpanels/local-control"
import {
  addWikiRawDocument,
  claimWikiTask,
  completeWikiTask,
  createCanvasProject,
  createOpenPanelsLocalContext,
  createLocalOpenPanelsRuntime as createRuntime,
  dataUrlToBuffer,
  deleteSession,
  deleteWikiRawDocument,
  extractWikiRawDocumentMarkdown,
  failWikiTask,
  getProjectBootstrap,
  getWikiBootstrap,
  listWikiAgentTargets,
  listWikiTasks,
  nextWikiTask,
  readActivePanel,
  readActiveSession,
  readWikiMarkdown,
  readWikiPage,
  readWikiRawDocumentOriginal,
  registerWikiAgentTarget,
  reindexWikiRawDocument,
  reindexWikiSpace,
  renameSession,
  revealWikiRawDocumentOriginal,
  savePanelState,
  saveSelectionState,
  setActivePanel,
  setActiveWikiSpace,
  setWikiLanguage,
  writeActiveSession,
  writeWikiMarkdown,
  writeWikiPage,
} from "@openpanels/local-control"
import type {
  CreateSessionInput,
  InsertArtifactInput,
  OpenPanelInput,
  OpenPanelsPanelKind,
} from "@openpanels/protocol"

export interface CreateLocalServerOptions {
  contextId?: string
  projectDir: string
  staticDir?: string
  storageDir?: string
}

export function createLocalOpenPanelsRuntime(
  projectDir: string,
  options: OpenPanelsLocalContextOptions = {}
) {
  return createRuntime(projectDir, options)
}

export function createLocalOpenPanelsServer(options: CreateLocalServerOptions) {
  const context = createOpenPanelsLocalContext(options.projectDir, options)

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
    const bootstrap = await getProjectBootstrap({
      projectDir: context.paths.projectDir,
      storageDir: context.paths.storageDir,
      contextId: context.paths.contextId,
      sessionId: url.searchParams.get("sessionId"),
      panelKind: panelKindParam(url.searchParams.get("panelKind")),
      panelId: url.searchParams.get("panelId"),
    })
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

  if (request.method === "GET" && url.pathname === "/api/active-panel") {
    return json(response, { activePanel: await readActivePanel(context) })
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

  if (request.method === "PUT" && url.pathname === "/api/active-panel") {
    const body = (await readBody(request)) as {
      kind?: string
      panelId?: string
      sessionId?: string
    }
    const bootstrap = await setActivePanel({
      projectDir: context.paths.projectDir,
      storageDir: context.paths.storageDir,
      contextId: context.paths.contextId,
      sessionId: body.sessionId,
      panelId: body.panelId,
      kind: panelKindParam(body.kind) ?? undefined,
    })
    return json(response, {
      activePanelId: bootstrap.activePanelId,
      activePanelKind: bootstrap.activePanelKind,
      panel: bootstrap.panel,
      state: bootstrap.state,
    })
  }

  if (url.pathname.startsWith("/api/wiki/")) {
    return routeWikiRequest(request, response, context, url)
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
        storageDir: context.paths.storageDir,
        contextId: context.paths.contextId,
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
        storageDir: context.paths.storageDir,
        contextId: context.paths.contextId,
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

async function routeWikiRequest(
  request: IncomingMessage,
  response: ServerResponse,
  context: ReturnType<typeof createOpenPanelsLocalContext>,
  url: URL
) {
  if (request.method === "GET" && url.pathname === "/api/wiki/context") {
    return json(response, await getWikiBootstrap(context.paths))
  }

  if (request.method === "GET" && url.pathname === "/api/wiki/raw-documents") {
    const wiki = await getWikiBootstrap(context.paths)
    return json(response, {
      documents: wiki.state.rawDocuments,
      state: wiki.state,
    })
  }

  if (request.method === "POST" && url.pathname === "/api/wiki/raw-documents") {
    const body = (await readBody(request)) as {
      content?: string
      dataUrl?: string
      fileName?: string
      mimeType?: string
      source?: "agent" | "user"
      title?: string
      wikiSpaceId?: string
    }
    const data = body.dataUrl ? dataUrlToBuffer(body.dataUrl) : null
    const result = await addWikiRawDocument({
      projectDir: context.paths.projectDir,
      storageDir: context.paths.storageDir,
      contextId: context.paths.contextId,
      fileName: body.fileName || "document.md",
      mimeType: body.mimeType || data?.mimeType,
      title: body.title,
      source: body.source ?? "user",
      wikiSpaceId: body.wikiSpaceId,
      content: data?.buffer ?? body.content ?? "",
    })
    return json(response, result)
  }

  const rawMarkdownMatch = url.pathname.match(
    /^\/api\/wiki\/raw-documents\/([^/]+)\/markdown$/
  )
  if (rawMarkdownMatch) {
    const documentId = decodeURIComponent(rawMarkdownMatch[1])
    if (request.method === "GET") {
      return json(
        response,
        await readWikiMarkdown({ ...context.paths, documentId })
      )
    }
    if (request.method === "PUT") {
      const body = (await readBody(request)) as {
        content?: string
        expectedVersion?: number
        taskId?: string
      }
      return json(
        response,
        await writeWikiMarkdown({
          ...context.paths,
          documentId,
          content: body.content ?? "",
          expectedVersion: body.expectedVersion,
          taskId: body.taskId,
        })
      )
    }
  }

  const rawOriginalMatch = url.pathname.match(
    /^\/api\/wiki\/raw-documents\/([^/]+)\/original$/
  )
  if (rawOriginalMatch && request.method === "GET") {
    const documentId = decodeURIComponent(rawOriginalMatch[1])
    const original = await readWikiRawDocumentOriginal({
      ...context.paths,
      documentId,
    })
    response.statusCode = 200
    response.setHeader("content-type", original.mimeType)
    response.setHeader("content-length", String(original.sizeBytes))
    response.setHeader(
      "content-disposition",
      contentDispositionInline(original.document.originalFileName)
    )
    const stream = createReadStream(original.filePath)
    stream.once("error", (error) => response.destroy(error))
    stream.pipe(response)
    return
  }

  const rawRevealMatch = url.pathname.match(
    /^\/api\/wiki\/raw-documents\/([^/]+)\/reveal$/
  )
  if (rawRevealMatch && request.method === "POST") {
    const documentId = decodeURIComponent(rawRevealMatch[1])
    return json(
      response,
      await revealWikiRawDocumentOriginal({ ...context.paths, documentId })
    )
  }

  const rawActionMatch = url.pathname.match(
    /^\/api\/wiki\/raw-documents\/([^/]+)\/(extract|reindex)$/
  )
  if (rawActionMatch && request.method === "POST") {
    const documentId = decodeURIComponent(rawActionMatch[1])
    const action = rawActionMatch[2]
    const wikiSpaceId = url.searchParams.get("wikiSpaceId")
    if (action === "extract") {
      return json(
        response,
        await extractWikiRawDocumentMarkdown({
          ...context.paths,
          documentId,
          wikiSpaceId,
        })
      )
    }
    return json(
      response,
      await reindexWikiRawDocument({
        ...context.paths,
        documentId,
        wikiSpaceId,
      })
    )
  }

  const rawDocumentMatch = url.pathname.match(
    /^\/api\/wiki\/raw-documents\/([^/]+)$/
  )
  if (rawDocumentMatch && request.method === "DELETE") {
    const documentId = decodeURIComponent(rawDocumentMatch[1])
    return json(
      response,
      await deleteWikiRawDocument({
        ...context.paths,
        documentId,
        wikiSpaceId: url.searchParams.get("wikiSpaceId"),
      })
    )
  }

  if (request.method === "GET" && url.pathname === "/api/wiki/tasks") {
    return json(
      response,
      await listWikiTasks({
        ...context.paths,
        status: wikiTaskStatusParam(url.searchParams.get("status")),
      })
    )
  }

  if (request.method === "GET" && url.pathname === "/api/wiki/tasks/next") {
    return json(response, { task: await nextWikiTask(context.paths) })
  }

  const taskActionMatch = url.pathname.match(
    /^\/api\/wiki\/tasks\/([^/]+)\/(claim|complete|fail)$/
  )
  if (taskActionMatch && request.method === "POST") {
    const taskId = decodeURIComponent(taskActionMatch[1])
    const action = taskActionMatch[2]
    const body = (await readBody(request)) as {
      agentHost?: string
      error?: string
      result?: Record<string, unknown>
      threadId?: string
    }
    if (action === "claim") {
      return json(
        response,
        await claimWikiTask({
          ...context.paths,
          taskId,
          agentHost: body.agentHost,
          threadId: body.threadId,
        })
      )
    }
    if (action === "complete") {
      return json(
        response,
        await completeWikiTask({
          ...context.paths,
          taskId,
          result: body.result,
        })
      )
    }
    return json(
      response,
      await failWikiTask({
        ...context.paths,
        taskId,
        error: body.error || "Wiki task failed",
      })
    )
  }

  if (request.method === "GET" && url.pathname === "/api/wiki/agent-targets") {
    return json(response, await listWikiAgentTargets(context.paths))
  }

  if (request.method === "POST" && url.pathname === "/api/wiki/agent-targets") {
    const body = (await readBody(request)) as {
      host?: string
      threadId?: string
      wakeUrl?: string | null
    }
    return json(
      response,
      await registerWikiAgentTarget({
        ...context.paths,
        host: body.host || "unknown",
        threadId: body.threadId || context.paths.contextId,
        wakeUrl: body.wakeUrl,
      })
    )
  }

  if (url.pathname === "/api/wiki/active-space") {
    if (request.method === "GET") {
      const wiki = await getWikiBootstrap(context.paths)
      return json(response, {
        wikiSpaceId: wiki.state.activeWikiSpaceId,
        wikiSpace: wiki.state.wikiSpaces.find(
          (space) => space.id === wiki.state.activeWikiSpaceId
        ),
      })
    }
    if (request.method === "PUT") {
      const body = (await readBody(request)) as { wikiSpaceId?: string }
      if (!body.wikiSpaceId) throw new Error("Missing wikiSpaceId")
      return json(
        response,
        await setActiveWikiSpace({
          ...context.paths,
          wikiSpaceId: body.wikiSpaceId,
        })
      )
    }
  }

  if (url.pathname === "/api/wiki/language") {
    if (request.method === "GET") {
      const wiki = await getWikiBootstrap(context.paths)
      return json(response, { language: wiki.state.wikiLanguage })
    }
    if (request.method === "PUT" || request.method === "POST") {
      const body = (await readBody(request)) as { language?: string }
      if (!(body.language === "en" || body.language === "zh-CN")) {
        throw new Error("Expected language to be one of: en, zh-CN")
      }
      return json(
        response,
        await setWikiLanguage({
          ...context.paths,
          language: body.language,
        })
      )
    }
  }

  if (request.method === "GET" && url.pathname === "/api/wiki/spaces") {
    const wiki = await getWikiBootstrap(context.paths)
    return json(response, { spaces: wiki.state.wikiSpaces, state: wiki.state })
  }

  const reindexMatch = url.pathname.match(
    /^\/api\/wiki\/spaces\/([^/]+)\/reindex$/
  )
  if (reindexMatch && request.method === "POST") {
    return json(
      response,
      await reindexWikiSpace({
        ...context.paths,
        wikiSpaceId: decodeURIComponent(reindexMatch[1]),
      })
    )
  }

  const pagesMatch = url.pathname.match(
    /^\/api\/wiki\/spaces\/([^/]+)\/pages(?:\/(.*))?$/
  )
  if (pagesMatch) {
    const wikiSpaceId = decodeURIComponent(pagesMatch[1])
    const pagePath = pagesMatch[2]
      ? pagesMatch[2].split("/").map(decodeURIComponent).join("/")
      : null
    if (request.method === "GET" && pagePath) {
      return json(
        response,
        await readWikiPage({ ...context.paths, wikiSpaceId, pagePath })
      )
    }
    if (request.method === "GET") {
      const wiki = await getWikiBootstrap(context.paths)
      const space = wiki.state.wikiSpaces.find(
        (item) => item.id === wikiSpaceId
      )
      return json(response, { pages: space?.pageIndex ?? [] })
    }
    if ((request.method === "POST" || request.method === "PUT") && pagePath) {
      const body = (await readBody(request)) as {
        content?: string
        expectedUpdatedAt?: string
        title?: string
      }
      return json(
        response,
        await writeWikiPage({
          ...context.paths,
          wikiSpaceId,
          pagePath,
          content: body.content ?? "",
          expectedUpdatedAt: body.expectedUpdatedAt,
          title: body.title,
        })
      )
    }
    if (request.method === "POST") {
      const body = (await readBody(request)) as {
        content?: string
        pagePath?: string
        title?: string
      }
      if (!body.pagePath) throw new Error("Missing pagePath")
      return json(
        response,
        await writeWikiPage({
          ...context.paths,
          wikiSpaceId,
          pagePath: body.pagePath,
          content: body.content ?? "",
          title: body.title,
        })
      )
    }
  }

  response.statusCode = 404
  response.end("Not found")
}

export function createOpenPanelsApiMiddleware(
  projectDir: string,
  options: OpenPanelsLocalContextOptions = {}
) {
  const context = createOpenPanelsLocalContext(projectDir, options)
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
  const rawBody = Buffer.concat(chunks).toString("utf8")
  if (!rawBody.trim()) return {}
  return JSON.parse(rawBody)
}

function json(response: ServerResponse, data: unknown): void {
  response.statusCode = 200
  response.setHeader("content-type", "application/json")
  response.end(JSON.stringify(data, jsonReplacer))
}

function contentDispositionInline(fileName: string): string {
  const fallback = basename(fileName).replace(/[^\w.-]+/g, "_") || "document"
  return `inline; filename="${fallback.replaceAll('"', "_")}"; filename*=UTF-8''${encodeURIComponent(
    basename(fileName)
  )}`
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

function panelKindParam(
  value: string | null | undefined
): OpenPanelsPanelKind | null {
  if (
    value === "wiki" ||
    value === "canvas" ||
    value === "image" ||
    value === "diff" ||
    value === "preview" ||
    value === "files"
  ) {
    return value
  }
  return null
}

function wikiTaskStatusParam(
  value: string | null | undefined
): WikiTaskStatus | undefined {
  if (
    value === "queued" ||
    value === "claimed" ||
    value === "running" ||
    value === "failed" ||
    value === "succeeded" ||
    value === "stale"
  ) {
    return value
  }
  return undefined
}
