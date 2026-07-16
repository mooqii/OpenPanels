import { Button, Tooltip } from "@heroui/react"
import { useCallback, useRef } from "react"
import { useEditor } from "../../EditorContext"
import type { Editor } from "../../editor"
import { useTool } from "../../hooks/use-editor-state"
import type { ShapeId } from "../../types/ids"
import { createImageFromFile } from "../../utils/clipboard"
import { getViewportCenter } from "../../utils/coordinates"
import { getToolAction, isToolActive } from "./tool-mapper"
import type { ToolConfigItem } from "./types"
import { useLocalizedToolLabel } from "./use-localized-tool-label"

interface ToolButtonProps {
  tool: ToolConfigItem
}

export function ToolButton({ tool }: ToolButtonProps) {
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
      <ImageToolButton editor={editor} label={localizedLabel} tool={tool} />
    )
  }

  return (
    <Tooltip>
      <Button
        aria-label={localizedLabel}
        className="cursor-pointer select-none"
        isIconOnly
        onPress={handleClick}
        variant={isActive ? "primary" : "ghost"}
      >
        {tool.icon}
      </Button>
      <Tooltip.Content placement="right">{localizedLabel}</Tooltip.Content>
    </Tooltip>
  )
}

function ImageToolButton({
  editor,
  label,
  tool,
}: {
  editor: Editor
  label: string
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
      <Tooltip>
        <Button
          aria-label={label}
          className="cursor-pointer select-none"
          isIconOnly
          onPress={() => inputRef.current?.click()}
          variant="ghost"
        >
          {tool.icon}
        </Button>
        <Tooltip.Content placement="right">{label}</Tooltip.Content>
      </Tooltip>
    </>
  )
}
