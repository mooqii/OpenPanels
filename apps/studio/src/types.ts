import type { JSONContent } from "@tiptap/core"
import type {
  MyOpenPanelsPanel,
  MyOpenPanelsPanelKind,
  MyOpenPanelsProject,
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
  project: MyOpenPanelsProject
  projects?: MyOpenPanelsProject[]
  readyCount?: number
  revision: number
  runningCount?: number
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
  projectId: string
  projectTitle?: string
  result: unknown
  status: "active" | "completed" | "failed" | "cancelled"
  updatedAt: string
}

export interface AppState extends BootstrapResponse {}

export interface AgentWorkerStatus {
  currentTask?: ProjectTask | null
  heartbeatAt?: string | null
  lastError?: string | null
  lastTask?: unknown
  queue?: AgentQueueStatus
  status: "idle" | "running" | "error" | string
  updatedAt?: string | null
}

export interface AgentQueueStatus {
  lifecycleState?: string
  onlineTargetCount?: number
  pendingCount?: number
  runningCount?: number
  status: string
  targetCount?: number
  targets?: AgentTarget[]
  unhandledCount?: number
  updatedAt?: string | null
}

export interface ModelGatewaySettings {
  byok: {
    baseUrl: string | null
    model: string | null
    providerId: string | null
  }
  localCli: {
    enabledProviderIds: string[]
    executablePaths: Record<string, string>
    model: string | null
    providerModels: Record<string, string>
    providerOrder: string[]
    providerId: string | null
    providerReasoning: Record<string, string>
    reasoning: string | null
  }
  maxConcurrency: number
  mode: "localCli" | "byok"
}

export interface LocalCliModelOption {
  id: string
  label: string
  reasoningOptions?: LocalCliModelOption[]
}

export interface LocalCliInfo {
  authMessage?: string | null
  authStatus: "ok" | "missing" | "unknown" | string
  available: boolean
  bin: string
  configuredPath?: string | null
  diagnostic?: string | null
  id: "codex" | "hermes" | string
  models: LocalCliModelOption[]
  modelsSource: "live" | "fallback" | string
  name: string
  path?: string | null
  reasoningOptions: LocalCliModelOption[]
  version?: string | null
}

export interface LocalCliConnectionTestResult {
  detail?: string | null
  kind: string
  latencyMs: number
  ok: boolean
  providerId: string
  providerName: string
  sample?: string
}

export interface AgentTarget {
  capabilities: string[]
  host: string
  id: string
  lastError?: string | null
  lastHeartbeatAt?: string | null
  modelGatewayConnectionId?: string | null
  name: string
  priority: number
  status: string
}

export interface MyOpenPanelsBuildInfo {
  agentCli?: string
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
  archivedAt?: string | null
  attempt?: number
  attemptLimit?: number
  attempts?: Array<{
    attempt: number
    error?: unknown
    failureClass?: string | null
    finishedAt?: string
    generation?: number
    result?: unknown
    runnerKey?: string | null
    status: string
  }>
  availableAt?: string | null
  blockedReason?: "attemptsExceeded" | "leased" | "retryLater" | string | null
  capability?: string
  compatibleTargetCount?: number
  completedAt?: string | null
  createdAt: string
  dependencies?: Array<{
    failurePolicy: string
    prerequisiteTaskId: string
    status: string
    successCondition: string
  }>
  dispatchState?: "eligible" | "noTarget" | "running" | "done" | string
  error?: unknown
  executionGeneration?: number
  executionMethod?: {
    connectionId?: string | null
    kind: "agentTarget" | "localCli" | "manualInstruction" | string
    label?: string | null
    providerId?: string | null
  } | null
  id: string
  input?: unknown
  lease?: {
    expiresAt?: string | null
    heartbeatAt?: string | null
    owner?: string | null
  }
  lifecycleState?: string
  mutationBlocked?: boolean
  mutationKey?: string | null
  mutationSequence?: number | null
  nextRunAt?: string | null
  panelId: string
  panelKind: MyOpenPanelsPanelKind | string
  projectId: string
  queue: string
  ready?: boolean
  result?: unknown
  retryAfter?: string | null
  source?: unknown
  status: string
  targetId: string
  terminalReason?: unknown
  type: string
  updatedAt: string
}

