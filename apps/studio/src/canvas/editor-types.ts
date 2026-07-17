import type { Editor } from "./editor"
import type { Transformer } from "./shapes/Transformer"
import type { createCanvasStore } from "./store"
import type { AssetStore } from "./types/assets"
import type { Shape } from "./types/shapes"
import type { ToolConfig } from "./types/tools"

export type CommandOptions = {
  editor: Editor
  transformer: Transformer | null
  selectedShapes: Shape[]
}

export type CommandHandler = (options: CommandOptions) => void

export type GetTools = (shapes: Shape[]) => ToolConfig[] | null

export interface EditorOptions {
  assetStore?: AssetStore
  commands?: Record<string, CommandHandler>
  getTools?: GetTools
  store?: ReturnType<typeof createCanvasStore>
}
