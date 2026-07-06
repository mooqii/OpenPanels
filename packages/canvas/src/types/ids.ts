/**
 * Branded ID types for type-safe identification of canvas entities.
 * Provides compile-time type safety for shape, page, and asset references.
 */

// Brand symbol for compile-time type safety
declare const brand: unique symbol

/**
 * Generic branded type utility
 */
type Brand<T, B extends string> = T & { readonly [brand]: B }

/**
 * Shape ID - uniquely identifies a shape in the canvas
 * Format: "shape:{uuid}"
 */
export type ShapeId = Brand<string, "ShapeId">

/**
 * Page ID - uniquely identifies a page/artboard
 * Format: "page:{number}" or "page:{uuid}"
 */
export type PageId = Brand<string, "PageId">

/**
 * Asset ID - uniquely identifies an asset (image, video, etc.)
 * Format: "asset:{uuid}"
 */
export type AssetId = Brand<string, "AssetId">

/**
 * Utility functions for creating IDs
 */
export const ShapeId = {
  create: (): ShapeId => `shape:${crypto.randomUUID()}` as ShapeId,
  from: (id: string): ShapeId => id as ShapeId,
  isValid: (id: string): id is ShapeId => id.startsWith("shape:"),
}

export const PageId = {
  create: (num?: number): PageId =>
    num === undefined
      ? (`page:${crypto.randomUUID()}` as PageId)
      : (`page:${num}` as PageId),
  from: (id: string): PageId => id as PageId,
  isValid: (id: string): id is PageId => id.startsWith("page:"),
}

export const AssetId = {
  create: (): AssetId => `asset:${crypto.randomUUID()}` as AssetId,
  from: (id: string): AssetId => id as AssetId,
  isValid: (id: string): id is AssetId => id.startsWith("asset:"),
}
