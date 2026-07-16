import { Button, cn, ListBox, Popover, Select, Separator } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { useCallback } from "react"
import type { Editor } from "../../editor"
import type { GeoShape, StrokePosition } from "../../types/shapes"
import { ColorPicker } from "../ColorPicker"
import { NumberInput } from "../number-input"
import { CanvasToolbarTooltip as Tooltip } from "./CanvasToolbarTooltip"

interface FillColorItemProps {
  editor: Editor
  shape: GeoShape
}

export function StrokeItem({ editor, shape }: FillColorItemProps) {
  const { t } = useLingui()
  const stroke = (shape.props.stroke as string) || "transparent"
  const strokeWidth = (shape.props.strokeWidth as number) || 0
  const strokePosition = shape.props.strokePosition || "center"
  const strokePositions: { value: StrokePosition; label: string }[] = [
    { value: "inside", label: t`Inside` },
    { value: "center", label: t`Center` },
    { value: "outside", label: t`Outside` },
  ]

  const handleChangeColor = useCallback(
    (stroke: string) => {
      editor.updateShape(shape.id, {
        props: {
          stroke,
        },
      })
    },
    [editor, shape.id]
  )

  const handleChangeWidth = useCallback(
    (strokeWidth: number) => {
      editor.updateShape(shape.id, {
        props: {
          strokeWidth,
        },
      })
    },
    [editor, shape.id]
  )

  const handleChangePosition = useCallback(
    (position: StrokePosition) => {
      editor.updateShape(shape.id, {
        props: {
          strokePosition: position,
        },
      })
    },
    [editor, shape.id]
  )

  return (
    <Popover>
      <Tooltip>
        <Button
          aria-label={t`Pick a color`}
          className={cn(
            "flex h-7 w-7 cursor-pointer items-center justify-center overflow-hidden rounded-full border-2 border-foreground p-0 outline-none",
            "ring-1 ring-border focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2"
          )}
        >
          <div
            className="h-full w-full rounded-full border-3 bg-surface"
            style={{
              borderColor: stroke,
            }}
          />
        </Button>
        <Tooltip.Content>{t`Stroke`}</Tooltip.Content>
      </Tooltip>
      <Popover.Content offset={12}>
        <Popover.Dialog className="w-68 p-0">
          <Popover.Heading className="px-4 py-3">
            <span>{t`Stroke`}</span>
          </Popover.Heading>
          <Separator />
          <div className="flex items-center gap-2 px-4 py-2">
            <NumberInput
              className="group flex items-center rounded-lg px-2 py-1"
              dragVelocity={1}
              min={0}
              onChange={handleChangeWidth}
              precision={1}
              step={1}
              value={strokeWidth}
            >
              <NumberInput.DragHandle />
              <NumberInput.Input className="group max-w-18 text-right" />
              <NumberInput.Unit>px</NumberInput.Unit>
            </NumberInput>
            <Select
              aria-label={t`Stroke Position`}
              className="min-w-24"
              onChange={(key) => {
                if (key) {
                  handleChangePosition(key as StrokePosition)
                }
              }}
              selectionMode="single"
              value={strokePosition}
              variant="secondary"
            >
              <Select.Trigger>
                <Select.Value />
                <Select.Indicator />
              </Select.Trigger>
              <Select.Popover>
                <ListBox>
                  {strokePositions.map((pos) => (
                    <ListBox.Item
                      id={pos.value}
                      key={pos.value}
                      textValue={pos.label}
                    >
                      {pos.label}
                    </ListBox.Item>
                  ))}
                </ListBox>
              </Select.Popover>
            </Select>
          </div>
          <Separator />
          <div className="flex flex-col gap-5 p-4">
            <ColorPicker onChange={handleChangeColor} value={stroke} />
          </div>
        </Popover.Dialog>
      </Popover.Content>
    </Popover>
  )
}
