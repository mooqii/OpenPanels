import type Konva from "konva"
import { useStore as useVanillaStore } from "zustand"
import { shallow } from "zustand/vanilla/shallow"
import { CANVAS_MAX_ZOOM, CANVAS_MIN_ZOOM } from "./constants"
import type { Transformer } from "./shapes/Transformer"
import { type CanvasStore, createCanvasStore, type Tool } from "./store"
import { type Asset, AssetRecordType, type AssetStore } from "./types/assets"
import type { AssetId, PageId, ShapeId } from "./types/ids"
import type {
  CanvasCameraState,
  CanvasRecord,
  Page,
  RecordId,
  RecordsDiff,
  StoreSnapshot,
} from "./types/records"
import type { Bounds, Shape } from "./types/shapes"
import type { ToolConfig } from "./types/tools"
import { canvasBoundsToStageBounds, getShapesBounds } from "./utils/coordinates"
import { createEmptyDiff } from "./utils/records"

export type CommandOptions = {
  editor: Editor
  transformer: Transformer | null
  selectedShapes: Shape[]
}

export type CommandHandler = (options: CommandOptions) => void

export type GetTools = (shapes: Shape[]) => ToolConfig[] | null

class Transaction {
  readonly #diff: RecordsDiff
  readonly #snapshot: StoreSnapshot

  #level: number
  #rolledback: boolean
  #committed: boolean

  constructor(snapshot: StoreSnapshot) {
    this.#snapshot = snapshot
    this.#diff = createEmptyDiff()
    this.#level = 0
    this.#rolledback = false
    this.#committed = false
  }

  get snapshot() {
    return this.#snapshot
  }

  incrementLevel() {
    this.#level += 1
  }

  rollback() {
    this.#rolledback = true
  }

  put(record: CanvasRecord) {
    const old = this.#snapshot.store[record.id]
    if (old) {
      this.#diff.updated[record.id] = [old, record]
    } else {
      this.#diff.added[record.id] = record
    }
  }

  remove(recordId: RecordId) {
    const old = this.#snapshot.store[recordId]
    if (old) {
      this.#diff.removed[recordId] = old
    }
  }

