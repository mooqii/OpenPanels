import { Button } from "@heroui/react"
import { useCallback } from "react"
import type { Editor } from "~/canvas/editor"
import type { CommandToolConfig } from "~/canvas/types/tools"
import type { Transformer } from "../../shapes/Transformer"
import { CanvasToolbarTooltip as Tooltip } from "./CanvasToolbarTooltip"

interface CommandItemProps {
  editor: Editor
  tool: CommandToolConfig
  transformerRef: React.RefObject<Transformer | null>
}

export function CommandItem({
  editor,
  tool,
  transformerRef,
}: CommandItemProps) {
  const handleCommand = useCallback(() => {
    editor.dispatch(tool.command, transformerRef.current)
  }, [editor, tool.command, transformerRef])

  const tooltip =
    typeof tool.tooltip === "function" ? tool.tooltip() : tool.tooltip

  const button = (
    <Button
      aria-label={tooltip}
      isIconOnly
      onClick={handleCommand}
      variant="ghost"
    >
      {tool.icon}
      {tool.label ? <span>{tool.label}</span> : null}
    </Button>
  )

  return tooltip ? (
    <Tooltip>
      {button}
      <Tooltip.Content>{tooltip}</Tooltip.Content>
    </Tooltip>
  ) : (
    button
  )
}
