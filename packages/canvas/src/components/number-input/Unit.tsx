import { cn } from "@heroui/react"
import type { NumberInputUnitProps } from "./types"

export function NumberInputUnit({ children, className }: NumberInputUnitProps) {
  return (
    <span
      className={cn(
        "font-medium text-gray-500 text-xs dark:text-gray-400",
        className
      )}
    >
      {children}
    </span>
  )
}
