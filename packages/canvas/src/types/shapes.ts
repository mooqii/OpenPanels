/**
 * Shape types built on Konva's configuration types.
 * Provides full access to Konva's shape configuration for design applications.
 */

import type { ImageConfig } from "konva/lib/shapes/Image"
import type { RectConfig } from "konva/lib/shapes/Rect"
import type { TextConfig } from "konva/lib/shapes/Text"
import type { AssetId, PageId, ShapeId } from "./ids"

/**
 * Bounding box interface for shape bounds
 */
export interface Bounds {
  height: number
  width: number
  x: number
  y: number
}

// =============================================================================
// Fill Types (Solid, Gradient, Image Pattern)
// =============================================================================

/**
 * Fill type discriminator
 */
export type FillType = "solid" | "linear-gradient" | "radial-gradient" | "image"

/**
 * A single color stop in a gradient
 */
export interface GradientColorStop {
  /** Color value (rgba string) */
  color: string
  /** Position in gradient (0-1) */
  offset: number
}

/**
 * Solid color fill
 */
export interface SolidFill {
  color: string
  type: "solid"
}

/**
 * Linear gradient fill
 */
export interface LinearGradientFill {
  /** Array of color stops */
  colorStops: GradientColorStop[]
  /** Rotation angle in degrees (0-360) */
  rotation: number
  type: "linear-gradient"
}

/**
 * Radial gradient fill
 */
export interface RadialGradientFill {
  /** Array of color stops */
  colorStops: GradientColorStop[]
  type: "radial-gradient"
}

/**
 * Image pattern fill
 */
export interface ImageFill {
  /** Asset ID reference for the fill image */
  assetId: AssetId
  /** Offset of the pattern */
  offset?: { x: number; y: number }
  /** Scale of the pattern */
  scale?: { x: number; y: number }
  type: "image"
}

/**
 * Union type for all fill types
 */
export type ShapeFill =
  | SolidFill
  | LinearGradientFill
  | RadialGradientFill
  | ImageFill

// =============================================================================
// Geo Shape (Rectangle, Ellipse, etc.)
// =============================================================================

/**
 * Geo types supported
 */
export type GeoType =
  | "rectangle"
  | "ellipse"
  | "triangle"
  | "diamond"
  | "star"
  | "line"

/**
 * Stroke position relative to shape boundary
 * - inside: Stroke is drawn inside the shape boundary
 * - center: Stroke is centered on the shape boundary (Konva default)
 * - outside: Stroke is drawn outside the shape boundary
 */
export type StrokePosition = "inside" | "center" | "outside"

/**
 * Line style types
 */
export type LineStyle = "solid" | "dashed" | "dotted"

/**
 * Line cap styles for line endings
 */
export type LineCap = "butt" | "round" | "square"

/**
 * Arrowhead style types
 */
export type ArrowheadStyle = "none" | "arrow" | "triangle" | "circle"

export type RectShapeProps = Omit<RectConfig, "fill"> & {
  geo: "rectangle"
  fill?: never
  strokePosition?: StrokePosition
  shapeFill?: ShapeFill
  shadowBlur?: number
  shadowColor?: string
  shadowOffsetX?: number
  shadowOffsetY?: number
  shadowOpacity?: number
  shadowEnabled?: boolean
}

// Ellipse uses width/height like RectConfig (not radiusX/radiusY like Konva.Ellipse)
// because GeoEllipseRenderer renders ellipses using a custom sceneFunc
export type EllipseShapeProps = Omit<RectConfig, "fill"> & {
  geo: "ellipse"
  fill?: never
  strokePosition?: StrokePosition
  shapeFill?: ShapeFill
  shadowBlur?: number
  shadowColor?: string
  shadowOffsetX?: number
  shadowOffsetY?: number
  shadowOpacity?: number
  shadowEnabled?: boolean
}

/**
 * Line shape props for geometric line shapes
 */
