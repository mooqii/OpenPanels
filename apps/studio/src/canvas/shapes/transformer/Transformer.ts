import Konva from "konva"
import { Group } from "konva/lib/Group"
import { type KonvaEventObject, Node } from "konva/lib/Node"
import type { Circle } from "konva/lib/shapes/Circle"
import type { Rect } from "konva/lib/shapes/Rect"
import { Transform, Util } from "konva/lib/Util"
import {
  type AnchorName,
  type AnchorType,
  CORNER_ANCHORS,
  ROTATION_ZONE_OUTER_OFFSET,
  type rotateAroundCenter,
  type TransformerConfig,
  transformerRuntime,
} from "./shared"

import { TransformerVisuals } from "./visuals"

export class Transformer extends TransformerVisuals {
  _updateSelectionOutlines() {
    const outlines = this._getSelectionOutlinesGroup()
    if (!outlines) return

    outlines.destroyChildren()
    outlines.visible(this._borderEnabled && this._nodes.length > 1)

    if (!outlines.visible()) return

    for (const node of this._nodes) {
      const outline = this._createSelectionOutlineForNode(node)
      if (outline) {
        outlines.add(outline)
      }
    }
  }

  _handleMouseDown(e: KonvaEventObject<PointerEvent>) {
    if (this._transforming) {
      return
    }
    this._movingAnchorName = e.target.name().split(" ")[0] as AnchorType

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

    this._transforming = true
    const ap = e.target.getAbsolutePosition()
    const pos = e.target.getStage()?.getPointerPosition()
    if (pos) {
      this._anchorDragOffset = {
        x: pos.x - ap.x,
        y: pos.y - ap.y,
      }
    }
    transformerRuntime.activeCount++
    this._fire("transformstart", { evt: e.evt, target: this.getNode() })
    for (const target of this._nodes) {
      target._fire("transformstart", { evt: e.evt, target })
    }
  }

