import {
  copyFile,
  mkdir,
  readdir,
  readFile,
  stat,
  writeFile,
} from "node:fs/promises"
import { basename, extname, join, relative, resolve, sep } from "node:path"
import type {
  OpenPanelsArtifact,
  OpenPanelsPanel,
  OpenPanelsSession,
} from "@openpanels/protocol"
import {
  artifactSchema,
  panelSchema,
  sessionSchema,
} from "@openpanels/protocol"
import type { OpenPanelsStorage } from "@openpanels/runtime"

export interface LocalOpenPanelsStorageOptions {
  projectDir: string
  storageDir?: string
}

export interface WrittenAsset {
  assetRef: string
  fileName: string
  filePath: string
}

export interface PanelSelectionState {
  assetRef?: string | null
  panelId: string
  selectedShapeIds: string[]
  selectedShapes: unknown[]
  sessionId: string
  updatedAt: string
}

export class LocalOpenPanelsStorage implements OpenPanelsStorage {
  readonly projectDir: string
  readonly rootDir: string

  constructor(options: LocalOpenPanelsStorageOptions) {
    this.projectDir = resolve(options.projectDir)
    this.rootDir = resolve(
      options.storageDir ?? join(this.projectDir, ".myopenpanels")
    )
    assertSafeRoot(this.projectDir, this.rootDir)
  }

  async listSessions(): Promise<OpenPanelsSession[]> {
    const sessionsDir = this.#resolve("sessions")
    try {
      const entries = await readdir(sessionsDir, { withFileTypes: true })
      const sessions = await Promise.all(
        entries
          .filter((entry) => entry.isDirectory())
          .map((entry) => this.readSession(entry.name))
      )
      return sessions
        .filter((session): session is OpenPanelsSession => Boolean(session))
        .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") return []
      throw error
    }
  }

  async readSession(sessionId: string): Promise<OpenPanelsSession | null> {
    return readJson(
      this.#resolve("sessions", sanitizePathPart(sessionId), "session.json"),
      sessionSchema
    )
  }

  async writeSession(session: OpenPanelsSession): Promise<void> {
    const sessionDir = this.#resolve("sessions", sanitizePathPart(session.id))
    await mkdir(sessionDir, { recursive: true })
    await writeJson(join(sessionDir, "session.json"), session)
    await this.#writeIndex()
  }

  async readPanel(
    sessionId: string,
    panelId: string
  ): Promise<OpenPanelsPanel | null> {
    return readJson(
      this.#panelFile(sessionId, panelId, "panel.json"),
      panelSchema
    )
  }

  async writePanel(panel: OpenPanelsPanel): Promise<void> {
    const panelDir = this.#panelDir(panel.sessionId, panel.id)
    await mkdir(panelDir, { recursive: true })
    await writeJson(join(panelDir, "panel.json"), panel)
  }

  async readPanelState<TState = unknown>(
    sessionId: string,
    panelId: string
  ): Promise<TState | null> {
    return readJson(
      this.#panelFile(sessionId, panelId, "state.json")
    ) as Promise<TState | null>
  }

  async writePanelState(
    sessionId: string,
    panelId: string,
    state: unknown
  ): Promise<void> {
    const panelDir = this.#panelDir(sessionId, panelId)
    await mkdir(panelDir, { recursive: true })
    await writeJson(join(panelDir, "state.json"), state)
  }

