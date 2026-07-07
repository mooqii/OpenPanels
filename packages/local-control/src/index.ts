import { randomUUID } from "node:crypto"
import { mkdir, readFile, stat, writeFile } from "node:fs/promises"
import {
  basename,
  dirname,
  extname,
  join,
  relative,
  resolve,
  sep,
} from "node:path"
import { LocalOpenPanelsStorage } from "@openpanels/local-storage"
import type { OpenPanelsPanel, OpenPanelsSession } from "@openpanels/protocol"
import { OpenPanelsRuntime } from "@openpanels/runtime"

export interface OpenPanelsLocalPaths {
  projectDir: string
  storageDir: string
}

export interface OpenPanelsLocalContext {
  paths: OpenPanelsLocalPaths
  runtime: OpenPanelsRuntime
  storage: LocalOpenPanelsStorage
}

export interface CanvasBootstrap {
  panel: OpenPanelsPanel
  panelDir: string
  session: OpenPanelsSession
  sessions: OpenPanelsSession[]
  state: unknown
  storageDir: string
}

export interface SelectionResult {
  base64: string | null
  mimeType: string | null
  selection: Record<string, unknown>
  selectionFile: string
}

export interface InsertImageInput {
  anchorShapeId?: string
  displayHeight?: number
  displayWidth?: number
  fileName?: string
  imagePath: string
  placement?: "below" | "left" | "right"
  projectDir?: string
  replaceShapeId?: string
  sessionId?: string
}

export interface InsertPlaceholderInput {
  anchorShapeId?: string
  displayHeight?: number
  displayWidth?: number
  projectDir?: string
  sessionId?: string
  text?: string
}

interface ImageDimensions {
  height: number
  width: number
}

interface Bounds {
  height: number
  width: number
  x: number
  y: number
}

interface OccupiedBounds {
  maxX: number
  maxY: number
  minX: number
  minY: number
}

const DEFAULT_CANVAS_GAP = 80
const DEFAULT_PLACEHOLDER_SIZE = 512
const MAX_POSITION_SCAN = 40

export function resolveOpenPanelsPaths(
  projectDir?: string
): OpenPanelsLocalPaths {
  const resolvedProjectDir = resolve(
    projectDir || process.env.OPENPANELS_PROJECT_DIR || process.cwd()
  )
  const storageDir = resolve(resolvedProjectDir, ".myopenpanels")
  assertInside(resolvedProjectDir, storageDir)
  return { projectDir: resolvedProjectDir, storageDir }
}

export function createOpenPanelsLocalContext(
  projectDir?: string
): OpenPanelsLocalContext {
  const paths = resolveOpenPanelsPaths(projectDir)
  const storage = new LocalOpenPanelsStorage({ projectDir: paths.projectDir })
  const runtime = new OpenPanelsRuntime({ storage })
  return { paths, runtime, storage }
}

export function createLocalOpenPanelsRuntime(projectDir: string) {
  return createOpenPanelsLocalContext(projectDir).runtime
}

export async function ensureCanvasBootstrap(
  context: OpenPanelsLocalContext,
  requestedSessionId?: string | null
): Promise<CanvasBootstrap> {
  const sessions = await context.runtime.listSessions()
  const activeSessionId = await readActiveSession(context)
  const session =
    (requestedSessionId
      ? await context.runtime.getSession(requestedSessionId)
      : null) ??
    (activeSessionId
      ? await context.runtime.getSession(activeSessionId)
      : null) ??
    sessions[0] ??
    (await context.runtime.createSession({ title: nextProjectTitle(sessions) }))
  const bootstrap = await ensureCanvasForSession(context, session)
  await writeActiveSession(context, bootstrap.session.id)
  return bootstrap
}

