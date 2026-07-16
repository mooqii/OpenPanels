import { cn } from "@heroui/react"
import type { NumberInputUnitProps } from "./types"

export function NumberInputUnit({ children, className }: NumberInputUnitProps) {
  return (
    <span className={cn("font-medium text-muted text-xs", className)}>
      {children}
    </span>
  )
}
