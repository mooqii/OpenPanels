import { useCallback, useEffect, useRef } from "react"
import { useEditor } from "../../EditorContext"
import { useTool } from "../../hooks/use-editor-state"
import { ToolMenuButton } from "./ToolMenuButton"
import {
  getActiveToolInGroup,
  getToolAction,
  isGroupActive,
} from "./tool-mapper"
import type { ToolGroupConfig } from "./types"
import { useLocalizedToolLabel } from "./use-localized-tool-label"

interface ToolGroupProps {
  group: ToolGroupConfig
  isMenuOpen: boolean
  onMenuClose: () => void
  onMenuOpen: () => void
}

export function ToolGroup({
  group,
  isMenuOpen,
  onMenuClose,
  onMenuOpen,
}: ToolGroupProps) {
  const editor = useEditor()
  const currentTool = useTool(editor)
  const getLocalizedLabel = useLocalizedToolLabel()
  const lastUsedRef = useRef<string | null>(null)

  const toolIds = group.tools.map((tool) => tool.id)
  const isActive = isGroupActive(toolIds, currentTool)
  const activeToolId = getActiveToolInGroup(toolIds, currentTool)

  // Determine which tool to display (active or last used)
  const displayToolId =
    activeToolId ?? lastUsedRef.current ?? group.tools[0]?.id
  const displayTool = group.tools.find((tool) => tool.id === displayToolId)

  // Track last used tool when it changes
  useEffect(() => {
    if (activeToolId) {
      lastUsedRef.current = activeToolId
    }
  }, [activeToolId])

  const handleToolSelect = useCallback(
    (toolId: string) => {
      const toolAction = getToolAction(toolId, editor, currentTool)
      if (toolAction) {
        toolAction()
      }
    },
    [editor, currentTool]
  )

  const handleSelect = useCallback(() => {
    if (displayToolId) {
      handleToolSelect(displayToolId)
    }
  }, [displayToolId, handleToolSelect])

  if (!displayTool) {
    return null
  }
  const displayToolLabel = getLocalizedLabel(displayTool.id, displayTool.label)

  return (
    <ToolMenuButton
      activeToolId={activeToolId}
      buttonIcon={displayTool.icon}
      buttonLabel={displayToolLabel}
      getToolLabel={(tool) => getLocalizedLabel(tool.id, tool.label)}
      isActive={isActive}
      isMenuOpen={isMenuOpen}
      onButtonPress={handleSelect}
      onMenuClose={onMenuClose}
      onMenuOpen={onMenuOpen}
      onToolSelect={handleToolSelect}
      tools={group.tools}
    />
  )
}
