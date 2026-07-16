import { Button, cn, Popover, Separator } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { Scan } from "lucide-react"
import { useCallback } from "react"
import type { Editor } from "../../editor"
import type { GeoShape } from "../../types/shapes"
import { NumberInput } from "../number-input"
import { CanvasToolbarTooltip as Tooltip } from "./CanvasToolbarTooltip"

interface CornerRadiusItemProps {
  editor: Editor
  shape: GeoShape
}

// Array order: [top-left, top-right, bottom-right, bottom-left]
type CornerRadiusArray = [number, number, number, number]

// Uniform mode icon - all corners linked
function UniformCornerIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      fill="none"
      height="16"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="1.5"
      viewBox="0 0 24 24"
      width="16"
    >
      <path d="M4 8V7a3 3 0 0 1 3-3h1" />
      <path d="M4 16v1a3 3 0 0 0 3 3h1" />
      <path d="M16 4h1a3 3 0 0 1 3 3v1" />
      <path d="M16 20h1a3 3 0 0 0 3-3v-1" />
    </svg>
  )
}

// Mixed mode icon - corners unlinked
function MixedCornerIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      fill="none"
      height="16"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="1.5"
      viewBox="0 0 24 24"
      width="16"
    >
      <path d="M4 8V7a3 3 0 0 1 3-3h1" />
      <path d="M4 16v1a3 3 0 0 0 3 3h1" />
      <path d="M16 4h1a3 3 0 0 1 3 3v1" />
      <path d="M16 20h1a3 3 0 0 0 3-3v-1" />
      <circle cx="7" cy="7" fill="currentColor" r="1.5" />
      <circle cx="17" cy="7" fill="currentColor" r="1.5" />
      <circle cx="7" cy="17" fill="currentColor" r="1.5" />
      <circle cx="17" cy="17" fill="currentColor" r="1.5" />
    </svg>
  )
}

// Top-left corner icon
function TopLeftCornerIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      fill="none"
      height="16"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="1.5"
      viewBox="0 0 24 24"
      width="16"
    >
      <path d="M4 12V7a3 3 0 0 1 3-3h5" />
    </svg>
  )
}

// Top-right corner icon
function TopRightCornerIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      fill="none"
      height="16"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="1.5"
      viewBox="0 0 24 24"
      width="16"
    >
      <path d="M12 4h5a3 3 0 0 1 3 3v5" />
    </svg>
  )
}

// Bottom-left corner icon
function BottomLeftCornerIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      fill="none"
      height="16"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="1.5"
      viewBox="0 0 24 24"
      width="16"
    >
      <path d="M4 12v5a3 3 0 0 0 3 3h5" />
    </svg>
  )
}

// Bottom-right corner icon
function BottomRightCornerIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      fill="none"
      height="16"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="1.5"
      viewBox="0 0 24 24"
      width="16"
    >
      <path d="M20 12v5a3 3 0 0 1-3 3h-5" />
    </svg>
  )
}

// Parse cornerRadius prop into normalized array
function parseCornerRadius(value: unknown): CornerRadiusArray {
  if (Array.isArray(value) && value.length === 4) {
    return value as CornerRadiusArray
  }
  if (typeof value === "number") {
    return [value, value, value, value]
  }
  return [0, 0, 0, 0]
}

// Check if all corners have the same value
function isUniformRadius(values: CornerRadiusArray): boolean {
  return (
    values[0] === values[1] &&
    values[1] === values[2] &&
    values[2] === values[3]
  )
}