export async function createCanvasProject(
  context: OpenPanelsLocalContext,
  title?: string
): Promise<CanvasBootstrap> {
  const nextTitle =
    title?.trim() || nextProjectTitle(await context.runtime.listSessions())
  const session = await context.runtime.createSession({ title: nextTitle })
  const bootstrap = await ensureCanvasForSession(context, session)
  await writeActiveSession(context, bootstrap.session.id)
  return bootstrap
}

export async function getCanvasState(input: {
  projectDir?: string
  sessionId?: string | null
}): Promise<CanvasBootstrap> {
  const context = createOpenPanelsLocalContext(input.projectDir)
  return ensureCanvasBootstrap(context, input.sessionId)
}

export async function readActiveSession(
  context: OpenPanelsLocalContext
): Promise<string | null> {
  try {
    const active = JSON.parse(
      await readFile(activeSessionPath(context), "utf8")
    ) as { sessionId?: unknown }
    return typeof active.sessionId === "string" ? active.sessionId : null
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return null
    throw error
  }
}

export async function writeActiveSession(
  context: OpenPanelsLocalContext,
  sessionId: string
): Promise<void> {
  const filePath = activeSessionPath(context)
  await mkdir(dirname(filePath), { recursive: true })
  await writeJson(filePath, {
    sessionId,
    updatedAt: new Date().toISOString(),
  })
}

export async function renameSession(
  context: OpenPanelsLocalContext,
  sessionId: string,
  title: string | undefined
): Promise<OpenPanelsSession> {
  const session = await context.storage.readSession(sessionId)
  if (!session) throw new Error(`OpenPanels session not found: ${sessionId}`)
  const nextTitle = title?.trim()
  if (!nextTitle) throw new Error("Project title is required")
  const updated = {
    ...session,
    title: nextTitle,
    updatedAt: new Date().toISOString(),
  }
  await context.storage.writeSession(updated)
  return updated
}

export async function savePanelState(input: {
  panelId: string
  projectDir?: string
  sessionId: string
  state: unknown
}): Promise<{ panelId: string; saved: true; sessionId: string }> {
  const context = createOpenPanelsLocalContext(input.projectDir)
  await context.runtime.savePanelState(
    input.sessionId,
    input.panelId,
    input.state
  )
  await writeActiveSession(context, input.sessionId)
  return { saved: true, sessionId: input.sessionId, panelId: input.panelId }
}

export async function saveSelectionState(input: {
  imageDataUrl?: string | null
  panelId: string
  projectDir?: string
  selection?: Record<string, unknown> | null
  sessionId: string
}): Promise<{ saved: true; selection: Record<string, unknown> }> {
  const context = createOpenPanelsLocalContext(input.projectDir)
  let assetRef =
    typeof input.selection?.assetRef === "string"
      ? input.selection.assetRef
      : null

  if (input.imageDataUrl) {
    const image = dataUrlToBuffer(input.imageDataUrl)
    const written = await context.storage.writeAssetFromBuffer({
      sessionId: input.sessionId,
      panelId: input.panelId,
      buffer: image.buffer,
      requestedName: "__selection/current.png",
      overwrite: true,
    })
    assetRef = written.assetRef
  }

  const selection = {
    sessionId: input.sessionId,
    panelId: input.panelId,
    selectedShapeIds: Array.isArray(input.selection?.selectedShapeIds)
      ? input.selection?.selectedShapeIds
      : [],
    selectedShapes: Array.isArray(input.selection?.selectedShapes)
      ? input.selection?.selectedShapes
      : [],
    assetRef,
    updatedAt: new Date().toISOString(),
  }
  await context.storage.writePanelSelection(selection)
  await writeActiveSession(context, input.sessionId)
  return { saved: true, selection }
}

