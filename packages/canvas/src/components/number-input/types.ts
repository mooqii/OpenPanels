import type { ReactNode } from "react"

export interface NumberInputProps {
  /** Child components for composition */
  children?: ReactNode
  /** Container className */
  className?: string
  /** Disabled state */
  disabled?: boolean
  /** Drag sensitivity multiplier (default: 0.1) */
  dragVelocity?: number
  /** Maximum allowed value */
  max?: number
  /** Minimum allowed value */
  min?: number
  /** Callback when value changes */
  onChange: (value: number) => void
  /** Decimal places for display (default: auto-detect) */
  precision?: number
  /** Increment step for keyboard arrows (default: 1) */
  step?: number
  /** Unit suffix (fallback if Unit component not used) */
  unit?: string
  /** Current numeric value */
  value: number
}

export interface NumberInputInputProps {
  /** Input element className */
  className?: string
  /** Input element id */
  id?: string
}

export interface NumberInputUnitProps {
  /** Unit display content or text */
  children?: ReactNode
  /** Unit element className */
  className?: string
}

export interface NumberInputDragHandleProps {
  /** Drag handle className */
  className?: string
  /** Custom drag icon (default: ⠿) */
  icon?: ReactNode
}

export interface NumberInputContextValue {
  disabled: boolean
  dragVelocity: number
  max?: number
  min?: number
  onChange: (value: number) => void
  precision?: number
  step: number
  value: number
}
