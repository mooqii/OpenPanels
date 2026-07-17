import Konva from "konva"
import type { ContainerConfig } from "konva/lib/Container"
import type { Node } from "konva/lib/Node"
import type { Circle } from "konva/lib/shapes/Circle"
import type { Rect } from "konva/lib/shapes/Rect"
import type { IRect, Vector2d } from "konva/lib/types"
import { Util } from "konva/lib/Util"

export {
  createRotationCursor,
  getRotationCursorAngle,
  ROTATION_ZONE_OUTER_OFFSET,
} from "../../utils/rotation"

export interface Box extends IRect {
  rotation: number
}

export interface TransformerConfig extends ContainerConfig {
  anchorCornerRadius?: number
  anchorDragBoundFunc?: (
    oldPos: Vector2d,
    newPos: Vector2d,
    evt: MouseEvent | TouchEvent
  ) => Vector2d
  anchorFill?: string
  anchorSize?: number
  anchorStroke?: string
  anchorStrokeWidth?: number
  anchorStyleFunc?: ((anchor: Rect | Circle) => void) | null
  borderDash?: number[]
  borderEnabled?: boolean
  borderStroke?: string
  borderStrokeWidth?: number
  boundBoxFunc?: (oldBox: Box, newBox: Box) => Box
  centeredScaling?: boolean
  enabledAnchors?: string[]
  flipEnabled?: boolean
  ignoreStroke?: boolean
  keepRatio?: boolean
  nodes?: Node[]
  resizeEnabled?: boolean
  rotateAnchorCursor?: string
  rotateAnchorOffset?: number
  rotateEnabled?: boolean
  rotationSnaps?: number[]
  rotationSnapTolerance?: number
  shiftBehavior?: string
  shouldOverdrawWholeArea?: boolean
  useSingleNodeRotation?: boolean
}

export const EVENTS_NAME = "tr-konva"

export const ATTR_CHANGE_LIST = [
  "resizeEnabledChange",
  "rotateAnchorOffsetChange",
  "rotateEnabledChange",
  "enabledAnchorsChange",
  "anchorSizeChange",
  "borderEnabledChange",
  "borderStrokeChange",
  "borderStrokeWidthChange",
  "borderDashChange",
  "anchorStrokeChange",
  "anchorStrokeWidthChange",
  "anchorFillChange",
  "anchorCornerRadiusChange",
  "ignoreStrokeChange",
  "anchorStyleFuncChange",
]
  .map((e) => `${e}.${EVENTS_NAME}`)
  .join(" ")

export const NODES_RECT = "nodesRect"

export const TRANSFORM_CHANGE_STR = [
  "widthChange",
  "heightChange",
  "scaleXChange",
  "scaleYChange",
  "skewXChange",
  "skewYChange",
  "rotationChange",
  "offsetXChange",
  "offsetYChange",
  "transformsEnabledChange",
  "strokeWidthChange",
  "draggableChange",
]

export type AnchorName =
  | "top-left"
  | "top-center"
  | "top-right"
  | "middle-right"
  | "middle-left"
  | "bottom-left"
  | "bottom-center"
  | "bottom-right"

export type RotaterAnchorName =
  | "rotater-top-left"
  | "rotater-top-right"
  | "rotater-bottom-left"
  | "rotater-bottom-right"

export type AnchorType = AnchorName | RotaterAnchorName

export const ROTATER_ANCHORS: RotaterAnchorName[] = [
  "rotater-top-left",
  "rotater-top-right",
  "rotater-bottom-left",
  "rotater-bottom-right",
]

// Map rotater anchor to corner index (for cursor angle calculation)
export const ROTATER_CORNER_INDEX: Record<RotaterAnchorName, number> = {
  "rotater-top-left": 0,
  "rotater-top-right": 1,
  "rotater-bottom-right": 2,
  "rotater-bottom-left": 3,
}

// Corner anchors use Circle shape, edge anchors use Rect
export const CORNER_ANCHORS: AnchorName[] = [
  "top-left",
  "top-right",
  "bottom-left",
  "bottom-right",
]

// Base rotation angles for arc-shaped rotation anchors (in degrees)
// Each arc extends outward from its corner
export const ROTATER_BASE_ANGLES: Record<RotaterAnchorName, number> = {
  "rotater-top-left": 180,
  "rotater-top-right": 270,
  "rotater-bottom-right": 0,
  "rotater-bottom-left": 90,
}

