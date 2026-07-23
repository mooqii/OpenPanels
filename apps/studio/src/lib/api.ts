import type { CanvasSelectionSnapshot, StoreSnapshot } from "../canvas"
import type { MyOpenPanelsPanelKind, MyOpenPanelsProject } from "../protocol"
import type {
  AppState,
  BootstrapResponse,
  MyDocument,
  MyOpenPanelsHealth,
  MyOpenPanelsTransport,
  MyOpenPanelsUpdateInstallRestartResponse,
  MyOpenPanelsUpdateStatus,
  OriginalPreviewKind,
  PublishingState,
  TraceEvent,
  TraceSnapshotResponse,
  TypesettingState,
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
  if (kind === "writing") {
    return normalizeWritingState(state)
  }
  if (kind === "typesetting") {
    return isTypesettingState(state) ? state : emptyTypesettingState()
  }
  if (kind === "publishing") {
    return isPublishingState(state) ? state : emptyPublishingState()
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

export function writingStateFromAppState(
  appState: AppState
): import("../types").WritingState {
  const state = appState.panels.find(
    ({ panel }) => panel.kind === "writing"
  )?.state
  return normalizeWritingState(state)
}

export function typesettingStateFromAppState(
  appState: AppState
): TypesettingState {
  const state = appState.panels.find(
    ({ panel }) => panel.kind === "typesetting"
  )?.state
  return isTypesettingState(state) ? state : emptyTypesettingState()
}

export function typesettingRevisionFromAppState(appState: AppState): number {
  return (
    appState.panels.find(({ panel }) => panel.kind === "typesetting")
      ?.revision ?? 0
  )
}

export function publishingStateFromAppState(
  appState: AppState
): PublishingState {
  const state = appState.panels.find(
    ({ panel }) => panel.kind === "publishing"
  )?.state
  return normalizePublishingState(state)
}

export function publishingRevisionFromAppState(appState: AppState): number {
  return (
    appState.panels.find(({ panel }) => panel.kind === "publishing")
      ?.revision ?? 0
  )
}

export function emptyTypesettingState(): TypesettingState {
  return {
    publications: [],
  }
}

export function emptyPublishingState(): PublishingState {
  return {
    releases: [],
    selectedPublicationId: null,
    selectedSkillIds: { xiaohongshu: "release-xiaohongshu" },
  }
}

export function isPublishingState(state: unknown): state is PublishingState {
  return (
    typeof state === "object" &&
    state !== null &&
    Array.isArray((state as { releases?: unknown }).releases) &&
    typeof (state as PublishingState).selectedSkillIds?.xiaohongshu === "string"
  )
}

function normalizePublishingState(state: unknown): PublishingState {
  if (isPublishingState(state)) return state
  return emptyPublishingState()
}

export function isTypesettingState(state: unknown): state is TypesettingState {
  return (
    typeof state === "object" &&
    state !== null &&
    Array.isArray((state as { publications?: unknown }).publications) &&
    (state as { publications: unknown[] }).publications.every(
      isTypesettingPublication
    )
  )
}

function isTypesettingPublication(value: unknown): boolean {
  if (!(typeof value === "object" && value !== null)) return false
  const publication = value as Record<string, unknown>
  return (
    typeof publication.id === "string" &&
    typeof publication.title === "string" &&
    typeof publication.createdAt === "string" &&
    typeof publication.updatedAt === "string" &&
    isTypesettingJsonContent(publication.content) &&
    Array.isArray(publication.covers) &&
    publication.covers.every(isTypesettingPublicationImage) &&
    (publication.titles === undefined ||
      (Array.isArray(publication.titles) &&
        publication.titles.length > 0 &&
        publication.titles.every(isTypesettingPublicationTitle))) &&
    (publication.selectedTitleId === undefined ||
      publication.selectedTitleId === null ||
      (typeof publication.selectedTitleId === "string" &&
        (publication.titles === undefined ||
          publication.titles.some(
            (title) =>
              typeof title === "object" &&
              title !== null &&
              title.id === publication.selectedTitleId
          )))) &&
    (publication.tags === undefined ||
      (Array.isArray(publication.tags) &&
        publication.tags.every((tag) => typeof tag === "string")))
  )
}

function isTypesettingPublicationTitle(value: unknown): boolean {
  if (!(typeof value === "object" && value !== null)) return false
  const title = value as Record<string, unknown>
  const source = title.source
  return (
    typeof title.id === "string" &&
    typeof title.value === "string" &&
    (source === undefined ||
      (isPlainObject(source) &&
        source.kind === "generated" &&
        typeof source.skillId === "string" &&
        typeof source.taskId === "string"))
  )
}

function isTypesettingJsonContent(value: unknown): boolean {
  if (!isTypesettingJsonNode(value)) return false
  const content = value as Record<string, unknown>
  return content.type === "doc"
}

function isTypesettingJsonNode(value: unknown): boolean {
  if (!(typeof value === "object" && value !== null && !Array.isArray(value))) {
    return false
  }
  const node = value as Record<string, unknown>
  return (
    typeof node.type === "string" &&
    (node.text === undefined || typeof node.text === "string") &&
    (node.attrs === undefined || isPlainObject(node.attrs)) &&
    (node.content === undefined ||
      (Array.isArray(node.content) &&
        node.content.every(isTypesettingJsonNode))) &&
    (node.marks === undefined ||
      (Array.isArray(node.marks) &&
        node.marks.every(
          (mark) =>
            isPlainObject(mark) &&
            typeof mark.type === "string" &&
            (mark.attrs === undefined || isPlainObject(mark.attrs))
        )))
  )
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}

function isTypesettingPublicationImage(value: unknown): boolean {
  if (!(typeof value === "object" && value !== null)) return false
  const image = value as Record<string, unknown>
  const source = isPlainObject(image.source) ? image.source : null
  const sourceValid =
    source?.kind === "canvas"
      ? typeof source.assetRef === "string" &&
        typeof source.projectId === "string" &&
        typeof source.panelId === "string"
      : source?.kind === "generated"
        ? typeof source.taskId === "string" &&
          typeof source.skillId === "string"
        : source?.kind === "upload"
  return (
    typeof image.assetRef === "string" &&
    typeof image.src === "string" &&
    image.src.startsWith("/") &&
    typeof image.fileName === "string" &&
    typeof image.mimeType === "string" &&
    sourceValid &&
    (image.width === undefined ||
      image.width === null ||
      typeof image.width === "number") &&
    (image.height === undefined ||
      image.height === null ||
      typeof image.height === "number")
  )
}

export function emptyWritingState(): import("../types").WritingState {
  return {
    createDraft: "",
    draft: "",
    mode: "create",
    distillationName: "",
    revisionDraft: "",
    selectedCreateWritingSkillIds: ["writing-default"],
    selectedDistillationSkillId: "writing-distillation-default",
    selectedRevisionWritingSkillId: "writing-default",
    targetMyDocumentId: null,
  }
}

export function normalizeWritingState(
  state: unknown
): import("../types").WritingState {
  if (typeof state !== "object" || state === null) {
    return emptyWritingState()
  }
  const legacy = state as Record<string, unknown>
  const selectedDistillationSkillId =
    legacy.selectedDistillationSkillId ?? legacy.selectedRefinementSkillId
  const normalized = {
    ...legacy,
    distillationName: legacy.distillationName ?? legacy.refinementName,
    mode: legacy.mode === "refine" ? "distill" : legacy.mode,
    selectedDistillationSkillId:
      selectedDistillationSkillId === "writing-refinement-default"
        ? "writing-distillation-default"
        : selectedDistillationSkillId,
  }
  return isWritingState(normalized) ? normalized : emptyWritingState()
}

export function isWritingState(
  state: unknown
): state is import("../types").WritingState {
  return (
    typeof state === "object" &&
    state !== null &&
    typeof (state as { createDraft?: unknown }).createDraft === "string" &&
    typeof (state as { draft?: unknown }).draft === "string" &&
    typeof (state as { distillationName?: unknown }).distillationName ===
      "string" &&
    typeof (state as { revisionDraft?: unknown }).revisionDraft === "string" &&
    Array.isArray(
      (state as { selectedCreateWritingSkillIds?: unknown })
        .selectedCreateWritingSkillIds
    ) &&
    (
      state as { selectedCreateWritingSkillIds: unknown[] }
    ).selectedCreateWritingSkillIds.every((id) => typeof id === "string") &&
    typeof (state as { selectedDistillationSkillId?: unknown })
      .selectedDistillationSkillId === "string" &&
    ((state as { selectedRevisionWritingSkillId?: unknown })
      .selectedRevisionWritingSkillId === null ||
      typeof (state as { selectedRevisionWritingSkillId?: unknown })
        .selectedRevisionWritingSkillId === "string") &&
    ((state as { mode?: unknown }).mode === "create" ||
      (state as { mode?: unknown }).mode === "revise" ||
      (state as { mode?: unknown }).mode === "distill")
  )
}

export function emptyWikiState(): WikiState {
  return {
    rawDocuments: [],
    myDocuments: [],
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
    Array.isArray((state as { rawDocuments?: unknown }).rawDocuments) &&
    Array.isArray((state as { myDocuments?: unknown }).myDocuments) &&
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

export function myDocumentOriginalUrl(
  apiBase: string,
  document: Pick<MyDocument, "id">
): string {
  return apiUrl(
    apiBase,
    `/api/my-documents/${encodeURIComponent(document.id)}/original`
  ).toString()
}

export function originalPreviewKind(document: {
  mimeType: string
  originalFileName: string
}): OriginalPreviewKind | null {
  const mimeType = (document.mimeType ?? "").toLowerCase()
  const extension = extensionFromFileName(document.originalFileName ?? "")
  if (
    mimeType.startsWith("image/") ||
    [
      ".avif",
      ".bmp",
      ".gif",
      ".ico",
      ".jpeg",
      ".jpg",
      ".png",
      ".svg",
      ".tif",
      ".tiff",
      ".webp",
    ].includes(extension)
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
  if (
    mimeType.startsWith("text/") ||
    [
      "application/javascript",
      "application/json",
      "application/ld+json",
      "application/sql",
      "application/toml",
      "application/x-httpd-php",
      "application/x-javascript",
      "application/x-sh",
      "application/x-yaml",
      "application/xml",
      "application/yaml",
    ].includes(mimeType.split(";", 1)[0] ?? "") ||
    [
      ".c",
      ".conf",
      ".cpp",
      ".css",
      ".csv",
      ".go",
      ".h",
      ".hpp",
      ".htm",
      ".html",
      ".ini",
      ".java",
      ".js",
      ".json",
      ".jsonl",
      ".jsx",
      ".kt",
      ".kts",
      ".log",
      ".markdown",
      ".md",
      ".mdx",
      ".php",
      ".py",
      ".rb",
      ".rs",
      ".sh",
      ".sql",
      ".svelte",
      ".swift",
      ".tex",
      ".toml",
      ".ts",
      ".tsv",
      ".tsx",
      ".txt",
      ".vue",
      ".xml",
      ".yaml",
      ".yml",
    ].includes(extension)
  ) {
    return "text"
  }
  return null
}

type BrowserWindowOpener = (url: string, target: string) => Window | null

export function tryOpenBrowserWindow(
  url: string,
  openWindow: BrowserWindowOpener = (nextUrl, target) =>
    window.open(nextUrl, target)
): boolean {
  try {
    const openedWindow = openWindow(url, "_blank")
    if (!openedWindow) return false
    try {
      openedWindow.opener = null
    } catch {
      // The browser window is already open; opener isolation can be restricted.
    }
    return true
  } catch {
    return false
  }
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

export async function saveCanvasPanelState(
  transport: MyOpenPanelsTransport,
  projectId: string,
  panelId: string,
  snapshot: StoreSnapshot,
  baseRevision: number
): Promise<{ revision: number }> {
  return savePanelState(
    transport,
    projectId,
    panelId,
    serializeSnapshot(snapshot),
    baseRevision
  )
}

export async function savePanelState(
  transport: MyOpenPanelsTransport,
  projectId: string,
  panelId: string,
  state: unknown,
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
        state,
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

export async function fetchSelectionState(
  transport: MyOpenPanelsTransport,
  projectId: string,
  panelId: string
): Promise<{ revision: number; selection: CanvasSelectionSnapshot }> {
  const response = await apiFetch(
    transport.apiBase,
    `/api/projects/${encodeURIComponent(projectId)}/panels/${encodeURIComponent(panelId)}/selection`
  )
  if (!response.ok) throw new Error(`HTTP ${response.status}`)
  const payload = (await response.json()) as {
    revision?: number
    selection?: Partial<CanvasSelectionSnapshot> | null
  }
  return {
    revision: payload.revision ?? 0,
    selection: {
      assetRef: payload.selection?.assetRef ?? null,
      selectedShapeIds: payload.selection?.selectedShapeIds ?? [],
      selectedShapes: payload.selection?.selectedShapes ?? [],
    },
  }
}

export interface SelectionMaterializationRequest {
  requestId: string
  selectedShapeIds: string[]
}

export async function fetchSelectionMaterializationRequest(
  transport: MyOpenPanelsTransport,
  projectId: string,
  panelId: string
): Promise<SelectionMaterializationRequest | null> {
  const response = await apiFetch(
    transport.apiBase,
    `/api/projects/${encodeURIComponent(projectId)}/panels/${encodeURIComponent(panelId)}/selection-materializations`
  )
  if (!response.ok) throw new Error(`HTTP ${response.status}`)
  const payload = (await response.json()) as {
    request?: SelectionMaterializationRequest | null
  }
  return payload.request ?? null
}

export async function completeSelectionMaterialization(
  transport: MyOpenPanelsTransport,
  projectId: string,
  panelId: string,
  requestId: string,
  imageDataUrl: string
) {
  const response = await apiFetch(
    transport.apiBase,
    `/api/projects/${encodeURIComponent(projectId)}/panels/${encodeURIComponent(panelId)}/selection-materializations/${encodeURIComponent(requestId)}`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ imageDataUrl }),
    }
  )
  if (!response.ok) throw new Error(`HTTP ${response.status}`)
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
