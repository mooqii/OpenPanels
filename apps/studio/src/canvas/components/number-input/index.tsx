import { cn } from "@heroui/react"
import { useMemo } from "react"
import { NumberInputContext } from "./Context"
import { NumberInputDragHandle } from "./DragHandle"
import { NumberInputInput } from "./Input"
import type { NumberInputContextValue, NumberInputProps } from "./types"
import { NumberInputUnit } from "./Unit"

export function NumberInput({
  value,
  onChange,
  min,
  max,
  step = 1,
  precision,
  dragVelocity = 0.1,
  disabled = false,
  className,
  children,
}: NumberInputProps) {
  const contextValue = useMemo<NumberInputContextValue>(
    () => ({
      value,
      onChange,
      min,
      max,
      step,
      precision,
      dragVelocity,
      disabled,
    }),
    [value, onChange, min, max, step, precision, dragVelocity, disabled]
  )

  return (
    <NumberInputContext.Provider value={contextValue}>
      <div
        className={cn(
          "flex items-center gap-1 border border-field-border bg-field text-field-foreground transition-colors",
          "focus-within:border-field-border-focus focus-within:ring-2 focus-within:ring-focus",
          disabled && "cursor-not-allowed opacity-50",
          className
        )}
        data-disabled={disabled || undefined}
      >
        {children}
      </div>
    </NumberInputContext.Provider>
  )
}

// Attach sub-components as properties for compound component pattern
NumberInput.Input = NumberInputInput
NumberInput.Unit = NumberInputUnit
NumberInput.DragHandle = NumberInputDragHandle

export type {
  NumberInputDragHandleProps,
  NumberInputInputProps,
  NumberInputProps,
  NumberInputUnitProps,
} from "./types"
