import type {
  OpenPanelsArtifact,
  OpenPanelsPanel,
  OpenPanelsPanelKind,
} from "@openpanels/protocol"

export type OpenPanelsIdPrefix = "session" | "panel" | "artifact" | "asset"

export function createOpenPanelsId(prefix: OpenPanelsIdPrefix): string {
  const random =
    globalThis.crypto?.randomUUID?.() ??
    `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`
  return `${prefix}:${random}`
}

export function nowIso(): string {
  return new Date().toISOString()
}

export interface PanelDefinition<TState = unknown> {
  applyArtifact?: (state: TState, artifact: OpenPanelsArtifact) => TState
  canHandleArtifact: (artifact: OpenPanelsArtifact) => boolean
  createInitialState: () => TState
  kind: OpenPanelsPanelKind
  title: string
}

export class OpenPanelsPanelRegistry {
  readonly #definitions = new Map<OpenPanelsPanelKind, PanelDefinition>()

  register(definition: PanelDefinition): void {
    this.#definitions.set(definition.kind, definition)
  }

  get(kind: OpenPanelsPanelKind): PanelDefinition | undefined {
    return this.#definitions.get(kind)
  }

  all(): PanelDefinition[] {
    return [...this.#definitions.values()]
  }

  findForArtifact(artifact: OpenPanelsArtifact): PanelDefinition | undefined {
    return this.all().find((definition) =>
      definition.canHandleArtifact(artifact)
    )
  }
}

export function createDefaultPanelRegistry(): OpenPanelsPanelRegistry {
  const registry = new OpenPanelsPanelRegistry()

  registry.register({
    kind: "image",
    title: "Images",
    canHandleArtifact: (artifact) => artifact.kind === "image",
    createInitialState: () => ({ images: [] }),
    applyArtifact: (state, artifact) => {
      const imageState = isImagePanelState(state) ? state : { images: [] }
      return artifact.kind === "image"
        ? { images: [...imageState.images, artifact] }
        : imageState
    },
  })

  registry.register({
    kind: "canvas",
    title: "Canvas",
    canHandleArtifact: (artifact) =>
      artifact.kind === "canvas" || artifact.kind === "image",
    createInitialState: () => ({
      schema: { schemaVersion: 1, recordVersions: {} },
      store: {},
      selectedShapeIds: [],
      currentPageId: null,
      openedGroupId: null,
    }),
  })

  return registry
}

function isImagePanelState(
  state: unknown
): state is { images: OpenPanelsArtifact[] } {
  return (
    typeof state === "object" &&
    state !== null &&
    "images" in state &&
    Array.isArray((state as { images?: unknown }).images)
  )
}

export function defaultTitleForPanel(kind: OpenPanelsPanelKind): string {
  switch (kind) {
    case "canvas":
      return "Canvas"
    case "image":
      return "Images"
    case "diff":
      return "Diff"
    case "preview":
      return "Preview"
    case "files":
      return "Files"
    default:
      return kind
  }
}

export function panelStateRef(panel: OpenPanelsPanel): string {
  return `sessions/${panel.sessionId}/panels/${panel.id}/state.json`
}
