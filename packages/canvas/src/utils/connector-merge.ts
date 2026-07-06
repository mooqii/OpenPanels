/**
 * Utilities for merging multiple connectors into a single multi-source or multi-target connector.
 *
 * Merge patterns:
 * - Merge sources: A->C + B->C becomes [A,B]->C (connectors share common target)
 * - Merge targets: A->B + A->C becomes A->[B,C] (connectors share common source)
 */

import type { Editor } from "../editor"
import type { ShapeId } from "../types/ids"
import type { ConnectorShape } from "../types/shapes"

export interface MergeResult {
  /** IDs of the original connectors that were deleted */
  deletedConnectorIds: ShapeId[]
  /** The newly created merged connector */
  mergedConnector: ConnectorShape
  /** Whether sources or targets were merged */
  type: "sources" | "targets"
}

/**
 * Check if the given connectors can be merged.
 * Connectors can be merged if:
 * 1. There are at least 2 connectors
 * 2. All connectors are simple 1-to-1 (single source, single target)
 * 3. All connectors share either a common source OR a common target
 *
 * @param connectors - Array of connector shapes to check
 * @returns true if the connectors can be merged
 */
export function canMergeConnectors(connectors: ConnectorShape[]): boolean {
  if (connectors.length < 2) return false

  // All must be simple 1-to-1 connectors
  const allSimple = connectors.every(
    (c) => c.props.fromBindings.length === 1 && c.props.toBindings.length === 1
  )
  if (!allSimple) return false

  // Check for common target (would merge sources)
  const firstTarget = connectors[0].props.toBindings[0].shapeId
  const shareTarget = connectors.every(
    (c) => c.props.toBindings[0].shapeId === firstTarget
  )
  if (shareTarget) return true

  // Check for common source (would merge targets)
  const firstSource = connectors[0].props.fromBindings[0].shapeId
  const shareSource = connectors.every(
    (c) => c.props.fromBindings[0].shapeId === firstSource
  )
  return shareSource
}

/**
 * Get the type of merge that would be performed on the given connectors.
 * Returns null if the connectors cannot be merged.
 *
 * @param connectors - Array of connector shapes to check
 * @returns "sources" if merging sources, "targets" if merging targets, null if cannot merge
 */
export function getMergeType(
  connectors: ConnectorShape[]
): "sources" | "targets" | null {
  if (!canMergeConnectors(connectors)) return null

  // Check for common target (would merge sources)
  const firstTarget = connectors[0].props.toBindings[0].shapeId
  const shareTarget = connectors.every(
    (c) => c.props.toBindings[0].shapeId === firstTarget
  )
  if (shareTarget) return "sources"

  return "targets"
}

/**
 * Merge multiple connectors into a single multi-source or multi-target connector.
 *
 * @param editor - The editor instance
 * @param connectors - Array of connector shapes to merge
 * @returns The merge result containing the new connector and deleted IDs, or null if merge not possible
 */
export function mergeConnectors(
  editor: Editor,
  connectors: ConnectorShape[]
): MergeResult | null {
  const mergeType = getMergeType(connectors)
  if (!mergeType) return null

  // Get the first connector's style properties to use for the merged connector
  const firstConnector = connectors[0]
  const {
    stroke,
    strokeWidth,
    lineStyle,
    arrowStart,
    arrowEnd,
    arrowSize,
    opacity,
  } = firstConnector.props

  let mergedConnector: ConnectorShape

  if (mergeType === "sources") {
    // Merge sources: collect all unique sources, keep common target
    const allSources = connectors.map((c) => c.props.fromBindings[0])
    mergedConnector = editor.createShape({
      type: "connector",
      props: {
        fromBindings: allSources,
        toBindings: [firstConnector.props.toBindings[0]],
        stroke,
        strokeWidth,
        lineStyle,
        arrowStart,
        arrowEnd,
        arrowSize,
        opacity,
      },
    }) as ConnectorShape
  } else {
    // Merge targets: keep common source, collect all unique targets
    const allTargets = connectors.map((c) => c.props.toBindings[0])
    mergedConnector = editor.createShape({
      type: "connector",
      props: {
        fromBindings: [firstConnector.props.fromBindings[0]],
        toBindings: allTargets,
        stroke,
        strokeWidth,
        lineStyle,
        arrowStart,
        arrowEnd,
        arrowSize,
        opacity,
      },
    }) as ConnectorShape
  }

  // Delete original connectors
  const deletedIds = connectors.map((c) => c.id)
  for (const id of deletedIds) {
    editor.deleteShape(id)
  }

  return {
    type: mergeType,
    mergedConnector,
    deletedConnectorIds: deletedIds,
  }
}
