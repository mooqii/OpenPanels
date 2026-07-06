import type { PageId, ShapeId } from "../types/ids"
import type { Shape } from "../types/shapes"

export interface CloneShapesOptions {
  baseIndex: number
  idFactory: () => ShapeId
  offset: { x: number; y: number }
  parentId?: PageId | ShapeId
}

function cloneProps<T>(props: T): T {
  if (typeof structuredClone === "function") {
    return structuredClone(props)
  }
  return JSON.parse(JSON.stringify(props)) as T
}

export function cloneShapesForPaste(
  shapes: Shape[],
  options: CloneShapesOptions
): Shape[] {
  const idMap = new Map<ShapeId, ShapeId>()
  for (const shape of shapes) {
    idMap.set(shape.id, options.idFactory())
  }

  return shapes.map((shape, index) => {
    const props = cloneProps(shape.props)
    const parentId = shape.parentId as ShapeId
    const hasClonedParent = idMap.has(parentId)
    const offset = hasClonedParent ? { x: 0, y: 0 } : options.offset

    if (typeof (props as any).x === "number") {
      ;(props as any).x += offset.x
    }
    if (typeof (props as any).y === "number") {
      ;(props as any).y += offset.y
    }

    return {
      ...shape,
      id: idMap.get(shape.id)!,
      parentId: hasClonedParent
        ? idMap.get(parentId)!
        : (options.parentId ?? parentId),
      index: options.baseIndex + index,
      props,
    } as Shape
  })
}

export function cloneShapesForClipboard(shapes: Shape[]): Shape[] {
  return shapes.map((shape) => ({
    ...shape,
    props: cloneProps(shape.props),
  })) as Shape[]
}

export function moveShapesToFront(
  selected: Shape[],
  allShapes: Shape[]
): Shape[] {
  if (selected.length === 0) return []
  const indices = allShapes
    .map((shape) => shape.index)
    .filter((index) => Number.isFinite(index)) as number[]
  const maxIndex = indices.length ? Math.max(...indices) : 0
  const ordered = [...selected].sort((a, b) => (a.index ?? 0) - (b.index ?? 0))

  return ordered.map((shape, index) => ({
    ...shape,
    index: maxIndex + 1 + index,
  })) as Shape[]
}

export function moveShapesToBack(
  selected: Shape[],
  allShapes: Shape[]
): Shape[] {
  if (selected.length === 0) return []
  const indices = allShapes
    .map((shape) => shape.index)
    .filter((index) => Number.isFinite(index)) as number[]
  const minIndex = indices.length ? Math.min(...indices) : 0
  const ordered = [...selected].sort((a, b) => (a.index ?? 0) - (b.index ?? 0))
  const baseIndex = minIndex - ordered.length

  return ordered.map((shape, index) => ({
    ...shape,
    index: baseIndex + index,
  })) as Shape[]
}
