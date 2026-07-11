import { Button } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { Group, Ungroup } from "lucide-react"
import { useCallback } from "react"
import type { Editor } from "~/canvas/editor"
import { useSelectedShapes } from "~/canvas/hooks/use-editor-state"
import type { Shape } from "~/canvas/types/shapes"
import { CanvasToolbarTooltip as Tooltip } from "./CanvasToolbarTooltip"

interface GroupItemProps {
  editor: Editor
}

function canGroup(shapes: Shape[]): boolean {
  if (shapes.length < 2) return false
  const nonConnectors = shapes.filter((s) => s.type !== "connector")
  if (nonConnectors.length < 2) return false
  const parentId = nonConnectors[0].parentId
  return nonConnectors.every((s) => s.parentId === parentId)
}

function canUngroup(shapes: Shape[]): boolean {
  return shapes.some((s) => s.type === "group")
}

export function GroupItem({ editor }: GroupItemProps) {
  const { t } = useLingui()
  const shapes = useSelectedShapes(editor)

  const handleClick = useCallback(() => {
    editor.groupSelectedShapes()
  }, [editor])

  if (!canGroup(shapes)) return null

  return (
    <Tooltip>
      <Button
        aria-label={t`Group`}
        isIconOnly
        onClick={handleClick}
        variant="ghost"
      >
        <Group size={16} strokeWidth={1.5} />
      </Button>
      <Tooltip.Content>{t`Group`}</Tooltip.Content>
    </Tooltip>
  )
}

export function UngroupItem({ editor }: GroupItemProps) {
  const { t } = useLingui()
  const shapes = useSelectedShapes(editor)

  const handleClick = useCallback(() => {
    editor.ungroupSelectedShapes()
  }, [editor])

  if (!canUngroup(shapes)) return null

  return (
    <Tooltip>
      <Button
        aria-label={t`Ungroup`}
        isIconOnly
        onClick={handleClick}
        variant="ghost"
      >
        <Ungroup size={16} strokeWidth={1.5} />
      </Button>
      <Tooltip.Content>{t`Ungroup`}</Tooltip.Content>
    </Tooltip>
  )
}
