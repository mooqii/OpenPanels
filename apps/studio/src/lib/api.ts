import type { CanvasSelectionSnapshot, StoreSnapshot } from "../canvas"
import type { MyOpenPanelsPanelKind, MyOpenPanelsProject } from "../protocol"
import type {
  AppState,
  BootstrapResponse,
  MyOpenPanelsHealth,
  MyOpenPanelsTransport,
  MyOpenPanelsUpdateInstallRestartResponse,
  MyOpenPanelsUpdateStatus,
  OriginalPreviewKind,
  TraceEvent,
  TraceSnapshotResponse,
  WikiRawDocument,
  WikiState,
} from "../types"

export function normalizeBootstrap(data: BootstrapResponse): AppState {
  const panels =
    data.panels?.map((snapshot) => ({
      panel: snapshot.panel,
      revision: snapshot.revision ?? 0,
      state: normalizePanelState(snapshot.panel.kind, snapshot.state),
    })) ?? []
  const activePanel =
    panels.find(({ panel }) => panel.id === data.activePanelId)?.panel ??
    data.panel
  const activeState = normalizePanelState(activePanel.kind, data.state)
  const normalizedPanels = panels.some(
    ({ panel }) => panel.id === activePanel.id
  )
    ? panels.map((snapshot) =>
        snapshot.panel.id === activePanel.id
          ? {
              panel: activePanel,
              revision:
                snapshot.revision ??
                (activePanel.id === data.panel.id ? data.revision : 0),
              state: activeState,
            }
          : snapshot
      )
    : [
        {
          panel: activePanel,
          revision: activePanel.id === data.panel.id ? data.revision : 0,
          state: activeState,
        },
        ...panels,
      ]

  return {
    ...data,
    activePanelId: activePanel.id,
    activePanelKind: activePanel.kind,
    agentWorker: data.agentWorker ?? { status: "idle" },
    agentOperations: data.agentOperations ?? [],
    panel: activePanel,
    panels: normalizedPanels,
    pendingTaskCount: data.pendingTaskCount ?? 0,
    state: activeState,
    tasks: data.tasks ?? [],
  }
}

export function normalizePanelState(
  kind: MyOpenPanelsPanelKind,
  state: unknown
): unknown {
  if (kind === "canvas") {
    return normalizeSnapshot(state as StoreSnapshot)
  }
  if (kind === "wiki") {
    return isWikiState(state) ? state : emptyWikiState()
  }
  return state ?? {}
}

export function canvasSnapshotFromState(
  appState: AppState
): StoreSnapshot | null {
  const snapshot = appState.panels.find(
    ({ panel }) => panel.kind === "canvas"
  )?.state
  return snapshot ? normalizeSnapshot(snapshot as StoreSnapshot) : null
}

export function canvasRevisionFromState(appState: AppState): number {
  return (
    appState.panels.find(({ panel }) => panel.kind === "canvas")?.revision ??
    appState.revision ??
    0
  )
}

export function wikiStateFromAppState(appState: AppState): WikiState {
  const state = appState.panels.find(
    ({ panel }) => panel.kind === "wiki"
  )?.state
  return isWikiState(state) ? state : emptyWikiState()
}

export function emptyWikiState(): WikiState {
  return {
    schemaVersion: 4,
    rawDocuments: [],
    generatedDocuments: [],
    ruleSets: [],
    wikiSpaces: [],
    activeRawDocumentId: null,
    activeWikiSpaceId: null,
    activeWikiPagePath: null,
  }
}

export function isWikiState(state: unknown): state is WikiState {
  return (
    typeof state === "object" &&
    state !== null &&
    (state as { schemaVersion?: unknown }).schemaVersion === 4 &&
    Array.isArray((state as { rawDocuments?: unknown }).rawDocuments) &&
    Array.isArray(
      (state as { generatedDocuments?: unknown }).generatedDocuments
    ) &&
    Array.isArray((state as { wikiSpaces?: unknown }).wikiSpaces)
  )
}

export function serializeBootstrapForCompare(appState: AppState): string {
  return JSON.stringify({
    activePanelId: appState.activePanelId,
    activePanelKind: appState.activePanelKind,
    panelIds: appState.panels.map(({ panel }) => panel.id),
    pendingTaskCount: appState.pendingTaskCount ?? 0,
    agentWorker: appState.agentWorker ?? { status: "idle" },
    agentOperations: appState.agentOperations ?? [],
    project: appState.project,
    states: appState.panels.map(({ panel, state }) => ({
      id: panel.id,
      kind: panel.kind,
      state:
        panel.kind === "canvas"
          ? serializeSnapshotForCompare(
              normalizeSnapshot(state as StoreSnapshot)
            )
          : state,
    })),
    tasks: appState.tasks ?? [],
  })
}

