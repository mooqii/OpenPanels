import {
  createDefaultPanelRegistry,
  createOpenPanelsId,
  defaultTitleForPanel,
  nowIso,
  type OpenPanelsPanelRegistry,
  panelStateRef,
} from "@openpanels/core"
import {
  artifactSchema,
  type CreateSessionInput,
  createSessionInputSchema,
  type InsertArtifactInput,
  insertArtifactInputSchema,
  type OpenPanelInput,
  type OpenPanelsArtifact,
  type OpenPanelsPanel,
  type OpenPanelsRuntimeEvent,
  type OpenPanelsSession,
  openPanelInputSchema,
} from "@openpanels/protocol"

export interface OpenPanelsStorage {
  listArtifacts(
    sessionId: string,
    panelId?: string
  ): Promise<OpenPanelsArtifact[]>
  listSessions(): Promise<OpenPanelsSession[]>
  readPanel(sessionId: string, panelId: string): Promise<OpenPanelsPanel | null>
  readPanelState<TState = unknown>(
    sessionId: string,
    panelId: string
  ): Promise<TState | null>
  readSession(sessionId: string): Promise<OpenPanelsSession | null>
  writeArtifact(sessionId: string, artifact: OpenPanelsArtifact): Promise<void>
  writePanel(panel: OpenPanelsPanel): Promise<void>
  writePanelState(
    sessionId: string,
    panelId: string,
    state: unknown
  ): Promise<void>
  writeSession(session: OpenPanelsSession): Promise<void>
}

export interface OpenPanelsRuntimeOptions {
  registry?: OpenPanelsPanelRegistry
  storage: OpenPanelsStorage
}

export type OpenPanelsRuntimeListener = (event: OpenPanelsRuntimeEvent) => void

export class OpenPanelsRuntime {
  readonly #storage: OpenPanelsStorage
  readonly #registry: OpenPanelsPanelRegistry
  readonly #listeners = new Set<OpenPanelsRuntimeListener>()

  constructor(options: OpenPanelsRuntimeOptions) {
    this.#storage = options.storage
    this.#registry = options.registry ?? createDefaultPanelRegistry()
  }

  async listSessions(): Promise<OpenPanelsSession[]> {
    return this.#storage.listSessions()
  }

  async createSession(
    input: CreateSessionInput = {}
  ): Promise<OpenPanelsSession> {
    const parsed = createSessionInputSchema.parse(input)
    const timestamp = nowIso()
    const session: OpenPanelsSession = {
      id: createOpenPanelsId("session"),
      title: parsed.title,
      createdAt: timestamp,
      updatedAt: timestamp,
      panelIds: [],
    }
    await this.#storage.writeSession(session)
    this.#emit({ type: "session-created", session })
    return session
  }

  async getSession(sessionId: string): Promise<OpenPanelsSession | null> {
    return this.#storage.readSession(sessionId)
  }

  async getPanel(
    sessionId: string,
    panelId: string
  ): Promise<OpenPanelsPanel | null> {
    return this.#storage.readPanel(sessionId, panelId)
  }

