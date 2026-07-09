import Konva from "konva"
import type { ContainerConfig } from "konva/lib/Container"
import { Group } from "konva/lib/Group"
import { type KonvaEventObject, Node } from "konva/lib/Node"
import { Arc } from "konva/lib/shapes/Arc"
import { Circle } from "konva/lib/shapes/Circle"
import { Ellipse } from "konva/lib/shapes/Ellipse"
import { Line } from "konva/lib/shapes/Line"
import { Path } from "konva/lib/shapes/Path"
import { Rect } from "konva/lib/shapes/Rect"
import type { IRect, Vector2d } from "konva/lib/types"
import { Transform, Util } from "konva/lib/Util"
import {
  createRotationCursor,
  getRotationCursorAngle,
  ROTATION_ZONE_OUTER_OFFSET,
} from "../utils/rotation"

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

const EVENTS_NAME = "tr-konva"

const ATTR_CHANGE_LIST = [
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

const NODES_RECT = "nodesRect"

const TRANSFORM_CHANGE_STR = [
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

type AnchorName =
  | "top-left"
  | "top-center"
  | "top-right"
  | "middle-right"
  | "middle-left"
  | "bottom-left"
  | "bottom-center"
  | "bottom-right"

type RotaterAnchorName =
  | "rotater-top-left"
  | "rotater-top-right"
  | "rotater-bottom-left"
  | "rotater-bottom-right"

export type AnchorType = AnchorName | RotaterAnchorName

const ROTATER_ANCHORS: RotaterAnchorName[] = [
  "rotater-top-left",
  "rotater-top-right",
  "rotater-bottom-left",
  "rotater-bottom-right",
]

// Map rotater anchor to corner index (for cursor angle calculation)
const ROTATER_CORNER_INDEX: Record<RotaterAnchorName, number> = {
  "rotater-top-left": 0,
  "rotater-top-right": 1,
  "rotater-bottom-right": 2,
  "rotater-bottom-left": 3,
}

// Corner anchors use Circle shape, edge anchors use Rect
const CORNER_ANCHORS: AnchorName[] = [
  "top-left",
  "top-right",
  "bottom-left",
  "bottom-right",
]

// Base rotation angles for arc-shaped rotation anchors (in degrees)
// Each arc extends outward from its corner
const ROTATER_BASE_ANGLES: Record<RotaterAnchorName, number> = {
  "rotater-top-left": 180,
  "rotater-top-right": 270,
  "rotater-bottom-right": 0,
  "rotater-bottom-left": 90,
}

const ANGLES = {
  "top-left": -45,
  "top-center": 0,
  "top-right": 45,
  "middle-right": -90,
  "middle-left": 90,
  "bottom-left": -135,
  "bottom-center": 180,
  "bottom-right": 135,
} as Record<AnchorName, number>

const TOUCH_DEVICE = "ontouchstart" in Konva._global
const SELECTION_OUTLINES_NAME = "selection-outlines"
const SELECTION_OUTLINE_ITEM_NAME = "selection-outline-item"

function getCursor(anchorName: AnchorName, rotationRad: number) {
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

const ANCHORS_NAMES: AnchorName[] = [
  "top-left",
  "top-center",
  "top-right",
  "middle-right",
  "middle-left",
  "bottom-left",
  "bottom-center",
  "bottom-right",
]

const MAX_SAFE_INTEGER = 100_000_000

function getCenter(shape: Box) {
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

function rotateAroundPoint(shape: Box, angleRad: number, point: Vector2d) {
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

function rotateAroundCenter(shape: Box, deltaRad: number) {
  const center = getCenter(shape)
  return rotateAroundPoint(shape, deltaRad, center)
}

function getSnap(snaps: number[], newRotationRad: number, tol: number) {
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

function validateAnchors(val: unknown): string[] {
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

let activeTransformersCount = 0

/**
 * Transformer constructor. Transformer is a special type of group that allow you transform Konva
 * primitives and shapes. This version updates width/height instead of scaleX/scaleY.
 */
export class Transformer extends Group {
  // Internal state
  #nodes: Node[] = []
  #movingAnchorName: AnchorType | null = null
  #transforming = false
  #anchorDragOffset: Vector2d = { x: 0, y: 0 }
  #cursorChanged = false
  sin = 0
  cos = 0

  // Corner-based rotation state
  #activeRotation: {
    cornerIndex: number
    startAngle: number
    startRotation: number
    centerX: number
    centerY: number
  } | null = null

  // Configuration private fields
  #enabledAnchors: string[] = [...ANCHORS_NAMES]
  #flipEnabled = true
  #resizeEnabled = true
  #anchorSize = 10
  #rotateEnabled = true
  #rotationSnaps: number[] = []
  #rotationSnapTolerance = 5
  #borderEnabled = true
  #anchorStroke = "rgb(0, 161, 255)"
  #anchorStrokeWidth = 1
  #anchorFill = "white"
  #anchorCornerRadius = 0
  #borderStroke = "rgb(0, 161, 255)"
  #borderStrokeWidth = 1
  #borderDash: number[] | undefined
  #keepRatio = true
  #shiftBehavior = "default"
  #centeredScaling = false
  #ignoreStroke = true
  #padding = 0
  #boundBoxFunc: ((oldBox: Box, newBox: Box) => Box) | undefined
  #anchorDragBoundFunc:
    | ((
        oldPos: Vector2d,
        newPos: Vector2d,
        evt: MouseEvent | TouchEvent
      ) => Vector2d)
    | undefined
  #anchorStyleFunc: ((anchor: Rect | Circle) => void) | null = null
  #shouldOverdrawWholeArea = false
  #useSingleNodeRotation = true

  static isTransforming = () => {
    return activeTransformersCount > 0
  }

  constructor(config?: TransformerConfig) {
    super(config)

    // Apply config values
    if (config) {
      if (config.enabledAnchors !== undefined)
        this.#enabledAnchors = validateAnchors(config.enabledAnchors)
      if (config.flipEnabled !== undefined)
        this.#flipEnabled = config.flipEnabled
      if (config.resizeEnabled !== undefined)
        this.#resizeEnabled = config.resizeEnabled
      if (config.anchorSize !== undefined) this.#anchorSize = config.anchorSize
      if (config.rotateEnabled !== undefined)
        this.#rotateEnabled = config.rotateEnabled
      if (config.rotationSnaps !== undefined)
        this.#rotationSnaps = config.rotationSnaps
      if (config.rotationSnapTolerance !== undefined)
        this.#rotationSnapTolerance = config.rotationSnapTolerance
      if (config.borderEnabled !== undefined)
        this.#borderEnabled = config.borderEnabled
      if (config.anchorStroke !== undefined)
        this.#anchorStroke = config.anchorStroke
      if (config.anchorStrokeWidth !== undefined)
        this.#anchorStrokeWidth = config.anchorStrokeWidth
      if (config.anchorFill !== undefined) this.#anchorFill = config.anchorFill
      if (config.anchorCornerRadius !== undefined)
        this.#anchorCornerRadius = config.anchorCornerRadius
      if (config.borderStroke !== undefined)
        this.#borderStroke = config.borderStroke
      if (config.borderStrokeWidth !== undefined)
        this.#borderStrokeWidth = config.borderStrokeWidth
      if (config.borderDash !== undefined) this.#borderDash = config.borderDash
      if (config.keepRatio !== undefined) this.#keepRatio = config.keepRatio
      if (config.shiftBehavior !== undefined)
        this.#shiftBehavior = config.shiftBehavior
      if (config.centeredScaling !== undefined)
        this.#centeredScaling = config.centeredScaling
      if (config.ignoreStroke !== undefined)
        this.#ignoreStroke = config.ignoreStroke
      if (config.boundBoxFunc !== undefined)
        this.#boundBoxFunc = config.boundBoxFunc
      if (config.anchorDragBoundFunc !== undefined)
        this.#anchorDragBoundFunc = config.anchorDragBoundFunc
      if (config.anchorStyleFunc !== undefined)
        this.#anchorStyleFunc = config.anchorStyleFunc
      if (config.shouldOverdrawWholeArea !== undefined)
        this.#shouldOverdrawWholeArea = config.shouldOverdrawWholeArea
      if (config.useSingleNodeRotation !== undefined)
        this.#useSingleNodeRotation = config.useSingleNodeRotation
    }

    this._createElements()

    // bindings
    this._handleMouseMove = this._handleMouseMove.bind(this)
    this._handleMouseUp = this._handleMouseUp.bind(this)
    this._handleRotaterMouseMove = this._handleRotaterMouseMove.bind(this)
    this._handleRotaterMouseUp = this._handleRotaterMouseUp.bind(this)
    this.update = this.update.bind(this)

    // update transformer data for certain attr changes
    this.on(ATTR_CHANGE_LIST, this.update)

    if (this.getNode()) {
      this.update()
    }
  }

  // ==========================================================================
  // Getters and Setters
  // ==========================================================================

  get enabledAnchors(): string[] {
    return this.#enabledAnchors
  }
  set enabledAnchors(value: string[]) {
    this.#enabledAnchors = validateAnchors(value)
  }

  get flipEnabled(): boolean {
    return this.#flipEnabled
  }
  set flipEnabled(value: boolean) {
    this.#flipEnabled = value
  }

  get resizeEnabled(): boolean {
    return this.#resizeEnabled
  }
  set resizeEnabled(value: boolean) {
    this.#resizeEnabled = value
  }

  get anchorSize(): number {
    return this.#anchorSize
  }
  set anchorSize(value: number) {
    this.#anchorSize = value
  }

  get rotateEnabled(): boolean {
    return this.#rotateEnabled
  }
  set rotateEnabled(value: boolean) {
    this.#rotateEnabled = value
  }

  get rotationSnaps(): number[] {
    return this.#rotationSnaps
  }
  set rotationSnaps(value: number[]) {
    this.#rotationSnaps = value
  }

  get rotationSnapTolerance(): number {
    return this.#rotationSnapTolerance
  }
  set rotationSnapTolerance(value: number) {
    this.#rotationSnapTolerance = value
  }

  get borderEnabled(): boolean {
    return this.#borderEnabled
  }
  set borderEnabled(value: boolean) {
    this.#borderEnabled = value
  }

  get anchorStroke(): string {
    return this.#anchorStroke
  }
  set anchorStroke(value: string) {
    this.#anchorStroke = value
  }

  get anchorStrokeWidth(): number {
    return this.#anchorStrokeWidth
  }
  set anchorStrokeWidth(value: number) {
    this.#anchorStrokeWidth = value
  }

  get anchorFill(): string {
    return this.#anchorFill
  }
  set anchorFill(value: string) {
    this.#anchorFill = value
  }

  get anchorCornerRadius(): number {
    return this.#anchorCornerRadius
  }
  set anchorCornerRadius(value: number) {
    this.#anchorCornerRadius = value
  }

  get borderStroke(): string {
    return this.#borderStroke
  }
  set borderStroke(value: string) {
    this.#borderStroke = value
  }

  get borderStrokeWidth(): number {
    return this.#borderStrokeWidth
  }
  set borderStrokeWidth(value: number) {
    this.#borderStrokeWidth = value
  }

  get borderDash(): number[] | undefined {
    return this.#borderDash
  }
  set borderDash(value: number[] | undefined) {
    this.#borderDash = value
  }

  get keepRatio(): boolean {
    return this.#keepRatio
  }
  set keepRatio(value: boolean) {
    this.#keepRatio = value
  }

  get shiftBehavior(): string {
    return this.#shiftBehavior
  }
  set shiftBehavior(value: string) {
    this.#shiftBehavior = value
  }

  get centeredScaling(): boolean {
    return this.#centeredScaling
  }
  set centeredScaling(value: boolean) {
    this.#centeredScaling = value
  }

  get ignoreStroke(): boolean {
    return this.#ignoreStroke
  }
  set ignoreStroke(value: boolean) {
    this.#ignoreStroke = value
  }

  get padding(): number {
    return this.#padding
  }
  set padding(value: number) {
    this.#padding = value
  }

  get boundBoxFunc(): ((oldBox: Box, newBox: Box) => Box) | undefined {
    return this.#boundBoxFunc
  }
  set boundBoxFunc(value: ((oldBox: Box, newBox: Box) => Box) | undefined) {
    this.#boundBoxFunc = value
  }

  get anchorDragBoundFunc():
    | ((
        oldPos: Vector2d,
        newPos: Vector2d,
        evt: MouseEvent | TouchEvent
      ) => Vector2d)
    | undefined {
    return this.#anchorDragBoundFunc
  }
  set anchorDragBoundFunc(value:
    | ((
        oldPos: Vector2d,
        newPos: Vector2d,
        evt: MouseEvent | TouchEvent
      ) => Vector2d)
    | undefined) {
    this.#anchorDragBoundFunc = value
  }

  get anchorStyleFunc(): ((anchor: Rect) => void) | null {
    return this.#anchorStyleFunc
  }
  set anchorStyleFunc(value: ((anchor: Rect | Circle) => void) | null) {
    this.#anchorStyleFunc = value
  }

  get shouldOverdrawWholeArea(): boolean {
    return this.#shouldOverdrawWholeArea
  }
  set shouldOverdrawWholeArea(value: boolean) {
    this.#shouldOverdrawWholeArea = value
  }

  get useSingleNodeRotation(): boolean {
    return this.#useSingleNodeRotation
  }
  set useSingleNodeRotation(value: boolean) {
    this.#useSingleNodeRotation = value
  }

  // ==========================================================================
  // Node management
  // ==========================================================================

  /**
   * alias to `tr.nodes([shape])`/ This method is deprecated and will be removed soon.
   */
  attachTo(node: Node): Transformer {
    this.setNode(node)
    return this
  }

  setNode(node: Node) {
    Util.warn(
      "tr.setNode(shape), tr.node(shape) and tr.attachTo(shape) methods are deprecated. Please use tr.nodes(nodesArray) instead."
    )
    return this.setNodes([node])
  }

  getNode() {
    return this.#nodes?.[0]
  }

  get nodes(): Node[] {
    return this.#nodes || []
  }

  set nodes(nodes: Node[]) {
    this.setNodes(nodes)
  }

  _getEventNamespace() {
    return EVENTS_NAME + this._id
  }

  setNodes(nodes: Node[] = []) {
    this.detach()

    const filteredNodes = nodes.filter((node) => {
      if (node.isAncestorOf(this)) {
        Util.error(
          "Transformer cannot be an a child of the node you are trying to attach"
        )
        return false
      }
      return true
    })

    this.#nodes = filteredNodes

    if (nodes.length === 1 && this.#useSingleNodeRotation) {
      this.rotation(nodes[0].getAbsoluteRotation())
    } else {
      this.rotation(0)
    }

    for (const node of this.#nodes) {
      const onChange = () => {
        if (this.nodes.length === 1 && this.#useSingleNodeRotation) {
          this.rotation(this.nodes[0].getAbsoluteRotation())
        }

        this._resetTransformCache()
        if (!(this.#transforming || this.isDragging())) {
          this.update()
        }
      }
      if (node._attrsAffectingSize.length) {
        const additionalEvents = node._attrsAffectingSize
          .map((prop) => `${prop}Change.${this._getEventNamespace()}`)
          .join(" ")
        node.on(additionalEvents, onChange)
      }
      node.on(
        TRANSFORM_CHANGE_STR.map(
          (e) => `${e}.${this._getEventNamespace()}`
        ).join(" "),
        onChange
      )
      node.on(`absoluteTransformChange.${this._getEventNamespace()}`, onChange)
      this._proxyDrag(node)
    }

    this._resetTransformCache()
    const elementsCreated = !!this.findOne(".top-left")
    if (elementsCreated) {
      this.update()
    }

    return this
  }

  getNodes() {
    return this.#nodes || []
  }

  _proxyDrag(node: Node) {
    let lastPos: Vector2d | null = null
    node.on(`dragstart.${this._getEventNamespace()}`, (e) => {
      lastPos = node.getAbsolutePosition()
      if (!this.isDragging() && node !== this.findOne(".back")) {
        this.startDrag(e, false)
      }
    })
    node.on(`dragmove.${this._getEventNamespace()}`, (e) => {
      if (!lastPos) {
        return
      }
      const abs = node.getAbsolutePosition()
      const dx = abs.x - lastPos.x
      const dy = abs.y - lastPos.y

      for (const otherNode of this.nodes) {
        if (otherNode === node) {
          continue
        }
        if (otherNode.isDragging()) {
          return
        }
        const otherAbs = otherNode.getAbsolutePosition()
        otherNode.setAbsolutePosition({
          x: otherAbs.x + dx,
          y: otherAbs.y + dy,
        })
        otherNode.startDrag(e)
      }

      lastPos = null
    })
  }

  /**
   * return the name of current active anchor
   */
  getActiveAnchor(): AnchorType | null {
    return this.#movingAnchorName
  }

  /**
   * detach transformer from an attached node
   */
  detach(): Transformer {
    if (this.#nodes) {
      for (const node of this.#nodes) {
        node.off(`.${this._getEventNamespace()}`)
      }
    }
    this.#nodes = []
    this._resetTransformCache()
    this._clearSelectionOutlines()
    return this
  }

  _resetTransformCache(): Transformer {
    this._clearCache(NODES_RECT)
    this._clearCache("transform")
    this._clearSelfAndDescendantCache("absoluteTransform")
    return this
  }

  _getNodeRect() {
    return this._getCache(NODES_RECT, this.__getNodeRect)
  }

  __getNodeShape(node: Node, rot = this.rotation(), relative?: Node) {
    // For Line shapes (draw, brush, marker), always include stroke since the stroke IS the shape
    // For other shapes (geo, etc), respect the ignoreStroke setting
    const isLineShape = node.getClassName() === "Line"
    const rect = node.getClientRect({
      skipTransform: true,
      skipShadow: true,
      skipStroke: isLineShape ? false : this.#ignoreStroke,
    })

    const absScale = node.getAbsoluteScale(relative)
    const absPos = node.getAbsolutePosition(relative)

    const dx = rect.x * absScale.x - node.offsetX() * absScale.x
    const dy = rect.y * absScale.y - node.offsetY() * absScale.y

    const rotation =
      (Konva.getAngle(node.getAbsoluteRotation()) + Math.PI * 2) % (Math.PI * 2)

    const box = {
      x: absPos.x + dx * Math.cos(rotation) + dy * Math.sin(-rotation),
      y: absPos.y + dy * Math.cos(rotation) + dx * Math.sin(rotation),
      width: rect.width * absScale.x,
      height: rect.height * absScale.y,
      rotation,
    }
    return rotateAroundPoint(box, -Konva.getAngle(rot), {
      x: 0,
      y: 0,
    })
  }

  __getNodeRect() {
    const node = this.getNode()
    if (!node) {
      return {
        x: -MAX_SAFE_INTEGER,
        y: -MAX_SAFE_INTEGER,
        width: 0,
        height: 0,
        rotation: 0,
      }
    }

    const totalPoints: Vector2d[] = []

    for (const node of this.nodes) {
      // For Line shapes (draw, brush, marker), always include stroke since the stroke IS the shape
      // For other shapes (geo, etc), respect the ignoreStroke setting
      const isLineShape = node.getClassName() === "Line"
      const box = node.getClientRect({
        skipTransform: true,
        skipShadow: true,
        skipStroke: isLineShape ? false : this.#ignoreStroke,
      })
      const points = [
        { x: box.x, y: box.y },
        { x: box.x + box.width, y: box.y },
        { x: box.x + box.width, y: box.y + box.height },
        { x: box.x, y: box.y + box.height },
      ]
      const trans = node.getAbsoluteTransform()
      for (const point of points) {
        const transformed = trans.point(point)
        totalPoints.push(transformed)
      }
    }

    const tr = new Transform()
    tr.rotate(-Konva.getAngle(this.rotation()))

    let minX: number = Number.POSITIVE_INFINITY,
      minY: number = Number.POSITIVE_INFINITY,
      maxX = Number.NEGATIVE_INFINITY,
      maxY = Number.NEGATIVE_INFINITY

    for (const point of totalPoints) {
      const transformed = tr.point(point)
      if (minX === undefined) {
        minX = maxX = transformed.x
        minY = maxY = transformed.y
      }
      minX = Math.min(minX, transformed.x)
      minY = Math.min(minY, transformed.y)
      maxX = Math.max(maxX, transformed.x)
      maxY = Math.max(maxY, transformed.y)
    }

    tr.invert()
    const p = tr.point({ x: minX, y: minY })
    return {
      x: p.x,
      y: p.y,
      width: maxX - minX,
      height: maxY - minY,
      rotation: Konva.getAngle(this.rotation()),
    }
  }

  getX() {
    return this._getNodeRect().x
  }

  getY() {
    return this._getNodeRect().y
  }

  getWidth() {
    return this._getNodeRect().width
  }

  getHeight() {
    return this._getNodeRect().height
  }

  _createElements() {
    this._createBack()
    this._createSelectionOutlines()

    // Create rotation anchors FIRST (beneath corner anchors)
    for (const name of ROTATER_ANCHORS) {
      this._createRotaterAnchor(name)
    }

    // Create resize anchors on top
    for (const name of ANCHORS_NAMES) {
      this._createAnchor(name)
    }
  }

  _changeCursor(cursor: string | null) {
    const content = this.getStage()?.content
    if (content) {
      content.style.cursor = cursor || ""
      this.#cursorChanged = !!cursor
    }
  }

  _createRotaterAnchor(name: RotaterAnchorName) {
    // Quarter-circle arc for rotation detection at corners
    const anchor = new Arc({
      name: `${name} _rotater`,
      innerRadius: 0,
      outerRadius: ROTATION_ZONE_OUTER_OFFSET,
      angle: 90,
      rotation: ROTATER_BASE_ANGLES[name],
      fill: "transparent",
      draggable: true,
      hitStrokeWidth: 0,
    })

    anchor.on("mousedown touchstart", (e: any) => {
      this._handleRotaterMouseDown(e, name)
    })
    anchor.on("dragstart", (e: any) => {
      anchor.stopDrag()
      e.cancelBubble = true
    })
    anchor.on("dragend", (e: any) => {
      e.cancelBubble = true
    })

    anchor.on("mouseenter", () => {
      if (this.#transforming) return
      if (!this.#rotateEnabled) return
      if (this.#nodes.length !== 1) return

      const node = this.getNode()
      if (!node) return

      const cornerIndex = ROTATER_CORNER_INDEX[name]
      const rotation = node.rotation()
      const cursorAngle = getRotationCursorAngle(cornerIndex, rotation)
      this._changeCursor(createRotationCursor(cursorAngle))
    })

    anchor.on("mouseout", () => {
      if (this.#transforming) return
      this._changeCursor(null)
    })

    this.add(anchor)
  }

  _handleRotaterMouseDown(
    e: KonvaEventObject<PointerEvent>,
    name: RotaterAnchorName
  ) {
    if (this.#transforming) return
    if (!this.#rotateEnabled) return
    if (this.#nodes.length !== 1) return

    e.cancelBubble = true
    this.#movingAnchorName = name

    const nodeRect = this._getNodeRect()
    const center = getCenter(nodeRect)

    const stage = e.target.getStage()
    if (!stage) return

    const pos = stage.getPointerPosition()
    if (!pos) return

    const startAngle = Math.atan2(pos.y - center.y, pos.x - center.x)

    this.#activeRotation = {
      cornerIndex: ROTATER_CORNER_INDEX[name],
      startAngle,
      startRotation: nodeRect.rotation,
      centerX: center.x,
      centerY: center.y,
    }

    this.#transforming = true
    activeTransformersCount++

    if (typeof window !== "undefined") {
      window.addEventListener("mousemove", this._handleRotaterMouseMove)
      window.addEventListener("touchmove", this._handleRotaterMouseMove)
      window.addEventListener("mouseup", this._handleRotaterMouseUp, true)
      window.addEventListener("touchend", this._handleRotaterMouseUp, true)
    }

    this._fire("transformstart", { evt: e.evt, target: this.getNode() })
    for (const target of this.#nodes) {
      target._fire("transformstart", { evt: e.evt, target })
    }
  }

  _handleRotaterMouseMove = (e: MouseEvent | TouchEvent) => {
    if (!this.#activeRotation) return
    if (this.#nodes.length !== 1) return

    const node = this.#nodes[0]
    const stage = node.getStage()
    if (!stage) return

    stage.setPointersPositions(e)
    const pos = stage.getPointerPosition()
    if (!pos) return

    const state = this.#activeRotation

    const dx = pos.x - state.centerX
    const dy = pos.y - state.centerY
    const currentAngle = Math.atan2(dy, dx)

    let angleDiff = currentAngle - state.startAngle
    let newRotationRad = state.startRotation + angleDiff

    // Apply rotation snaps
    if (this.#rotationSnaps.length > 0) {
      const tol = Konva.getAngle(this.#rotationSnapTolerance)
      const snappedRad = getSnap(this.#rotationSnaps, newRotationRad, tol)
      newRotationRad = snappedRad
      angleDiff = newRotationRad - state.startRotation
    }

    // Use rotateAroundCenter + _fitNodesInto for proper rotation
    // This correctly handles nodes with any offset/pivot configuration
    const attrs = this._getNodeRect()
    const diff = newRotationRad - attrs.rotation
    const rotatedShape = rotateAroundCenter(attrs, diff)
    this._fitNodesInto(rotatedShape, e)

    // Update cursor angle during rotation
    const cursorAngle = getRotationCursorAngle(
      state.cornerIndex,
      Util.radToDeg(newRotationRad)
    )
    this._changeCursor(createRotationCursor(cursorAngle))
  }

  _handleRotaterMouseUp = () => {
    if (!this.#activeRotation) return

    if (typeof window !== "undefined") {
      window.removeEventListener("mousemove", this._handleRotaterMouseMove)
      window.removeEventListener("touchmove", this._handleRotaterMouseMove)
      window.removeEventListener("mouseup", this._handleRotaterMouseUp, true)
      window.removeEventListener("touchend", this._handleRotaterMouseUp, true)
    }

    this.#activeRotation = null
    this.#transforming = false
    this.#movingAnchorName = null
    activeTransformersCount--
    this._changeCursor(null)

    const node = this.getNode()
    this._fire("transformend", { evt: null, target: node })
    this.getLayer()?.batchDraw()

    if (node) {
      for (const target of this.#nodes) {
        target._fire("transformend", { evt: null, target })
        target.getLayer()?.batchDraw()
      }
    }
  }

  _createAnchor(name: AnchorName) {
    const isCorner = CORNER_ANCHORS.includes(name)

    // Use Circle for corner anchors, Rect for edge anchors
    const anchor: any = isCorner
      ? new Circle({
          stroke: "rgb(0, 161, 255)",
          fill: "white",
          strokeWidth: 1,
          name: `${name} _anchor`,
          dragDistance: 0,
          draggable: true,
          hitStrokeWidth: TOUCH_DEVICE ? 10 : "auto",
        })
      : new Rect({
          fill: "transparent",
          opacity: 0,
          name: `${name} _anchor`,
          dragDistance: 0,
          draggable: true,
          hitStrokeWidth: TOUCH_DEVICE ? 10 : "auto",
        })

    anchor.on("mousedown touchstart", (e: any) => {
      this._handleMouseDown(e)
    })
    anchor.on("dragstart", (e: any) => {
      anchor.stopDrag()
      e.cancelBubble = true
    })
    anchor.on("dragend", (e: any) => {
      e.cancelBubble = true
    })

    anchor.on("mouseenter", () => {
      const rad = Konva.getAngle(this.rotation())
      const cursor = getCursor(name, rad)
      this._changeCursor(cursor)
    })
    anchor.on("mouseout", () => {
      this._changeCursor(null)
    })
    this.add(anchor)
  }

  _createBack() {
    const back = new Konva.Shape({
      name: "back",
      width: 0,
      height: 0,
      sceneFunc: (ctx, shape) => {
        const tr = shape.getParent() as Transformer
        const padding = tr.padding
        ctx.beginPath()
        ctx.rect(
          -padding,
          -padding,
          shape.width() + padding * 2,
          shape.height() + padding * 2
        )
        ctx.moveTo(shape.width() / 2, -padding)
        ctx.fillStrokeShape(shape)
      },
      hitFunc: (ctx, shape) => {
        if (!this.#shouldOverdrawWholeArea) {
          return
        }
        const padding = this.#padding
        ctx.beginPath()
        ctx.rect(
          -padding,
          -padding,
          shape.width() + padding * 2,
          shape.height() + padding * 2
        )
        ctx.fillStrokeShape(shape)
      },
    })
    this.add(back)
    this._proxyDrag(back)
    back.on("dragstart", (e) => {
      e.cancelBubble = true
    })
    back.on("dragmove", (e) => {
      e.cancelBubble = true
    })
    back.on("dragend", (e) => {
      e.cancelBubble = true
    })
    this.on("dragmove", (_e) => {
      this.update()
    })
  }

  _createSelectionOutlines() {
    const outlines = new Group({
      name: SELECTION_OUTLINES_NAME,
      listening: false,
    })
    this.add(outlines)
  }

  _getSelectionOutlinesGroup(): Group | null {
    return this.findOne(`.${SELECTION_OUTLINES_NAME}`) as Group | null
  }

  _clearSelectionOutlines() {
    const outlines = this._getSelectionOutlinesGroup()
    if (!outlines) return
    outlines.destroyChildren()
  }

  _getTransformerLocalPoint(point: Vector2d): Vector2d {
    const transform = this.getAbsoluteTransform().copy()
    transform.invert()
    return transform.point(point)
  }

  _getNodePointInTransformer(node: Node, point: Vector2d): Vector2d {
    return this._getTransformerLocalPoint(
      node.getAbsoluteTransform().point(point)
    )
  }

  _getNodeRelativeTransformAttrs(node: Node) {
    const relativeTransform = this.getAbsoluteTransform()
      .copy()
      .invert()
      .multiply(node.getAbsoluteTransform())

    return relativeTransform.decompose()
  }

  _createOutlineLine(
    points: number[],
    closed: boolean,
    attrs: Record<string, unknown> = {}
  ) {
    if (points.length < 4) return null

    return new Line({
      name: SELECTION_OUTLINE_ITEM_NAME,
      points,
      closed,
      stroke: this.#borderStroke,
      strokeWidth: this.#borderStrokeWidth,
      dash: this.#borderDash,
      fillEnabled: false,
      listening: false,
      perfectDrawEnabled: false,
      strokeScaleEnabled: false,
      ...attrs,
    })
  }

  _createLineNodeOutline(node: Node) {
    const points = ((node as any).points?.() ?? []) as number[]
    if (points.length < 4) return null

    const outlinePoints: number[] = []
    for (let index = 0; index < points.length; index += 2) {
      const point = this._getNodePointInTransformer(node, {
        x: points[index],
        y: points[index + 1],
      })
      outlinePoints.push(point.x, point.y)
    }

    return this._createOutlineLine(
      outlinePoints,
      Boolean((node as any).closed?.()),
      {
        lineCap: (node as any).lineCap?.(),
        lineJoin: (node as any).lineJoin?.(),
        tension: (node as any).tension?.(),
      }
    )
  }

  _createGeoLineOutline(node: Node) {
    const start = this._getNodePointInTransformer(node, { x: 0, y: 0 })
    const end = this._getNodePointInTransformer(node, {
      x: node.width(),
      y: node.height(),
    })

    return this._createOutlineLine([start.x, start.y, end.x, end.y], false, {
      lineCap: "round",
      lineJoin: "round",
    })
  }

  _createPathNodeOutline(node: Node) {
    const data = (node as any).data?.()
    if (typeof data !== "string" || data.length === 0) return null

    const outline = new Path({
      name: SELECTION_OUTLINE_ITEM_NAME,
      data,
      stroke: this.#borderStroke,
      strokeWidth: this.#borderStrokeWidth,
      dash: this.#borderDash,
      fillEnabled: false,
      listening: false,
      perfectDrawEnabled: false,
      strokeScaleEnabled: false,
    })
    outline.setAttrs(this._getNodeRelativeTransformAttrs(node) as any)
    return outline
  }

  _createEllipseNodeOutline(node: Node) {
    const width = Math.abs(node.width())
    const height = Math.abs(node.height())
    if (width <= 0 || height <= 0) return null

    const outline = new Ellipse({
      name: SELECTION_OUTLINE_ITEM_NAME,
      radiusX: width / 2,
      radiusY: height / 2,
      stroke: this.#borderStroke,
      strokeWidth: this.#borderStrokeWidth,
      dash: this.#borderDash,
      fillEnabled: false,
      listening: false,
      perfectDrawEnabled: false,
      strokeScaleEnabled: false,
    })
    outline.setAttrs(this._getNodeRelativeTransformAttrs(node) as any)
    return outline
  }

  _createNodeBoundsOutline(node: Node) {
    const isLineShape = node.getClassName() === "Line"
    const rect = node.getClientRect({
      skipTransform: true,
      skipShadow: true,
      skipStroke: isLineShape ? false : this.#ignoreStroke,
    })

    if (rect.width <= 0 || rect.height <= 0) return null

    const points = [
      { x: rect.x, y: rect.y },
      { x: rect.x + rect.width, y: rect.y },
      { x: rect.x + rect.width, y: rect.y + rect.height },
      { x: rect.x, y: rect.y + rect.height },
    ]

    const outlinePoints = points.flatMap((point) => {
      const transformedPoint = this._getNodePointInTransformer(node, point)
      return [transformedPoint.x, transformedPoint.y]
    })

    return this._createOutlineLine(outlinePoints, true)
  }

  _createSelectionOutlineForNode(node: Node) {
    const className = node.getClassName()
    const names = node.name().split(/\s+/)

    if (className === "Line") {
      return this._createLineNodeOutline(node)
    }

    if (names.includes("geo-line")) {
      return this._createGeoLineOutline(node)
    }

    if (className === "Path") {
      return this._createPathNodeOutline(node)
    }

    if (names.includes("geo-ellipse")) {
      return this._createEllipseNodeOutline(node)
    }

    return this._createNodeBoundsOutline(node)
  }

  _updateSelectionOutlines() {
    const outlines = this._getSelectionOutlinesGroup()
    if (!outlines) return

    outlines.destroyChildren()
    outlines.visible(this.#borderEnabled && this.#nodes.length > 1)

    if (!outlines.visible()) return

    for (const node of this.#nodes) {
      const outline = this._createSelectionOutlineForNode(node)
      if (outline) {
        outlines.add(outline)
      }
    }
  }

  _handleMouseDown(e: KonvaEventObject<PointerEvent>) {
    if (this.#transforming) {
      return
    }
    this.#movingAnchorName = e.target.name().split(" ")[0] as AnchorType

    const attrs = this._getNodeRect()
    const width = attrs.width
    const height = attrs.height

    const hypotenuse = Math.sqrt(width ** 2 + height ** 2)
    this.sin = Math.abs(height / hypotenuse)
    this.cos = Math.abs(width / hypotenuse)

    if (typeof window !== "undefined") {
      window.addEventListener("mousemove", this._handleMouseMove)
      window.addEventListener("touchmove", this._handleMouseMove)
      window.addEventListener("mouseup", this._handleMouseUp, true)
      window.addEventListener("touchend", this._handleMouseUp, true)
    }

    this.#transforming = true
    const ap = e.target.getAbsolutePosition()
    const pos = e.target.getStage()?.getPointerPosition()
    if (pos) {
      this.#anchorDragOffset = {
        x: pos.x - ap.x,
        y: pos.y - ap.y,
      }
    }
    activeTransformersCount++
    this._fire("transformstart", { evt: e.evt, target: this.getNode() })
    for (const target of this.#nodes) {
      target._fire("transformstart", { evt: e.evt, target })
    }
  }

  _handleMouseMove(e: MouseEvent | TouchEvent) {
    let x: number, y: number, newHypotenuse: number
    const anchorNode = this.findOne(`.${this.#movingAnchorName}`)!
    const stage = anchorNode.getStage()!

    stage.setPointersPositions(e)

    const pp = stage.getPointerPosition()!
    let newNodePos = {
      x: pp.x - this.#anchorDragOffset.x,
      y: pp.y - this.#anchorDragOffset.y,
    }
    const oldAbs = anchorNode.getAbsolutePosition()

    if (this.#anchorDragBoundFunc) {
      newNodePos = this.#anchorDragBoundFunc(oldAbs, newNodePos, e)
    }
    anchorNode.setAbsolutePosition(newNodePos)
    const newAbs = anchorNode.getAbsolutePosition()

    if (oldAbs.x === newAbs.x && oldAbs.y === newAbs.y) {
      return
    }

    /*
     * This is the builtin transformer's single rotater logic, keep it for reference
    // rotater is working very differently, so do it first
    if (this.#movingAnchorName === "rotater") {
      const attrs = this._getNodeRect()
      x = anchorNode.x() - attrs.width / 2
      y = -anchorNode.y() + attrs.height / 2

      let delta = Math.atan2(-y, x) + Math.PI / 2

      if (attrs.height < 0) {
        delta -= Math.PI
      }

      const oldRotation = Konva.getAngle(this.rotation())
      const newRotation = oldRotation + delta

      const tol = Konva.getAngle(this.#rotationSnapTolerance)
      const snappedRot = getSnap(this.#rotationSnaps, newRotation, tol)

      const diff = snappedRot - attrs.rotation

      const shape = rotateAroundCenter(attrs, diff)
      this._fitNodesInto(shape, e)
      return
    }
    */

    let keepProportion: boolean
    if (this.#shiftBehavior === "inverted") {
      keepProportion = this.#keepRatio && !e.shiftKey
    } else if (this.#shiftBehavior === "none") {
      keepProportion = this.#keepRatio
    } else {
      keepProportion = this.#keepRatio || e.shiftKey
    }

    let centeredScaling = this.#centeredScaling || e.altKey

    if (this.#movingAnchorName === "top-left") {
      if (keepProportion) {
        const comparePoint = centeredScaling
          ? {
              x: this.width() / 2,
              y: this.height() / 2,
            }
          : {
              x: this.findOne(".bottom-right")!.x(),
              y: this.findOne(".bottom-right")!.y(),
            }
        newHypotenuse = Math.sqrt(
          (comparePoint.x - anchorNode.x()) ** 2 +
            (comparePoint.y - anchorNode.y()) ** 2
        )

        const reverseX =
          this.findOne(".top-left")!.x() > comparePoint.x ? -1 : 1

        const reverseY =
          this.findOne(".top-left")!.y() > comparePoint.y ? -1 : 1

        x = newHypotenuse * this.cos * reverseX
        y = newHypotenuse * this.sin * reverseY

        this.findOne(".top-left")!.x(comparePoint.x - x)
        this.findOne(".top-left")!.y(comparePoint.y - y)
      }
    } else if (this.#movingAnchorName === "top-center") {
      this.findOne(".top-left")!.y(anchorNode.y())
    } else if (this.#movingAnchorName === "top-right") {
      if (keepProportion) {
        const comparePoint = centeredScaling
          ? {
              x: this.width() / 2,
              y: this.height() / 2,
            }
          : {
              x: this.findOne(".bottom-left")!.x(),
              y: this.findOne(".bottom-left")!.y(),
            }

        newHypotenuse = Math.sqrt(
          (anchorNode.x() - comparePoint.x) ** 2 +
            (comparePoint.y - anchorNode.y()) ** 2
        )

        const reverseX =
          this.findOne(".top-right")!.x() < comparePoint.x ? -1 : 1

        const reverseY =
          this.findOne(".top-right")!.y() > comparePoint.y ? -1 : 1

        x = newHypotenuse * this.cos * reverseX
        y = newHypotenuse * this.sin * reverseY

        this.findOne(".top-right")!.x(comparePoint.x + x)
        this.findOne(".top-right")!.y(comparePoint.y - y)
      }
      const pos = anchorNode.position()
      this.findOne(".top-left")!.y(pos.y)
      this.findOne(".bottom-right")!.x(pos.x)
    } else if (this.#movingAnchorName === "middle-left") {
      this.findOne(".top-left")!.x(anchorNode.x())
    } else if (this.#movingAnchorName === "middle-right") {
      this.findOne(".bottom-right")!.x(anchorNode.x())
    } else if (this.#movingAnchorName === "bottom-left") {
      if (keepProportion) {
        const comparePoint = centeredScaling
          ? {
              x: this.width() / 2,
              y: this.height() / 2,
            }
          : {
              x: this.findOne(".top-right")!.x(),
              y: this.findOne(".top-right")!.y(),
            }

        newHypotenuse = Math.sqrt(
          (comparePoint.x - anchorNode.x()) ** 2 +
            (anchorNode.y() - comparePoint.y) ** 2
        )

        const reverseX = comparePoint.x < anchorNode.x() ? -1 : 1

        const reverseY = anchorNode.y() < comparePoint.y ? -1 : 1

        x = newHypotenuse * this.cos * reverseX
        y = newHypotenuse * this.sin * reverseY

        anchorNode.x(comparePoint.x - x)
        anchorNode.y(comparePoint.y + y)
      }

      const pos = anchorNode.position()

      this.findOne(".top-left")!.x(pos.x)
      this.findOne(".bottom-right")!.y(pos.y)
    } else if (this.#movingAnchorName === "bottom-center") {
      this.findOne(".bottom-right")!.y(anchorNode.y())
    } else if (this.#movingAnchorName === "bottom-right") {
      if (keepProportion) {
        const comparePoint = centeredScaling
          ? {
              x: this.width() / 2,
              y: this.height() / 2,
            }
          : {
              x: this.findOne(".top-left")!.x(),
              y: this.findOne(".top-left")!.y(),
            }

        newHypotenuse = Math.sqrt(
          (anchorNode.x() - comparePoint.x) ** 2 +
            (anchorNode.y() - comparePoint.y) ** 2
        )

        const reverseX =
          this.findOne(".bottom-right")!.x() < comparePoint.x ? -1 : 1

        const reverseY =
          this.findOne(".bottom-right")!.y() < comparePoint.y ? -1 : 1

        x = newHypotenuse * this.cos * reverseX
        y = newHypotenuse * this.sin * reverseY

        this.findOne(".bottom-right")!.x(comparePoint.x + x)
        this.findOne(".bottom-right")!.y(comparePoint.y + y)
      }
    } else {
      console.error(
        new Error(
          "Wrong position argument of selection resizer: " +
            this.#movingAnchorName
        )
      )
    }

    centeredScaling = this.#centeredScaling || e.altKey
    if (centeredScaling) {
      const topLeft = this.findOne(".top-left")!
      const bottomRight = this.findOne(".bottom-right")!
      const topOffsetX = topLeft.x()
      const topOffsetY = topLeft.y()

      const bottomOffsetX = this.getWidth() - bottomRight.x()
      const bottomOffsetY = this.getHeight() - bottomRight.y()

      bottomRight.move({
        x: -topOffsetX,
        y: -topOffsetY,
      })

      topLeft.move({
        x: bottomOffsetX,
        y: bottomOffsetY,
      })
    }

    const absPos = this.findOne(".top-left")!.getAbsolutePosition()

    x = absPos.x
    y = absPos.y

    const width =
      this.findOne(".bottom-right")!.x() - this.findOne(".top-left")!.x()

    const height =
      this.findOne(".bottom-right")!.y() - this.findOne(".top-left")!.y()

    this._fitNodesInto(
      {
        x,
        y,
        width,
        height,
        rotation: Konva.getAngle(this.rotation()),
      },
      e
    )
  }

  _handleMouseUp(e: MouseEvent | TouchEvent) {
    this._removeEvents(e)
  }

  getAbsoluteTransform() {
    return this.getTransform()
  }

  _removeEvents(e?: MouseEvent | TouchEvent) {
    if (this.#transforming) {
      this.#transforming = false
      this._changeCursor(null)

      if (typeof window !== "undefined") {
        window.removeEventListener("mousemove", this._handleMouseMove)
        window.removeEventListener("touchmove", this._handleMouseMove)
        window.removeEventListener("mouseup", this._handleMouseUp, true)
        window.removeEventListener("touchend", this._handleMouseUp, true)
      }
      const node = this.getNode()
      activeTransformersCount--
      this._fire("transformend", { evt: e, target: node })
      this.getLayer()?.batchDraw()

      if (node) {
        for (const target of this.#nodes) {
          target._fire("transformend", { evt: e, target })
          target.getLayer()?.batchDraw()
        }
      }
      this.#movingAnchorName = null
    }
  }

  _fitNodesInto(
    newAttrs: ReturnType<typeof rotateAroundCenter>,
    evt?: MouseEvent | TouchEvent
  ) {
    const oldAttrs = this._getNodeRect()

    const minSize = 1

    if (Util._inRange(newAttrs.width, -this.#padding * 2 - minSize, minSize)) {
      this.update()
      return
    }
    if (Util._inRange(newAttrs.height, -this.#padding * 2 - minSize, minSize)) {
      this.update()
      return
    }

    const t = new Transform()
    t.rotate(Konva.getAngle(this.rotation()))
    if (
      this.#movingAnchorName &&
      newAttrs.width < 0 &&
      this.#movingAnchorName.indexOf("left") >= 0
    ) {
      const offset = t.point({
        x: -this.#padding * 2,
        y: 0,
      })
      newAttrs.x += offset.x
      newAttrs.y += offset.y
      newAttrs.width += this.#padding * 2
      this.#movingAnchorName = this.#movingAnchorName.replace(
        "left",
        "right"
      ) as AnchorType
      this.#anchorDragOffset.x -= offset.x
      this.#anchorDragOffset.y -= offset.y
    } else if (
      this.#movingAnchorName &&
      newAttrs.width < 0 &&
      this.#movingAnchorName.indexOf("right") >= 0
    ) {
      const offset = t.point({
        x: this.#padding * 2,
        y: 0,
      })
      this.#movingAnchorName = this.#movingAnchorName.replace(
        "right",
        "left"
      ) as AnchorType
      this.#anchorDragOffset.x -= offset.x
      this.#anchorDragOffset.y -= offset.y
      newAttrs.width += this.#padding * 2
    }
    if (
      this.#movingAnchorName &&
      newAttrs.height < 0 &&
      this.#movingAnchorName.indexOf("top") >= 0
    ) {
      const offset = t.point({
        x: 0,
        y: -this.#padding * 2,
      })
      newAttrs.x += offset.x
      newAttrs.y += offset.y
      this.#movingAnchorName = this.#movingAnchorName.replace(
        "top",
        "bottom"
      ) as AnchorType
      this.#anchorDragOffset.x -= offset.x
      this.#anchorDragOffset.y -= offset.y
      newAttrs.height += this.#padding * 2
    } else if (
      this.#movingAnchorName &&
      newAttrs.height < 0 &&
      this.#movingAnchorName.indexOf("bottom") >= 0
    ) {
      const offset = t.point({
        x: 0,
        y: this.#padding * 2,
      })
      this.#movingAnchorName = this.#movingAnchorName.replace(
        "bottom",
        "top"
      ) as AnchorType
      this.#anchorDragOffset.x -= offset.x
      this.#anchorDragOffset.y -= offset.y
      newAttrs.height += this.#padding * 2
    }

    let newAttrs2 = newAttrs

    if (this.#boundBoxFunc) {
      const bounded = this.#boundBoxFunc(oldAttrs, newAttrs)
      if (bounded) {
        newAttrs2 = bounded
      } else {
        Util.warn(
          "boundBoxFunc returned falsy. You should return new bound rect from it!"
        )
      }
    }

    const baseSize = 10_000_000
    const oldTr = new Transform()
    oldTr.translate(oldAttrs.x, oldAttrs.y)
    oldTr.rotate(oldAttrs.rotation)
    oldTr.scale(oldAttrs.width / baseSize, oldAttrs.height / baseSize)

    const newTr = new Transform()
    const newScaleX = newAttrs2.width / baseSize
    const newScaleY = newAttrs2.height / baseSize

    if (this.#flipEnabled === false) {
      newTr.translate(newAttrs2.x, newAttrs2.y)
      newTr.rotate(newAttrs2.rotation)
      newTr.translate(
        newAttrs2.width < 0 ? newAttrs2.width : 0,
        newAttrs2.height < 0 ? newAttrs2.height : 0
      )
      newTr.scale(Math.abs(newScaleX), Math.abs(newScaleY))
    } else {
      newTr.translate(newAttrs2.x, newAttrs2.y)
      newTr.rotate(newAttrs2.rotation)
      newTr.scale(newScaleX, newScaleY)
    }

    const delta = newTr.multiply(oldTr.invert())

    for (const node of this.#nodes) {
      if (!node.getStage()) {
        return
      }
      const parentTransform = node.getParent()!.getAbsoluteTransform()
      const localTransform = node.getTransform().copy()
      localTransform.translate(node.offsetX(), node.offsetY())

      const newLocalTransform = new Transform()
      newLocalTransform
        .multiply(parentTransform.copy().invert())
        .multiply(delta)
        .multiply(parentTransform)
        .multiply(localTransform)

      const attrs = newLocalTransform.decompose()

      const nodeType = node.getClassName()
      if (nodeType === "Line" || nodeType === "Path" || nodeType === "Group") {
        // update scaleX/scaleY for line and path
        node.setAttrs(attrs)
      } else {
        // update width/height for all other types (including geo shapes)
        node.setAttrs({
          ...attrs,
          width: node.width() * attrs.scaleX,
          height: node.height() * attrs.scaleY,
          scaleX: 1,
          scaleY: 1,
        })
      }

      node.getLayer()?.batchDraw()
    }

    this.rotation(Util._getRotation(newAttrs2.rotation))
    for (const node of this.#nodes) {
      this._fire("transform", { evt, target: node })
      node._fire("transform", { evt, target: node })
    }
    this._resetTransformCache()
    this.update()
    this.getLayer()?.batchDraw()
  }

  /**
   * force update of Transformer.
   * Use it when you updated attached Group and now you need to reset transformer size
   */
  forceUpdate() {
    this._resetTransformCache()
    this.update()
  }

  _batchChangeChild(selector: string, attrs: Record<string, unknown>) {
    const anchor = this.findOne(selector)!
    anchor.setAttrs(attrs)
  }

  update() {
    const attrs = this._getNodeRect()
    this.rotation(Util._getRotation(attrs.rotation))
    const width = attrs.width
    const height = attrs.height

    const enabledAnchors = this.#enabledAnchors
    const resizeEnabled = this.#resizeEnabled
    const padding = this.#padding

    const anchorSize = this.#anchorSize
    const anchorRadius = anchorSize / 2
    const edgeMargin = anchorSize + 4 // gap between corner and edge anchors

    // Check if all nodes have transformsEnabled === "none" (non-transformable)
    const allNodesNonTransformable =
      this.#nodes.length > 0 &&
      this.#nodes.every((node) => node.transformsEnabled() === "none")

    // Update corner anchors (Circle shapes)
    const cornerAnchors = this.find<Circle>("._anchor").filter((node) =>
      CORNER_ANCHORS.includes(node.name().split(" ")[0] as AnchorName)
    )
    for (const node of cornerAnchors) {
      node.setAttrs({
        radius: anchorRadius,
        stroke: this.#anchorStroke,
        strokeWidth: this.#anchorStrokeWidth,
        fill: this.#anchorFill,
      })
    }

    // Update edge anchors (Rect shapes)
    const edgeAnchors = this.find<Rect>("._anchor").filter(
      (node) =>
        !CORNER_ANCHORS.includes(node.name().split(" ")[0] as AnchorName)
    )
    for (const node of edgeAnchors) {
      node.setAttrs({
        stroke: this.#anchorStroke,
        strokeWidth: this.#anchorStrokeWidth,
        fill: this.#anchorFill,
        cornerRadius: this.#anchorCornerRadius,
      })
    }

    // Corner anchors (circles positioned at corners)
    this._batchChangeChild(".top-left", {
      x: -padding,
      y: -padding,
      visible:
        !allNodesNonTransformable &&
        resizeEnabled &&
        enabledAnchors.indexOf("top-left") >= 0,
    })
    this._batchChangeChild(".top-right", {
      x: width + padding,
      y: -padding,
      visible:
        !allNodesNonTransformable &&
        resizeEnabled &&
        enabledAnchors.indexOf("top-right") >= 0,
    })
    this._batchChangeChild(".bottom-left", {
      x: -padding,
      y: height + padding,
      visible:
        !allNodesNonTransformable &&
        resizeEnabled &&
        enabledAnchors.indexOf("bottom-left") >= 0,
    })
    this._batchChangeChild(".bottom-right", {
      x: width + padding,
      y: height + padding,
      visible:
        !allNodesNonTransformable &&
        resizeEnabled &&
        enabledAnchors.indexOf("bottom-right") >= 0,
    })

    // Edge anchors (bars spanning full edge with margin to corners)
    const horizontalEdgeWidth = Math.max(0, width - 2 * edgeMargin)
    const verticalEdgeHeight = Math.max(0, height - 2 * edgeMargin)

    this._batchChangeChild(".top-center", {
      x: edgeMargin,
      y: -padding,
      width: horizontalEdgeWidth,
      height: anchorSize,
      offsetX: 0,
      offsetY: anchorRadius,
      visible:
        !allNodesNonTransformable &&
        resizeEnabled &&
        enabledAnchors.indexOf("top-center") >= 0,
    })
    this._batchChangeChild(".bottom-center", {
      x: edgeMargin,
      y: height + padding,
      width: horizontalEdgeWidth,
      height: anchorSize,
      offsetX: 0,
      offsetY: anchorRadius,
      visible:
        !allNodesNonTransformable &&
        resizeEnabled &&
        enabledAnchors.indexOf("bottom-center") >= 0,
    })
    this._batchChangeChild(".middle-left", {
      x: -padding,
      y: edgeMargin,
      width: anchorSize,
      height: verticalEdgeHeight,
      offsetX: anchorRadius,
      offsetY: 0,
      visible:
        !allNodesNonTransformable &&
        resizeEnabled &&
        enabledAnchors.indexOf("middle-left") >= 0,
    })
    this._batchChangeChild(".middle-right", {
      x: width + padding,
      y: edgeMargin,
      width: anchorSize,
      height: verticalEdgeHeight,
      offsetX: anchorRadius,
      offsetY: 0,
      visible:
        !allNodesNonTransformable &&
        resizeEnabled &&
        enabledAnchors.indexOf("middle-right") >= 0,
    })

    // Update rotation anchors (quarter-circle arcs at corners)
    // Each arc is centered at the corner and extends outward
    this._batchChangeChild(".rotater-top-left", {
      x: -padding,
      y: -padding,
      outerRadius: ROTATION_ZONE_OUTER_OFFSET,
      visible: !allNodesNonTransformable && this.#rotateEnabled,
    })
    this._batchChangeChild(".rotater-top-right", {
      x: width + padding,
      y: -padding,
      outerRadius: ROTATION_ZONE_OUTER_OFFSET,
      visible: !allNodesNonTransformable && this.#rotateEnabled,
    })
    this._batchChangeChild(".rotater-bottom-left", {
      x: -padding,
      y: height + padding,
      outerRadius: ROTATION_ZONE_OUTER_OFFSET,
      visible: !allNodesNonTransformable && this.#rotateEnabled,
    })
    this._batchChangeChild(".rotater-bottom-right", {
      x: width + padding,
      y: height + padding,
      outerRadius: ROTATION_ZONE_OUTER_OFFSET,
      visible: !allNodesNonTransformable && this.#rotateEnabled,
    })

    this._batchChangeChild(".back", {
      width,
      height,
      visible: this.#borderEnabled,
      stroke: this.#borderStroke,
      strokeWidth: this.#borderStrokeWidth,
      dash: this.#borderDash,
      draggable: this.nodes.some((node) => node.draggable()),
      x: 0,
      y: 0,
    })

    this._updateSelectionOutlines()

    const styleFunc = this.#anchorStyleFunc
    if (styleFunc) {
      const anchors = this.find<Rect | Circle>("._anchor")
      for (const node of anchors) {
        styleFunc(node)
      }
    }
    this.getLayer()?.batchDraw()
  }

  /**
   * determine if transformer is in active transform
   */
  isTransforming(): boolean {
    return this.#transforming
  }

  /**
   * Stop active transform action
   */
  stopTransform() {
    if (this.#transforming) {
      this._removeEvents()
      const anchorNode = this.findOne(`.${this.#movingAnchorName}`)
      if (anchorNode) {
        anchorNode.stopDrag()
      }
    }
  }

  destroy() {
    if (this.getStage() && this.#cursorChanged && this.getStage()?.content) {
      this.getStage()!.content.style.cursor = ""
    }
    Group.prototype.destroy.call(this)
    this.detach()
    this._removeEvents()
    return this
  }

  toObject() {
    return Node.prototype.toObject.call(this)
  }

  clone(obj?: TransformerConfig) {
    const node = Node.prototype.clone.call(this, obj)
    return node as this
  }

  getClientRect() {
    if (this.nodes.length > 0) {
      return super.getClientRect()
    }
    return { x: 0, y: 0, width: 0, height: 0 }
  }
}
