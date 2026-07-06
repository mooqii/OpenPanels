import { Circle, Group, Line, Path } from "react-konva"
import {
  PEN_ANCHOR_FILL,
  PEN_ANCHOR_SIZE,
  PEN_ANCHOR_STROKE,
  PEN_CLOSE_THRESHOLD,
  PEN_HANDLE_LINE_COLOR,
  PEN_HANDLE_SIZE,
  PEN_STROKE_COLOR,
  PEN_STROKE_WIDTH,
} from "../constants"
import { pointsToSvgPath } from "../renderers/PathShapeRenderer"
import type { PathPoint, PathShape } from "../types/shapes"

interface PathEditorOverlayProps {
  /** Shape being edited (path edit mode) */
  editingShape?: PathShape | null
  /** Callback when clicking near first point (for closing path) */
  isNearFirstPoint?: boolean
  /** Current mouse position for line preview */
  mousePosition?: { x: number; y: number } | null
  onAnchorDragEnd?: (index: number, e: any) => void
  onAnchorDragMove?: (index: number, e: any) => void
  /** Callback when an anchor point is clicked/dragged */
  onAnchorDragStart?: (index: number, e: any) => void
  onHandleDragEnd?: (
    pointIndex: number,
    handleType: "in" | "out",
    e: any
  ) => void
  onHandleDragMove?: (
    pointIndex: number,
    handleType: "in" | "out",
    e: any
  ) => void
  /** Callback when a handle is dragged */
  onHandleDragStart?: (
    pointIndex: number,
    handleType: "in" | "out",
    e: any
  ) => void
  /** Points being created (pen tool preview) */
  previewPoints?: PathPoint[]
  /** Current zoom/scale level of the stage */
  scale?: number
  /** Currently selected point index (for edit mode) */
  selectedPointIndex?: number | null
}

export function PathEditorOverlay({
  previewPoints,
  editingShape,
  selectedPointIndex,
  mousePosition,
  onAnchorDragStart,
  onAnchorDragMove,
  onAnchorDragEnd,
  onHandleDragStart,
  onHandleDragMove,
  onHandleDragEnd,
  isNearFirstPoint,
  scale = 1,
}: PathEditorOverlayProps) {
  const points = previewPoints || editingShape?.props.points || []
  const shapeX = editingShape?.props.x ?? 0
  const shapeY = editingShape?.props.y ?? 0
  const isPreviewMode = !!previewPoints && previewPoints.length > 0

  if (points.length === 0) return null

  // Generate path data for preview
  const pathData = pointsToSvgPath(points, false)

  // Scale sizes inversely with zoom so they appear consistent visually
  const anchorRadius = PEN_ANCHOR_SIZE / 2 / scale
  const handleRadius = PEN_HANDLE_SIZE / 2 / scale
  const strokeWidth = 1 / scale
  const closeThresholdRadius = PEN_CLOSE_THRESHOLD / scale

  return (
    <Group x={shapeX} y={shapeY}>
      {/* Path preview (only in preview mode, actual shape is rendered separately in edit mode) */}
      {isPreviewMode && (
        <Path
          data={pathData}
          lineCap="round"
          lineJoin="round"
          listening={false}
          stroke={PEN_STROKE_COLOR}
          strokeWidth={PEN_STROKE_WIDTH / scale}
        />
      )}

      {/* Line from last point to mouse (preview mode) */}
      {isPreviewMode && mousePosition && points.length > 0 && (
        <Line
          dash={[5 / scale, 5 / scale]}
          listening={false}
          points={[
            points.at(-1)!.x,
            points.at(-1)!.y,
            mousePosition.x,
            mousePosition.y,
          ]}
          stroke={PEN_HANDLE_LINE_COLOR}
          strokeWidth={strokeWidth}
        />
      )}

      {/* Render handles and anchors for each point */}
      {points.map((point, index) => {
        const isSelected = selectedPointIndex === index
        const showHandles = isPreviewMode || isSelected || editingShape

        return (
          <Group key={`point-${point.x}-${point.y}-${index}`}>
            {/* Handle lines and circles */}
            {showHandles && (
              <>
                {/* Handle In line and circle */}
                {point.handleIn &&
                  (point.handleIn.x !== 0 || point.handleIn.y !== 0) && (
                    <>
                      <Line
                        listening={false}
                        points={[
                          point.x,
                          point.y,
                          point.x + point.handleIn.x,
                          point.y + point.handleIn.y,
                        ]}
                        stroke={PEN_HANDLE_LINE_COLOR}
                        strokeWidth={strokeWidth}
                      />
                      <Circle
                        draggable={!!editingShape}
                        fill={PEN_ANCHOR_FILL}
                        onDragEnd={(e) => onHandleDragEnd?.(index, "in", e)}
                        onDragMove={(e) => onHandleDragMove?.(index, "in", e)}
                        onDragStart={(e) => onHandleDragStart?.(index, "in", e)}
                        radius={handleRadius}
                        stroke={PEN_HANDLE_LINE_COLOR}
                        strokeWidth={strokeWidth}
                        x={point.x + point.handleIn.x}
                        y={point.y + point.handleIn.y}
                      />
                    </>
                  )}

                {/* Handle Out line and circle */}
                {point.handleOut &&
                  (point.handleOut.x !== 0 || point.handleOut.y !== 0) && (
                    <>
                      <Line
                        listening={false}
                        points={[
                          point.x,
                          point.y,
                          point.x + point.handleOut.x,
                          point.y + point.handleOut.y,
                        ]}
                        stroke={PEN_HANDLE_LINE_COLOR}
                        strokeWidth={strokeWidth}
                      />
                      <Circle
                        draggable={!!editingShape}
                        fill={PEN_ANCHOR_FILL}
                        onDragEnd={(e) => onHandleDragEnd?.(index, "out", e)}
                        onDragMove={(e) => onHandleDragMove?.(index, "out", e)}
                        onDragStart={(e) =>
                          onHandleDragStart?.(index, "out", e)
                        }
                        radius={handleRadius}
                        stroke={PEN_HANDLE_LINE_COLOR}
                        strokeWidth={strokeWidth}
                        x={point.x + point.handleOut.x}
                        y={point.y + point.handleOut.y}
                      />
                    </>
                  )}
              </>
            )}

            {/* Anchor point */}
            <Circle
              draggable={!!editingShape}
              fill={
                index === 0 && isPreviewMode && isNearFirstPoint
                  ? PEN_ANCHOR_STROKE
                  : PEN_ANCHOR_FILL
              }
              onDragEnd={(e) => onAnchorDragEnd?.(index, e)}
              onDragMove={(e) => onAnchorDragMove?.(index, e)}
              onDragStart={(e) => onAnchorDragStart?.(index, e)}
              radius={
                index === 0 && isPreviewMode
                  ? anchorRadius + 2 / scale
                  : anchorRadius
              }
              stroke={PEN_ANCHOR_STROKE}
              strokeWidth={isSelected ? 2 / scale : strokeWidth}
              x={point.x}
              y={point.y}
            />

            {/* Close path indicator (ring around first point when mouse is near) */}
            {index === 0 && isPreviewMode && points.length > 1 && (
              <Circle
                listening={false}
                radius={closeThresholdRadius}
                stroke={isNearFirstPoint ? PEN_ANCHOR_STROKE : "transparent"}
                strokeWidth={strokeWidth}
                x={point.x}
                y={point.y}
              />
            )}
          </Group>
        )
      })}
    </Group>
  )
}
