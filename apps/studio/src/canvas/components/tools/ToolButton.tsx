import { useCallback, useRef } from "react"
import { useEditor } from "../../EditorContext"
import type { Editor } from "../../editor"
import { useTool } from "../../hooks/use-editor-state"
import type { ShapeId } from "../../types/ids"
import { createImageFromFile } from "../../utils/clipboard"
import { getViewportCenter } from "../../utils/coordinates"
import { ToolMenuButton } from "./ToolMenuButton"
import { getToolAction, isToolActive } from "./tool-mapper"
import type { ToolConfigItem } from "./types"
import { useLocalizedToolLabel } from "./use-localized-tool-label"

interface ToolButtonProps {
  isMenuOpen?: boolean
  onMenuClose?: () => void
  onMenuOpen?: () => void
  tool: ToolConfigItem
}

const NOOP = () => undefined

export function ToolButton({
  isMenuOpen = false,
  onMenuClose = NOOP,
  onMenuOpen = NOOP,
  tool,
}: ToolButtonProps) {
  const editor = useEditor()
  const currentTool = useTool(editor)
  const getLocalizedLabel = useLocalizedToolLabel()

  const isActive = isToolActive(tool.id, currentTool)
  const toolAction = getToolAction(tool.id, editor, currentTool)
  const localizedLabel = getLocalizedLabel(tool.id, tool.label)

  const handleClick = useCallback(() => {
    toolAction?.()
  }, [toolAction])

  if (tool.id === "image") {
    return (
      <ImageToolButton
        editor={editor}
        isMenuOpen={isMenuOpen}
        label={localizedLabel}
        onMenuClose={onMenuClose}
        onMenuOpen={onMenuOpen}
        tool={tool}
      />
    )
  }

  return (
    <ToolMenuButton
      activeToolId={isActive ? tool.id : null}
      buttonIcon={tool.icon}
      buttonLabel={localizedLabel}
      getToolLabel={() => localizedLabel}
      isActive={isActive}
      isMenuOpen={isMenuOpen}
      onButtonPress={handleClick}
      onMenuClose={onMenuClose}
      onMenuOpen={onMenuOpen}
      onToolSelect={() => handleClick()}
      tools={[tool]}
    />
  )
}

function ImageToolButton({
  editor,
  isMenuOpen,
  label,
  onMenuClose,
  onMenuOpen,
  tool,
}: {
  editor: Editor
  isMenuOpen: boolean
  label: string
  onMenuClose: () => void
  onMenuOpen: () => void
  tool: ToolConfigItem
}) {
  const inputRef = useRef<HTMLInputElement | null>(null)

  const handleFiles = useCallback(
    async (files: FileList | null) => {
      const fileList = Array.from(files ?? []).filter((file) =>
        file.type.startsWith("image/")
      )
      if (fileList.length === 0) return

      const center = getViewportCenter(editor.stage) ?? { x: 400, y: 300 }
      const assetStore = editor.getAssetStore()
      const shapeIds: ShapeId[] = []
      let offsetX = 0

      for (const file of fileList) {
        const shapeId = await createImageFromFile(
          editor,
          file,
          { x: center.x + offsetX, y: center.y },
          assetStore,
          true
        )
        shapeIds.push(shapeId)
        offsetX += 32
      }

      if (shapeIds.length > 0) {
        editor.setSelectedShapes(shapeIds)
      }
    },
    [editor]
  )

  return (
    <>
      <input
        accept="image/*"
        multiple
        onChange={async (event) => {
          await handleFiles(event.currentTarget.files)
          event.currentTarget.value = ""
        }}
        ref={inputRef}
        style={{ display: "none" }}
        type="file"
      />
      <ToolMenuButton
        buttonIcon={tool.icon}
        buttonLabel={label}
        getToolLabel={() => label}
        isActive={false}
        isMenuOpen={isMenuOpen}
        onButtonPress={() => inputRef.current?.click()}
        onMenuClose={onMenuClose}
        onMenuOpen={onMenuOpen}
        onToolSelect={() => inputRef.current?.click()}
        tools={[tool]}
      />
    </>
  )
}
