import type { CanvasSelectionSnapshot, StoreSnapshot } from "../canvas"
import type { OpenPanelsPanelKind, OpenPanelsSession } from "../protocol"
import type {
  AppState,
  BootstrapResponse,
  OpenPanelsTransport,
  OpenPanelsUpdateStatus,
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
    panel: activePanel,
    panels: normalizedPanels,
    pendingTaskCount: data.pendingTaskCount ?? 0,
    state: activeState,
    tasks: data.tasks ?? [],
  }
}

export function normalizePanelState(
  kind: OpenPanelsPanelKind,
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
    schemaVersion: 2,
    rawDocuments: [],
    ruleSets: [],
    wikiSpaces: [],
    activeRawDocumentId: null,
    activeWikiSpaceId: null,
    activeWikiPagePath: null,
    tasks: [],
  }
}

export function isWikiState(state: unknown): state is WikiState {
  return (
    typeof state === "object" &&
    state !== null &&
    (state as { schemaVersion?: unknown }).schemaVersion === 2 &&
    Array.isArray((state as { rawDocuments?: unknown }).rawDocuments) &&
    Array.isArray((state as { wikiSpaces?: unknown }).wikiSpaces) &&
    Array.isArray((state as { tasks?: unknown }).tasks)
  )
}

export function serializeBootstrapForCompare(appState: AppState): string {
  return JSON.stringify({
    activePanelId: appState.activePanelId,
    activePanelKind: appState.activePanelKind,
    panelIds: appState.panels.map(({ panel }) => panel.id),
    pendingTaskCount: appState.pendingTaskCount ?? 0,
    session: appState.session,
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

export async function apiJson<T>(
  apiBase: string,
  path: string | URL,
  init?: RequestInit
): Promise<T> {
  const response = await apiFetch(apiBase, path, init)
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`)
  }
  return (await response.json()) as T
}

export function isNotFoundError(error: unknown): boolean {
  return error instanceof Error && error.message === "HTTP 404"
}

export async function loadBootstrap(
  transport: OpenPanelsTransport,
  sessionId?: string | null
): Promise<BootstrapResponse> {
  const url = apiUrl(transport.apiBase, "/api/bootstrap")
  if (sessionId) {
    url.searchParams.set("sessionId", sessionId)
  }
  const response = await apiFetch(transport.apiBase, url)
  return (await response.json()) as BootstrapResponse
}

export function fetchUpdateStatus(
  transport: OpenPanelsTransport
): Promise<OpenPanelsUpdateStatus> {
  return apiJson(transport.apiBase, "/api/update/status")
}

export function requestUpdateDownload(
  transport: OpenPanelsTransport
): Promise<OpenPanelsUpdateStatus> {
  return apiJson(transport.apiBase, "/api/update/download", {
    method: "POST",
  })
}

export function requestUpdateInstallRestart(
  transport: OpenPanelsTransport
): Promise<{ ok: true; restarting: true }> {
  return apiJson(transport.apiBase, "/api/update/install-restart", {
    method: "POST",
  })
}

export async function savePanelState(
  transport: OpenPanelsTransport,
  sessionId: string,
  panelId: string,
  snapshot: StoreSnapshot,
  baseRevision: number
): Promise<{ revision: number }> {
  const response = await apiFetch(
    transport.apiBase,
    `/api/panels/${encodeURIComponent(sessionId)}/${encodeURIComponent(panelId)}/state`,
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
  transport: OpenPanelsTransport,
  sessionId: string,
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
    `/api/panels/${encodeURIComponent(sessionId)}/${encodeURIComponent(panelId)}/selection`,
    {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(payload),
    }
  )
}

export async function fetchTraceSnapshot(
  transport: OpenPanelsTransport,
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

export async function fetchSessions(transport: OpenPanelsTransport) {
  const response = await apiFetch(transport.apiBase, "/api/sessions")
  return (await response.json()) as OpenPanelsSession[]
}

export async function fetchActiveSessionId(transport: OpenPanelsTransport) {
  const response = await apiFetch(transport.apiBase, "/api/active-session")
  const data = (await response.json()) as { sessionId?: string | null }
  return data.sessionId ?? null
}
