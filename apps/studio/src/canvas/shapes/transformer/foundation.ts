import Konva from "konva"
import { Group } from "konva/lib/Group"
import type { KonvaEventObject, Node } from "konva/lib/Node"
import type { Circle } from "konva/lib/shapes/Circle"
import type { Rect } from "konva/lib/shapes/Rect"
import type { Vector2d } from "konva/lib/types"
import { Transform, Util } from "konva/lib/Util"
import {
  ANCHORS_NAMES,
  type AnchorType,
  ATTR_CHANGE_LIST,
  type Box,
  EVENTS_NAME,
  MAX_SAFE_INTEGER,
  NODES_RECT,
  type rotateAroundCenter,
  rotateAroundPoint,
  TRANSFORM_CHANGE_STR,
  type TransformerConfig,
  transformerRuntime,
  validateAnchors,
} from "./shared"

/**
 * Transformer constructor. Transformer is a special type of group that allow you transform Konva
 * primitives and shapes. This version updates width/height instead of scaleX/scaleY.
 */
export abstract class TransformerFoundation extends Group {
  // Internal state
  _nodes: Node[] = []
  _movingAnchorName: AnchorType | null = null
  _transforming = false
  _anchorDragOffset: Vector2d = { x: 0, y: 0 }
  _cursorChanged = false
  sin = 0
  cos = 0

  // Corner-based rotation state
  _activeRotation: {
    cornerIndex: number
    startAngle: number
    startRotation: number
    centerX: number
    centerY: number
  } | null = null

  // Configuration private fields
  _enabledAnchors: string[] = [...ANCHORS_NAMES]
  _flipEnabled = true
  _resizeEnabled = true
  _anchorSize = 10
  _rotateEnabled = true
  _rotationSnaps: number[] = []
  _rotationSnapTolerance = 5
  _borderEnabled = true
  _anchorStroke = "rgb(0, 161, 255)"
  _anchorStrokeWidth = 1
  _anchorFill = "white"
  _anchorCornerRadius = 0
  _borderStroke = "rgb(0, 161, 255)"
  _borderStrokeWidth = 1
  _borderDash: number[] | undefined
  _keepRatio = true
  _shiftBehavior = "default"
  _centeredScaling = false
  _ignoreStroke = true
  _padding = 0
  _boundBoxFunc: ((oldBox: Box, newBox: Box) => Box) | undefined
  _anchorDragBoundFunc:
    | ((
        oldPos: Vector2d,
        newPos: Vector2d,
        evt: MouseEvent | TouchEvent
      ) => Vector2d)
    | undefined
  _anchorStyleFunc: ((anchor: Rect | Circle) => void) | null = null
  _shouldOverdrawWholeArea = false
  _useSingleNodeRotation = true

  static isTransforming = () => {
    return transformerRuntime.activeCount > 0
  }