export async function getSelection(input: {
  includeImageBase64?: boolean
  projectDir?: string
  sessionId?: string | null
}): Promise<SelectionResult> {
  const context = createOpenPanelsLocalContext(input.projectDir)
  const bootstrap = await ensureCanvasBootstrap(context, input.sessionId)
  const rawState =
    (await context.storage.readPanelState(
      bootstrap.session.id,
      bootstrap.panel.id
    )) ?? emptyCanvasSnapshot()
  const rawSelection =
    (await context.storage.readPanelSelection(
      bootstrap.session.id,
      bootstrap.panel.id
    )) ?? emptySelection(bootstrap.session.id, bootstrap.panel.id)
  const selection = withLastImageFallback(rawSelection, rawState)
  let base64: string | null = null
  const assetRef =
    typeof selection.assetRef === "string" ? selection.assetRef : null
  if (input.includeImageBase64 && assetRef) {
    base64 = (await context.storage.readAsset(assetRef)).toString("base64")
  }
  return {
    selection: selection as Record<string, unknown>,
    selectionFile: panelFile(
      context,
      bootstrap.session.id,
      bootstrap.panel.id,
      "selection.json"
    ),
    base64,
    mimeType: assetRef ? mimeTypeForFile(assetRef) : null,
  }
}

export async function readPanelAsset(input: {
  assetRef: string
  projectDir?: string
}): Promise<{ assetRef: string; base64: string; mimeType: string }> {
  const context = createOpenPanelsLocalContext(input.projectDir)
  const data = await context.storage.readAsset(input.assetRef)
  return {
    assetRef: input.assetRef,
    base64: data.toString("base64"),
    mimeType: mimeTypeForFile(input.assetRef),
  }
}

export async function readSelectionAsset(input: {
  projectDir?: string
  sessionId?: string | null
}): Promise<{
  assetRef: string
  base64: string
  filePath: string
  mimeType: string
}> {
  const context = createOpenPanelsLocalContext(input.projectDir)
  const selection = await getSelection({
    projectDir: input.projectDir,
    sessionId: input.sessionId,
    includeImageBase64: true,
  })
  const assetRef =
    typeof selection.selection.assetRef === "string"
      ? selection.selection.assetRef
      : null
  if (!(assetRef && selection.base64)) {
    throw new Error("No MyOpenPanels selection asset is available.")
  }
  return {
    assetRef,
    base64: selection.base64,
    filePath: context.storage.assetPath(assetRef),
    mimeType: selection.mimeType ?? mimeTypeForFile(assetRef),
  }
}

export async function writePanelAsset(input: {
  panelId: string
  projectDir?: string
  requestedName?: string
  sessionId: string
  sourcePath: string
}) {
  const context = createOpenPanelsLocalContext(input.projectDir)
  return context.storage.writeAssetFromFile({
    sessionId: input.sessionId,
    panelId: input.panelId,
    sourcePath: input.sourcePath,
    requestedName: input.requestedName,
  })
}