export function CornerRadiusItem({ editor, shape }: CornerRadiusItemProps) {
  const { t } = useLingui()
  const width = (shape.props.width as number) || 100
  const height = (shape.props.height as number) || 100
  const maxRadius = Math.min(width, height) / 2

  // Parse current values from shape props
  const cornerValues = parseCornerRadius(shape.props.cornerRadius)
  const isUniformMode = typeof shape.props.cornerRadius === "number"
  const uniformValue = isUniformMode
    ? (shape.props.cornerRadius as number)
    : cornerValues[0]

  // Handle uniform value change
  const handleUniformChange = useCallback(
    (value: number) => {
      if (isUniformMode) {
        // Stay in uniform mode
        editor.updateShape(shape.id, {
          props: { cornerRadius: value },
        })
      } else {
        // In mixed mode, update all corners
        editor.updateShape(shape.id, {
          props: { cornerRadius: [value, value, value, value] },
        })
      }
    },
    [editor, shape.id, isUniformMode]
  )

  // Handle individual corner change
  const handleCornerChange = useCallback(
    (index: 0 | 1 | 2 | 3, value: number) => {
      const newValues: CornerRadiusArray = [
        ...cornerValues,
      ] as CornerRadiusArray
      newValues[index] = value
      editor.updateShape(shape.id, {
        props: { cornerRadius: newValues },
      })
    },
    [editor, shape.id, cornerValues]
  )

  // Toggle between uniform and mixed mode
  const handleToggleMode = useCallback(() => {
    if (isUniformMode) {
      // Switch to mixed mode - convert number to array
      editor.updateShape(shape.id, {
        props: {
          cornerRadius: [
            uniformValue,
            uniformValue,
            uniformValue,
            uniformValue,
          ],
        },
      })
    } else {
      // Switch to uniform mode - use first corner value or average
      const avg = Math.round(
        (cornerValues[0] +
          cornerValues[1] +
          cornerValues[2] +
          cornerValues[3]) /
          4
      )
      editor.updateShape(shape.id, {
        props: { cornerRadius: avg },
      })
    }
  }, [editor, shape.id, isUniformMode, uniformValue, cornerValues])

  // Display value for uniform input
  const displayValue = isUniformMode
    ? uniformValue
    : isUniformRadius(cornerValues)
      ? cornerValues[0]
      : Math.round(
          (cornerValues[0] +
            cornerValues[1] +
            cornerValues[2] +
            cornerValues[3]) /
            4
        )

  return (
    <Popover>
      <Tooltip>
        <Button aria-label={t`Corner radius`} isIconOnly variant="ghost">
          <Scan size={16} strokeWidth={2} />
        </Button>
        <Tooltip.Content>{t`Corner Radius`}</Tooltip.Content>
      </Tooltip>
      <Popover.Content offset={12}>
        <Popover.Dialog className="w-64 p-0">
          <Popover.Heading className="px-4 py-3">
            <span>{t`Corner radius`}</span>
          </Popover.Heading>
          <Separator />
          <div className="p-4">
            {/* Top row: Uniform input and mode toggle */}
            <div className="mb-3 flex items-center gap-2">
              <NumberInput
                className="rounded-lg px-2 py-1"
                dragVelocity={1}
                max={maxRadius}
                min={0}
                onChange={handleUniformChange}
                precision={0}
                step={1}
                value={displayValue}
              >
                <NumberInput.DragHandle
                  icon={
                    <UniformCornerIcon className="h-3.5 w-3.5 text-muted" />
                  }
                />
                <NumberInput.Input className="w-18 text-right" />
              </NumberInput>
              <Button
                aria-label={
                  isUniformMode
                    ? t`Switch to mixed mode`
                    : t`Switch to uniform mode`
                }
                className={cn(
                  "h-8 w-8 min-w-8 p-0",
                  !isUniformMode && "bg-accent-soft text-accent-soft-foreground"
                )}
                isIconOnly
                onClick={handleToggleMode}
                variant="ghost"
              >
                <MixedCornerIcon className="h-4 w-4" />
              </Button>
            </div>

            {/* Individual corner inputs (shown in mixed mode) */}
            {!isUniformMode && (
              <div className="grid grid-cols-2 gap-2">
                {/* Top-left (index 0) */}
                <NumberInput
                  className="rounded-lg px-2 py-1"
                  dragVelocity={1}
                  max={maxRadius}
                  min={0}
                  onChange={(value) => handleCornerChange(0, value)}
                  precision={0}
                  step={1}
                  value={cornerValues[0]}
                >
                  <NumberInput.DragHandle
                    icon={
                      <TopLeftCornerIcon className="h-3.5 w-3.5 text-muted" />
                    }
                  />
                  <NumberInput.Input className="w-18 text-right" />
                </NumberInput>

                {/* Top-right (index 1) */}
                <NumberInput
                  className="rounded-lg px-2 py-1"
                  dragVelocity={1}
                  max={maxRadius}
                  min={0}
                  onChange={(value) => handleCornerChange(1, value)}
                  precision={0}
                  step={1}
                  value={cornerValues[1]}
                >
                  <NumberInput.DragHandle
                    icon={
                      <TopRightCornerIcon className="h-3.5 w-3.5 text-muted" />
                    }
                  />
                  <NumberInput.Input className="w-18 text-right" />
                </NumberInput>

                {/* Bottom-left (index 3) - visually in bottom-left position */}
                <NumberInput
                  className="rounded-lg px-2 py-1"
                  dragVelocity={1}
                  max={maxRadius}
                  min={0}
                  onChange={(value) => handleCornerChange(3, value)}
                  precision={0}
                  step={1}
                  value={cornerValues[3]}
                >
                  <NumberInput.DragHandle
                    icon={
                      <BottomLeftCornerIcon className="h-3.5 w-3.5 text-muted" />
                    }
                  />
                  <NumberInput.Input className="w-18 text-right" />
                </NumberInput>

                {/* Bottom-right (index 2) - visually in bottom-right position */}
                <NumberInput
                  className="rounded-lg px-2 py-1"
                  dragVelocity={1}
                  max={maxRadius}
                  min={0}
                  onChange={(value) => handleCornerChange(2, value)}
                  precision={0}
                  step={1}
                  value={cornerValues[2]}
                >
                  <NumberInput.DragHandle
                    icon={
                      <BottomRightCornerIcon className="h-3.5 w-3.5 text-muted" />
                    }
                  />
                  <NumberInput.Input className="w-18 text-right" />
                </NumberInput>
              </div>
            )}
          </div>
        </Popover.Dialog>
      </Popover.Content>
    </Popover>
  )
}