export type LineShapeProps = {
  geo: "line"
  /** Start point x */
  x: number
  /** Start point y */
  y: number
  /** End point x relative to start */
  width: number
  /** End point y relative to start */
  height: number
  /** Line color */
  stroke: string
  /** Line thickness */
  strokeWidth: number
  /** Line style (solid, dashed, dotted) */
  lineStyle?: LineStyle
  /** Join style at ends */
  lineCap?: LineCap
  /** Arrowhead at start */
  arrowStart?: ArrowheadStyle
  /** Arrowhead at end */
  arrowEnd?: ArrowheadStyle
  /** Size of arrowheads (default: strokeWidth * 2) */
  arrowSize?: number
  scaleX?: number
  scaleY?: number
  opacity?: number
  rotation?: number
  // Optional properties for type compatibility (not used for lines)
  fill?: never
  shapeFill?: never
  strokePosition?: never
  cornerRadius?: never
  shadowBlur?: never
  shadowColor?: never
  shadowOffsetX?: never
  shadowOffsetY?: never
  shadowOpacity?: never
  shadowEnabled?: never
}

export type GeoShapeProps = RectShapeProps | EllipseShapeProps | LineShapeProps

/**
 * Geo shape definition
 */
export interface GeoShape {
  id: ShapeId
  index: number
  /** Can be a page or a group shape */
  parentId: PageId | ShapeId
  props: GeoShapeProps
  type: "geo"
  typeName: "shape"
}

// =============================================================================
// Text Shape
// =============================================================================

export type TextBoxSizeMode = "auto" | "manual"

export type TextShapeProps = TextConfig & {
  textBoxHeightMode?: TextBoxSizeMode
  textBoxWidthMode?: TextBoxSizeMode
}

/**
 * Text shape definition
 */
export interface TextShape {
  id: ShapeId
  index: number
  /** Can be a page or a group shape */
  parentId: PageId | ShapeId
  props: TextShapeProps
  type: "text"
  typeName: "shape"
}

// =============================================================================
// Image Shape
// =============================================================================

/**
 * Image shape props - extends Konva's ImageConfig properties
 * Inherits: image, crop, cornerRadius, etc.
 */
export interface ImageShapeProps extends Omit<ImageConfig, "image"> {
  /** Asset ID reference */
  assetId: AssetId | null
  /** Whether the image is playing (for animated images) */
  playing?: boolean
}

/**
 * Image shape definition
 */
export interface ImageShape {
  id: ShapeId
  index: number
  /** Can be a page or a group shape */
  parentId: PageId | ShapeId
  props: ImageShapeProps
  type: "image"
  typeName: "shape"
}

// =============================================================================
// Placeholder Shape (rectangle look + animated opacity/background)
// =============================================================================

/**
 * Placeholder shape props — rectangle-like (x, y, width, height, rotation, etc.)
 */
export interface PlaceholderShapeProps {
  cornerRadius?: number
  fill?: string
  height?: number
  rotation?: number
  scaleX?: number
  scaleY?: number
  /** Optional label; when set, rendered centered in the placeholder */
  text?: string
  width?: number
  x?: number
  y?: number
}

/**
 * Placeholder shape definition (created via code only)
 */
export interface PlaceholderShape {
  id: ShapeId
  index: number
  /** Can be a page or a group shape */
  parentId: PageId | ShapeId
  props: PlaceholderShapeProps
  type: "placeholder"
  typeName: "shape"
}

// =============================================================================
// Draw Shape (Pencil drawing / Line)
// =============================================================================

/**
 * Draw shape props for pencil lines
 */
export interface DrawShapeProps {
  /** Stroke color */
  color: string
  height?: number
  /** Flat array of points [x1, y1, x2, y2, ...] */
  points: number[]
  scaleX?: number
  scaleY?: number
  /** Stroke size (in pixels) */
  size: number
  width?: number
  /** X position of the shape origin */
  x: number
  /** Y position of the shape origin */
  y: number
}

/**
 * Draw shape definition for pencil lines
 */
export interface DrawShape {
  id: ShapeId
  index: number
  /** Can be a page or a group shape */
  parentId: PageId | ShapeId
  props: DrawShapeProps
  type: "draw"
  typeName: "shape"
}

// =============================================================================
// Path Shape (Bezier paths / Pen tool)
// =============================================================================

/**
 * Point type for bezier path anchors
 * - corner: Sharp corner point (handles can be independent)
 * - smooth: Smooth curve point (handles are collinear but can have different lengths)
 * - symmetric: Symmetric curve point (handles are collinear and same length)
 */
export type PathPointType = "corner" | "smooth" | "symmetric"

/**
 * A single anchor point in a bezier path
 */