export async function insertImage(input: InsertImageInput) {
  const context = createOpenPanelsLocalContext(input.projectDir)
  const bootstrap = await ensureCanvasBootstrap(context, input.sessionId)
  const source = resolve(input.imagePath)
  await stat(source)
  const imageBuffer = await readFile(source)
  const dimensions: Partial<ImageDimensions> =
    readImageDimensions(imageBuffer) ?? {}
  const written = await context.storage.writeAssetFromFile({
    sessionId: bootstrap.session.id,
    panelId: bootstrap.panel.id,
    sourcePath: source,
    requestedName: input.fileName ?? basename(source),
  })

  const state =
    ((await context.storage.readPanelState(
      bootstrap.session.id,
      bootstrap.panel.id
    )) as Record<string, any> | null) ?? emptyCanvasSnapshot()
  const store =
    state.store && typeof state.store === "object" ? state.store : {}
  const pageId = state.currentPageId || findFirstPageId(store) || "page:main"
  if (!store[pageId]) {
    store[pageId] = { id: pageId, typeName: "page", name: "Page 1", index: 1 }
  }

  const replaceShape = input.replaceShapeId ? store[input.replaceShapeId] : null
  const replaceBounds =
    replaceShape?.typeName === "shape" ? shapeBounds(replaceShape) : null
  const width =
    input.displayWidth ??
    (replaceBounds ? replaceBounds.width : undefined) ??
    dimensions.width ??
    512
  const height =
    input.displayHeight ??
    (replaceBounds ? replaceBounds.height : undefined) ??
    (dimensions.width && dimensions.height && input.displayWidth
      ? Math.round((input.displayWidth * dimensions.height) / dimensions.width)
      : (dimensions.height ?? 512))
  const anchor = input.anchorShapeId ? store[input.anchorShapeId] : null
  const anchorBounds = anchor?.typeName === "shape" ? shapeBounds(anchor) : null
  const position = replaceBounds
    ? { x: replaceBounds.x, y: replaceBounds.y }
    : placeImage(anchorBounds, width, height, input.placement)
  const assetId = createId("asset")
  const shapeId = createId("shape")
  const assetUrl = `/api/panels/${encodeURIComponent(bootstrap.session.id)}/${encodeURIComponent(bootstrap.panel.id)}/assets/${encodeURIComponent(written.fileName)}`
  const mimeType = mimeTypeForFile(written.fileName)
  const parentId = replaceShape?.parentId || anchor?.parentId || pageId

  store[assetId] = {
    id: assetId,
    typeName: "asset",
    type: "image",
    props: {
      name: written.fileName,
      src: assetUrl,
      w: dimensions.width ?? width,
      h: dimensions.height ?? height,
      mimeType,
      isAnimated: false,
    },
    meta: { assetRef: written.assetRef },
  }
  store[shapeId] = {
    id: shapeId,
    typeName: "shape",
    type: "image",
    parentId,
    index: nextShapeIndex(store, pageId),
    props: {
      x: position.x,
      y: position.y,
      width,
      height,
      assetId,
    },
  }
  if (replaceShape?.typeName === "shape" && input.replaceShapeId) {
    delete store[input.replaceShapeId]
  }
  state.store = store
  state.currentPageId = pageId
  state.selectedShapeIds = [shapeId]
  await context.storage.writePanelState(
    bootstrap.session.id,
    bootstrap.panel.id,
    state
  )
  await writeActiveSession(context, bootstrap.session.id)
  return {
    sessionId: bootstrap.session.id,
    panelId: bootstrap.panel.id,
    assetId,
    shapeId,
    assetRef: written.assetRef,
    assetFile: written.filePath,
    assetUrl,
    replacedShapeId:
      replaceShape?.typeName === "shape" ? input.replaceShapeId : null,
    bounds: { x: position.x, y: position.y, width, height },
  }
}

export async function insertPlaceholder(input: InsertPlaceholderInput) {
  const context = createOpenPanelsLocalContext(input.projectDir)
  const bootstrap = await ensureCanvasBootstrap(context, input.sessionId)
  const state =
    ((await context.storage.readPanelState(
      bootstrap.session.id,
      bootstrap.panel.id
    )) as Record<string, any> | null) ?? emptyCanvasSnapshot()
  const store =
    state.store && typeof state.store === "object" ? state.store : {}
  const pageId = state.currentPageId || findFirstPageId(store) || "page:main"
  if (!store[pageId]) {
    store[pageId] = { id: pageId, typeName: "page", name: "Page 1", index: 1 }
  }

  const width = input.displayWidth ?? DEFAULT_PLACEHOLDER_SIZE
  const height = input.displayHeight ?? DEFAULT_PLACEHOLDER_SIZE
  const anchor = input.anchorShapeId ? store[input.anchorShapeId] : null
  const anchorBounds = anchor?.typeName === "shape" ? shapeBounds(anchor) : null
  const position = findCanvasPlacementPosition({
    anchorBounds,
    height,
    preferredPosition: { x: 160, y: 160 },
    store,
    width,
  })
  const shapeId = createId("shape")

  store[shapeId] = {
    id: shapeId,
    typeName: "shape",
    type: "placeholder",
    parentId: anchor?.parentId || pageId,
    index: nextShapeIndex(store, pageId),
    props: {
      cornerRadius: 0,
      height,
      text: input.text ?? "正在生成图片",
      width,
      x: position.x,
      y: position.y,
    },
    meta: {
      openpanelsGenerationPlaceholder: true,
      createdAt: new Date().toISOString(),
    },
  }
  state.store = store
  state.currentPageId = pageId
  state.selectedShapeIds = [shapeId]
  await context.storage.writePanelState(
    bootstrap.session.id,
    bootstrap.panel.id,
    state
  )
  await writeActiveSession(context, bootstrap.session.id)
  return {
    sessionId: bootstrap.session.id,
    panelId: bootstrap.panel.id,
    shapeId,
    bounds: { x: position.x, y: position.y, width, height },
  }
}

