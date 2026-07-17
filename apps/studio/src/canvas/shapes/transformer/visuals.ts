import Konva from "konva"
import { Group } from "konva/lib/Group"
import type { KonvaEventObject, Node } from "konva/lib/Node"
import { Arc } from "konva/lib/shapes/Arc"
import { Circle } from "konva/lib/shapes/Circle"
import { Ellipse } from "konva/lib/shapes/Ellipse"
import { Line } from "konva/lib/shapes/Line"
import { Path } from "konva/lib/shapes/Path"
import { Rect } from "konva/lib/shapes/Rect"
import type { Vector2d } from "konva/lib/types"
import { Util } from "konva/lib/Util"
import { TransformerFoundation } from "./foundation"
import {
  ANCHORS_NAMES,
  type AnchorName,
  CORNER_ANCHORS,
  createRotationCursor,
  getCenter,
  getCursor,
  getRotationCursorAngle,
  getSnap,
  ROTATER_ANCHORS,
  ROTATER_BASE_ANGLES,
  ROTATER_CORNER_INDEX,
  ROTATION_ZONE_OUTER_OFFSET,
  type RotaterAnchorName,
  rotateAroundCenter,
  SELECTION_OUTLINE_ITEM_NAME,
  SELECTION_OUTLINES_NAME,
  TOUCH_DEVICE,
  transformerRuntime,
} from "./shared"

export abstract class TransformerVisuals extends TransformerFoundation {
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
      this._cursorChanged = !!cursor
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
      if (this._transforming) return
      if (!this._rotateEnabled) return
      if (this._nodes.length !== 1) return

      const node = this.getNode()
      if (!node) return

      const cornerIndex = ROTATER_CORNER_INDEX[name]
      const rotation = node.rotation()
      const cursorAngle = getRotationCursorAngle(cornerIndex, rotation)
      this._changeCursor(createRotationCursor(cursorAngle))
    })

    anchor.on("mouseout", () => {
      if (this._transforming) return
      this._changeCursor(null)
    })

    this.add(anchor)
  }

  _handleRotaterMouseDown(
    e: KonvaEventObject<PointerEvent>,
    name: RotaterAnchorName
  ) {
    if (this._transforming) return
    if (!this._rotateEnabled) return
    if (this._nodes.length !== 1) return

    e.cancelBubble = true
    this._movingAnchorName = name

    const nodeRect = this._getNodeRect()
    const center = getCenter(nodeRect)

    const stage = e.target.getStage()
    if (!stage) return

    const pos = stage.getPointerPosition()
    if (!pos) return

    const startAngle = Math.atan2(pos.y - center.y, pos.x - center.x)

    this._activeRotation = {
      cornerIndex: ROTATER_CORNER_INDEX[name],
      startAngle,
      startRotation: nodeRect.rotation,
      centerX: center.x,
      centerY: center.y,
    }

    this._transforming = true
    transformerRuntime.activeCount++

    if (typeof window !== "undefined") {
      window.addEventListener("mousemove", this._handleRotaterMouseMove)
      window.addEventListener("touchmove", this._handleRotaterMouseMove)
      window.addEventListener("mouseup", this._handleRotaterMouseUp, true)
      window.addEventListener("touchend", this._handleRotaterMouseUp, true)
    }

    this._fire("transformstart", { evt: e.evt, target: this.getNode() })
    for (const target of this._nodes) {
      target._fire("transformstart", { evt: e.evt, target })
    }
  }

  _handleRotaterMouseMove(e: MouseEvent | TouchEvent) {
    if (!this._activeRotation) return
    if (this._nodes.length !== 1) return

    const node = this._nodes[0]
    const stage = node.getStage()
    if (!stage) return

    stage.setPointersPositions(e)
    const pos = stage.getPointerPosition()
    if (!pos) return

    const state = this._activeRotation

    const dx = pos.x - state.centerX
    const dy = pos.y - state.centerY
    const currentAngle = Math.atan2(dy, dx)

    let angleDiff = currentAngle - state.startAngle
    let newRotationRad = state.startRotation + angleDiff

    // Apply rotation snaps
    if (this._rotationSnaps.length > 0) {
      const tol = Konva.getAngle(this._rotationSnapTolerance)
      const snappedRad = getSnap(this._rotationSnaps, newRotationRad, tol)
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

  _handleRotaterMouseUp() {
    if (!this._activeRotation) return

    if (typeof window !== "undefined") {
      window.removeEventListener("mousemove", this._handleRotaterMouseMove)
      window.removeEventListener("touchmove", this._handleRotaterMouseMove)
      window.removeEventListener("mouseup", this._handleRotaterMouseUp, true)
      window.removeEventListener("touchend", this._handleRotaterMouseUp, true)
    }

    this._activeRotation = null
    this._transforming = false
    this._movingAnchorName = null
    transformerRuntime.activeCount--
    this._changeCursor(null)

    const node = this.getNode()
    this._fire("transformend", { evt: null, target: node })
    this.getLayer()?.batchDraw()

    if (node) {
      for (const target of this._nodes) {
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
        const tr = shape.getParent() as TransformerVisuals
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
        if (!this._shouldOverdrawWholeArea) {
          return
        }
        const padding = this._padding
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
      stroke: this._borderStroke,
      strokeWidth: this._borderStrokeWidth,
      dash: this._borderDash,
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
      stroke: this._borderStroke,
      strokeWidth: this._borderStrokeWidth,
      dash: this._borderDash,
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
      stroke: this._borderStroke,
      strokeWidth: this._borderStrokeWidth,
      dash: this._borderDash,
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
      skipStroke: isLineShape ? false : this._ignoreStroke,
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
}