  async listArtifacts(
    sessionId: string,
    panelId?: string
  ): Promise<OpenPanelsArtifact[]> {
    const artifactsPath = this.#resolve(
      "sessions",
      sanitizePathPart(sessionId),
      "artifacts.json"
    )
    const artifacts = (await readJson(artifactsPath)) as
      | OpenPanelsArtifact[]
      | null
    const list = artifacts ?? []
    return panelId
      ? list.filter((artifact) => artifact.panelId === panelId)
      : list
  }

  async writeArtifact(
    sessionId: string,
    artifact: OpenPanelsArtifact
  ): Promise<void> {
    const parsed = artifactSchema.parse(artifact)
    const sessionDir = this.#resolve("sessions", sanitizePathPart(sessionId))
    await mkdir(sessionDir, { recursive: true })
    const artifactsPath = join(sessionDir, "artifacts.json")
    const artifacts =
      ((await readJson(artifactsPath)) as OpenPanelsArtifact[] | null) ?? []
    await writeJson(artifactsPath, [...artifacts, parsed])
  }

  async readPanelSelection(
    sessionId: string,
    panelId: string
  ): Promise<PanelSelectionState | null> {
    return readJson(this.#panelFile(sessionId, panelId, "selection.json"))
  }

  async writePanelSelection(selection: PanelSelectionState): Promise<void> {
    const panelDir = this.#panelDir(selection.sessionId, selection.panelId)
    await mkdir(panelDir, { recursive: true })
    await writeJson(join(panelDir, "selection.json"), selection)
  }

  async writeAssetFromFile(input: {
    sessionId: string
    panelId: string
    sourcePath: string
    requestedName?: string
  }): Promise<WrittenAsset> {
    const sourcePath = resolve(input.sourcePath)
    await stat(sourcePath)
    const assetsDir = this.#panelFile(input.sessionId, input.panelId, "assets")
    await mkdir(assetsDir, { recursive: true })
    const fileName = await uniqueFileName(
      assetsDir,
      input.requestedName ?? basename(sourcePath)
    )
    const filePath = join(assetsDir, fileName)
    assertInside(this.rootDir, filePath)
    await copyFile(sourcePath, filePath)
    return {
      fileName,
      filePath,
      assetRef: [
        "sessions",
        sanitizePathPart(input.sessionId),
        "panels",
        sanitizePathPart(input.panelId),
        "assets",
        fileName,
      ].join("/"),
    }
  }

  async writeAssetFromBuffer(input: {
    sessionId: string
    panelId: string
    buffer: Buffer
    requestedName: string
    overwrite?: boolean
  }): Promise<WrittenAsset> {
    const assetsDir = this.#panelFile(input.sessionId, input.panelId, "assets")
    await mkdir(assetsDir, { recursive: true })
    const fileName = input.overwrite
      ? sanitizeAssetPath(input.requestedName)
      : await uniqueFileName(assetsDir, input.requestedName)
    const filePath = join(assetsDir, fileName)
    assertInside(this.rootDir, filePath)
    await mkdir(resolve(filePath, ".."), { recursive: true })
    await writeFile(filePath, input.buffer)
    return {
      fileName,
      filePath,
      assetRef: [
        "sessions",
        sanitizePathPart(input.sessionId),
        "panels",
        sanitizePathPart(input.panelId),
        "assets",
        ...fileName.split("/").map(sanitizePathPart),
      ].join("/"),
    }
  }

  async readAsset(assetRef: string): Promise<Buffer> {
    return readFile(this.assetPath(assetRef))
  }

  assetPath(assetRef: string): string {
    const parts = assetRef.split("/").map(sanitizePathPart)
    return this.#resolve(...parts)
  }

  #panelDir(sessionId: string, panelId: string): string {
    return this.#resolve(
      "sessions",
      sanitizePathPart(sessionId),
      "panels",
      sanitizePathPart(panelId)
    )
  }

  #panelFile(sessionId: string, panelId: string, fileName: string): string {
    return join(this.#panelDir(sessionId, panelId), fileName)
  }

  #resolve(...parts: string[]): string {
    const target = resolve(this.rootDir, ...parts)
    assertInside(this.rootDir, target)
    return target
  }

  async #writeIndex(): Promise<void> {
    const sessions = await this.listSessions()
    await mkdir(this.rootDir, { recursive: true })
    await writeJson(join(this.rootDir, "index.json"), {
      schemaVersion: 1,
      sessions: sessions.map(({ id, title, updatedAt }) => ({
        id,
        title,
        updatedAt,
      })),
    })
  }
}

export function sanitizePathPart(value: string): string {
  const safe = basename(String(value))
    .replace(/[^a-zA-Z0-9._:-]+/g, "-")
    .replace(/^-+|-+$/g, "")
  if (!safe || safe === "." || safe === "..") {
    throw new Error(`Unsafe OpenPanels path part: ${value}`)
  }
  return safe
}

async function readJson<T>(
  filePath: string,
  schema?: { parse: (value: unknown) => T }
): Promise<T | null> {
  try {
    const raw = await readFile(filePath, "utf8")
    const data = JSON.parse(raw)
    return schema ? schema.parse(data) : (data as T)
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return null
    throw error
  }
}

async function writeJson(filePath: string, data: unknown): Promise<void> {
  await mkdir(resolve(filePath, ".."), { recursive: true })
  await writeFile(filePath, `${JSON.stringify(data, null, 2)}\n`, "utf8")
}

async function uniqueFileName(
  dir: string,
  requestedName: string
): Promise<string> {
  const safe = sanitizeFileName(requestedName)
  const ext = extname(safe)
  const base = safe.slice(0, safe.length - ext.length)
  let candidate = safe
  let counter = 2
  while (true) {
    try {
      await stat(join(dir, candidate))
      candidate = `${base}-${counter}${ext}`
      counter += 1
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") return candidate
      throw error
    }
  }
}

function sanitizeFileName(value: string): string {
  const raw = basename(value || "asset.bin")
  const ext = extname(raw)
  const base = raw
    .slice(0, raw.length - ext.length)
    .replace(/[^a-zA-Z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "")
  return `${base || "asset"}${ext || ".bin"}`
}

function sanitizeAssetPath(value: string): string {
  const parts = value.split("/").filter(Boolean)
  return parts
    .map((part, index) =>
      index === parts.length - 1
        ? sanitizeFileName(part)
        : sanitizePathPart(part)
    )
    .join("/")
}

function assertSafeRoot(projectDir: string, rootDir: string): void {
  if (basename(rootDir) !== ".myopenpanels") {
    throw new Error("OpenPanels local storage root must be named .myopenpanels")
  }
  assertInside(projectDir, rootDir)
}

function assertInside(parent: string, child: string): void {
  const rel = relative(parent, child)
  if (rel !== "" && (rel.startsWith("..") || rel.includes(`..${sep}`))) {
    throw new Error(`Path escapes OpenPanels root: ${child}`)
  }
}