export interface PathPoint {
  /**
   * Incoming control handle (relative to anchor point)
   * Used for the curve coming INTO this point
   */
  handleIn?: { x: number; y: number }
  /**
   * Outgoing control handle (relative to anchor point)
   * Used for the curve going OUT of this point
   */
  handleOut?: { x: number; y: number }
  /** Type of the anchor point */
  type: PathPointType
  /** X coordinate of the anchor point */
  x: number
  /** Y coordinate of the anchor point */
  y: number
}

/**
 * Path shape props for bezier paths (pen tool)
 */
export interface PathShapeProps {
  /** Whether the path is closed */
  closed: boolean
  /** Fill color (only applies to closed paths) */
  fill?: string
  /** Height of the bounding box */
  height?: number
  /** Array of anchor points defining the path */
  points: PathPoint[]
  scaleX?: number
  scaleY?: number
  /** Stroke color */
  stroke: string
  /** Stroke width */
  strokeWidth: number
  /** Width of the bounding box */
  width?: number
  /** X position of the shape origin */
  x: number
  /** Y position of the shape origin */
  y: number
}

/**
 * Path shape definition for bezier paths
 */
export interface PathShape {
  id: ShapeId
  index: number
  /** Can be a page or a group shape */
  parentId: PageId | ShapeId
  props: PathShapeProps
  type: "path"
  typeName: "shape"
}

// =============================================================================
// Brush Shape (Pressure-sensitive brush strokes)
// =============================================================================

/**
 * A single point in a brush stroke with pressure information
 */
export interface BrushPoint {
  /** Pressure value (0-1), derived from drawing velocity */
  pressure: number
  /** X coordinate */
  x: number
  /** Y coordinate */
  y: number
}

/**
 * Brush shape props for pressure-sensitive brush strokes
 */
export interface BrushShapeProps {
  /** Stroke/fill color */
  color: string
  /** Height of the bounding box */
  height?: number
  /** Array of points with pressure data */
  points: BrushPoint[]
  scaleX?: number
  scaleY?: number
  /** Base stroke size (in pixels) */
  size: number
  /** Width of the bounding box */
  width?: number
  /** X position of the shape origin */
  x: number
  /** Y position of the shape origin */
  y: number
}

/**
 * Brush shape definition for pressure-sensitive strokes
 */
export interface BrushShape {
  id: ShapeId
  index: number
  /** Can be a page or a group shape */
  parentId: PageId | ShapeId
  props: BrushShapeProps
  type: "brush"
  typeName: "shape"
}

// =============================================================================
// Marker Shape (Highlighter-style strokes with no color accumulation)
// =============================================================================

/**
 * Marker shape props for highlighter-style strokes
 * Similar to DrawShape but with opacity for transparent highlighting
 */
export interface MarkerShapeProps {
  /** Stroke color */
  color: string
  /** Height of the bounding box */
  height?: number
  /** Opacity of the marker stroke (0-1) */
  opacity: number
  /** Flat array of points [x1, y1, x2, y2, ...] */
  points: number[]
  scaleX?: number
  scaleY?: number
  /** Stroke size (in pixels) */
  size: number
  /** Width of the bounding box */
  width?: number
  /** X position of the shape origin */
  x: number
  /** Y position of the shape origin */
  y: number
}

/**
 * Marker shape definition for highlighter-style strokes
 * Rendered in a cached group to prevent color accumulation on overlap
 */
export interface MarkerShape {
  id: ShapeId
  index: number
  /** Can be a page or a group shape */
  parentId: PageId | ShapeId
  props: MarkerShapeProps
  type: "marker"
  typeName: "shape"
}

// =============================================================================
// Group Shape (Container for nested shapes)
// =============================================================================

/**
 * Group shape props for container shapes
 */
export interface GroupShapeProps {
  /** Opacity (0-1) */
  opacity?: number
  /** Rotation angle in degrees */
  rotation?: number
  /** Scale X */
  scaleX?: number
  /** Scale Y */
  scaleY?: number
  /** X position of the group origin */
  x: number
  /** Y position of the group origin */
  y: number
}

/**
 * Group shape definition for container shapes
 */
export interface GroupShape {
  id: ShapeId
  index: number
  /** Can be a page or another shape (for nested groups) */
  parentId: PageId | ShapeId
  props: GroupShapeProps
  type: "group"
  typeName: "shape"
}

// =============================================================================
// Connector Shape (Shape-to-shape connections with bezier curves)
// =============================================================================

