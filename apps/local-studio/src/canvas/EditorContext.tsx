import { createContext, type ReactNode, useContext } from "react"
import type { ToolbarConfig } from "./components/tools/types"
import type { Editor } from "./editor"

interface EditorContextValue {
  editor: Editor
  toolbarConfig: ToolbarConfig[]
}

const EditorContext = createContext<EditorContextValue | null>(null)

export function EditorProvider({
  editor,
  toolbarConfig,
  children,
}: {
  editor: Editor
  toolbarConfig: ToolbarConfig[]
  children: ReactNode
}) {
  return (
    <EditorContext.Provider value={{ editor, toolbarConfig }}>
      {children}
    </EditorContext.Provider>
  )
}

export function useEditor(): Editor {
  const context = useContext(EditorContext)
  if (!context) {
    throw new Error("useEditor must be used within EditorProvider")
  }
  return context.editor
}

export function useOptionalEditor(): Editor | null {
  const context = useContext(EditorContext)
  return context?.editor ?? null
}

export function useToolbarConfig(): ToolbarConfig[] {
  const context = useContext(EditorContext)
  if (!context) {
    throw new Error("useToolbarConfig must be used within EditorProvider")
  }
  return context.toolbarConfig
}