  _handleMouseMove(e: MouseEvent | TouchEvent) {
    let x: number, y: number, newHypotenuse: number
    const anchorNode = this.findOne(`.${this._movingAnchorName}`)!
    const stage = anchorNode.getStage()!

    stage.setPointersPositions(e)

    const pp = stage.getPointerPosition()!
    let newNodePos = {
      x: pp.x - this._anchorDragOffset.x,
      y: pp.y - this._anchorDragOffset.y,
    }
    const oldAbs = anchorNode.getAbsolutePosition()

    if (this._anchorDragBoundFunc) {
      newNodePos = this._anchorDragBoundFunc(oldAbs, newNodePos, e)
    }
    anchorNode.setAbsolutePosition(newNodePos)
    const newAbs = anchorNode.getAbsolutePosition()

    if (oldAbs.x === newAbs.x && oldAbs.y === newAbs.y) {
      return
    }

    /*
     * This is the builtin transformer's single rotater logic, keep it for reference
    // rotater is working very differently, so do it first
    if (this._movingAnchorName === "rotater") {
      const attrs = this._getNodeRect()
      x = anchorNode.x() - attrs.width / 2
      y = -anchorNode.y() + attrs.height / 2

      let delta = Math.atan2(-y, x) + Math.PI / 2

      if (attrs.height < 0) {
        delta -= Math.PI
      }

      const oldRotation = Konva.getAngle(this.rotation())
      const newRotation = oldRotation + delta

      const tol = Konva.getAngle(this._rotationSnapTolerance)
      const snappedRot = getSnap(this._rotationSnaps, newRotation, tol)

      const diff = snappedRot - attrs.rotation

      const shape = rotateAroundCenter(attrs, diff)
      this._fitNodesInto(shape, e)
      return
    }
    */

    let keepProportion: boolean
    if (this._shiftBehavior === "inverted") {
      keepProportion = this._keepRatio && !e.shiftKey
    } else if (this._shiftBehavior === "none") {
      keepProportion = this._keepRatio
    } else {
      keepProportion = this._keepRatio || e.shiftKey
    }

    let centeredScaling = this._centeredScaling || e.altKey

    if (this._movingAnchorName === "top-left") {
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
    } else if (this._movingAnchorName === "top-center") {
      this.findOne(".top-left")!.y(anchorNode.y())
    } else if (this._movingAnchorName === "top-right") {
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
    } else if (this._movingAnchorName === "middle-left") {
      this.findOne(".top-left")!.x(anchorNode.x())
    } else if (this._movingAnchorName === "middle-right") {
      this.findOne(".bottom-right")!.x(anchorNode.x())
    } else if (this._movingAnchorName === "bottom-left") {
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
    } else if (this._movingAnchorName === "bottom-center") {
      this.findOne(".bottom-right")!.y(anchorNode.y())
    } else if (this._movingAnchorName === "bottom-right") {
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
            this._movingAnchorName
        )
      )
    }

    centeredScaling = this._centeredScaling || e.altKey
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
    if (this._transforming) {
      this._transforming = false
      this._changeCursor(null)

      if (typeof window !== "undefined") {
        window.removeEventListener("mousemove", this._handleMouseMove)
        window.removeEventListener("touchmove", this._handleMouseMove)
        window.removeEventListener("mouseup", this._handleMouseUp, true)
        window.removeEventListener("touchend", this._handleMouseUp, true)
      }
      const node = this.getNode()
      transformerRuntime.activeCount--
      this._fire("transformend", { evt: e, target: node })
      this.getLayer()?.batchDraw()

      if (node) {
        for (const target of this._nodes) {
          target._fire("transformend", { evt: e, target })
          target.getLayer()?.batchDraw()
        }
      }
      this._movingAnchorName = null
    }
  }

  _fitNodesInto(
    newAttrs: ReturnType<typeof rotateAroundCenter>,
    evt?: MouseEvent | TouchEvent
  ) {
    const oldAttrs = this._getNodeRect()

    const minSize = 1

    if (Util._inRange(newAttrs.width, -this._padding * 2 - minSize, minSize)) {
      this.update()
      return
    }
    if (Util._inRange(newAttrs.height, -this._padding * 2 - minSize, minSize)) {
      this.update()
      return
    }

    const t = new Transform()
    t.rotate(Konva.getAngle(this.rotation()))
    if (
      this._movingAnchorName &&
      newAttrs.width < 0 &&
      this._movingAnchorName.indexOf("left") >= 0
    ) {
      const offset = t.point({
        x: -this._padding * 2,
        y: 0,
      })
      newAttrs.x += offset.x
      newAttrs.y += offset.y
      newAttrs.width += this._padding * 2
      this._movingAnchorName = this._movingAnchorName.replace(
        "left",
        "right"
      ) as AnchorType
      this._anchorDragOffset.x -= offset.x
      this._anchorDragOffset.y -= offset.y
    } else if (
      this._movingAnchorName &&
      newAttrs.width < 0 &&
      this._movingAnchorName.indexOf("right") >= 0
    ) {
      const offset = t.point({
        x: this._padding * 2,
        y: 0,
      })
      this._movingAnchorName = this._movingAnchorName.replace(
        "right",
        "left"
      ) as AnchorType
      this._anchorDragOffset.x -= offset.x
      this._anchorDragOffset.y -= offset.y
      newAttrs.width += this._padding * 2
    }
    if (
      this._movingAnchorName &&
      newAttrs.height < 0 &&
      this._movingAnchorName.indexOf("top") >= 0
    ) {
      const offset = t.point({
        x: 0,
        y: -this._padding * 2,
      })
      newAttrs.x += offset.x
      newAttrs.y += offset.y
      this._movingAnchorName = this._movingAnchorName.replace(
        "top",
        "bottom"
      ) as AnchorType
      this._anchorDragOffset.x -= offset.x
      this._anchorDragOffset.y -= offset.y
      newAttrs.height += this._padding * 2
    } else if (
      this._movingAnchorName &&
      newAttrs.height < 0 &&
      this._movingAnchorName.indexOf("bottom") >= 0
    ) {
      const offset = t.point({
        x: 0,
        y: this._padding * 2,
      })
      this._movingAnchorName = this._movingAnchorName.replace(
        "bottom",
        "top"
      ) as AnchorType
      this._anchorDragOffset.x -= offset.x
      this._anchorDragOffset.y -= offset.y
      newAttrs.height += this._padding * 2
    }

    let newAttrs2 = newAttrs

    if (this._boundBoxFunc) {
      const bounded = this._boundBoxFunc(oldAttrs, newAttrs)
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

    if (this._flipEnabled === false) {
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

    for (const node of this._nodes) {
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
    for (const node of this._nodes) {
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

    const enabledAnchors = this._enabledAnchors
    const resizeEnabled = this._resizeEnabled
    const padding = this._padding

    const anchorSize = this._anchorSize
    const anchorRadius = anchorSize / 2
    const edgeMargin = anchorSize + 4 // gap between corner and edge anchors

    // Check if all nodes have transformsEnabled === "none" (non-transformable)
    const allNodesNonTransformable =
      this._nodes.length > 0 &&
      this._nodes.every((node) => node.transformsEnabled() === "none")

    // Update corner anchors (Circle shapes)
    const cornerAnchors = this.find<Circle>("._anchor").filter((node) =>
      CORNER_ANCHORS.includes(node.name().split(" ")[0] as AnchorName)
    )
    for (const node of cornerAnchors) {
      node.setAttrs({
        radius: anchorRadius,
        stroke: this._anchorStroke,
        strokeWidth: this._anchorStrokeWidth,
        fill: this._anchorFill,
      })
    }

    // Update edge anchors (Rect shapes)
    const edgeAnchors = this.find<Rect>("._anchor").filter(
      (node) =>
        !CORNER_ANCHORS.includes(node.name().split(" ")[0] as AnchorName)
    )
    for (const node of edgeAnchors) {
      node.setAttrs({
        stroke: this._anchorStroke,
        strokeWidth: this._anchorStrokeWidth,
        fill: this._anchorFill,
        cornerRadius: this._anchorCornerRadius,
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
      visible: !allNodesNonTransformable && this._rotateEnabled,
    })
    this._batchChangeChild(".rotater-top-right", {
      x: width + padding,
      y: -padding,
      outerRadius: ROTATION_ZONE_OUTER_OFFSET,
      visible: !allNodesNonTransformable && this._rotateEnabled,
    })
    this._batchChangeChild(".rotater-bottom-left", {
      x: -padding,
      y: height + padding,
      outerRadius: ROTATION_ZONE_OUTER_OFFSET,
      visible: !allNodesNonTransformable && this._rotateEnabled,
    })
    this._batchChangeChild(".rotater-bottom-right", {
      x: width + padding,
      y: height + padding,
      outerRadius: ROTATION_ZONE_OUTER_OFFSET,
      visible: !allNodesNonTransformable && this._rotateEnabled,
    })

    this._batchChangeChild(".back", {
      width,
      height,
      visible: this._borderEnabled,
      stroke: this._borderStroke,
      strokeWidth: this._borderStrokeWidth,
      dash: this._borderDash,
      draggable: this.nodes.some((node) => node.draggable()),
      x: 0,
      y: 0,
    })

    this._updateSelectionOutlines()

    const styleFunc = this._anchorStyleFunc
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
    return this._transforming
  }

  /**
   * Stop active transform action
   */
  stopTransform() {
    if (this._transforming) {
      this._removeEvents()
      const anchorNode = this.findOne(`.${this._movingAnchorName}`)
      if (anchorNode) {
        anchorNode.stopDrag()
      }
    }
  }

  destroy() {
    if (this.getStage() && this._cursorChanged && this.getStage()?.content) {
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
