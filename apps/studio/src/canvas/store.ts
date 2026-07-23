import { produce } from "immer"
import { subscribeWithSelector } from "zustand/middleware"
import { createStore } from "zustand/vanilla"
import { persistCanvasTool } from "./tool-persistence"
import type { PageId, ShapeId } from "./types/ids"
import type {
  CanvasCameraState,
  CanvasRecord,
  RecordsDiff,
  StoreSnapshot,
} from "./types/records"
import { objectMapEntries } from "./utils/object"

export type Tool =
  | { name: "select" }
  | { name: "hand" }
  | { name: "draw"; shape: "rectangle" | "ellipse" | "line" }
  | { name: "pencil"; color: string; size: number }
  | { name: "brush"; color: string; size: number }
  | { name: "marker"; color: string; size: number; opacity: number }
  | { name: "pen" }
  | TextTool
  | { name: "connector"; stroke?: string; strokeWidth?: number }

export type ToolName = Tool["name"]
export type TextToolAlign = "left" | "center" | "right"
export type TextToolFontWeight = "normal" | "700"
export type TextTool = {
  name: "text"
  align: TextToolAlign
  color: string
  fontFamily: string
  fontSize: number
  fontWeight: TextToolFontWeight
}

// History state for undo/redo
type StoreData = { [id: string]: CanvasRecord }

interface HistoryState {
  future: StoreData[]
  past: StoreData[]
  present: StoreData
}

const MAX_HISTORY_SIZE = 100

export interface CanvasStoreState {
  camera: CanvasCameraState
  canRedo: boolean
  canUndo: boolean
  currentPageId: PageId | null

  // History state for undo/redo
  history: HistoryState

  // Listeners for change tracking
  listeners: Set<(diff: RecordsDiff) => void>
  openedGroupId: ShapeId | null

  // Editor state
  selectedShapeIds: Set<ShapeId>
  // Store format: { [recordId]: CanvasRecord }
  store: { [id: string]: CanvasRecord }
  tool: Tool
}

export interface CanvasStoreActions {
  // Apply a diff to the store
  applyDiff: (
    diff: RecordsDiff,
    options: { skipHistory?: boolean; notify: boolean }
  ) => void

  // Clear all records
  clear: () => void

  // Get a record by ID
  get: (id: string) => CanvasRecord | undefined

  // Get a snapshot of the current store state
  getSnapshot: () => StoreSnapshot

  // Listen to changes in the store
  listen: (
    callback: (diff: RecordsDiff) => void,
    options?: { scope?: "document" | "all" }
  ) => () => void

  // Load a snapshot into the store
  loadSnapshot: (snapshot: StoreSnapshot) => void

  // Put a record into the store (with history)
  put: (record: CanvasRecord, options?: { skipHistory?: boolean }) => void
  redo: () => void

  // Remove a record from the store (with history)
  remove: (id: string, options?: { skipHistory?: boolean }) => void
  setCamera: (camera: CanvasCameraState) => void
  setCurrentPageId: (pageId: PageId | null) => void
  setOpenedGroupId: (groupId: ShapeId | null) => void

  // Editor state actions
  setSelectedShapeIds: (ids: ShapeId[]) => void
  setTool: (tool: Tool) => void

  // History actions
  undo: () => void
}

export type CanvasStore = CanvasStoreState & CanvasStoreActions

function createRecordsDiff(
  added: { [id: string]: CanvasRecord },
  updated: { [id: string]: [from: CanvasRecord, to: CanvasRecord] },
  removed: { [id: string]: CanvasRecord }
): RecordsDiff {
  return { added, updated, removed }
}

