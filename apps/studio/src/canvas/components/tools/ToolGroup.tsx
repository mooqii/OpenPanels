import { Button, Dropdown, Label, Tooltip } from "@heroui/react"
import { useCallback, useEffect, useRef } from "react"
import { useEditor } from "../../EditorContext"
import { useTool } from "../../hooks/use-editor-state"
import { Shortcut } from "./ToolShortcut"
import {
  getActiveToolInGroup,
  getToolAction,
  isGroupActive,
} from "./tool-mapper"
import type { ToolGroupConfig } from "./types"
import { useLocalizedToolLabel } from "./use-localized-tool-label"

interface ToolGroupProps {
  group: ToolGroupConfig
}

export function ToolGroup({ group }: ToolGroupProps) {
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
      lastUsedRef.current = toolId
      const toolAction = getToolAction(toolId, editor, currentTool)
      if (toolAction) {
        toolAction()
      }
    },
    [editor, currentTool]
  )

  if (!displayTool) {
    return null
  }
  const displayToolLabel = getLocalizedLabel(displayTool.id, displayTool.label)

  return (
    <Dropdown>
      <Tooltip>
        <Button
          aria-label={displayToolLabel}
          className="cursor-pointer select-none"
          isIconOnly
          variant={isActive ? "primary" : "ghost"}
        >
          {displayTool.icon}
        </Button>
        <Tooltip.Content placement="right">{displayToolLabel}</Tooltip.Content>
      </Tooltip>
      <Dropdown.Popover placement="right">
        <Dropdown.Menu
          aria-label={displayToolLabel}
          onAction={(key) => handleToolSelect(String(key))}
          selectedKeys={activeToolId ? [activeToolId] : []}
          selectionMode="single"
        >
          {group.tools.map((tool) => {
            const label = getLocalizedLabel(tool.id, tool.label)
            return (
              <Dropdown.Item id={tool.id} key={tool.id} textValue={label}>
                <Dropdown.ItemIndicator />
                <span className="shrink-0">{tool.icon}</span>
                <Label className="flex-1">{label}</Label>
                {tool.shortcut ? <Shortcut shortcut={tool.shortcut} /> : null}
              </Dropdown.Item>
            )
          })}
        </Dropdown.Menu>
      </Dropdown.Popover>
    </Dropdown>
  )
}
