import { z } from "zod"

export const panelKindSchema = z.enum([
  "canvas",
  "image",
  "diff",
  "preview",
  "files",
])

export const isoDateSchema = z.string().datetime()

export const sessionSchema = z.object({
  id: z.string().min(1),
  title: z.string().min(1),
  createdAt: isoDateSchema,
  updatedAt: isoDateSchema,
  panelIds: z.array(z.string().min(1)),
})

export const panelSchema = z.object({
  id: z.string().min(1),
  sessionId: z.string().min(1),
  kind: panelKindSchema,
  title: z.string().min(1),
  createdAt: isoDateSchema,
  updatedAt: isoDateSchema,
  stateRef: z.string().optional(),
})

const artifactBaseSchema = z.object({
  id: z.string().min(1),
  panelId: z.string().min(1).optional(),
  title: z.string().optional(),
  createdAt: isoDateSchema,
})

export const imageArtifactSchema = artifactBaseSchema.extend({
  kind: z.literal("image"),
  mimeType: z.string().min(1),
  assetRef: z.string().min(1),
  width: z.number().positive().optional(),
  height: z.number().positive().optional(),
})

export const canvasArtifactSchema = artifactBaseSchema.extend({
  kind: z.literal("canvas"),
  snapshot: z.unknown(),
})

export const diffArtifactSchema = artifactBaseSchema.extend({
  kind: z.literal("diff"),
  diff: z.string(),
})

export const fileArtifactSchema = artifactBaseSchema.extend({
  kind: z.literal("file"),
  path: z.string().min(1),
  mimeType: z.string().optional(),
})

export const previewArtifactSchema = artifactBaseSchema.extend({
  kind: z.literal("preview"),
  url: z.string().min(1),
})

export const artifactSchema = z.discriminatedUnion("kind", [
  imageArtifactSchema,
  canvasArtifactSchema,
  diffArtifactSchema,
  fileArtifactSchema,
  previewArtifactSchema,
])

export const createSessionInputSchema = z.object({
  title: z.string().min(1).default("OpenPanels Session"),
})

export const openPanelInputSchema = z.object({
  sessionId: z.string().min(1),
  kind: panelKindSchema,
  title: z.string().min(1).optional(),
  initialState: z.unknown().optional(),
})

export const insertArtifactInputSchema = z.object({
  sessionId: z.string().min(1),
  panelId: z.string().min(1).optional(),
  artifact: z.discriminatedUnion("kind", [
    imageArtifactSchema.omit({ id: true, createdAt: true }).extend({
      id: z.string().min(1).optional(),
      createdAt: isoDateSchema.optional(),
    }),
    canvasArtifactSchema.omit({ id: true, createdAt: true }).extend({
      id: z.string().min(1).optional(),
      createdAt: isoDateSchema.optional(),
    }),
    diffArtifactSchema.omit({ id: true, createdAt: true }).extend({
      id: z.string().min(1).optional(),
      createdAt: isoDateSchema.optional(),
    }),
    fileArtifactSchema.omit({ id: true, createdAt: true }).extend({
      id: z.string().min(1).optional(),
      createdAt: isoDateSchema.optional(),
    }),
    previewArtifactSchema.omit({ id: true, createdAt: true }).extend({
      id: z.string().min(1).optional(),
      createdAt: isoDateSchema.optional(),
    }),
  ]),
})

export const runtimeEventSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("session-created"), session: sessionSchema }),
  z.object({ type: z.literal("panel-opened"), panel: panelSchema }),
  z.object({ type: z.literal("artifact-inserted"), artifact: artifactSchema }),
  z.object({
    type: z.literal("panel-state-saved"),
    sessionId: z.string(),
    panelId: z.string(),
  }),
])

export type OpenPanelsSessionId = string
export type OpenPanelsPanelId = string
export type OpenPanelsArtifactId = string
export type OpenPanelsPanelKind = z.infer<typeof panelKindSchema>
export type OpenPanelsSession = z.infer<typeof sessionSchema>
export type OpenPanelsPanel = z.infer<typeof panelSchema>
export type ImageArtifact = z.infer<typeof imageArtifactSchema>
export type CanvasArtifact = z.infer<typeof canvasArtifactSchema>
export type DiffArtifact = z.infer<typeof diffArtifactSchema>
export type FileArtifact = z.infer<typeof fileArtifactSchema>
export type PreviewArtifact = z.infer<typeof previewArtifactSchema>
export type OpenPanelsArtifact = z.infer<typeof artifactSchema>
export type CreateSessionInput = z.input<typeof createSessionInputSchema>
export type OpenPanelInput = z.input<typeof openPanelInputSchema>
export type InsertArtifactInput = z.input<typeof insertArtifactInputSchema>
export type OpenPanelsRuntimeEvent = z.infer<typeof runtimeEventSchema>

export interface PanelSnapshot<TState = unknown> {
  artifacts: OpenPanelsArtifact[]
  panel: OpenPanelsPanel
  state: TState
}