export function normalizeSnapshot(snapshot: StoreSnapshot): StoreSnapshot {
  if (!snapshot || typeof snapshot !== "object") {
    return {
      schema: { schemaVersion: 1, recordVersions: {} },
      store: {},
      selectedShapeIds: new Set(),
      currentPageId: null,
      openedGroupId: null,
    } as StoreSnapshot
  }
  const selectedShapeIds = Array.isArray(snapshot.selectedShapeIds)
    ? new Set(snapshot.selectedShapeIds)
    : snapshot.selectedShapeIds instanceof Set
      ? snapshot.selectedShapeIds
      : new Set<string>()
  return {
    ...snapshot,
    selectedShapeIds,
  } as StoreSnapshot
}

export function serializeSnapshot(snapshot: StoreSnapshot) {
  return {
    ...snapshot,
    selectedShapeIds: [...snapshot.selectedShapeIds],
  }
}

export function serializeSnapshotForCompare(snapshot: StoreSnapshot) {
  return {
    ...serializeSnapshot(snapshot),
    selectedShapeIds: [],
  }
}

export function fileToDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(reader.result as string)
    reader.onerror = reject
    reader.readAsDataURL(file)
  })
}

export function titleFromFileName(fileName: string): string {
  const lastSlash = Math.max(
    fileName.lastIndexOf("/"),
    fileName.lastIndexOf("\\")
  )
  const base = lastSlash === -1 ? fileName : fileName.slice(lastSlash + 1)
  const dot = base.lastIndexOf(".")
  return dot > 0 ? base.slice(0, dot) : base
}

export function wikiRawOriginalUrl(
  apiBase: string,
  document: Pick<WikiRawDocument, "id">
): string {
  return apiUrl(
    apiBase,
    `/api/wiki/raw-documents/${encodeURIComponent(document.id)}/original`
  ).toString()
}

export function originalPreviewKind(
  document: Pick<WikiRawDocument, "mimeType" | "originalFileName">
): OriginalPreviewKind | null {
  const mimeType = (document.mimeType ?? "").toLowerCase()
  const extension = extensionFromFileName(document.originalFileName ?? "")
  if (
    mimeType.startsWith("image/") ||
    [".gif", ".jpeg", ".jpg", ".png", ".svg", ".webp"].includes(extension)
  ) {
    return "image"
  }
  if (mimeType === "application/pdf" || extension === ".pdf") {
    return "pdf"
  }
  if (
    mimeType.startsWith("audio/") ||
    [".aac", ".m4a", ".mp3", ".wav"].includes(extension)
  ) {
    return "audio"
  }
  if (
    mimeType.startsWith("video/") ||
    [".avi", ".mov", ".mp4", ".webm"].includes(extension)
  ) {
    return "video"
  }
  return null
}

export function extensionFromFileName(fileName: string): string {
  const base = fileName.split(/[\\/]/).at(-1) ?? fileName
  const dot = base.lastIndexOf(".")
  return dot >= 0 ? base.slice(dot).toLowerCase() : ""
}

export function formatBytes(sizeBytes: number): string {
  if (!Number.isFinite(sizeBytes) || sizeBytes < 0) return ""
  if (sizeBytes < 1024) return `${sizeBytes} B`
  const units = ["KB", "MB", "GB", "TB"]
  let size = sizeBytes / 1024
  let unitIndex = 0
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024
    unitIndex += 1
  }
  return `${size >= 10 ? size.toFixed(0) : size.toFixed(1)} ${units[unitIndex]}`
}

export function clampImageScale(scale: number): number {
  return Math.min(4, Math.max(0.25, scale))
}

export function apiUrl(apiBase: string, path: string | URL): URL {
  if (path instanceof URL) return path
  return new URL(path, normalizedApiBase(apiBase))
}

export function normalizedApiBase(apiBase: string): string {
  return apiBase.endsWith("/") ? apiBase : `${apiBase}/`
}

export function apiFetch(
  apiBase: string,
  path: string | URL,
  init?: RequestInit
): Promise<Response> {
  return fetch(apiUrl(apiBase, path), init)
}

export function apiFetchWithTimeout(
  apiBase: string,
  path: string | URL,
  init?: RequestInit,
  timeoutMs = 30_000
): Promise<Response> {
  const controller = new AbortController()
  const timeout = window.setTimeout(() => controller.abort(), timeoutMs)
  return fetch(apiUrl(apiBase, path), {
    ...init,
    signal: init?.signal ?? controller.signal,
  }).finally(() => window.clearTimeout(timeout))
}

export async function apiJson<T>(
  apiBase: string,
  path: string | URL,
  init?: RequestInit
): Promise<T> {
  const response = await apiFetch(apiBase, path, init)
  if (!response.ok) {
    throw new Error(await apiErrorMessage(response))
  }
  return (await response.json()) as T
}

export async function apiJsonWithTimeout<T>(
  apiBase: string,
  path: string | URL,
  init?: RequestInit,
  timeoutMs?: number
): Promise<T> {
  const response = await apiFetchWithTimeout(apiBase, path, init, timeoutMs)
  if (!response.ok) {
    throw new Error(await apiErrorMessage(response))
  }
  return (await response.json()) as T
}