export type TaskExecutionScope =
  | {
      kind: "project-drain"
      projectId: string
    }
  | {
      kind: "exact-task"
      taskId: string
    }
  | {
      kind: "wiki-mutation-drain"
      mutationKey: string
      projectId: string
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
  myDocuments: MyDocument[]
  rawDocuments: WikiRawDocument[]
  ruleSets: unknown[]
  wikiAgentSkillConfigured?: boolean
  wikiAgentSkillId?: string | null
  wikiSpaces: WikiSpace[]
}

export interface WritingState {
  createDraft: string
  distillationName: string
  draft: string
  mode: "create" | "revise" | "distill"
  revisionDraft: string
  selectedCreateWritingSkillIds: string[]
  selectedDistillationSkillId: string
  selectedRevisionWritingSkillId: string | null
  targetMyDocumentId: string | null
}

export interface TypesettingState {
  publications: TypesettingPublication[]
}

export interface PublishingState {
  releases: PublishingRelease[]
  selectedPublicationId: string | null
  selectedSkillIds: {
    xiaohongshu: string
  }
}

export type PublishingOutcome =
  | "published"
  | "needs_user_action"
  | "not_published"
  | "unknown"

export interface PublishingMediaSnapshot {
  assetRef: string
  contentHash: string
  fileName: string
  height?: number | null
  isPrimary: boolean
  mimeType: string
  src: string
  width?: number | null
}

export interface PublishingAttempt {
  completedAt: string | null
  createdAt: string
  id: string
  mode: "auto" | "manual"
  outcome: PublishingOutcome | null
  phase: "queued" | "prepared" | "committing" | "completed"
  publishedAt?: string | null
  reasonCode: string | null
  remoteUrl: string | null
  requestId: string
  skillHash: string
  skillId: string
  skillName: string
  summary: string | null
  taskId: string
  updatedAt?: string
}

export interface PublishingRelease {
  attempts: PublishingAttempt[]
  createdAt: string
  id: string
  platform: "wechat_official_account" | "xiaohongshu"
  snapshot: {
    bodyText: string
    media: PublishingMediaSnapshot[]
    tags?: string[]
    title: string
  }
  sourcePublicationId: string
  sourceUpdatedAt: string | null
  updatedAt: string
}

export interface TypesettingPublication {
  content: JSONContent
  covers: TypesettingPublicationImage[]
  createdAt: string
  id: string
  selectedTitleId?: string | null
  tags?: string[]
  title: string
  titles?: TypesettingPublicationTitle[]
  updatedAt: string
}

export interface TypesettingPublicationTitle {
  id: string
  source?: {
    kind: "generated"
    skillId: string
    taskId: string
  }
  value: string
}

export interface TypesettingPublicationImage {
  assetRef: string
  fileName: string
  height?: number | null
  mimeType: string
  source:
    | {
        assetRef: string
        kind: "canvas"
        panelId: string
        projectId: string
      }
    | {
        kind: "generated"
        skillId: string
        taskId: string
      }
    | {
        kind: "upload"
      }
  src: string
  width?: number | null
}

export interface TypesettingCanvasAsset {
  assetId: string
  assetRef: string
  canvasPanelId: string
  height?: number
  id: string
  mimeType: string
  name: string
  projectId: string
  projectTitle: string
  src: string
  width?: number
}

