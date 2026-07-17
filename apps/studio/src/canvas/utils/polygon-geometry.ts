import type { Bounds } from "../types/shapes"
import type { Point } from "./hit-testing"

/**
 * Tests if two convex polygons intersect.
 * Uses vertex containment and edge crossing checks.
 */
export function polygonsIntersect(polyA: Point[], polyB: Point[]): boolean {
  // Check if any vertex of A is inside B
  for (const p of polyA) {
    if (pointInPolygon(p, polyB)) {
      return true
    }
  }

  // Check if any vertex of B is inside A
  for (const p of polyB) {
    if (pointInPolygon(p, polyA)) {
      return true
    }
  }

  // Check if any edges cross
  for (let i = 0; i < polyA.length; i++) {
    const a1 = polyA[i]
    const a2 = polyA[(i + 1) % polyA.length]

    for (let j = 0; j < polyB.length; j++) {
      const b1 = polyB[j]
      const b2 = polyB[(j + 1) % polyB.length]

      if (segmentsIntersect(a1, a2, b1, b2)) {
        return true
      }
    }
  }

  return false
}

/**
 * Tests if a polygon is fully contained within a rectangle.
 */
export function isPolygonFullyContained(
  polygon: Point[],
  rect: Bounds
): boolean {
  return polygon.every((p) => pointInRect(p, rect))
}

/**
 * Point-in-polygon test using ray casting algorithm.
 */
export function pointInPolygon(point: Point, polygon: Point[]): boolean {
  let inside = false
  const { x, y } = point

  for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
    const xi = polygon[i].x
    const yi = polygon[i].y
    const xj = polygon[j].x
    const yj = polygon[j].y

    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) {
      inside = !inside
    }
  }

  return inside
}

/**
 * Tests if two line segments intersect.
 */
export function segmentsIntersect(
  a1: Point,
  a2: Point,
  b1: Point,
  b2: Point
): boolean {
  const d1 = direction(b1, b2, a1)
  const d2 = direction(b1, b2, a2)
  const d3 = direction(a1, a2, b1)
  const d4 = direction(a1, a2, b2)

  if (
    ((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) &&
    ((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0))
  ) {
    return true
  }

  if (d1 === 0 && onSegment(b1, b2, a1)) return true
  if (d2 === 0 && onSegment(b1, b2, a2)) return true
  if (d3 === 0 && onSegment(a1, a2, b1)) return true
  if (d4 === 0 && onSegment(a1, a2, b2)) return true

  return false
}

/**
 * Computes the cross product direction.
 */
export function direction(p1: Point, p2: Point, p3: Point): number {
  return (p3.x - p1.x) * (p2.y - p1.y) - (p2.x - p1.x) * (p3.y - p1.y)
}

/**
 * Checks if point p is on line segment (p1, p2).
 */
export function onSegment(p1: Point, p2: Point, p: Point): boolean {
  return (
    Math.min(p1.x, p2.x) <= p.x &&
    p.x <= Math.max(p1.x, p2.x) &&
    Math.min(p1.y, p2.y) <= p.y &&
    p.y <= Math.max(p1.y, p2.y)
  )
}

// =============================================================================
// Rectangle Utilities
// =============================================================================

/**
 * Converts a Bounds rectangle to a polygon (array of 4 corner points).
 */
export function rectToPolygon(rect: Bounds): Point[] {
  return [
    { x: rect.x, y: rect.y },
    { x: rect.x + rect.width, y: rect.y },
    { x: rect.x + rect.width, y: rect.y + rect.height },
    { x: rect.x, y: rect.y + rect.height },
  ]
}

/**
 * Gets the 4 edges of a rectangle as line segments.
 */
export function getRectEdges(rect: Bounds): [Point, Point][] {
  const corners = rectToPolygon(rect)
  return [
    [corners[0], corners[1]],
    [corners[1], corners[2]],
    [corners[2], corners[3]],
    [corners[3], corners[0]],
  ]
}

/**
 * Tests if a point is inside a rectangle.
 */
export function pointInRect(point: Point, rect: Bounds): boolean {
  return (
    point.x >= rect.x &&
    point.x <= rect.x + rect.width &&
    point.y >= rect.y &&
    point.y <= rect.y + rect.height
  )
}
