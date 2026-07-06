import { createContext, type ReactNode, useContext } from "react"
import type { UseCropReturn } from "../hooks/use-crop"
import type { ShapeId } from "../types/ids"

/**
 * Context value for crop mode.
 * Exposes the useCrop hook's return value to child components.
 */
export type CropContextValue = UseCropReturn

const CropContext = createContext<CropContextValue | null>(null)

/**
 * Provider for crop context.
 * Should wrap components that need access to crop functionality.
 */
export function CropProvider({
  value,
  children,
}: {
  value: CropContextValue
  children: ReactNode
}) {
  return <CropContext.Provider value={value}>{children}</CropContext.Provider>
}

/**
 * Hook to access crop context.
 * Throws if used outside of CropProvider.
 */
export function useCropContext(): CropContextValue {
  const context = useContext(CropContext)
  if (!context) {
    throw new Error("useCropContext must be used within CropProvider")
  }
  return context
}

/**
 * Hook to safely access crop context.
 * Returns null if used outside of CropProvider.
 */
export function useCropContextSafe(): CropContextValue | null {
  return useContext(CropContext)
}

/**
 * Type guard to check if we're in crop mode
 */
export function isInCropMode(context: CropContextValue | null): boolean {
  return context?.cropShapeId != null
}

/**
 * Type guard to check if a specific shape is being cropped
 */
export function isShapeBeingCropped(
  context: CropContextValue | null,
  shapeId: ShapeId
): boolean {
  return context?.cropShapeId === shapeId
}
