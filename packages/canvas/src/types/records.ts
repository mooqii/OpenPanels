/**
 * Record and store types for canvas state management.
 * Provides type-safe state management for shapes, assets, and pages.
 */

import type { Asset } from "./assets"
import type { AssetId, PageId, ShapeId } from "./ids"
import type { Shape } from "./shapes"

// =============================================================================
// Page Record
// =============================================================================

/**
 * Page record - represents a canvas page/artboard
 */
export interface Page {
  id: PageId
  index: number
  name: string
  typeName: "page"
}

// =============================================================================
// Record Types
// =============================================================================

/**
 * All record types in the canvas store
 */
export type CanvasRecord = Shape | Asset | Page

/**
 * Record ID types
 */
export type RecordId = ShapeId | AssetId | PageId

/**
 * Record type discriminator
 */
export type RecordTypeName = CanvasRecord["typeName"]

/**
 * Extract record by type name
 */
export type RecordByTypeName<T extends RecordTypeName> = Extract<
  CanvasRecord,
  { typeName: T }
>

// =============================================================================
// Records Diff
// =============================================================================

/**
 * Represents changes to the record store
 * Used for undo/redo, collaboration, and change tracking
 */
export interface RecordsDiff {
  /** Records that were added */
  added: { [id: string]: CanvasRecord }
  /** Records that were removed */
  removed: { [id: string]: CanvasRecord }
  /** Records that were updated: [from, to] */
  updated: { [id: string]: [from: CanvasRecord, to: CanvasRecord] }
}

// =============================================================================
// Store Snapshot
// =============================================================================

/**
 * Schema information for the store
 */
export interface StoreSchema {
  /** Record type versions */
  recordVersions: {
    [typeName: string]: number
  }
  /** Schema version */
  schemaVersion: number
}

/**
 * Default schema for the canvas store
 */
export const DEFAULT_SCHEMA: StoreSchema = {
  schemaVersion: 1,
  recordVersions: {
    shape: 1,
    asset: 1,
    page: 1,
  },
}

/**
 * Complete snapshot of the canvas store
 * Used for save/load, export/import
 */
export interface StoreSnapshot {
  currentPageId: PageId | null
  openedGroupId: ShapeId | null
  /** Schema information */
  schema: StoreSchema
  selectedShapeIds: Set<ShapeId>
  /** All records keyed by ID */
  store: { [id: string]: CanvasRecord }
}

/**
 * Create an empty store snapshot
 */
export function createEmptySnapshot(): StoreSnapshot {
  return {
    store: {},
    schema: DEFAULT_SCHEMA,
    selectedShapeIds: new Set<ShapeId>(),
    currentPageId: null,
    openedGroupId: null,
  }
}

// =============================================================================
// Type Guards
// =============================================================================

export function isShapeRecord(record: CanvasRecord): record is Shape {
  return record.typeName === "shape"
}

export function isAssetRecord(record: CanvasRecord): record is Asset {
  return record.typeName === "asset"
}

export function isPageRecord(record: CanvasRecord): record is Page {
  return record.typeName === "page"
}