export const createCanvasStore = () =>
  createStore<CanvasStore>()(
    subscribeWithSelector((set, get) => {
      const initialStore: StoreData = {}
      const initialHistory: HistoryState = {
        past: [],
        present: initialStore,
        future: [],
      }

      // Helper to update history state
      const updateHistory = (newHistory: HistoryState) => {
        set({
          history: newHistory,
          store: newHistory.present,
          canUndo: newHistory.past.length > 0,
          canRedo: newHistory.future.length > 0,
        })
      }

      // Helper to push state to history
      const pushState = (updater: (draft: StoreData) => void) => {
        const { history } = get()
        const newPresent = produce(history.present, updater)

        // Skip if no changes were made
        // if (JSON.stringify(newPresent) === JSON.stringify(history.present)) {
        //   return
        // }

        const newPast = [...history.past, history.present]

        // Limit history size
        if (newPast.length > MAX_HISTORY_SIZE) {
          newPast.shift()
        }

        updateHistory({
          past: newPast,
          present: newPresent,
          future: [], // Clear future on new action
        })
      }

      return {
        store: initialStore,
        history: initialHistory,
        canUndo: false,
        canRedo: false,
        camera: { x: 0, y: 0, zoom: 1 },
        listeners: new Set(),
        selectedShapeIds: new Set<ShapeId>(),
        currentPageId: null,
        tool: { name: "select" } as Tool,
        openedGroupId: null,

        applyDiff: (diff: RecordsDiff, { notify, skipHistory }) => {
          if (skipHistory) {
            set(
              produce((draft) => {
                for (const id of Object.keys(diff.removed)) {
                  delete draft.store[id]
                }

                for (const [id, [_, to]] of objectMapEntries(diff.updated)) {
                  draft.store[id] = to
                }

                for (const [id, record] of objectMapEntries(diff.added)) {
                  draft.store[id] = record
                }
              })
            )
          } else {
            pushState((draft) => {
              for (const id of Object.keys(diff.removed)) {
                delete draft[id]
              }

              for (const [id, [_, to]] of objectMapEntries(diff.updated)) {
                draft[id] = to
              }

              for (const [id, record] of objectMapEntries(diff.added)) {
                draft[id] = record
              }
            })
          }

          if (notify) {
            const { listeners } = get()
            for (const listener of listeners) {
              listener(diff)
            }
          }
        },

        getSnapshot: (): StoreSnapshot => {
          const {
            store,
            selectedShapeIds,
            currentPageId,
            openedGroupId,
            camera,
          } = get()

          return {
            store,
            camera,
            selectedShapeIds,
            currentPageId,
            openedGroupId,
          }
        },

        loadSnapshot: (snapshot: StoreSnapshot) => {
          const selectedShapeIds =
            snapshot.selectedShapeIds instanceof Set
              ? snapshot.selectedShapeIds
              : new Set<ShapeId>(snapshot.selectedShapeIds ?? [])
          // Reset history when loading a new snapshot
          const newHistory: HistoryState = {
            past: [],
            present: snapshot.store,
            future: [],
          }
          set({
            store: snapshot.store,
            history: newHistory,
            canUndo: false,
            canRedo: false,
            camera: normalizeCamera(snapshot.camera) ?? get().camera,
            selectedShapeIds,
            currentPageId: snapshot.currentPageId,
            openedGroupId: snapshot.openedGroupId,
          })
        },

        listen: (
          callback: (diff: RecordsDiff) => void,
          _options?: { scope?: "document" | "all" }
        ) => {
          const { listeners } = get()
          listeners.add(callback)

          // Return unsubscribe function
          return () => {
            const currentListeners = get().listeners
            currentListeners.delete(callback)
          }
        },

        get: (id: string) => {
          return get().store[id]
        },

        put: (record: CanvasRecord, options?: { skipHistory?: boolean }) => {
          const existingRecord = get().store[record.id]

          if (options?.skipHistory) {
            // Direct update without history
            set(
              produce((draft) => {
                draft.store[record.id] = record
              })
            )
          } else {
            // Update with history tracking
            pushState((draft) => {
              draft[record.id] = record
            })
          }

          const { listeners } = get()

          if (listeners.size > 0) {
            // Notify listeners of the change
            const diff = existingRecord
              ? createRecordsDiff(
                  {},
                  { [record.id]: [existingRecord, record] },
                  {}
                )
              : createRecordsDiff({ [record.id]: record }, {}, {})
            for (const listener of listeners) {
              listener(diff)
            }
          }
        },

        remove: (id: string, options?: { skipHistory?: boolean }) => {
          const record = get().store[id]
          if (!record) return

          if (options?.skipHistory) {
            // Direct removal without history
            set(
              produce((draft) => {
                delete draft.store[id]
              })
            )
          } else {
            // Removal with history tracking
            pushState((draft) => {
              delete draft[id]
            })
          }

          const { listeners } = get()

          if (listeners.size > 0) {
            // Notify listeners of the removal
            const diff = createRecordsDiff({}, {}, { [id]: record })
            for (const listener of listeners) {
              listener(diff)
            }
          }
        },

        clear: () => {
          const { store } = get()
          const removed = { ...store }

          // Clear with history tracking
          pushState((draft) => {
            for (const id of Object.keys(draft)) {
              delete draft[id]
            }
          })

          const { listeners } = get()
          if (listeners.size > 0) {
            // Notify listeners of the clear
            const diff = createRecordsDiff({}, {}, removed)
            for (const listener of listeners) {
              listener(diff)
            }
          }
        },

        undo: () => {
          const { history } = get()
          if (history.past.length === 0) return

          const previous = history.past.at(-1)
          if (!previous) return

          const newPast = history.past.slice(0, -1)

          updateHistory({
            past: newPast,
            present: previous,
            future: [history.present, ...history.future],
          })
        },

        redo: () => {
          const { history } = get()
          if (history.future.length === 0) return

          const next = history.future[0]
          const newFuture = history.future.slice(1)

          updateHistory({
            past: [...history.past, history.present],
            present: next,
            future: newFuture,
          })
        },

        setSelectedShapeIds: (ids: ShapeId[]) => {
          const { selectedShapeIds } = get()
          const newIds = new Set(ids)
          if (equalsIdSet(selectedShapeIds, newIds)) return
          set({ selectedShapeIds: newIds })
        },

        setCurrentPageId: (pageId: PageId | null) => {
          set({ currentPageId: pageId })
        },

        setCamera: (camera: CanvasCameraState) => {
          const current = get().camera
          if (equalsCamera(current, camera)) return
          set({ camera })
        },

        setTool: (tool: Tool) => {
          persistCanvasTool(tool)
          set({ tool })
        },

        setOpenedGroupId: (groupId: ShapeId | null) => {
          set({ openedGroupId: groupId })
        },
      }
    })
  )

function equalsIdSet(setA: Set<ShapeId>, setB: Set<ShapeId>) {
  if (setA.size !== setB.size) return false

  for (const v of setA) {
    if (!setB.has(v)) return false
  }

  return true
}

function equalsCamera(a: CanvasCameraState, b: CanvasCameraState) {
  return a.x === b.x && a.y === b.y && a.zoom === b.zoom
}

function normalizeCamera(
  camera: CanvasCameraState | null | undefined
): CanvasCameraState | null {
  if (!camera) return null
  const { x, y, zoom } = camera
  if (!(Number.isFinite(x) && Number.isFinite(y) && Number.isFinite(zoom))) {
    return null
  }
  if (zoom <= 0) return null
  return { x, y, zoom }
}
