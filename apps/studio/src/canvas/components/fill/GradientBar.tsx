/**
 * GradientBar component with draggable color stops.
 * Displays a gradient preview bar with interactive handles for adjusting color positions.
 */

import { cn, Popover } from "@heroui/react"
import { useCallback, useRef, useState } from "react"
import type { GradientColorStop } from "../../types/shapes"
import { toCssLinearGradient } from "../../utils/fill"
import { ColorPicker, TRANSPARENT_BG } from "../ColorPicker"

interface GradientBarProps {
  className?: string
  colorStops: GradientColorStop[]
  onChange: (stops: GradientColorStop[]) => void
}

export function GradientBar({
  colorStops,
  onChange,
  className,
}: GradientBarProps) {
  const barRef = useRef<HTMLDivElement>(null)
  const [draggingIndex, setDraggingIndex] = useState<number | null>(null)
  const [selectedIndex, setSelectedIndex] = useState<number>(0)

  // Calculate position from mouse event
  const getPositionFromEvent = useCallback((clientX: number): number => {
    if (!barRef.current) return 0
    const rect = barRef.current.getBoundingClientRect()
    const x = clientX - rect.left
    const position = Math.max(0, Math.min(1, x / rect.width))
    return Math.round(position * 100) / 100 // Round to 2 decimal places
  }, [])

  // Handle drag start
  const handleDragStart = useCallback(
    (index: number) => (e: React.MouseEvent) => {
      e.preventDefault()
      setDraggingIndex(index)
      setSelectedIndex(index)

      const handleMouseMove = (moveEvent: MouseEvent) => {
        const newOffset = getPositionFromEvent(moveEvent.clientX)
        const newStops = [...colorStops]
        newStops[index] = { ...newStops[index], offset: newOffset }
        // Sort stops by offset
        newStops.sort((a, b) => a.offset - b.offset)
        // Find the new index after sorting
        const newIndex = newStops.findIndex(
          (stop) => stop === newStops[index] || stop.offset === newOffset
        )
        setSelectedIndex(newIndex >= 0 ? newIndex : index)
        onChange(newStops)
      }

      const handleMouseUp = () => {
        setDraggingIndex(null)
        document.removeEventListener("mousemove", handleMouseMove)
        document.removeEventListener("mouseup", handleMouseUp)
      }

      document.addEventListener("mousemove", handleMouseMove)
      document.addEventListener("mouseup", handleMouseUp)
    },
    [colorStops, getPositionFromEvent, onChange]
  )

  // Handle click on bar to add new stop
  const handleBarClick = useCallback(
    (e: React.MouseEvent) => {
      // Don't add stop if clicking on existing handle
      if ((e.target as HTMLElement).closest(".gradient-handle")) return

      const position = getPositionFromEvent(e.clientX)

      // Find the color stop to the left of the clicked position
      let leftStop = colorStops[0]

      for (let i = 0; i < colorStops.length - 1; i++) {
        if (
          colorStops[i].offset <= position &&
          colorStops[i + 1].offset >= position
        ) {
          leftStop = colorStops[i]
          break
        }
      }

      // Simple color interpolation (just use left color for now)
      const newStop: GradientColorStop = {
        offset: position,
        color: leftStop.color,
      }

      const newStops = [...colorStops, newStop].sort(
        (a, b) => a.offset - b.offset
      )
      const newIndex = newStops.findIndex((stop) => stop.offset === position)
      setSelectedIndex(newIndex)
      onChange(newStops)
    },
    [colorStops, getPositionFromEvent, onChange]
  )

  // Handle color change for selected stop
  const handleColorChange = useCallback(
    (color: string) => {
      const newStops = [...colorStops]
      newStops[selectedIndex] = { ...newStops[selectedIndex], color }
      onChange(newStops)
    },
    [colorStops, selectedIndex, onChange]
  )

  // Handle stop deletion
  const handleDeleteStop = useCallback(
    (index: number) => {
      if (colorStops.length <= 2) return // Keep at least 2 stops
      const newStops = colorStops.filter((_, i) => i !== index)
      setSelectedIndex(Math.min(selectedIndex, newStops.length - 1))
      onChange(newStops)
    },
    [colorStops, selectedIndex, onChange]
  )

  // Generate CSS gradient for preview
  const gradientCss = toCssLinearGradient({
    type: "linear-gradient",
    colorStops,
    rotation: 90, // Always show horizontal in the bar
  })

  return (
    <div className={cn("flex flex-col gap-2", className)}>
      {/* Gradient bar */}
      <div
        className="relative h-4 cursor-crosshair rounded-full"
        onClick={handleBarClick}
        ref={barRef}
        style={{ background: gradientCss }}
      >
        {/* Color stop handles */}
        {colorStops.map((stop, index) => (
          <Popover key={`stop-${stop.offset.toFixed(4)}-${stop.color}`}>
            <Popover.Trigger>
              <button
                className={cn(
                  "gradient-handle absolute top-1/2 h-5 w-5 -translate-x-1/2 -translate-y-1/2 cursor-grab rounded-md border-2 border-foreground shadow-md transition-transform",
                  "hover:scale-110 focus:outline-none focus:ring-2 focus:ring-focus",
                  selectedIndex === index && "ring-2 ring-focus",
                  draggingIndex === index && "scale-110 cursor-grabbing"
                )}
                onDoubleClick={() => handleDeleteStop(index)}
                onMouseDown={handleDragStart(index)}
                style={{
                  left: `${stop.offset * 100}%`,
                  backgroundColor: stop.color,
                  background:
                    stop.color === "transparent" ? TRANSPARENT_BG : stop.color,
                }}
                type="button"
              />
            </Popover.Trigger>
            <Popover.Content offset={12}>
              <Popover.Dialog className="w-68 p-4">
                <ColorPicker onChange={handleColorChange} value={stop.color} />
              </Popover.Dialog>
            </Popover.Content>
          </Popover>
        ))}
      </div>
    </div>
  )
}