  commit(store: ReturnType<typeof createCanvasStore>) {
    if (this.#level > 0 || this.#committed) return false
    if (this.#rolledback) return true
    store.getState().applyDiff(this.#diff, { notify: true, skipHistory: false })
    this.#committed = true
    return true
  }
}

export interface EditorOptions {
  assetStore?: AssetStore
  commands?: Record<string, CommandHandler>
  getTools?: GetTools
  store?: ReturnType<typeof createCanvasStore>
}

export class Editor {
  private readonly store: ReturnType<typeof createCanvasStore>
  private readonly assetStore: AssetStore | undefined

  // Caches to prevent recalculation / rerender
  private currentPageShapes: Shape[] = []
  private selectedShapes: Shape[] = []

  private readonly listeners: Set<() => void> = new Set()
  private readonly commands: Map<string, CommandHandler> = new Map()
  private readonly getTools: GetTools | null = null

  #transaction: Transaction | null = null
  #stage: Konva.Stage | null = null

  constructor(options: EditorOptions = {}) {
    this.store = options.store || createCanvasStore()
    this.assetStore = options.assetStore

    if (options.commands) {
      for (const [k, h] of Object.entries(options.commands)) {
        this.commands.set(k, h)
      }
    }

    if (options.getTools) {
      this.getTools = options.getTools
    }

    this.initializePage()

    this.store.subscribe(
      (state) => ({
        selectedShapeIds: state.selectedShapeIds,
        store: state.store,
      }),
      (data) => {
        this.selectedShapes = calculateSelectedShapes(data)
        this.notify()
      },
      { equalityFn: shallow }
    )
    this.store.subscribe(
      (state) => ({
        currentPageId: state.currentPageId,
        store: state.store,
      }),
      (data) => {
        this.currentPageShapes = calculateCurrentPageShapes(data)
        this.notify()
      },
      { equalityFn: shallow }
    )
  }

  get stage(): Konva.Stage | null {
    return this.#stage
  }

  set stage(newStage: Konva.Stage | null) {
    if (this.#stage === newStage) return
    this.#stage = newStage
    if (newStage) {
      this.applySnapshotCamera()
    }
  }

  // Subscribe to editor state changes: selectedShapes, currentTool etc.
  // If you want subscribe to document state changes, use listen instead.
  subscribe(cb: () => void) {
    this.listeners.add(cb)
    return () => {
      this.listeners.delete(cb)
    }
  }

  private notify() {
    for (const listener of this.listeners) {
      listener()
    }
  }

  listen(callback: (diff: RecordsDiff) => void) {
    return this.store.getState().listen(callback)
  }

  dispatch(command: string, transformer: Transformer | null) {
    const handler = this.commands.get(command)
    if (handler) {
      handler({
        editor: this,
        transformer,
        selectedShapes: this.selectedShapes,
      })
    }
  }

  getToolsForShape(shapes: Shape[]) {
    if (this.getTools) {
      return this.getTools(shapes)
    }
    return null
  }

  private initializePage() {
    // Find or create a page
    const store = this.store.getState().getSnapshot()
    const pages = Object.values(store.store).filter(
      (record: CanvasRecord) => record.typeName === "page"
    )

    if (pages.length > 0) {
      this.store.getState().setCurrentPageId(pages[0].id as PageId)
    } else {
      // Create a default page
      const maxId = pages
        .map(({ id }) => Number.parseInt(id.substring(5), 10))
        .reduce((a, b) => Math.max(a, b), 0)
      const pageId = `page:${maxId + 1}` as PageId
      this.store.getState().setCurrentPageId(pageId)
      this.store.getState().put({
        id: pageId,
        typeName: "page",
        name: "Page 1",
        index: 1,
      } as Page)
    }
  }

  private getStoreSnapshot() {
    return this.#transaction
      ? this.#transaction.snapshot
      : this.store.getState().getSnapshot()
  }

  private getStoreRecords() {
    return this.getStoreSnapshot().store
  }

  private putRecord(record: CanvasRecord) {
    if (this.#transaction) {
      this.#transaction.put(record)
    } else {
      this.store.getState().put(record)
    }
  }

  private getRecord(recordId: RecordId) {
    return this.#transaction
      ? this.#transaction.snapshot.store[recordId]
      : this.store.getState().get(recordId)
  }

  private removeRecord(recordId: RecordId) {
    if (this.#transaction) {
      this.#transaction.remove(recordId)
    } else {
      this.store.getState().remove(recordId)
    }
  }

  private getMaxShapeIndex(pageId: PageId | null): number {
    if (!pageId) return 1

    const store = this.getStoreSnapshot().store
    let maxIndex = 0

    for (const record of Object.values(store)) {
      if (
        record.typeName === "shape" &&
        (record as Shape).parentId === pageId
      ) {
        const shape = record as Shape
        if (shape.index > maxIndex) {
          maxIndex = shape.index
        }
      }
    }

    return maxIndex + 1
  }

  useStore<U>(selector: (state: CanvasStore) => U) {
    return useVanillaStore(this.store, selector)
  }

  // Shape operations
  createShape(shape: Partial<Shape>): Shape {
    const shapeId =
      (shape.id as ShapeId) || (`shape:${crypto.randomUUID()}` as ShapeId)
    const currentPageId = this.getStoreSnapshot().currentPageId
    const fallbackPageId =
      currentPageId || (`page:${crypto.randomUUID()}` as PageId)
    const parentId =
      (shape.parentId as PageId | ShapeId | undefined) ?? fallbackPageId
    const shapeType = shape.type ?? "geo"
    const index = shape.index ?? this.getMaxShapeIndex(currentPageId)

    const newShape: Shape = {
      id: shapeId,
      typeName: "shape",
      type: shapeType,
      index,
      parentId,
      props: shape.props || {},
    } as Shape

    this.putRecord(newShape)

    return newShape
  }

  deleteShape(shape: Shape | ShapeId): void {
    const shapeId = typeof shape === "string" ? shape : shape.id
    const shapeRecord = this.getShape(shapeId)

    // If deleting a group, cascade delete all descendants first
    if (shapeRecord?.type === "group") {
      const descendants = this.getShapeDescendants(shapeId)
      for (const descendant of descendants) {
        this.removeRecord(descendant.id)
      }
    }

    this.removeRecord(shapeId)

    // Update selection to remove deleted shape and any of its descendants
    const deletedIds = new Set([shapeId])
    if (shapeRecord?.type === "group") {
      const descendants = this.getShapeDescendants(shapeId)
      for (const d of descendants) {
        deletedIds.add(d.id)
      }
    }

    const currentSelection = Array.from(
      this.store.getState().selectedShapeIds
    ).filter((id) => !deletedIds.has(id))

    this.store.getState().setSelectedShapeIds(currentSelection)
  }

  updateShape<T extends Shape>(
    id: ShapeId,
    updates: Partial<T> | ((shape: T) => T)
  ): void {
    const existing = this.getRecord(id) as T | undefined
    if (!existing) return

    const updated =
      typeof updates === "function"
        ? updates(existing)
        : ({
            ...existing,
            ...updates,
            props: {
              ...existing.props,
              ...(updates.props || {}),
            },
          } as T)

    this.putRecord(updated)
  }

  getShape<T extends Shape = Shape>(id: ShapeId): T | undefined {
    const record = this.getRecord(id)
    if (record && record.typeName === "shape") {
      return record as T
    }
    return undefined
  }

  // ==========================================================================
  // Group hierarchy operations
  // ==========================================================================

  /**
   * Get the parent shape if this shape is inside a group.
   * Returns undefined if the shape's parent is a page (not grouped).
   */
  getShapeParent(shapeId: ShapeId): Shape | undefined {
    const shape = this.getShape(shapeId)
    if (!shape) return undefined

    // If parentId starts with "shape:", it's inside a group
    if (shape.parentId.startsWith("shape:")) {
      return this.getShape(shape.parentId as ShapeId)
    }
    return undefined
  }

  /**
   * Get direct children of a group shape.
   * Returns empty array if the shape is not a group or has no children.
   */
  getShapeChildren(groupId: ShapeId): Shape[] {
    const store = this.getStoreRecords()
    const children: Shape[] = []

    for (const record of Object.values(store)) {
      if (record.typeName === "shape") {
        const shape = record as Shape
        if (shape.parentId === groupId) {
          children.push(shape)
        }
      }
    }

    return children.sort((a, b) => a.index - b.index)
  }

  /**
   * Get all descendants (children, grandchildren, etc.) of a group shape recursively.
   */
  getShapeDescendants(groupId: ShapeId): Shape[] {
    const descendants: Shape[] = []
    const children = this.getShapeChildren(groupId)

    for (const child of children) {
      descendants.push(child)
      // Recursively get descendants of child groups
      if (child.type === "group") {
        descendants.push(...this.getShapeDescendants(child.id))
      }
    }

    return descendants
  }

  /**
   * Get the top-level ancestor of a shape (the outermost group that contains it).
   * If the shape is not in any group, returns the shape itself.
   * If the shape doesn't exist, returns undefined.
   */
  getTopLevelAncestor(shapeId: ShapeId): ShapeId | undefined {
    const shape = this.getShape(shapeId)
    if (!shape) return undefined

    // If parent is a page, this shape is already top-level
    if (!shape.parentId.startsWith("shape:")) {
      return shapeId
    }

    // Walk up the tree to find the top-level ancestor
    let current = shape
    while (current.parentId.startsWith("shape:")) {
      const parent = this.getShape(current.parentId as ShapeId)
      if (!parent) break
      current = parent
    }

    return current.id
  }

  /**
   * Check if a shape is a descendant of another shape (is inside the group hierarchy).
   */
  isDescendantOf(shapeId: ShapeId, ancestorId: ShapeId): boolean {
    const shape = this.getShape(shapeId)
    if (!shape) return false

    // Walk up the tree looking for the ancestor
    let current = shape
    while (current.parentId.startsWith("shape:")) {
      if (current.parentId === ancestorId) {
        return true
      }
      const parent = this.getShape(current.parentId as ShapeId)
      if (!parent) break
      current = parent
    }

    return false
  }

  /**
   * Get the page that a shape belongs to, walking up through any groups.
   */
  getPageForShape(shapeId: ShapeId): PageId | undefined {
    const shape = this.getShape(shapeId)
    if (!shape) return undefined

    // If parent is a page, return it
    if (shape.parentId.startsWith("page:")) {
      return shape.parentId as PageId
    }

    // Walk up through groups to find the page
    let current = shape
    while (current.parentId.startsWith("shape:")) {
      const parent = this.getShape(current.parentId as ShapeId)
      if (!parent) return undefined
      current = parent
    }

    return current.parentId as PageId
  }

  // ==========================================================================
  // Group operations
  // ==========================================================================

  /**
   * Group the currently selected shapes into a new group.
   * Returns the new group shape, or null if grouping is not possible.
   */
  groupSelectedShapes(): Shape | null {
    const selected = this.getSelectedShapes()
    if (selected.length < 2) return null

    // Filter out connectors - they shouldn't be grouped
    const shapesToGroup = selected.filter((s) => s.type !== "connector")
    if (shapesToGroup.length < 2) return null

    // All shapes must have the same parent
    const parentId = shapesToGroup[0].parentId
    if (!shapesToGroup.every((s) => s.parentId === parentId)) {
      console.warn("Cannot group shapes with different parents")
      return null
    }

    // Calculate group bounds from selected shapes
    const bounds = getShapesBounds(shapesToGroup)

    // Create group shape
    const group = this.createShape({
      type: "group",
      parentId,
      props: {
        x: bounds.x,
        y: bounds.y,
      },
    })

    // Update children parentId and adjust positions to be relative to group
    for (const shape of shapesToGroup) {
      const shapeProps = shape.props as { x?: number; y?: number }
      const relativeX = (shapeProps.x ?? 0) - bounds.x
      const relativeY = (shapeProps.y ?? 0) - bounds.y

      this.updateShape(shape.id, {
        parentId: group.id as ShapeId,
        props: {
          ...shape.props,
          x: relativeX,
          y: relativeY,
        },
      })
    }

    // Select the new group
    this.setSelectedShapes([group.id])
    return group
  }

  /**
   * Ungroup the selected group shapes.
   * Returns an array of shapes that were ungrouped.
   */
  ungroupSelectedShapes(): Shape[] {
    const selected = this.getSelectedShapes()
    const groups = selected.filter((s) => s.type === "group")
    const ungroupedShapes: Shape[] = []

    for (const group of groups) {
      const children = this.getShapeChildren(group.id)
      const groupProps = group.props as { x?: number; y?: number }
      const groupX = groupProps.x ?? 0
      const groupY = groupProps.y ?? 0

      // Move children to group's parent with absolute positions
      for (const child of children) {
        const childProps = child.props as { x?: number; y?: number }
        const absoluteX = (childProps.x ?? 0) + groupX
        const absoluteY = (childProps.y ?? 0) + groupY

        this.updateShape(child.id, {
          parentId: group.parentId,
          props: {
            ...child.props,
            x: absoluteX,
            y: absoluteY,
          },
        })

        ungroupedShapes.push(this.getShape(child.id)!)
      }

      // Delete the group (but not its children which we've already moved)
      this.removeRecord(group.id)
    }

    this.setSelectedShapes([])

    return ungroupedShapes
  }

  // Connector operations
  /**
   * Create a connector between shapes
   */
  createConnector(props: {
    fromShapeIds: ShapeId[]
    toShapeIds: ShapeId[]
    stroke?: string
    strokeWidth?: number
    arrowStart?: "none" | "arrow" | "triangle" | "circle"
    arrowEnd?: "none" | "arrow" | "triangle" | "circle"
  }): Shape {
    const fromBindings = props.fromShapeIds.map((shapeId) => ({
      shapeId,
      anchor: "auto" as const,
    }))
    const toBindings = props.toShapeIds.map((shapeId) => ({
      shapeId,
      anchor: "auto" as const,
    }))

    return this.createShape({
      type: "connector",
      props: {
        fromBindings,
        toBindings,
        stroke: props.stroke ?? "#666666",
        strokeWidth: props.strokeWidth ?? 2,
        arrowStart: props.arrowStart ?? "none",
        arrowEnd: props.arrowEnd ?? "arrow",
      },
    })
  }

  /**
   * Get all connectors that reference a specific shape
   */
  getConnectorsForShape(shapeId: ShapeId): Shape[] {
    const shapes = this.getCurrentPageShapes()
    return shapes.filter((shape) => {
      if (shape.type !== "connector") return false
      const props = shape.props as {
        fromBindings: Array<{ shapeId: ShapeId }>
        toBindings: Array<{ shapeId: ShapeId }>
      }
      return (
        props.fromBindings.some((b) => b.shapeId === shapeId) ||
        props.toBindings.some((b) => b.shapeId === shapeId)
      )
    })
  }

  /**
   * Update a connector's bindings
   */
  updateConnectorBindings(
    connectorId: ShapeId,
    updates: {
      fromBindings?: Array<{
        shapeId: ShapeId
        anchor?: "auto" | { x: number; y: number }
      }>
      toBindings?: Array<{
        shapeId: ShapeId
        anchor?: "auto" | { x: number; y: number }
      }>
    }
  ): void {
    const connector = this.getShape(connectorId)
    if (!connector || connector.type !== "connector") return

    const existingProps = connector.props as {
      fromBindings: Array<{
        shapeId: ShapeId
        anchor: "auto" | { x: number; y: number }
      }>
      toBindings: Array<{
        shapeId: ShapeId
        anchor: "auto" | { x: number; y: number }
      }>
    }

    const newFromBindings =
      updates.fromBindings?.map((b) => ({
        shapeId: b.shapeId,
        anchor: b.anchor ?? ("auto" as const),
      })) ?? existingProps.fromBindings

    const newToBindings =
      updates.toBindings?.map((b) => ({
        shapeId: b.shapeId,
        anchor: b.anchor ?? ("auto" as const),
      })) ?? existingProps.toBindings

    this.updateShape(connectorId, {
      props: {
        ...connector.props,
        fromBindings: newFromBindings,
        toBindings: newToBindings,
      },
    })
  }

  /**
   * Merge selected connectors into a single multi-source or multi-target connector.
   * Connectors can be merged if they share a common source (merge targets) or
   * a common target (merge sources).
   *
   * @returns The merged connector shape, or null if merge is not possible
   */
  mergeSelectedConnectors(): Shape | null {
    const {
      canMergeConnectors,
      mergeConnectors,
    } = require("./utils/connector-merge")
    const { isConnectorShape } = require("./types/shapes")

    const selectedConnectors = this.selectedShapes.filter(isConnectorShape)
    if (selectedConnectors.length < 2) return null

    if (!canMergeConnectors(selectedConnectors)) return null

    const result = mergeConnectors(this, selectedConnectors)
    if (!result) return null

    this.setSelectedShapes([result.mergedConnector.id])
    return result.mergedConnector
  }

  getShapeNode<ChildNode extends Konva.Node>(
    id: ShapeId
  ): ChildNode | undefined {
    return this.stage?.findOne(`#${id}`)
  }

  findShapeNodes<ChildNode extends Konva.Node>(selector: any): ChildNode[] {
    return this.stage?.find(selector) ?? []
  }

  // Selection operations
  getSelectedShapes(): Shape[] {
    return this.selectedShapes
  }

  setSelectedShapes(ids: ShapeId[]): void {
    this.store.getState().setSelectedShapeIds(ids)
  }

  // Tool operations
  getTool(): Tool {
    return this.store.getState().tool
  }

  setTool(tool: Tool): void {
    this.store.getState().setTool(tool)
    this.notify()
  }

  // Group editing operations
  getOpenedGroupId(): ShapeId | null {
    return this.store.getState().openedGroupId
  }

  setOpenedGroupId(groupId: ShapeId | null): void {
    this.store.getState().setOpenedGroupId(groupId)
    this.notify()
  }

  openGroup(groupId: ShapeId): void {
    this.setOpenedGroupId(groupId)
  }

  closeGroup(): void {
    this.setOpenedGroupId(null)
  }

  /**
   * Check if a shape is inside the currently opened group.
   * Returns true if the shape is a descendant of the opened group.
   */
  isShapeInOpenedGroup(shapeId: ShapeId): boolean {
    const openedGroupId = this.getOpenedGroupId()
    if (!openedGroupId) return false

    // If the shape is the opened group itself, return false (we want to edit its children, not the group)
    if (shapeId === openedGroupId) return false

    // Check if the shape is a descendant of the opened group
    return this.isDescendantOf(shapeId, openedGroupId)
  }

  // Asset operations
  createAssets(
    assets: Array<
      Partial<Asset> & { id?: string; type: string; typeName: "asset" }
    >
  ): Asset[] {
    const result: Asset[] = []

    for (const asset of assets) {
      const assetId = asset.id || AssetRecordType.createId()
      const newAsset: Asset = {
        id: assetId,
        typeName: "asset",
        type: asset.type,
        props: asset.props || {},
        meta: asset.meta || {},
      } as Asset

      this.putRecord(newAsset)
      result.push(newAsset)
    }

    return result
  }

  getAsset(id: AssetId): Asset | undefined {
    const record = this.getRecord(id)
    if (record && record.typeName === "asset") {
      return record as Asset
    }
    return undefined
  }

  updateAsset(id: AssetId, updates: Partial<Asset>): void {
    const existing = this.getRecord(id) as Asset | undefined
    if (!existing || existing.typeName !== "asset") return

    console.log("✅ updateAsset", existing, updates)

    const updated: Asset = {
      ...existing,
      ...updates,
      props: {
        ...existing.props,
        ...(updates.props || {}),
      },
      meta: {
        ...existing.meta,
        ...(updates.meta || {}),
      },
    } as Asset

    this.putRecord(updated)
  }

  getAssetStore(): AssetStore | undefined {
    return this.assetStore
  }

  // Page operations
  getCurrentPageShapes(): Shape[] {
    return this.currentPageShapes
  }

  getCurrentPageId() {
    return this.store.getState().currentPageId
  }

  // Snapshot operations
  getSnapshot(): StoreSnapshot {
    return this.store.getState().getSnapshot()
  }

  loadSnapshot(snapshot: StoreSnapshot): void {
    this.store.getState().loadSnapshot(snapshot)
    this.initializePage()
    this.applySnapshotCamera()
  }

  // Batch operations
  run<T>(fn: () => T): T {
    if (this.#transaction) {
      this.#transaction.incrementLevel()
    } else {
      this.#transaction = new Transaction(this.getSnapshot())
    }

    try {
      return fn()
    } catch (error) {
      console.error("Failed to execute transaction", error)
      this.#transaction.rollback()
      throw error
    } finally {
      if (this.#transaction.commit(this.store)) {
        this.#transaction = null
      }
    }
  }

  // Zoom operations
  getCamera(): CanvasCameraState {
    if (!this.stage) return this.store.getState().camera
    return {
      x: this.stage.x(),
      y: this.stage.y(),
      zoom: this.stage.scaleX(),
    }
  }

  setCamera(camera: CanvasCameraState): void {
    const normalizedCamera = normalizeCamera(camera)
    if (!normalizedCamera) return

    this.store.getState().setCamera(normalizedCamera)
    if (!this.stage) return

    this.stage.scale({
      x: normalizedCamera.zoom,
      y: normalizedCamera.zoom,
    })
    this.stage.position({
      x: normalizedCamera.x,
      y: normalizedCamera.y,
    })
  }

  syncCameraFromStage(): void {
    if (!this.stage) return
    this.store.getState().setCamera(this.getCamera())
  }

  applySnapshotCamera(): void {
    this.setCamera(this.store.getState().camera)
  }

  zoomToBounds(
    bounds: Bounds,
    options?: {
      animation?: { duration: number }
      padding?: number
    }
  ): void {
    if (!this.stage) return
    if (bounds.width <= 0 || bounds.height <= 0) return

    const stageWidth = this.stage.width()
    const stageHeight = this.stage.height()
    const padding = options?.padding ?? 40
    const availableWidth = stageWidth - padding * 2
    const availableHeight = stageHeight - padding * 2

    const scaleX = availableWidth / bounds.width
    const scaleY = availableHeight / bounds.height
    const scale = Math.min(scaleX, scaleY, 1)

    const contentCenterX = bounds.x + bounds.width / 2
    const contentCenterY = bounds.y + bounds.height / 2
    const posX = stageWidth / 2 - contentCenterX * scale
    const posY = stageHeight / 2 - contentCenterY * scale

    if (options?.animation) {
      this.stage.to({
        scaleX: scale,
        scaleY: scale,
        x: posX,
        y: posY,
        duration: options.animation.duration / 1000,
      })
    } else {
      this.stage.scale({ x: scale, y: scale })
      this.stage.position({ x: posX, y: posY })
    }
  }

  zoomToFit(options?: {
    animation?: { duration: number }
    padding?: number
  }): void {
    if (!this.stage) return

    const shapes = this.getCurrentPageShapes()
    if (shapes.length === 0) {
      this.stage.scale({ x: 1, y: 1 })
      this.stage.position({ x: 0, y: 0 })
      return
    }

    const bounds = getShapesBounds(shapes)
    this.zoomToBounds(bounds, options)
  }

  getZoom(): number {
    if (!this.stage) return 1
    return this.stage.scaleX()
  }

  zoom(
    scale: number,
    center?: { x: number; y: number },
    options?: { animation?: { duration: number } }
  ): void {
    if (!this.stage) return

    const clampedScale = Math.max(
      CANVAS_MIN_ZOOM,
      Math.min(CANVAS_MAX_ZOOM, scale)
    )

    if (center) {
      // Zoom towards a specific point
      const pointer = center
      const oldScale = this.stage.scaleX()
      const mousePointTo = {
        x: (pointer.x - this.stage.x()) / oldScale,
        y: (pointer.y - this.stage.y()) / oldScale,
      }

      const newPos = {
        x: pointer.x - mousePointTo.x * clampedScale,
        y: pointer.y - mousePointTo.y * clampedScale,
      }

      if (options?.animation) {
        this.stage.to({
          scaleX: clampedScale,
          scaleY: clampedScale,
          x: newPos.x,
          y: newPos.y,
          duration: options.animation.duration / 1000,
        })
      } else {
        this.stage.scale({ x: clampedScale, y: clampedScale })
        this.stage.position(newPos)
      }
    } else {
      // Zoom towards center of viewport
      const stageWidth = this.stage.width()
      const stageHeight = this.stage.height()
      const centerX = stageWidth / 2
      const centerY = stageHeight / 2

      const oldScale = this.stage.scaleX()
      const mousePointTo = {
        x: (centerX - this.stage.x()) / oldScale,
        y: (centerY - this.stage.y()) / oldScale,
      }

      const newPos = {
        x: centerX - mousePointTo.x * clampedScale,
        y: centerY - mousePointTo.y * clampedScale,
      }

      if (options?.animation) {
        this.stage.to({
          scaleX: clampedScale,
          scaleY: clampedScale,
          x: newPos.x,
          y: newPos.y,
          duration: options.animation.duration / 1000,
        })
      } else {
        this.stage.scale({ x: clampedScale, y: clampedScale })
        this.stage.position(newPos)
      }
    }
  }

  zoomIn(
    center?: { x: number; y: number },
    options?: { animation?: { duration: number } }
  ): void {
    if (!this.stage) return
    const currentScale = this.stage.scaleX()
    const newScale = Math.min(5, currentScale * 1.2)
    this.zoom(newScale, center, options)
  }

  zoomOut(
    center?: { x: number; y: number },
    options?: { animation?: { duration: number } }
  ): void {
    if (!this.stage) return
    const currentScale = this.stage.scaleX()
    const newScale = Math.max(CANVAS_MIN_ZOOM, currentScale / 1.2)
    this.zoom(newScale, center, options)
  }

  // Selection bounds in canvas coordinates
  getSelectionBounds(): Bounds | null {
    if (!this.stage) return null

    const shapes = this.getSelectedShapes()
    if (shapes.length === 0) return null

    return getShapesBounds(shapes)
  }

  // Selection bounds in stage coordinates
  getSelectionScreenBounds(): Bounds | null {
    const bounds = this.getSelectionBounds()
    if (!bounds) return null

    return canvasBoundsToStageBounds(this.stage, bounds)
  }

  // History operations
  undo(): void {
    this.store.getState().undo()
  }

  redo(): void {
    this.store.getState().redo()
  }

  canUndo(): boolean {
    return this.store.getState().canUndo
  }

  canRedo(): boolean {
    return this.store.getState().canRedo
  }
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
  return {
    x,
    y,
    zoom: Math.max(CANVAS_MIN_ZOOM, Math.min(CANVAS_MAX_ZOOM, zoom)),
  }
}

function calculateSelectedShapes({
  selectedShapeIds,
  store,
}: {
  selectedShapeIds: Set<ShapeId>
  store: { [id: string]: CanvasRecord }
}) {
  const shapes: Shape[] = []
  for (const id of selectedShapeIds) {
    const shape = store[id]
    if (shape && shape.typeName === "shape") {
      shapes.push(shape as Shape)
    }
  }
  return shapes
}

/**
 * Helper function to find the page a shape belongs to by walking up the hierarchy.
 */
function getPageForShapeInStore(
  shape: Shape,
  store: { [id: string]: CanvasRecord }
): PageId | undefined {
  // If parent is a page, return it
  if (shape.parentId.startsWith("page:")) {
    return shape.parentId as PageId
  }

  // Walk up through groups to find the page
  let currentParentId = shape.parentId
  while (currentParentId.startsWith("shape:")) {
    const parent = store[currentParentId]
    if (!parent || parent.typeName !== "shape") return undefined
    const parentShape = parent as Shape
    if (parentShape.parentId.startsWith("page:")) {
      return parentShape.parentId as PageId
    }
    currentParentId = parentShape.parentId
  }

  return currentParentId as PageId
}

function calculateCurrentPageShapes({
  currentPageId,
  store,
}: {
  currentPageId: PageId | null
  store: { [id: string]: CanvasRecord }
}) {
  if (!currentPageId) return []

  const shapes: Shape[] = []

  for (const record of Object.values(store)) {
    if (record.typeName === "shape") {
      const shape = record as Shape
      // Include shapes that belong to this page (either directly or through groups)
      const pageForShape = getPageForShapeInStore(shape, store)
      if (pageForShape === currentPageId) {
        shapes.push(shape)
      }
    }
  }

  return shapes.sort((a, b) => {
    // Sort by index
    return a.index - b.index
  })
}