  constructor(config?: TransformerConfig) {
    super(config)

    // Apply config values
    if (config) {
      if (config.enabledAnchors !== undefined)
        this._enabledAnchors = validateAnchors(config.enabledAnchors)
      if (config.flipEnabled !== undefined)
        this._flipEnabled = config.flipEnabled
      if (config.resizeEnabled !== undefined)
        this._resizeEnabled = config.resizeEnabled
      if (config.anchorSize !== undefined) this._anchorSize = config.anchorSize
      if (config.rotateEnabled !== undefined)
        this._rotateEnabled = config.rotateEnabled
      if (config.rotationSnaps !== undefined)
        this._rotationSnaps = config.rotationSnaps
      if (config.rotationSnapTolerance !== undefined)
        this._rotationSnapTolerance = config.rotationSnapTolerance
      if (config.borderEnabled !== undefined)
        this._borderEnabled = config.borderEnabled
      if (config.anchorStroke !== undefined)
        this._anchorStroke = config.anchorStroke
      if (config.anchorStrokeWidth !== undefined)
        this._anchorStrokeWidth = config.anchorStrokeWidth
      if (config.anchorFill !== undefined) this._anchorFill = config.anchorFill
      if (config.anchorCornerRadius !== undefined)
        this._anchorCornerRadius = config.anchorCornerRadius
      if (config.borderStroke !== undefined)
        this._borderStroke = config.borderStroke
      if (config.borderStrokeWidth !== undefined)
        this._borderStrokeWidth = config.borderStrokeWidth
      if (config.borderDash !== undefined) this._borderDash = config.borderDash
      if (config.keepRatio !== undefined) this._keepRatio = config.keepRatio
      if (config.shiftBehavior !== undefined)
        this._shiftBehavior = config.shiftBehavior
      if (config.centeredScaling !== undefined)
        this._centeredScaling = config.centeredScaling
      if (config.ignoreStroke !== undefined)
        this._ignoreStroke = config.ignoreStroke
      if (config.boundBoxFunc !== undefined)
        this._boundBoxFunc = config.boundBoxFunc
      if (config.anchorDragBoundFunc !== undefined)
        this._anchorDragBoundFunc = config.anchorDragBoundFunc
      if (config.anchorStyleFunc !== undefined)
        this._anchorStyleFunc = config.anchorStyleFunc
      if (config.shouldOverdrawWholeArea !== undefined)
        this._shouldOverdrawWholeArea = config.shouldOverdrawWholeArea
      if (config.useSingleNodeRotation !== undefined)
        this._useSingleNodeRotation = config.useSingleNodeRotation
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
    return this._enabledAnchors
  }
  set enabledAnchors(value: string[]) {
    this._enabledAnchors = validateAnchors(value)
  }

  get flipEnabled(): boolean {
    return this._flipEnabled
  }
  set flipEnabled(value: boolean) {
    this._flipEnabled = value
  }

  get resizeEnabled(): boolean {
    return this._resizeEnabled
  }
  set resizeEnabled(value: boolean) {
    this._resizeEnabled = value
  }

  get anchorSize(): number {
    return this._anchorSize
  }
  set anchorSize(value: number) {
    this._anchorSize = value
  }

  get rotateEnabled(): boolean {
    return this._rotateEnabled
  }
  set rotateEnabled(value: boolean) {
    this._rotateEnabled = value
  }

  get rotationSnaps(): number[] {
    return this._rotationSnaps
  }
  set rotationSnaps(value: number[]) {
    this._rotationSnaps = value
  }

  get rotationSnapTolerance(): number {
    return this._rotationSnapTolerance
  }
  set rotationSnapTolerance(value: number) {
    this._rotationSnapTolerance = value
  }

  get borderEnabled(): boolean {
    return this._borderEnabled
  }
  set borderEnabled(value: boolean) {
    this._borderEnabled = value
  }

  get anchorStroke(): string {
    return this._anchorStroke
  }
  set anchorStroke(value: string) {
    this._anchorStroke = value
  }

  get anchorStrokeWidth(): number {
    return this._anchorStrokeWidth
  }
  set anchorStrokeWidth(value: number) {
    this._anchorStrokeWidth = value
  }

  get anchorFill(): string {
    return this._anchorFill
  }
  set anchorFill(value: string) {
    this._anchorFill = value
  }

  get anchorCornerRadius(): number {
    return this._anchorCornerRadius
  }
  set anchorCornerRadius(value: number) {
    this._anchorCornerRadius = value
  }

  get borderStroke(): string {
    return this._borderStroke
  }
  set borderStroke(value: string) {
    this._borderStroke = value
  }

  get borderStrokeWidth(): number {
    return this._borderStrokeWidth
  }
  set borderStrokeWidth(value: number) {
    this._borderStrokeWidth = value
  }

  get borderDash(): number[] | undefined {
    return this._borderDash
  }
  set borderDash(value: number[] | undefined) {
    this._borderDash = value
  }

  get keepRatio(): boolean {
    return this._keepRatio
  }
  set keepRatio(value: boolean) {
    this._keepRatio = value
  }

  get shiftBehavior(): string {
    return this._shiftBehavior
  }
  set shiftBehavior(value: string) {
    this._shiftBehavior = value
  }

  get centeredScaling(): boolean {
    return this._centeredScaling
  }
  set centeredScaling(value: boolean) {
    this._centeredScaling = value
  }

  get ignoreStroke(): boolean {
    return this._ignoreStroke
  }
  set ignoreStroke(value: boolean) {
    this._ignoreStroke = value
  }

  get padding(): number {
    return this._padding
  }
  set padding(value: number) {
    this._padding = value
  }

  get boundBoxFunc(): ((oldBox: Box, newBox: Box) => Box) | undefined {
    return this._boundBoxFunc
  }
  set boundBoxFunc(value: ((oldBox: Box, newBox: Box) => Box) | undefined) {
    this._boundBoxFunc = value
  }

  get anchorDragBoundFunc():
    | ((
        oldPos: Vector2d,
        newPos: Vector2d,
        evt: MouseEvent | TouchEvent
      ) => Vector2d)
    | undefined {
    return this._anchorDragBoundFunc
  }
  set anchorDragBoundFunc(value:
    | ((
        oldPos: Vector2d,
        newPos: Vector2d,
        evt: MouseEvent | TouchEvent
      ) => Vector2d)
    | undefined) {
    this._anchorDragBoundFunc = value
  }

  get anchorStyleFunc(): ((anchor: Rect) => void) | null {
    return this._anchorStyleFunc
  }
  set anchorStyleFunc(value: ((anchor: Rect | Circle) => void) | null) {
    this._anchorStyleFunc = value
  }

  get shouldOverdrawWholeArea(): boolean {
    return this._shouldOverdrawWholeArea
  }
  set shouldOverdrawWholeArea(value: boolean) {
    this._shouldOverdrawWholeArea = value
  }

  get useSingleNodeRotation(): boolean {
    return this._useSingleNodeRotation
  }
  set useSingleNodeRotation(value: boolean) {
    this._useSingleNodeRotation = value
  }

  // ==========================================================================
  // Node management
  // ==========================================================================

  /**
   * alias to `tr.nodes([shape])`/ This method is deprecated and will be removed soon.
   */
  attachTo(node: Node): TransformerFoundation {
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
    return this._nodes?.[0]
  }

  get nodes(): Node[] {
    return this._nodes || []
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

    this._nodes = filteredNodes

    if (nodes.length === 1 && this._useSingleNodeRotation) {
      this.rotation(nodes[0].getAbsoluteRotation())
    } else {
      this.rotation(0)
    }

    for (const node of this._nodes) {
      const onChange = () => {
        if (this.nodes.length === 1 && this._useSingleNodeRotation) {
          this.rotation(this.nodes[0].getAbsoluteRotation())
        }

        this._resetTransformCache()
        if (!(this._transforming || this.isDragging())) {
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
    return this._nodes || []
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
    return this._movingAnchorName
  }

  /**
   * detach transformer from an attached node
   */
  detach(): TransformerFoundation {
    if (this._nodes) {
      for (const node of this._nodes) {
        node.off(`.${this._getEventNamespace()}`)
      }
    }
    this._nodes = []
    this._resetTransformCache()
    this._clearSelectionOutlines()
    return this
  }

  _resetTransformCache(): TransformerFoundation {
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
      skipStroke: isLineShape ? false : this._ignoreStroke,
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
        skipStroke: isLineShape ? false : this._ignoreStroke,
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

  abstract _createElements(): void
  abstract _clearSelectionOutlines(): void
  abstract _handleRotaterMouseMove(e: MouseEvent | TouchEvent): void
  abstract _handleRotaterMouseUp(e: MouseEvent | TouchEvent): void
  abstract _handleMouseDown(e: KonvaEventObject<PointerEvent>): void
  abstract _handleMouseMove(e: MouseEvent | TouchEvent): void
  abstract _handleMouseUp(e: MouseEvent | TouchEvent): void
  abstract _fitNodesInto(
    newAttrs: ReturnType<typeof rotateAroundCenter>,
    evt?: MouseEvent | TouchEvent
  ): void
  abstract update(): void
}
