import { Button, cn, Popover, Separator, Tooltip } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { useCallback } from "react"
import {
  PENCIL_STROKE_WIDTH,
  PENCIL_STROKE_WIDTH_THICK,
  PENCIL_STROKE_WIDTH_THIN,
} from "../constants"
import { useEditor } from "../EditorContext"
import { useTool } from "../hooks/use-editor-state"
import { ColorPicker } from "./ColorPicker"
import { NumberInput } from "./number-input"

const PENCIL_SIZE_OPTIONS = [
  {
    id: "thin",
    strokeWidth: PENCIL_STROKE_WIDTH_THIN,
    visualWeight: 2,
  },
  {
    id: "medium",
    strokeWidth: PENCIL_STROKE_WIDTH,
    visualWeight: 4,
  },
  {
    id: "thick",
    strokeWidth: PENCIL_STROKE_WIDTH_THICK,
    visualWeight: 6,
  },
] as const

function PencilSizeIcon({ strokeWidth }: { strokeWidth: number }) {
  return (
    <svg
      aria-hidden="true"
      className="h-4 w-4"
      fill="none"
      viewBox="0 0 16 16"
      xmlns="http://www.w3.org/2000/svg"
    >
      <line
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth={strokeWidth}
        x1="3"
        x2="13"
        y1="8"
        y2="8"
      />
    </svg>
  )
}

export function BrushToolbar() {
  const { t } = useLingui()
  const editor = useEditor()
  const tool = useTool(editor)

  const handleColorChange = useCallback(
    (newColor: string) => {
      if (
        tool.name === "pencil" ||
        tool.name === "brush" ||
        tool.name === "marker"
      ) {
        editor.setTool({ ...tool, color: newColor })
      }
    },
    [editor, tool]
  )

  const handleSizeChange = useCallback(
    (newSize: number) => {
      if (
        tool.name === "pencil" ||
        tool.name === "brush" ||
        tool.name === "marker"
      ) {
        editor.setTool({ ...tool, size: newSize })
      }
    },
    [editor, tool]
  )

  // Only show for pencil or brush tools
  if (
    tool.name !== "pencil" &&
    tool.name !== "brush" &&
    tool.name !== "marker"
  ) {
    return null
  }

  const color = tool.color
  const size = tool.size
  const toolLabel =
    tool.name === "pencil"
      ? t`Pencil`
      : tool.name === "marker"
        ? t`Marker`
        : t`Brush`
  const minSize = 1
  const maxSize = Number.MAX_SAFE_INTEGER

  return (
    <div className="fixed top-2 left-1/2 z-10 flex -translate-x-1/2 items-center gap-2 rounded-full bg-canvas-toolbar px-3 py-1.5 shadow-sm backdrop-blur-lg">
      {/* Color Picker */}
      <Popover>
        <Tooltip>
          <Button
            aria-label={t`Pick a color`}
            className={cn(
              "flex h-7 w-7 cursor-pointer items-center justify-center overflow-hidden rounded-full border-2 border-white p-0 outline-none",
              "ring-1 ring-zinc-300 focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:ring-offset-2"
            )}
          >
            <div
              className="h-full w-full"
              style={{
                backgroundColor: color,
              }}
            />
          </Button>
          <Tooltip.Content>{toolLabel} Color</Tooltip.Content>
        </Tooltip>
        <Popover.Content className="border border-border" offset={12}>
          <Popover.Dialog className="w-68 p-0">
            <Popover.Heading className="px-4 py-3">
              <span>{toolLabel} Color</span>
            </Popover.Heading>
            <Separator />
            <div className="flex flex-col gap-5 p-4">
              <ColorPicker onChange={handleColorChange} value={color} />
            </div>
          </Popover.Dialog>
        </Popover.Content>
      </Popover>

      {/* Divider */}
      <Separator orientation="vertical" />

      {tool.name === "pencil" ? (
        <div
          aria-label={t`Pencil sizes`}
          className="flex items-center gap-1"
          role="group"
        >
          {PENCIL_SIZE_OPTIONS.map((option) => {
            const isActive = size === option.strokeWidth
            const ariaLabel =
              option.id === "thin"
                ? t`Pencil size thin`
                : option.id === "medium"
                  ? t`Pencil size medium`
                  : t`Pencil size thick`

            return (
              <Button
                aria-label={ariaLabel}
                className={cn(
                  "flex h-8 min-w-8 cursor-pointer items-center justify-center rounded-full px-0 text-text-tertiary transition-colors",
                  isActive
                    ? "text-foreground"
                    : "hover:bg-bg-muted hover:text-foreground"
                )}
                key={option.id}
                onPress={() => {
                  handleSizeChange(option.strokeWidth)
                }}
                variant="ghost"
              >
                <PencilSizeIcon strokeWidth={option.visualWeight} />
              </Button>
            )
          })}
        </div>
      ) : (
        <div className="flex items-center gap-2">
          <span className="text-gray-500 text-xs">{t`Size`}</span>
          <NumberInput
            className="rounded-lg bg-surface-secondary px-2"
            dragVelocity={1}
            max={maxSize}
            min={minSize}
            onChange={handleSizeChange}
            precision={0}
            step={1}
            value={size}
          >
            <NumberInput.DragHandle />
            <NumberInput.Input className="w-14 text-right" />
            <NumberInput.Unit>px</NumberInput.Unit>
          </NumberInput>
        </div>
      )}
    </div>
  )
}