/**
 * Anchor position type for connector bindings
 * - "auto": Automatically calculate optimal anchor point based on connection direction
 * - { x, y }: Normalized position (0-1) on the shape's bounding box edge
 */
export type ConnectorAnchor = "auto" | { x: number; y: number }

/**
 * A binding that connects a connector to a shape
 */
export interface ConnectorBinding {
  /** Anchor position on the shape (auto-calculated or explicit) */
  anchor: ConnectorAnchor
  /** The ID of the shape this connector binds to */
  shapeId: ShapeId
}

/**
 * Connector shape props for bezier-curved connections between shapes
 */
export interface ConnectorShapeProps {
  /** Arrowhead style at target ends */
  arrowEnd?: ArrowheadStyle
  /** Size of arrowheads (default: strokeWidth * 3) */
  arrowSize?: number
  /** Arrowhead style at source ends */
  arrowStart?: ArrowheadStyle
  /** Source shape bindings (multiple for merge topology) */
  fromBindings: ConnectorBinding[]
  /** Line style (solid, dashed, dotted) */
  lineStyle?: LineStyle
  /** Opacity of the connector */
  opacity?: number
  /** Stroke color */
  stroke: string
  /** Stroke width */
  strokeWidth: number
  /** Target shape bindings (multiple for branch topology) */
  toBindings: ConnectorBinding[]
}

/**
 * Connector shape definition for shape-to-shape connections
 */
export interface ConnectorShape {
  id: ShapeId
  index: number
  /** Can be a page or a group shape */
  parentId: PageId | ShapeId
  props: ConnectorShapeProps
  type: "connector"
  typeName: "shape"
}

// =============================================================================
// Union Types
// =============================================================================

/**
 * All shape types union
 */
export type Shape =
  | GeoShape
  | TextShape
  | ImageShape
  | PlaceholderShape
  | DrawShape
  | PathShape
  | BrushShape
  | MarkerShape
  | ConnectorShape
  | GroupShape

/**
 * Shape type discriminator
 */
export type ShapeType = Shape["type"] | "transformer"

/**
 * Extract shape by type
 */
export type ShapeByType<T extends ShapeType> = Extract<Shape, { type: T }>

/**
 * Shape props by type
 */
export type ShapeProps = Shape["props"]

/**
 * Shape props by shape type
 */
export type ShapePropsByType<T extends ShapeType> = ShapeByType<T>["props"]

// =============================================================================
// Type Guards
// =============================================================================

export function isGeoShape(shape: Shape): shape is GeoShape {
  return shape.type === "geo"
}

export function isTextShape(shape: Shape): shape is TextShape {
  return shape.type === "text"
}

export function isImageShape(shape: Shape): shape is ImageShape {
  return shape.type === "image"
}

export function isPlaceholderShape(shape: Shape): shape is PlaceholderShape {
  return shape.type === "placeholder"
}

export function isDrawShape(shape: Shape): shape is DrawShape {
  return shape.type === "draw"
}

export function isPathShape(shape: Shape): shape is PathShape {
  return shape.type === "path"
}

export function isBrushShape(shape: Shape): shape is BrushShape {
  return shape.type === "brush"
}

export function isMarkerShape(shape: Shape): shape is MarkerShape {
  return shape.type === "marker"
}

export function isConnectorShape(shape: Shape): shape is ConnectorShape {
  return shape.type === "connector"
}

export function isGroupShape(shape: Shape): shape is GroupShape {
  return shape.type === "group"
}

export function isLineShape(
  shape: GeoShape
): shape is GeoShape & { props: LineShapeProps } {
  return shape.type === "geo" && shape.props.geo === "line"
}

// =============================================================================
// Fill Type Guards
// =============================================================================

export function isSolidFill(fill: ShapeFill): fill is SolidFill {
  return fill.type === "solid"
}

export function isLinearGradientFill(
  fill: ShapeFill
): fill is LinearGradientFill {
  return fill.type === "linear-gradient"
}

export function isRadialGradientFill(
  fill: ShapeFill
): fill is RadialGradientFill {
  return fill.type === "radial-gradient"
}

export function isImageFill(fill: ShapeFill): fill is ImageFill {
  return fill.type === "image"
}

export function isGradientFill(
  fill: ShapeFill
): fill is LinearGradientFill | RadialGradientFill {
  return fill.type === "linear-gradient" || fill.type === "radial-gradient"
}