export const ANGLES = {
  "top-left": -45,
  "top-center": 0,
  "top-right": 45,
  "middle-right": -90,
  "middle-left": 90,
  "bottom-left": -135,
  "bottom-center": 180,
  "bottom-right": 135,
} as Record<AnchorName, number>

export const TOUCH_DEVICE = "ontouchstart" in Konva._global
export const SELECTION_OUTLINES_NAME = "selection-outlines"
export const SELECTION_OUTLINE_ITEM_NAME = "selection-outline-item"

export function getCursor(anchorName: AnchorName, rotationRad: number) {
  const rad = rotationRad + Util.degToRad(ANGLES[anchorName] || 0)
  const angle = ((Util.radToDeg(rad) % 360) + 360) % 360

  if (Util._inRange(angle, 315 + 22.5, 360) || Util._inRange(angle, 0, 22.5)) {
    return "ns-resize"
  }
  if (Util._inRange(angle, 45 - 22.5, 45 + 22.5)) {
    return "nesw-resize"
  }
  if (Util._inRange(angle, 90 - 22.5, 90 + 22.5)) {
    return "ew-resize"
  }
  if (Util._inRange(angle, 135 - 22.5, 135 + 22.5)) {
    return "nwse-resize"
  }
  if (Util._inRange(angle, 180 - 22.5, 180 + 22.5)) {
    return "ns-resize"
  }
  if (Util._inRange(angle, 225 - 22.5, 225 + 22.5)) {
    return "nesw-resize"
  }
  if (Util._inRange(angle, 270 - 22.5, 270 + 22.5)) {
    return "ew-resize"
  }
  if (Util._inRange(angle, 315 - 22.5, 315 + 22.5)) {
    return "nwse-resize"
  }
  Util.error(`Transformer has unknown angle for cursor detection: ${angle}`)
  return "pointer"
}

export const ANCHORS_NAMES: AnchorName[] = [
  "top-left",
  "top-center",
  "top-right",
  "middle-right",
  "middle-left",
  "bottom-left",
  "bottom-center",
  "bottom-right",
]

export const MAX_SAFE_INTEGER = 100_000_000

export function getCenter(shape: Box) {
  return {
    x:
      shape.x +
      (shape.width / 2) * Math.cos(shape.rotation) +
      (shape.height / 2) * Math.sin(-shape.rotation),
    y:
      shape.y +
      (shape.height / 2) * Math.cos(shape.rotation) +
      (shape.width / 2) * Math.sin(shape.rotation),
  }
}

export function rotateAroundPoint(
  shape: Box,
  angleRad: number,
  point: Vector2d
) {
  const x =
    point.x +
    (shape.x - point.x) * Math.cos(angleRad) -
    (shape.y - point.y) * Math.sin(angleRad)
  const y =
    point.y +
    (shape.x - point.x) * Math.sin(angleRad) +
    (shape.y - point.y) * Math.cos(angleRad)
  return {
    ...shape,
    rotation: shape.rotation + angleRad,
    x,
    y,
  }
}

export function rotateAroundCenter(shape: Box, deltaRad: number) {
  const center = getCenter(shape)
  return rotateAroundPoint(shape, deltaRad, center)
}

export function getSnap(snaps: number[], newRotationRad: number, tol: number) {
  let snapped = newRotationRad
  for (const snap of snaps) {
    const angle = Konva.getAngle(snap)

    const absDiff = Math.abs(angle - newRotationRad) % (Math.PI * 2)
    const dif = Math.min(absDiff, Math.PI * 2 - absDiff)

    if (dif < tol) {
      snapped = angle
    }
  }
  return snapped
}

export function validateAnchors(val: unknown): string[] {
  if (!Array.isArray(val)) {
    Util.warn("enabledAnchors value should be an array")
    return []
  }
  for (const name of val) {
    if (ANCHORS_NAMES.indexOf(name as AnchorName) === -1) {
      Util.warn(
        `Unknown anchor name: ${name}. Available names are: ${ANCHORS_NAMES.join(", ")}`
      )
    }
  }
  return val
}

export const transformerRuntime = { activeCount: 0 }
