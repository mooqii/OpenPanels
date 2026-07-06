import {
  type CanvasRecord,
  createEmptySnapshot,
  type RecordsDiff,
  type StoreSnapshot,
} from "../types/records"
import { objectMapEntries } from "./object"

/**
 * Create an empty records diff
 */
export function createEmptyDiff(): RecordsDiff {
  return {
    added: {},
    updated: {},
    removed: {},
  }
}

/**
 * Check if a diff is empty (no changes)
 */
export function isDiffEmpty(diff: RecordsDiff): boolean {
  return (
    Object.keys(diff.added).length === 0 &&
    Object.keys(diff.updated).length === 0 &&
    Object.keys(diff.removed).length === 0
  )
}

/**
 * Create a diff from added records
 */
export function createAddedDiff(records: {
  [id: string]: CanvasRecord
}): RecordsDiff {
  return {
    added: records,
    updated: {},
    removed: {},
  }
}

/**
 * Create a diff from removed records
 */
export function createRemovedDiff(records: {
  [id: string]: CanvasRecord
}): RecordsDiff {
  return {
    added: {},
    updated: {},
    removed: records,
  }
}

/**
 * Combines multiple RecordsDiff objects into a single consolidated diff.
 * This function intelligently merges changes, handling cases where the same record
 * is modified multiple times across different diffs. For example, if a record is
 * added in one diff and then updated in another, the result will show it as added
 * with the final state.
 *
 * @param diffs - An array of diffs to combine into a single diff
 * @param options - Configuration options for the squashing operation
 *   - mutateFirstDiff - If true, modifies the first diff in place instead of creating a new one
 * @returns A single diff that represents the cumulative effect of all input diffs
 * @example
 * ```ts
 * const diff1: RecordsDiff = {
 *   added: { 'book:1': { id: 'book:1', title: 'New Book' } },
 *   updated: {},
 *   removed: {}
 * }
 *
 * const diff2: RecordsDiff = {
 *   added: {},
 *   updated: { 'book:1': [{ id: 'book:1', title: 'New Book' }, { id: 'book:1', title: 'Updated Title' }] },
 *   removed: {}
 * }
 *
 * const squashed = squashRecordDiffs([diff1, diff2])
 * // Result: {
 * //   added: { 'book:1': { id: 'book:1', title: 'Updated Title' } },
 * //   updated: {},
 * //   removed: {}
 * // }
 * ```
 *
 * @public
 */
export function squashRecordDiffs(
  diffs: RecordsDiff[],
  options?: {
    mutateFirstDiff?: boolean
  }
): RecordsDiff {
  const result = options?.mutateFirstDiff
    ? diffs[0]
    : ({ added: {}, removed: {}, updated: {} } as RecordsDiff)

  squashRecordDiffsMutable(
    result,
    options?.mutateFirstDiff ? diffs.slice(1) : diffs
  )
  return result
}

/**
 * Applies an array of diffs to a target diff by mutating the target in-place.
 * This is the core implementation used by squashRecordDiffs. It handles complex
 * scenarios where records move between added/updated/removed states across multiple diffs.
 *
 * The function processes each diff sequentially, applying the following logic:
 * - Added records: If the record was previously removed, convert to an update; otherwise add it
 * - Updated records: Chain updates together, preserving the original 'from' state
 * - Removed records: If the record was added in this sequence, cancel both operations
 *
 * @param target - The diff to modify in-place (will be mutated)
 * @param diffs - Array of diffs to apply to the target
 * @example
 * ```ts
 * const targetDiff: RecordsDiff = {
 *   added: {},
 *   updated: {},
 *   removed: { 'book:1': oldBook }
 * }
 *
 * const newDiffs = [{
 *   added: { 'book:1': newBook },
 *   updated: {},
 *   removed: {}
 * }]
 *
 * squashRecordDiffsMutable(targetDiff, newDiffs)
 * // targetDiff is now: {
 * //   added: {},
 * //   updated: { 'book:1': [oldBook, newBook] },
 * //   removed: {}
 * // }
 * ```
 *
 * @internal
 */
export function squashRecordDiffsMutable(
  target: RecordsDiff,
  diffs: RecordsDiff[]
): void {
  for (const diff of diffs) {
    for (const [id, value] of objectMapEntries(diff.added)) {
      if (target.removed[id]) {
        const original = target.removed[id]
        delete target.removed[id]
        if (original !== value) {
          target.updated[id] = [original, value]
        }
      } else {
        target.added[id] = value
      }
    }

    for (const [id, [_from, to]] of objectMapEntries(diff.updated)) {
      if (target.added[id]) {
        target.added[id] = to
        delete target.updated[id]
        delete target.removed[id]
        continue
      }
      if (target.updated[id]) {
        target.updated[id] = [target.updated[id][0], to]
        delete target.removed[id]
        continue
      }

      target.updated[id] = diff.updated[id]
      delete target.removed[id]
    }

    for (const [id, value] of objectMapEntries(diff.removed)) {
      // the same record was added in this diff sequence, just drop it
      if (target.added[id]) {
        delete target.added[id]
      } else if (target.updated[id]) {
        target.removed[id] = target.updated[id][0]
        delete target.updated[id]
      } else {
        target.removed[id] = value
      }
    }
  }
}

export function applyDiff(old: StoreSnapshot | null, patches: RecordsDiff[]) {
  const snapshot = old || createEmptySnapshot()

  for (const patch of patches) {
    for (const id of Object.keys(patch.removed)) {
      delete snapshot.store[id]
    }

    for (const [id, [_from, to]] of objectMapEntries(patch.updated)) {
      snapshot.store[id] = to
    }

    for (const [id, record] of objectMapEntries(patch.added)) {
      snapshot.store[id] = record
    }
  }

  return snapshot
}