export interface MyDocument {
  contentRef: string
  contentVersion: number
  conversion?: {
    error: string | null
    status:
      | "cancelled"
      | "failed"
      | "not_required"
      | "queued"
      | "converting"
      | "ready"
    taskId: string | null
    updatedAt: string
  }
  createdAt: string
  format: "markdown" | "text"
  id: string
  importSource?: {
    fileName: string
    mimeType: string
    originalRef: string
    sha256: string
    sizeBytes: number
  }
  mimeType: "text/markdown" | "text/plain"
  originalFileName: string
  publishHistory: Array<{
    documentVersion: number
    publishedAt: string
    rawDocumentId: string
  }>
  taskId: string | null
  threadId: string | null
  title: string
  updatedAt: string
  wordCount?: number | null
  writeOperation?: {
    error: string | null
    operationId?: string
    status: "writing" | "completed" | "failed"
  }
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
    name: string
    tokens: string
  }
  source: string
}

export type ManagedSkillKind = "system" | "preset" | "custom"

export interface ManagedSkillProvenance {
  revision?: string
  sourceLocator: string
  sourceType: "github" | "skills-sh" | "clawhub" | "skillhub" | "device"
  subpath?: string
}

export interface ManagedProjectSkill {
  canCheckUpdates: boolean
  canDelete: boolean
  canEdit: boolean
  description: string
  id: string
  kind: ManagedSkillKind
  localDir: string
  moduleKinds: string[]
  name: string
  provenance?: ManagedSkillProvenance
}

export interface SkillUpdateState {
  checkedAt: string
  localModified: boolean
  message?: string
  skillId: string
  sourceLocator?: string
  sourceType?: ManagedSkillProvenance["sourceType"]
  status: "unmanaged" | "upToDate" | "updateAvailable" | "sourceUnavailable"
}

export type RecommendedSkillInstallStatus =
  | "notInstalled"
  | "installed"
  | "bindingsMissing"
  | "conflict"

export interface RecommendedSkill {
  canCheckUpdates: boolean
  conflictReason?: "reservedName" | "differentSource" | "unmanagedSource"
  description: string
  id: string
  installedModuleKinds: string[]
  installedSkillId?: string
  installStatus: RecommendedSkillInstallStatus
  missingModuleKinds: string[]
  moduleKinds: string[]
  name: string
  sourceLocator: string
  sourceType: "github" | "skills-sh" | "clawhub" | "skillhub"
  sourceUrl: string
}

export interface RecommendedSkillsResponse {
  skills: RecommendedSkill[]
}

export interface ManagedSkillModule {
  kind: string
  skills: ManagedProjectSkill[]
}

export interface DeviceSkillLocation {
  agents: string[]
  comparison: "same" | "different" | "ignored" | "not-installed"
  contentHash: string
  description: string
  path: string
  scope: "global" | "project"
  skillPath: string
}

export interface DeviceSkillGroup {
  description: string
  installed: {
    canManageAssociations: boolean
    contentHash: string
    id: string
    kind: ManagedSkillKind
    moduleKinds: string[]
  } | null
  key: string
  locations: DeviceSkillLocation[]
  name: string
}

export interface WikiRawDocument {
  conversion: {
    error: string | null
    status:
      | "cancelled"
      | "failed"
      | "not_required"
      | "queued"
      | "converting"
      | "ready"
    taskId: string | null
    updatedAt: string
  }
  createdAt: string
  id: string
  ingestionByWikiSpace: Record<
    string,
    {
      error: string | null
      disposition?: "already_covered" | "excluded" | "included" | null
      markdownVersion?: number
      reasonCode?: string | null
      status: string
      summary?: string | null
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
  wordCount?: number | null
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
  wordCount?: number | null
}

export interface WikiTask {
  error: string | null
  id: string
  status: string
  targetId: string
  type: string
  wikiSpaceId: string | null
}

export type OriginalPreviewKind = "audio" | "image" | "pdf" | "text" | "video"

export interface WikiOriginalPreviewDocument {
  id: string
  mimeType: string
  originalFileName: string
  sizeBytes: number
  title: string
}

export type MyOpenPanelsTransport = {
  apiBase: string
  kind: "http"
}
