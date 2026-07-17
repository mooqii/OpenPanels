import { CANVAS_MAX_ZOOM, CANVAS_MIN_ZOOM } from "./constants"
import type { createCanvasStore } from "./store"
import type { PageId, ShapeId } from "./types/ids"
import type {
  CanvasCameraState,
  CanvasRecord,
  RecordId,
  RecordsDiff,
  StoreSnapshot,
} from "./types/records"
import type { Shape } from "./types/shapes"
import { createEmptyDiff } from "./utils/records"

export class Transaction {
  readonly #diff: RecordsDiff
  readonly #snapshot: StoreSnapshot

  #level: number
  #rolledback: boolean
  #committed: boolean

  constructor(snapshot: StoreSnapshot) {
    this.#snapshot = snapshot
    this.#diff = createEmptyDiff()
    this.#level = 0
    this.#rolledback = false
    this.#committed = false
  }

  get snapshot() {
    return this.#snapshot
  }

  incrementLevel() {
    this.#level += 1
  }

  rollback() {
    this.#rolledback = true
  }

  put(record: CanvasRecord) {
    const old = this.#snapshot.store[record.id]
    if (old) {
      this.#diff.updated[record.id] = [old, record]
    } else {
      this.#diff.added[record.id] = record
    }
  }

  remove(recordId: RecordId) {
    const old = this.#snapshot.store[recordId]
    if (old) {
      this.#diff.removed[recordId] = old
    }
  }

  commit(store: ReturnType<typeof createCanvasStore>) {
    if (this.#level > 0 || this.#committed) return false
    if (this.#rolledback) return true
    store.getState().applyDiff(this.#diff, { notify: true, skipHistory: false })
    this.#committed = true
    return true
  }
}

export function normalizeCamera(
  camera: CanvasCameraState | null | undefined
): CanvasCameraState | null {
  if (!camera) return null
  const { x, y, zoom } = camera
  if (!(Number.isFinite(x) && Number.isFinite(y) && Number.isFinite(zoom))) {
    return null
  }
  if (zoom <= 0) return null
  return {
    x,
    y,
    zoom: Math.max(CANVAS_MIN_ZOOM, Math.min(CANVAS_MAX_ZOOM, zoom)),
  }
}

export function calculateSelectedShapes({
  selectedShapeIds,
  store,
}: {
  selectedShapeIds: Set<ShapeId>
  store: { [id: string]: CanvasRecord }
}) {
  const shapes: Shape[] = []
  for (const id of selectedShapeIds) {
    const shape = store[id]
    if (shape && shape.typeName === "shape") {
      shapes.push(shape as Shape)
    }
  }
  return shapes
}

/**
 * Helper function to find the page a shape belongs to by walking up the hierarchy.
 */
export function getPageForShapeInStore(
  shape: Shape,
  store: { [id: string]: CanvasRecord }
): PageId | undefined {
  // If parent is a page, return it
  if (shape.parentId.startsWith("page:")) {
    return shape.parentId as PageId
  }

  // Walk up through groups to find the page
  let currentParentId = shape.parentId
  while (currentParentId.startsWith("shape:")) {
    const parent = store[currentParentId]
    if (!parent || parent.typeName !== "shape") return undefined
    const parentShape = parent as Shape
    if (parentShape.parentId.startsWith("page:")) {
      return parentShape.parentId as PageId
    }
    currentParentId = parentShape.parentId
  }

  return currentParentId as PageId
}

export function calculateCurrentPageShapes({
  currentPageId,
  store,
}: {
  currentPageId: PageId | null
  store: { [id: string]: CanvasRecord }
}) {
  if (!currentPageId) return []

  const shapes: Shape[] = []

  for (const record of Object.values(store)) {
    if (record.typeName === "shape") {
      const shape = record as Shape
      // Include shapes that belong to this page (either directly or through groups)
      const pageForShape = getPageForShapeInStore(shape, store)
      if (pageForShape === currentPageId) {
        shapes.push(shape)
      }
    }
  }

  return shapes.sort((a, b) => {
    // Sort by index
    return a.index - b.index
  })
}