  async openPanel(input: OpenPanelInput): Promise<OpenPanelsPanel> {
    const parsed = openPanelInputSchema.parse(input)
    const session = await this.#expectSession(parsed.sessionId)
    const timestamp = nowIso()
    const panel: OpenPanelsPanel = {
      id: createOpenPanelsId("panel"),
      sessionId: session.id,
      kind: parsed.kind,
      title: parsed.title ?? defaultTitleForPanel(parsed.kind),
      createdAt: timestamp,
      updatedAt: timestamp,
    }
    panel.stateRef = panelStateRef(panel)

    const definition = this.#registry.get(parsed.kind)
    const initialState =
      parsed.initialState ?? definition?.createInitialState() ?? {}

    await this.#storage.writePanel(panel)
    await this.#storage.writePanelState(session.id, panel.id, initialState)
    await this.#storage.writeSession({
      ...session,
      updatedAt: timestamp,
      panelIds: [...session.panelIds, panel.id],
    })
    this.#emit({ type: "panel-opened", panel })
    return panel
  }

  async insertArtifact(
    input: InsertArtifactInput
  ): Promise<OpenPanelsArtifact> {
    const parsed = insertArtifactInputSchema.parse(input)
    const session = await this.#expectSession(parsed.sessionId)
    const timestamp = nowIso()
    const artifact = artifactSchema.parse({
      ...parsed.artifact,
      id: parsed.artifact.id ?? createOpenPanelsId("artifact"),
      createdAt: parsed.artifact.createdAt ?? timestamp,
    })
    const panelId =
      artifact.panelId ??
      parsed.panelId ??
      (await this.#ensurePanel(session, artifact))
    const artifactForPanel = { ...artifact, panelId } as OpenPanelsArtifact

    await this.#storage.writeArtifact(session.id, artifactForPanel)
    await this.#applyArtifactToPanel(session.id, panelId, artifactForPanel)
    this.#emit({ type: "artifact-inserted", artifact: artifactForPanel })
    return artifactForPanel
  }

  async savePanelState(
    sessionId: string,
    panelId: string,
    state: unknown
  ): Promise<void> {
    await this.#expectPanel(sessionId, panelId)
    await this.#storage.writePanelState(sessionId, panelId, state)
    this.#emit({ type: "panel-state-saved", sessionId, panelId })
  }

  async readPanelState<TState = unknown>(
    sessionId: string,
    panelId: string
  ): Promise<TState | null> {
    return this.#storage.readPanelState<TState>(sessionId, panelId)
  }

  async listArtifacts(
    sessionId: string,
    panelId?: string
  ): Promise<OpenPanelsArtifact[]> {
    return this.#storage.listArtifacts(sessionId, panelId)
  }

  subscribe(listener: OpenPanelsRuntimeListener): () => void {
    this.#listeners.add(listener)
    return () => this.#listeners.delete(listener)
  }

  async #expectSession(sessionId: string): Promise<OpenPanelsSession> {
    const session = await this.#storage.readSession(sessionId)
    if (!session) throw new Error(`OpenPanels session not found: ${sessionId}`)
    return session
  }

  async #expectPanel(
    sessionId: string,
    panelId: string
  ): Promise<OpenPanelsPanel> {
    const panel = await this.#storage.readPanel(sessionId, panelId)
    if (!panel) throw new Error(`OpenPanels panel not found: ${panelId}`)
    return panel
  }

  async #ensurePanel(
    session: OpenPanelsSession,
    artifact: OpenPanelsArtifact
  ): Promise<string> {
    const definition = this.#registry.findForArtifact(artifact)
    const panel = await this.openPanel({
      sessionId: session.id,
      kind:
        definition?.kind ??
        (artifact.kind === "file" ? "files" : artifact.kind),
      title: definition?.title,
    })
    return panel.id
  }

  async #applyArtifactToPanel(
    sessionId: string,
    panelId: string,
    artifact: OpenPanelsArtifact
  ): Promise<void> {
    const panel = await this.#expectPanel(sessionId, panelId)
    const definition = this.#registry.get(panel.kind)
    if (!definition?.applyArtifact) return
    const currentState = await this.#storage.readPanelState(sessionId, panelId)
    const nextState = definition.applyArtifact(currentState, artifact)
    await this.#storage.writePanelState(sessionId, panelId, nextState)
  }

  #emit(event: OpenPanelsRuntimeEvent): void {
    for (const listener of this.#listeners) {
      listener(event)
    }
  }
}

export class InMemoryOpenPanelsStorage implements OpenPanelsStorage {
  readonly #sessions = new Map<string, OpenPanelsSession>()
  readonly #panels = new Map<string, OpenPanelsPanel>()
  readonly #states = new Map<string, unknown>()
  readonly #artifacts = new Map<string, OpenPanelsArtifact[]>()

  async listSessions(): Promise<OpenPanelsSession[]> {
    return [...this.#sessions.values()]
  }

  async readSession(sessionId: string): Promise<OpenPanelsSession | null> {
    return this.#sessions.get(sessionId) ?? null
  }

  async writeSession(session: OpenPanelsSession): Promise<void> {
    this.#sessions.set(session.id, session)
  }

  async readPanel(
    sessionId: string,
    panelId: string
  ): Promise<OpenPanelsPanel | null> {
    return this.#panels.get(this.#panelKey(sessionId, panelId)) ?? null
  }

  async writePanel(panel: OpenPanelsPanel): Promise<void> {
    this.#panels.set(this.#panelKey(panel.sessionId, panel.id), panel)
  }

  async readPanelState<TState = unknown>(
    sessionId: string,
    panelId: string
  ): Promise<TState | null> {
    return (
      (this.#states.get(this.#panelKey(sessionId, panelId)) as TState) ?? null
    )
  }

  async writePanelState(
    sessionId: string,
    panelId: string,
    state: unknown
  ): Promise<void> {
    this.#states.set(this.#panelKey(sessionId, panelId), state)
  }

  async listArtifacts(
    sessionId: string,
    panelId?: string
  ): Promise<OpenPanelsArtifact[]> {
    const artifacts = this.#artifacts.get(sessionId) ?? []
    return panelId
      ? artifacts.filter((artifact) => artifact.panelId === panelId)
      : artifacts
  }

  async writeArtifact(
    sessionId: string,
    artifact: OpenPanelsArtifact
  ): Promise<void> {
    const artifacts = this.#artifacts.get(sessionId) ?? []
    this.#artifacts.set(sessionId, [...artifacts, artifact])
  }

  #panelKey(sessionId: string, panelId: string): string {
    return `${sessionId}/${panelId}`
  }
}
