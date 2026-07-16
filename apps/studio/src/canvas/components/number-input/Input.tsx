import { cn } from "@heroui/react"
import { useCallback, useEffect, useState } from "react"
import { useNumberInputContext } from "./Context"
import type { NumberInputInputProps } from "./types"
import { clampValue, roundToPrecision } from "./utils"

export function NumberInputInput({ className, id }: NumberInputInputProps) {
  const { value, onChange, min, max, step, precision, disabled } =
    useNumberInputContext()

  const [localValue, setLocalValue] = useState(String(value))

  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const input = e.target.value

      // Allow empty input for editing
      if (input === "") {
        setLocalValue("")
        return
      }

      const parsed = Number.parseFloat(input)
      if (Number.isNaN(parsed)) return

      const clamped = clampValue(parsed, min, max)
      const rounded = roundToPrecision(clamped, precision)

      onChange(rounded)
      setLocalValue(String(rounded))
    },
    [onChange, min, max, precision]
  )

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      let newValue: number | undefined

      switch (e.key) {
        case "ArrowUp":
          newValue = value + (e.shiftKey ? step * 10 : step)
          break
        case "ArrowDown":
          newValue = value - (e.shiftKey ? step * 10 : step)
          break
        case "Home":
          if (min !== undefined) newValue = min
          break
        case "End":
          if (max !== undefined) newValue = max
          break
        default:
          // Ignore other keys
          break
      }

      if (newValue !== undefined) {
        e.preventDefault()
        const clamped = clampValue(newValue, min, max)
        onChange(clamped)
      }
    },
    [value, step, min, max, onChange]
  )

  const handleBlur = useCallback(() => {
    if (localValue === "") {
      setLocalValue(String(value))
    }
  }, [localValue, value])

  // Sync local value when prop changes externally
  useEffect(() => {
    setLocalValue(String(value))
  }, [value])

  return (
    <input
      className={cn(
        "bg-transparent px-1 py-1 text-field-foreground outline-none placeholder:text-field-placeholder",
        disabled && "cursor-not-allowed opacity-50",
        className
      )}
      disabled={disabled}
      id={id}
      onBlur={handleBlur}
      onChange={handleChange}
      onKeyDown={handleKeyDown}
      type="number"
      value={localValue}
    />
  )
}