export function emptyCanvasSnapshot(): Record<string, any> {
  return {
    schema: {
      schemaVersion: 1,
      recordVersions: { page: 1, shape: 1, asset: 1 },
    },
    currentPageId: "page:main",
    openedGroupId: null,
    selectedShapeIds: [],
    store: {
      "page:main": {
        id: "page:main",
        typeName: "page",
        name: "Page 1",
        index: 1,
      },
    },
  }
}

export function normalizeSerializableSnapshot(value: unknown): unknown {
  if (
    value &&
    typeof value === "object" &&
    "selectedShapeIds" in value &&
    !Array.isArray((value as { selectedShapeIds?: unknown }).selectedShapeIds)
  ) {
    return {
      ...(value as Record<string, unknown>),
      selectedShapeIds:
        (value as { selectedShapeIds?: unknown }).selectedShapeIds instanceof
        Set
          ? [...(value as { selectedShapeIds: Set<string> }).selectedShapeIds]
          : [],
    }
  }
  return value
}

export function dataUrlToBuffer(dataUrl: string): {
  buffer: Buffer
  mimeType: string
} {
  const match = dataUrl.match(/^data:([^;,]+)?(;base64)?,(.*)$/)
  if (!match) throw new Error("Expected a data URL")
  const mimeType = match[1] || "application/octet-stream"
  const isBase64 = Boolean(match[2])
  const data = match[3] || ""
  return {
    mimeType,
    buffer: isBase64
      ? Buffer.from(data, "base64")
      : Buffer.from(decodeURIComponent(data), "utf8"),
  }
}

export function mimeTypeForFile(fileName: string): string {
  switch (extname(fileName).toLowerCase()) {
    case ".gif":
      return "image/gif"
    case ".jpg":
    case ".jpeg":
      return "image/jpeg"
    case ".png":
      return "image/png"
    case ".webp":
      return "image/webp"
    default:
      return "application/octet-stream"
  }
}

async function ensureCanvasForSession(
  context: OpenPanelsLocalContext,
  session: OpenPanelsSession
): Promise<CanvasBootstrap> {
  let currentSession = session
  let panel: OpenPanelsPanel | null = null
  for (const panelId of currentSession.panelIds) {
    const candidate = await context.runtime.getPanel(currentSession.id, panelId)
    if (candidate?.kind === "canvas") {
      panel = candidate
      break
    }
  }
  if (!panel) {
    panel = await context.runtime.openPanel({
      sessionId: session.id,
      kind: "canvas",
      title: "Design canvas",
      initialState: emptyCanvasSnapshot(),
    })
    currentSession =
      (await context.runtime.getSession(currentSession.id)) ?? currentSession
  }
  const state =
    (await context.runtime.readPanelState(currentSession.id, panel.id)) ??
    emptyCanvasSnapshot()
  return {
    session: currentSession,
    panel,
    sessions: await context.runtime.listSessions(),
    state: normalizeSerializableSnapshot(state),
    storageDir: context.paths.storageDir,
    panelDir: panelDir(context, currentSession.id, panel.id),
  }
}