async function apiErrorMessage(response: Response): Promise<string> {
  try {
    const data = (await response.json()) as { error?: unknown }
    if (typeof data.error === "string" && data.error.trim()) {
      return data.error
    }
  } catch {
    // Fall back to the HTTP status below.
  }
  return `HTTP ${response.status}`
}

export function isNotFoundError(error: unknown): boolean {
  return error instanceof Error && error.message === "HTTP 404"
}

export function fetchStudioHealth(
  transport: MyOpenPanelsTransport,
  options?: { timeoutMs?: number }
): Promise<MyOpenPanelsHealth> {
  return apiJsonWithTimeout(
    transport.apiBase,
    "/api/health",
    undefined,
    options?.timeoutMs ?? 1200
  )
}

export async function loadBootstrap(
  transport: MyOpenPanelsTransport,
  projectId?: string | null
): Promise<BootstrapResponse> {
  const url = apiUrl(transport.apiBase, "/api/bootstrap")
  if (projectId) {
    url.searchParams.set("projectId", projectId)
  }
  const response = await apiFetch(transport.apiBase, url)
  if (!response.ok) {
    throw new Error(await apiErrorMessage(response))
  }
  return (await response.json()) as BootstrapResponse
}

export function fetchUpdateStatus(
  transport: MyOpenPanelsTransport,
  options?: { refresh?: boolean }
): Promise<MyOpenPanelsUpdateStatus> {
  const path = options?.refresh
    ? "/api/update/status?refresh=1"
    : "/api/update/status"
  return apiJson(transport.apiBase, path)
}

export function requestUpdateDownload(
  transport: MyOpenPanelsTransport
): Promise<MyOpenPanelsUpdateStatus> {
  return apiJsonWithTimeout(
    transport.apiBase,
    "/api/update/download",
    {
      method: "POST",
    },
    120_000
  )
}

export function requestUpdateInstallRestart(
  transport: MyOpenPanelsTransport
): Promise<MyOpenPanelsUpdateInstallRestartResponse> {
  return apiJsonWithTimeout(
    transport.apiBase,
    "/api/update/install-restart",
    {
      method: "POST",
    },
    120_000
  )
}

export async function savePanelState(
  transport: MyOpenPanelsTransport,
  projectId: string,
  panelId: string,
  snapshot: StoreSnapshot,
  baseRevision: number
): Promise<{ revision: number }> {
  const response = await apiFetch(
    transport.apiBase,
    `/api/projects/${encodeURIComponent(projectId)}/panels/${encodeURIComponent(panelId)}/state`,
    {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        baseRevision,
        state: serializeSnapshot(snapshot),
      }),
    }
  )
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`)
  }
  return (await response.json()) as { revision: number }
}

export async function saveSelectionState(
  transport: MyOpenPanelsTransport,
  projectId: string,
  panelId: string,
  selection: CanvasSelectionSnapshot
) {
  const payload = {
    imageDataUrl: selection.imageDataUrl,
    selection: {
      assetRef: selection.assetRef,
      selectedShapeIds: selection.selectedShapeIds,
      selectedShapes: selection.selectedShapes,
    },
  }
  await apiFetch(
    transport.apiBase,
    `/api/projects/${encodeURIComponent(projectId)}/panels/${encodeURIComponent(panelId)}/selection`,
    {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(payload),
    }
  )
}

export async function fetchTraceSnapshot(
  transport: MyOpenPanelsTransport,
  audience: "development" | "release"
): Promise<TraceSnapshotResponse> {
  const response = await apiFetch(
    transport.apiBase,
    `/api/trace/snapshot?audience=${encodeURIComponent(audience)}`
  )
  return (await response.json()) as TraceSnapshotResponse
}

export function appendTraceEvent(
  current: TraceEvent[],
  event: TraceEvent
): TraceEvent[] {
  if (current.some((item) => item.id === event.id || item.seq === event.seq)) {
    return current
  }
  return [...current, event].slice(-500)
}

export function formatTraceConnection(
  state: "connecting" | "live" | "paused" | "offline"
): string {
  switch (state) {
    case "live":
      return "live"
    case "paused":
      return "paused"
    case "offline":
      return "offline"
    default:
      return "connecting"
  }
}

export function formatTraceTime(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return "--:--:--"
  return [date.getHours(), date.getMinutes(), date.getSeconds()]
    .map(padDatePart)
    .join(":")
}

function padDatePart(value: number): string {
  return String(value).padStart(2, "0")
}

export async function fetchProjects(transport: MyOpenPanelsTransport) {
  const response = await apiFetch(transport.apiBase, "/api/projects")
  return (await response.json()) as MyOpenPanelsProject[]
}

export async function fetchActiveProjectId(transport: MyOpenPanelsTransport) {
  const response = await apiFetch(transport.apiBase, "/api/active-project")
  const data = (await response.json()) as { projectId?: string | null }
  return data.projectId ?? null
}
