import type {
  MyOpenPanelsPanel,
  MyOpenPanelsPanelKind,
  MyOpenPanelsSession,
} from "./protocol"

export interface BootstrapResponse {
  activePanelId: string
  activePanelKind: MyOpenPanelsPanelKind
  agentOperations?: AgentOperation[]
  agentWorker?: AgentWorkerStatus
  blockedCount?: number
  buildInfo?: MyOpenPanelsBuildInfo
  panel: MyOpenPanelsPanel
  panels: PanelStateSnapshot[]
  pendingTaskCount?: number
  readyCount?: number
  revision: number
  runningCount?: number
  session: MyOpenPanelsSession
  sessions?: MyOpenPanelsSession[]
  state: unknown
  tasks?: ProjectTask[]
  unhandledCount?: number
}

export interface AgentOperation {
  completedAt: string | null
  createdAt: string
  error: { message?: string } | null
  id: string
  intent: string
  panelId: string
  panelKind: MyOpenPanelsPanelKind
  panelTitle?: string
  projectTitle?: string
  result: unknown
  sessionId: string
  status: "active" | "completed" | "failed" | "cancelled"
  updatedAt: string
}

export interface AppState extends BootstrapResponse {}

export interface AgentWorkerStatus {
  currentTask?: ProjectTask | null
  dispatcher?: AgentDispatcherStatus
  heartbeatAt?: string | null
  lastError?: string | null
  lastTask?: unknown
  status: "idle" | "running" | "error" | string
  updatedAt?: string | null
}

export interface AgentDispatcherStatus {
  lastDelivery?: TaskDelivery | null
  onlineTargetCount?: number
  pendingCount?: number
  retryCount?: number
  runningCount?: number
  status: string
  targetCount?: number
  targets?: AgentTarget[]
  unhandledCount?: number
  updatedAt?: string | null
}

export interface AgentTarget {
  capabilities: string[]
  endpoint?: string | null
  host: string
  id: string
  lastError?: string | null
  lastHeartbeatAt?: string | null
  name: string
  priority: number
  status: string
  transport: "webhook" | "poll" | "command" | string
}

export interface TaskDelivery {
  acknowledgedAt?: string | null
  attempts: number
  deliveredAt?: string | null
  id: string
  lastError?: string | null
  nextAttemptAt?: string | null
  status: string
  targetId: string
  taskId: string
  updatedAt: string
}

export interface MyOpenPanelsBuildInfo {
  buildTime?: string
  channel: "development" | "release"
  label: string
  version: string
}

export interface MyOpenPanelsUpdateStatus {
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

export interface MyOpenPanelsHealth {
  contextId: string
  ok: boolean
  version: string
}

export interface MyOpenPanelsUpdateInstallRestartResponse {
  message?: string
  ok: true
  restarting: boolean
  session?: unknown
  update: {
    currentVersion: string
    installedPath?: string | null
    latestVersion?: string | null
    manifestUrl?: string
    target?: string
    updated: boolean
  }
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
  assignedTarget?: AgentTarget | null
  assignedTargetId?: string | null
  attempt?: number
  blockedReason?: "attemptsExceeded" | "leased" | "retryLater" | string | null
  capability?: string
  completedAt?: string | null
  createdAt: string
  dispatchState?:
    | "eligible"
    | "noTarget"
    | "delivering"
    | "running"
    | "retry"
    | "deliveryFailed"
    | "done"
    | string
  error?: unknown
  id: string
  input?: unknown
  lastDelivery?: TaskDelivery | null
  lease?: {
    expiresAt?: string | null
    heartbeatAt?: string | null
    owner?: string | null
  }
  matchedTargetCount?: number
  maxAttempts?: number
  nextRunAt?: string | null
  panelId: string
  panelKind: MyOpenPanelsPanelKind | string
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
  panel: MyOpenPanelsPanel
  revision: number
  state: unknown
}

export interface WikiState {
  activeRawDocumentId: string | null
  activeWikiPagePath: string | null
  activeWikiSpaceId: string | null
  generatedDocuments: WikiGeneratedDocument[]
  rawDocuments: WikiRawDocument[]
  ruleSets: unknown[]
  schemaVersion: 3
  tasks: WikiTask[]
  wikiAgentSkillConfigured?: boolean
  wikiAgentSkillId?: string | null
  wikiSpaces: WikiSpace[]
}

export interface WikiGeneratedDocument {
  contentRef: string
  contentVersion: number
  createdAt: string
  format: "markdown" | "text"
  generation?: {
    error: string | null
    operationId?: string
    status: "generating" | "completed" | "failed"
  }
  id: string
  mimeType: "text/markdown" | "text/plain"
  originalFileName: string
  publishHistory: Array<{
    generatedVersion: number
    publishedAt: string
    rawDocumentId: string
  }>
  taskId: string | null
  threadId: string | null
  title: string
  updatedAt: string
}

export interface AgentSkillListing {
  localDir: string
  localPath: string
  skill: {
    appliesTo: string[]
    description: string
    id: string
    loadWhen: string[]
    requiresCapabilities: string[]
    source: string
    taskTypes: string[]
    title: string
    tokens: string
  }
  source: string
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

export type MyOpenPanelsTransport = {
  apiBase: string
  kind: "http"
}
