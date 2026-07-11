import { cn } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { useCallback, useEffect, useRef, useState } from "react"
import { useNumberInputContext } from "./Context"
import type { NumberInputDragHandleProps } from "./types"
import { clampValue } from "./utils"

const DEFAULT_DRAG_ICON = "⠿"

export function NumberInputDragHandle({
  icon = DEFAULT_DRAG_ICON,
  className,
}: NumberInputDragHandleProps) {
  const { t } = useLingui()
  const { value, onChange, min, max, dragVelocity, disabled } =
    useNumberInputContext()
  const [isDragging, setIsDragging] = useState(false)

  // Keep latest values without re-binding listeners
  const dragStateRef = useRef({ startX: 0, startValue: 0 })
  const latestRef = useRef({ min, max, dragVelocity, onChange })

  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (disabled) return
      e.preventDefault()
      e.stopPropagation()

      dragStateRef.current = { startX: e.clientX, startValue: value }
      setIsDragging(true)
    },
    [value, disabled]
  )

  useEffect(() => {
    latestRef.current = { min, max, dragVelocity, onChange }
  }, [min, max, dragVelocity, onChange])

  // Setup/remove listeners based on drag state
  useEffect(() => {
    if (!isDragging) return

    const handleDragMove = (e: MouseEvent) => {
      const { startX, startValue } = dragStateRef.current
      const { min, max, dragVelocity, onChange } = latestRef.current
      const deltaX = e.clientX - startX
      const newValue = clampValue(startValue + deltaX * dragVelocity, min, max)
      onChange(newValue)
    }

    const handleDragEnd = () => {
      setIsDragging(false)
    }

    document.body.style.cursor = "ew-resize"
    document.addEventListener("mousemove", handleDragMove)
    document.addEventListener("mouseup", handleDragEnd)

    return () => {
      document.body.style.cursor = ""
      document.removeEventListener("mousemove", handleDragMove)
      document.removeEventListener("mouseup", handleDragEnd)
    }
  }, [isDragging])

  return (
    <button
      aria-label={t`Drag to adjust value`}
      className={cn(
        "flex items-center justify-center",
        "h-4 w-4",
        "text-gray-400 dark:text-gray-500",
        "hover:text-gray-600 dark:hover:text-gray-300",
        "active:text-blue-500 dark:active:text-blue-400",
        "cursor-ew-resize",
        "transition-colors",
        isDragging && "scale-110 text-blue-500",
        disabled && "cursor-not-allowed opacity-50",
        className
      )}
      disabled={disabled}
      onMouseDown={handleMouseDown}
      type="button"
    >
      {icon}
    </button>
  )
}