function activeSessionPath(context: OpenPanelsLocalContext): string {
  return join(context.storage.rootDir, "active-session.json")
}

function nextProjectTitle(sessions: OpenPanelsSession[]): string {
  let maxProjectNumber = 0
  for (const session of sessions) {
    const match = session.title.match(/^Project (\d+)$/)
    if (match) {
      maxProjectNumber = Math.max(maxProjectNumber, Number(match[1]))
    }
  }
  return `Project ${maxProjectNumber + 1}`
}

function emptySelection(sessionId: string, panelId: string) {
  return {
    sessionId,
    panelId,
    selectedShapeIds: [],
    selectedShapes: [],
    assetRef: null,
    updatedAt: new Date().toISOString(),
  }
}

function withLastImageFallback(selection: unknown, state: unknown) {
  const current =
    selection && typeof selection === "object"
      ? (selection as Record<string, any>)
      : {}
  const selectedShapes = Array.isArray(current.selectedShapes)
    ? current.selectedShapes
    : []
  if (selectedShapes.length > 0) return current
  const fallback = findLastImageSelectionShape(state)
  if (!fallback) return current
  return {
    ...current,
    selectedShapeIds: [fallback.id],
    selectedShapes: [fallback],
    assetRef: fallback.asset?.assetRef ?? null,
    fallback: "last-image",
  }
}

function findLastImageSelectionShape(state: unknown) {
  const snapshot =
    state && typeof state === "object" ? (state as Record<string, any>) : {}
  const store =
    snapshot.store && typeof snapshot.store === "object" ? snapshot.store : {}
  const images = Object.values(store)
    .filter(
      (record: any) => record?.typeName === "shape" && record.type === "image"
    )
    .sort((a: any, b: any) => {
      const indexDiff = (Number(b.index) || 0) - (Number(a.index) || 0)
      if (indexDiff !== 0) return indexDiff
      return String(b.id).localeCompare(String(a.id))
    })
  const shape = images[0]
  if (!shape) return null
  return summarizeShapeForAgent(shape, store)
}

function summarizeShapeForAgent(shape: any, store: Record<string, any>) {
  const asset = shape?.props?.assetId ? store[shape.props.assetId] : null
  const assetRef = assetRefFromAsset(asset)
  return {
    id: shape.id,
    type: shape.type,
    parentId: shape.parentId,
    props: shape.props ?? {},
    bounds: shapeBounds(shape),
    asset: asset
      ? {
          id: asset.id,
          type: asset.type,
          name: asset.props?.name ?? null,
          src: asset.props?.src ?? null,
          w: asset.props?.w ?? null,
          h: asset.props?.h ?? null,
          mimeType: asset.props?.mimeType ?? null,
          assetRef,
        }
      : null,
  }
}

function assetRefFromAsset(asset: any): string | null {
  if (!asset) return null
  if (typeof asset.meta?.assetRef === "string") return asset.meta.assetRef
  const src = asset.props?.src
  if (typeof src !== "string") return null
  const match = src.match(/^\/api\/panels\/([^/]+)\/([^/]+)\/assets\/(.+)$/)
  if (!match) return null
  const sessionId = decodeURIComponent(match[1])
  const panelId = decodeURIComponent(match[2])
  const assetPath = match[3].split("/").map(decodeURIComponent).join("/")
  return ["sessions", sessionId, "panels", panelId, "assets", assetPath].join(
    "/"
  )
}

function findFirstPageId(store: Record<string, any>) {
  return (
    Object.values(store).find((record: any) => record?.typeName === "page")
      ?.id ?? null
  )
}

function nextShapeIndex(store: Record<string, any>, pageId: string) {
  let max = 0
  for (const record of Object.values(store)) {
    if (
      record?.typeName === "shape" &&
      record.parentId === pageId &&
      Number.isFinite(record.index)
    ) {
      max = Math.max(max, record.index)
    }
  }
  return max + 1
}

