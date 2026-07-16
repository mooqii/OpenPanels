import { Button, cn, Popover, Separator } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { useCallback } from "react"
import type { Editor } from "../../editor"
import { getTextToolFromShape } from "../../text-tool"
import { persistCanvasTool } from "../../tool-persistence"
import type { TextShape } from "../../types/shapes"
import { ColorPicker, TRANSPARENT_BG } from "../ColorPicker"
import { CanvasToolbarTooltip as Tooltip } from "./CanvasToolbarTooltip"

interface TextColorItemProps {
  editor: Editor
  shape: TextShape
}

export function TextColorItem({ editor, shape }: TextColorItemProps) {
  const { t } = useLingui()
  const fill = (shape.props.fill as string) || "black"

  const handleFillChange = useCallback(
    (color: string) => {
      editor.updateShape(shape.id, {
        props: { fill: color },
      })
      persistCanvasTool(
        getTextToolFromShape(shape, editor.getZoom(), {
          color,
        })
      )
    },
    [editor, shape]
  )

  return (
    <Popover>
      <Tooltip>
        <Button
          aria-label={t`Text Fill Color`}
          className={cn(
            "flex h-7 w-7 cursor-pointer items-center justify-center overflow-hidden rounded-full border-2 border-foreground p-0 outline-none",
            "ring-1 ring-border focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2"
          )}
          style={{
            background: TRANSPARENT_BG,
          }}
        >
          <div
            className="h-full w-full"
            style={{
              backgroundColor: fill,
            }}
          />
        </Button>
        <Tooltip.Content>{t`Fill Color`}</Tooltip.Content>
      </Tooltip>
      <Popover.Content offset={12}>
        <Popover.Dialog className="w-68 p-0">
          <Popover.Heading className="px-4 py-3">
            <span>{t`Fill`}</span>
          </Popover.Heading>
          <Separator />
          <div className="flex flex-col gap-5 p-4">
            <ColorPicker onChange={handleFillChange} value={fill} />
          </div>
        </Popover.Dialog>
      </Popover.Content>
    </Popover>
  )
}
