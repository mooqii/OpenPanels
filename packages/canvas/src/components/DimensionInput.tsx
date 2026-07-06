import { cn } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { Link, Unlink } from "lucide-react"
import { useCallback, useEffect, useRef, useState } from "react"
import { NumberInput } from "./number-input"

interface DimensionInputProps {
  className?: string
  height: number
  onChange: (width: number, height: number) => void
  width: number
}

export function DimensionInput({
  width,
  height,
  onChange,
  className,
}: DimensionInputProps) {
  const { t } = useLingui()
  const [isLinked, setIsLinked] = useState(false)
  const [localWidth, setLocalWidth] = useState(Math.round(width))
  const [localHeight, setLocalHeight] = useState(Math.round(height))
  const aspectRatioRef = useRef(width / height)

  // Update local values when props change
  useEffect(() => {
    setLocalWidth(Math.round(width))
    setLocalHeight(Math.round(height))
    aspectRatioRef.current = width / height
  }, [width, height])

  const handleWidthChange = useCallback(
    (newWidth: number) => {
      setLocalWidth(newWidth)

      if (isLinked) {
        const newHeight = Math.round(newWidth / aspectRatioRef.current)
        setLocalHeight(newHeight)
        onChange(newWidth, newHeight)
      } else {
        onChange(newWidth, localHeight)
      }
    },
    [isLinked, localHeight, onChange]
  )

  const handleHeightChange = useCallback(
    (newHeight: number) => {
      setLocalHeight(newHeight)

      if (isLinked) {
        const newWidth = Math.round(newHeight * aspectRatioRef.current)
        setLocalWidth(newWidth)
        onChange(newWidth, newHeight)
      } else {
        onChange(localWidth, newHeight)
      }
    },
    [isLinked, localWidth, onChange]
  )

  const toggleLinked = useCallback(() => {
    if (!isLinked) {
      // When linking, store current aspect ratio
      aspectRatioRef.current = localWidth / localHeight
    }
    setIsLinked((prev) => !prev)
  }, [isLinked, localWidth, localHeight])

  return (
    <div className={cn("flex items-center gap-1 text-sm", className)}>
      {/* Width Input */}
      <NumberInput
        className="group flex w-20 items-center rounded-lg bg-surface-secondary px-1 py-0.5 focus-within:ring-2 focus-within:ring-blue-500"
        dragVelocity={1}
        min={0}
        onChange={handleWidthChange}
        precision={1}
        step={1}
        value={localWidth}
      >
        <NumberInput.DragHandle icon="W" />
        <NumberInput.Input className="w-14 bg-transparent text-right" />
      </NumberInput>

      {/* Link/Unlink Button */}
      <button
        aria-label={isLinked ? t`Unlink dimensions` : t`Link dimensions`}
        className={cn(
          "flex h-6 w-6 items-center justify-center rounded-md transition-colors",
          isLinked
            ? "bg-interactive-active text-foreground"
            : "text-gray-400 hover:bg-interactive-hover"
        )}
        onClick={toggleLinked}
        type="button"
      >
        {isLinked ? <Link size={14} /> : <Unlink size={14} />}
      </button>

      {/* Height Input */}
      <NumberInput
        className="group flex w-20 items-center rounded-lg bg-surface-secondary px-1 py-0.5 focus-within:ring-2 focus-within:ring-blue-500"
        dragVelocity={1}
        min={0}
        onChange={handleHeightChange}
        precision={1}
        step={1}
        value={localHeight}
      >
        <NumberInput.DragHandle icon="H" />
        <NumberInput.Input className="w-14 bg-transparent text-right" />
      </NumberInput>
    </div>
  )
}