function shapeBounds(shape: any): Bounds {
  const props = shape.props || {}
  return {
    x: Number(props.x) || 0,
    y: Number(props.y) || 0,
    width: Number(props.width || props.w) || 160,
    height: Number(props.height || props.h) || 120,
  }
}

function toOccupiedBounds(bounds: Bounds): OccupiedBounds {
  return {
    maxX: bounds.x + bounds.width,
    maxY: bounds.y + bounds.height,
    minX: bounds.x,
    minY: bounds.y,
  }
}

function intersectsWithPadding(
  left: OccupiedBounds,
  right: OccupiedBounds,
  padding: number
) {
  return !(
    left.maxX <= right.minX - padding ||
    left.minX >= right.maxX + padding ||
    left.maxY <= right.minY - padding ||
    left.minY >= right.maxY + padding
  )
}

function hasOverlap(
  target: OccupiedBounds,
  occupiedBounds: OccupiedBounds[],
  padding: number
) {
  return occupiedBounds.some((bounds) =>
    intersectsWithPadding(target, bounds, padding)
  )
}

function canvasOccupiedBounds(store: Record<string, any>): OccupiedBounds[] {
  return Object.values(store)
    .filter(
      (record: any) =>
        record?.typeName === "shape" &&
        (record.type === "image" || record.type === "placeholder")
    )
    .map((record: any) => toOccupiedBounds(shapeBounds(record)))
}

function overallBounds(bounds: OccupiedBounds[]): OccupiedBounds | null {
  const first = bounds[0]
  if (!first) return null
  return bounds.reduce(
    (current, bound) => ({
      maxX: Math.max(current.maxX, bound.maxX),
      maxY: Math.max(current.maxY, bound.maxY),
      minX: Math.min(current.minX, bound.minX),
      minY: Math.min(current.minY, bound.minY),
    }),
    first
  )
}

function placementBelowExistingImages(
  occupiedBounds: OccupiedBounds[],
  padding: number
): { x: number; y: number } | null {
  const overall = overallBounds(occupiedBounds)
  if (!overall) return null
  const bottomMost = occupiedBounds.reduce((current, bounds) => {
    if (bounds.maxY > current.maxY) return bounds
    if (bounds.maxY === current.maxY && bounds.minX < current.minX) {
      return bounds
    }
    return current
  }, occupiedBounds[0])
  return { x: bottomMost.minX, y: overall.maxY + padding }
}

function scanForAvailablePosition(input: {
  basePosition: { x: number; y: number }
  height: number
  occupiedBounds: OccupiedBounds[]
  padding: number
  width: number
}) {
  const initialCandidate = toOccupiedBounds({
    x: input.basePosition.x,
    y: input.basePosition.y,
    width: input.width,
    height: input.height,
  })
  if (!hasOverlap(initialCandidate, input.occupiedBounds, input.padding)) {
    return input.basePosition
  }

  const stepX = Math.max(input.width + input.padding, input.padding)
  const stepY = Math.max(input.height + input.padding, input.padding)
  for (let row = 0; row < MAX_POSITION_SCAN; row += 1) {
    for (let col = 0; col < MAX_POSITION_SCAN; col += 1) {
      const x = input.basePosition.x + col * stepX
      const y = input.basePosition.y + row * stepY
      const candidate = toOccupiedBounds({
        x,
        y,
        width: input.width,
        height: input.height,
      })
      if (!hasOverlap(candidate, input.occupiedBounds, input.padding)) {
        return { x, y }
      }
    }
  }

  const overall = overallBounds(input.occupiedBounds)
  return overall
    ? { x: overall.minX, y: overall.maxY + input.padding }
    : input.basePosition
}

