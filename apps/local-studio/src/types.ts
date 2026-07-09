import type { OpenPanelsLocale } from "./canvas"
import type {
  OpenPanelsPanel,
  OpenPanelsPanelKind,
  OpenPanelsSession,
} from "./protocol"

export interface BootstrapResponse {
  activePanelId: string
  activePanelKind: OpenPanelsPanelKind
  agentWorker?: AgentWorkerStatus
  buildInfo?: OpenPanelsBuildInfo
  panel: OpenPanelsPanel
  panels: PanelStateSnapshot[]
  pendingTaskCount?: number
  revision: number
  session: OpenPanelsSession
  sessions?: OpenPanelsSession[]
  state: unknown
  tasks?: ProjectTask[]
}

export interface AppState extends BootstrapResponse {}

export interface AgentWorkerStatus {
  currentTask?: ProjectTask | null
  heartbeatAt?: string | null
  lastError?: string | null
  lastTask?: unknown
  status: "idle" | "running" | "error" | string
  updatedAt?: string | null
}

export interface OpenPanelsBuildInfo {
  buildTime?: string
  channel: "development" | "release"
  label: string
  version: string
}

export interface OpenPanelsUpdateStatus {
  assetAvailable?: boolean
  cached?: boolean
  checkedAtUnix?: number
  currentVersion: string
  downloaded?: boolean
  latestVersion?: string | null
  readyToInstall?: boolean
  target?: string
  updateAvailable: boolean
}

export type TraceCategory =
  | "agent"
  | "api"
  | "cli"
  | "error"
  | "system"
  | "task"

export interface TraceEvent {
  category: TraceCategory
  detail?: unknown
  direction?: string | null
  id: string
  releaseSummary?: string | null
  runId?: string | null
  seq: number
  source?: string | null
  summary: string
  taskId?: string | null
  timestamp: string
}

export interface TraceSnapshotResponse {
  events: TraceEvent[]
  nextSeq: number
}

export interface ProjectTask {
  attempt?: number
  blockedReason?: "attemptsExceeded" | "leased" | "retryLater" | string | null
  capability?: string
  createdAt: string
  error?: unknown
  id: string
  input?: unknown
  lease?: {
    expiresAt?: string | null
    heartbeatAt?: string | null
    owner?: string | null
  }
  maxAttempts?: number
  nextRunAt?: string | null
  panelId: string
  panelKind: OpenPanelsPanelKind | string
  queue: string
  ready?: boolean
  result?: unknown
  retryAfter?: string | null
  sessionId: string
  source?: unknown
  status: string
  targetId: string
  task?: unknown
  type: string
  updatedAt: string
}

export interface PanelStateSnapshot {
  panel: OpenPanelsPanel
  revision: number
  state: unknown
}

export interface WikiState {
  activeRawDocumentId: string | null
  activeWikiPagePath: string | null
  activeWikiSpaceId: string | null
  rawDocuments: WikiRawDocument[]
  ruleSets: unknown[]
  schemaVersion: 2
  tasks: WikiTask[]
  wikiLanguage?: OpenPanelsLocale | null
  wikiSpaces: WikiSpace[]
}

export interface WikiRawDocument {
  conversion: {
    error: string | null
    status: "failed" | "not_required" | "queued" | "converting" | "ready"
    taskId: string | null
    updatedAt: string
  }
  createdAt: string
  id: string
  ingestionByWikiSpace: Record<
    string,
    {
      error: string | null
      markdownVersion?: number
      status: string
      taskId: string | null
      updatedAt?: string
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

export interface WikiSpace {
  id: string
  pageIndex: WikiPageIndexItem[]
  title: string
}

export interface WikiPageIndexItem {
  path: string
  summary: string
  title: string
  type: string
  updatedAt: string
}

export interface WikiTask {
  error: string | null
  id: string
  status: string
  targetId: string
  type: string
  wikiSpaceId: string | null
}

export type OriginalPreviewKind = "audio" | "image" | "pdf" | "video"

export type OpenPanelsTransport = {
  apiBase: string
  kind: "http"
}