function findCanvasPlacementPosition(input: {
  anchorBounds: Bounds | null
  height: number
  preferredPosition: { x: number; y: number }
  store: Record<string, any>
  width: number
}) {
  const occupiedBounds = canvasOccupiedBounds(input.store)
  if (occupiedBounds.length === 0) return input.preferredPosition

  if (input.anchorBounds) {
    const anchorPosition = {
      x: input.anchorBounds.x + input.anchorBounds.width + DEFAULT_CANVAS_GAP,
      y: input.anchorBounds.y,
    }
    const anchorCandidate = toOccupiedBounds({
      ...anchorPosition,
      width: input.width,
      height: input.height,
    })
    if (!hasOverlap(anchorCandidate, occupiedBounds, DEFAULT_CANVAS_GAP)) {
      return anchorPosition
    }
  }

  const basePosition =
    placementBelowExistingImages(occupiedBounds, DEFAULT_CANVAS_GAP) ??
    input.preferredPosition

  return scanForAvailablePosition({
    basePosition,
    height: input.height,
    occupiedBounds,
    padding: DEFAULT_CANVAS_GAP,
    width: input.width,
  })
}

function placeImage(
  anchorBounds: { height: number; width: number; x: number; y: number } | null,
  width: number,
  _height: number,
  placement: "below" | "left" | "right" = "right"
) {
  if (!anchorBounds) return { x: 160, y: 160 }
  const margin = 40
  switch (placement) {
    case "left":
      return { x: anchorBounds.x - width - margin, y: anchorBounds.y }
    case "below":
      return {
        x: anchorBounds.x,
        y: anchorBounds.y + anchorBounds.height + margin,
      }
    default:
      return {
        x: anchorBounds.x + anchorBounds.width + margin,
        y: anchorBounds.y,
      }
  }
}

function readImageDimensions(buffer: Buffer): ImageDimensions | null {
  if (
    buffer.length >= 24 &&
    buffer[0] === 0x89 &&
    buffer.toString("ascii", 1, 4) === "PNG"
  ) {
    return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) }
  }
  if (buffer.length >= 10 && buffer.toString("ascii", 0, 3) === "GIF") {
    return { width: buffer.readUInt16LE(6), height: buffer.readUInt16LE(8) }
  }
  if (buffer.length >= 4 && buffer[0] === 0xff && buffer[1] === 0xd8) {
    let offset = 2
    while (offset < buffer.length) {
      if (buffer[offset] !== 0xff) break
      const marker = buffer[offset + 1]
      const length = buffer.readUInt16BE(offset + 2)
      if (marker >= 0xc0 && marker <= 0xc3) {
        return {
          height: buffer.readUInt16BE(offset + 5),
          width: buffer.readUInt16BE(offset + 7),
        }
      }
      offset += 2 + length
    }
  }
  return null
}

function panelDir(
  context: OpenPanelsLocalContext,
  sessionId: string,
  panelId: string
) {
  const dir = join(
    context.paths.storageDir,
    "sessions",
    safePart(sessionId),
    "panels",
    safePart(panelId)
  )
  assertInside(context.paths.storageDir, dir)
  return dir
}

function panelFile(
  context: OpenPanelsLocalContext,
  sessionId: string,
  panelId: string,
  name: string
) {
  return join(panelDir(context, sessionId, panelId), safePart(name))
}

async function writeJson(filePath: string, value: unknown) {
  await mkdir(dirname(filePath), { recursive: true })
  await writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`, "utf8")
}

function createId(prefix: string) {
  return `${prefix}:${randomUUID()}`
}

function safePart(value: string) {
  const safe = basename(String(value))
    .replace(/[^a-zA-Z0-9._:-]+/g, "-")
    .replace(/^-+|-+$/g, "")
  if (!safe || safe === "." || safe === "..") {
    throw new Error(`Unsafe path part: ${value}`)
  }
  return safe
}

function assertInside(parent: string, child: string) {
  const rel = relative(parent, child)
  if (rel.startsWith("..") || rel.includes(`..${sep}`)) {
    throw new Error(`Path escapes OpenPanels storage: ${child}`)
  }
}
