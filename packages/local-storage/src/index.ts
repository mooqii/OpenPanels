import { mkdirSync } from "node:fs"
import {
  copyFile,
  mkdir,
  readFile,
  rm,
  stat,
  writeFile,
} from "node:fs/promises"
import { createRequire } from "node:module"
import { basename, extname, join, relative, resolve, sep } from "node:path"
import type { DatabaseSync } from "node:sqlite"
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
import { migrationChecksum, migrations } from "./migrations/index.ts"

const DATABASE_FILE_NAME = "main.sqlite3"
const require = createRequire(import.meta.url)
const ACCEPTED_MIGRATION_CHECKSUMS: Record<string, string[]> = {
  // 0001 originally used Function#toString in the checksum, which changed
  // between TypeScript source and bundled CLI builds even when the SQL schema
  // was identical. Keep the shipped checksum readable so dev rebuilds do not
  // strand existing local databases.
  "0001_initial": [
    "1271255098e102e2e83ab3ad2f21507e70329341b3008751a1aaef2284c47abb",
  ],
}

type SQLiteRow = Record<string, unknown>

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
  readonly databasePath: string
  readonly projectDir: string
  readonly rootDir: string
  readonly #db: DatabaseSync

  constructor(options: LocalOpenPanelsStorageOptions) {
    this.projectDir = resolve(options.projectDir)
    this.rootDir = resolve(
      options.storageDir ?? join(this.projectDir, ".myopenpanels")
    )
    assertSafeRoot(this.rootDir)
    mkdirSync(this.rootDir, { recursive: true })
    this.databasePath = join(this.rootDir, DATABASE_FILE_NAME)
    this.#db = openDatabase(this.databasePath)
    migrate(this.#db)
  }

  async listSessions(): Promise<OpenPanelsSession[]> {
    const rows = this.#db
      .prepare(
        `SELECT session_json
          FROM sessions
          ORDER BY updated_at DESC, id ASC`
      )
      .all() as SQLiteRow[]
    return rows.map((row) =>
      sessionSchema.parse(parseJsonColumn(row, "session_json"))
    )
  }

  async readSession(sessionId: string): Promise<OpenPanelsSession | null> {
    const row = this.#db
      .prepare("SELECT session_json FROM sessions WHERE id = ?")
      .get(sessionId) as SQLiteRow | undefined
    if (!row) return null
    return sessionSchema.parse(parseJsonColumn(row, "session_json"))
  }

  async writeSession(session: OpenPanelsSession): Promise<void> {
    const parsed = sessionSchema.parse(session)
    this.#db
      .prepare(
        `INSERT INTO sessions (
          id, title, created_at, updated_at, panel_ids_json, session_json
        )
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
          title = excluded.title,
          created_at = excluded.created_at,
          updated_at = excluded.updated_at,
          panel_ids_json = excluded.panel_ids_json,
          session_json = excluded.session_json`
      )
      .run(
        parsed.id,
        parsed.title,
        parsed.createdAt,
        parsed.updatedAt,
        JSON.stringify(parsed.panelIds),
        JSON.stringify(parsed)
      )
  }

  async deleteSession(sessionId: string): Promise<void> {
    runInTransaction(this.#db, () => {
      this.#db.prepare("DELETE FROM sessions WHERE id = ?").run(sessionId)
    })
    await rm(this.#resolve("sessions", sanitizePathPart(sessionId)), {
      recursive: true,
      force: true,
    })
  }

  async readPanel(
    sessionId: string,
    panelId: string
  ): Promise<OpenPanelsPanel | null> {
    const row = this.#db
      .prepare(
        `SELECT panel_json
          FROM panels
          WHERE session_id = ? AND id = ?`
      )
      .get(sessionId, panelId) as SQLiteRow | undefined
    if (!row) return null
    return panelSchema.parse(parseJsonColumn(row, "panel_json"))
  }

  async writePanel(panel: OpenPanelsPanel): Promise<void> {
    const parsed = panelSchema.parse(panel)
    this.#db
      .prepare(
        `INSERT INTO panels (
          id, session_id, kind, title, created_at, updated_at, state_ref,
          panel_json
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(session_id, id) DO UPDATE SET
          kind = excluded.kind,
          title = excluded.title,
          created_at = excluded.created_at,
          updated_at = excluded.updated_at,
          state_ref = excluded.state_ref,
          panel_json = excluded.panel_json`
      )
      .run(
        parsed.id,
        parsed.sessionId,
        parsed.kind,
        parsed.title,
        parsed.createdAt,
        parsed.updatedAt,
        parsed.stateRef ?? null,
        JSON.stringify(parsed)
      )
  }

  async readPanelState<TState = unknown>(
    sessionId: string,
    panelId: string
  ): Promise<TState | null> {
    const row = this.#db
      .prepare(
        `SELECT state_json
          FROM panel_states
          WHERE session_id = ? AND panel_id = ?`
      )
      .get(sessionId, panelId) as SQLiteRow | undefined
    if (!row) return null
    return parseJsonColumn(row, "state_json") as TState
  }

  async writePanelState(
    sessionId: string,
    panelId: string,
    state: unknown
  ): Promise<void> {
    const stateJson = JSON.stringify(state)
    const updatedAt = new Date().toISOString()
    runInTransaction(this.#db, () => {
      this.#db
        .prepare(
          `INSERT INTO panel_states (
            session_id, panel_id, schema_version, state_json, updated_at
          )
          VALUES (?, ?, ?, ?, ?)
          ON CONFLICT(session_id, panel_id) DO UPDATE SET
            schema_version = excluded.schema_version,
            state_json = excluded.state_json,
            updated_at = excluded.updated_at`
        )
        .run(
          sessionId,
          panelId,
          extractSchemaVersion(state),
          stateJson,
          updatedAt
        )
      syncWikiTasks(this.#db, sessionId, panelId, state)
    })
  }

  async listArtifacts(
    sessionId: string,
    panelId?: string
  ): Promise<OpenPanelsArtifact[]> {
    const rows = panelId
      ? (this.#db
          .prepare(
            `SELECT artifact_json
              FROM artifacts
              WHERE session_id = ? AND panel_id = ?
              ORDER BY created_at ASC, id ASC`
          )
          .all(sessionId, panelId) as SQLiteRow[])
      : (this.#db
          .prepare(
            `SELECT artifact_json
              FROM artifacts
              WHERE session_id = ?
              ORDER BY created_at ASC, id ASC`
          )
          .all(sessionId) as SQLiteRow[])
    return rows.map((row) =>
      artifactSchema.parse(parseJsonColumn(row, "artifact_json"))
    )
  }

  async writeArtifact(
    sessionId: string,
    artifact: OpenPanelsArtifact
  ): Promise<void> {
    const parsed = artifactSchema.parse(artifact)
    this.#db
      .prepare(
        `INSERT INTO artifacts (
          id, session_id, panel_id, kind, title, created_at, artifact_json
        )
        VALUES (?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(session_id, id) DO UPDATE SET
          panel_id = excluded.panel_id,
          kind = excluded.kind,
          title = excluded.title,
          created_at = excluded.created_at,
          artifact_json = excluded.artifact_json`
      )
      .run(
        parsed.id,
        sessionId,
        parsed.panelId ?? null,
        parsed.kind,
        parsed.title ?? null,
        parsed.createdAt,
        JSON.stringify(parsed)
      )
  }

  async readPanelSelection(
    sessionId: string,
    panelId: string
  ): Promise<PanelSelectionState | null> {
    const row = this.#db
      .prepare(
        `SELECT selection_json
          FROM panel_selections
          WHERE session_id = ? AND panel_id = ?`
      )
      .get(sessionId, panelId) as SQLiteRow | undefined
    if (!row) return null
    return parseJsonColumn(row, "selection_json") as PanelSelectionState
  }

  async writePanelSelection(selection: PanelSelectionState): Promise<void> {
    this.#db
      .prepare(
        `INSERT INTO panel_selections (
          session_id, panel_id, asset_ref, selected_shape_ids_json,
          selection_json, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(session_id, panel_id) DO UPDATE SET
          asset_ref = excluded.asset_ref,
          selected_shape_ids_json = excluded.selected_shape_ids_json,
          selection_json = excluded.selection_json,
          updated_at = excluded.updated_at`
      )
      .run(
        selection.sessionId,
        selection.panelId,
        selection.assetRef ?? null,
        JSON.stringify(selection.selectedShapeIds),
        JSON.stringify(selection),
        selection.updatedAt
      )
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

  close(): void {
    this.#db.close()
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

function openDatabase(databasePath: string): DatabaseSync {
  const { DatabaseSync } =
    require("node:sqlite") as typeof import("node:sqlite")
  const db = new DatabaseSync(databasePath)
  db.exec("PRAGMA journal_mode = WAL")
  db.exec("PRAGMA foreign_keys = ON")
  db.exec("PRAGMA busy_timeout = 5000")
  return db
}

function migrate(db: DatabaseSync): void {
  db.exec(`
    CREATE TABLE IF NOT EXISTS schema_migrations (
      id TEXT PRIMARY KEY NOT NULL,
      description TEXT NOT NULL,
      checksum TEXT NOT NULL,
      applied_at TEXT NOT NULL
    )
  `)
  const appliedRows = db
    .prepare("SELECT id, checksum FROM schema_migrations")
    .all() as SQLiteRow[]
  const applied = new Map(
    appliedRows.map((row) => [
      stringColumn(row, "id"),
      stringColumn(row, "checksum"),
    ])
  )

  for (const migration of migrations) {
    const checksum = migrationChecksum(migration)
    const appliedChecksum = applied.get(migration.id)
    if (appliedChecksum) {
      const acceptedChecksums = new Set([
        checksum,
        ...(ACCEPTED_MIGRATION_CHECKSUMS[migration.id] ?? []),
      ])
      if (!acceptedChecksums.has(appliedChecksum)) {
        throw new Error(`SQLite migration checksum mismatch: ${migration.id}`)
      }
      continue
    }
    runInTransaction(db, () => {
      migration.up(db)
      db.prepare(
        `INSERT INTO schema_migrations (id, description, checksum, applied_at)
          VALUES (?, ?, ?, ?)`
      ).run(
        migration.id,
        migration.description,
        checksum,
        new Date().toISOString()
      )
    })
  }
}

function runInTransaction(db: DatabaseSync, callback: () => void): void {
  db.exec("BEGIN")
  try {
    callback()
    db.exec("COMMIT")
  } catch (error) {
    db.exec("ROLLBACK")
    throw error
  }
}

function parseJsonColumn(row: SQLiteRow, column: string): unknown {
  return JSON.parse(stringColumn(row, column))
}

function stringColumn(row: SQLiteRow, column: string): string {
  const value = row[column]
  if (typeof value !== "string") {
    throw new Error(`Expected SQLite column ${column} to be a string`)
  }
  return value
}

function extractSchemaVersion(state: unknown): number | null {
  if (!isRecord(state)) return null
  if (typeof state.schemaVersion === "number") return state.schemaVersion
  const schema = state.schema
  if (isRecord(schema) && typeof schema.schemaVersion === "number") {
    return schema.schemaVersion
  }
  return null
}

function syncWikiTasks(
  db: DatabaseSync,
  sessionId: string,
  panelId: string,
  state: unknown
): void {
  if (!(isRecord(state) && Array.isArray(state.tasks))) return
  db.prepare(
    "DELETE FROM wiki_tasks WHERE session_id = ? AND panel_id = ?"
  ).run(sessionId, panelId)
  const insert = db.prepare(
    `INSERT INTO wiki_tasks (
      id, session_id, panel_id, type, status, target_id, document_id,
      wiki_space_id, markdown_version, claimed_by_process_id, created_at,
      updated_at, task_json
    )
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`
  )
  for (const task of state.tasks) {
    if (!isRecord(task)) continue
    const id = optionalString(task.id)
    const type = optionalString(task.type)
    const status = optionalString(task.status)
    const targetId = optionalString(task.targetId)
    const createdAt = optionalString(task.createdAt)
    const updatedAt = optionalString(task.updatedAt)
    if (!(id && type && status && targetId && createdAt && updatedAt)) {
      continue
    }
    insert.run(
      id,
      sessionId,
      panelId,
      type,
      status,
      targetId,
      nullableString(task.documentId),
      nullableString(task.wikiSpaceId),
      nullableInteger(task.markdownVersion),
      nullableString(task.claimedByProcessId),
      createdAt,
      updatedAt,
      JSON.stringify(task)
    )
  }
}

function optionalString(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null
}

function nullableString(value: unknown): string | null {
  return typeof value === "string" ? value : null
}

function nullableInteger(value: unknown): number | null {
  return Number.isInteger(value) ? (value as number) : null
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null
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

function assertSafeRoot(rootDir: string): void {
  if (basename(rootDir) !== ".myopenpanels") {
    throw new Error("OpenPanels local storage root must be named .myopenpanels")
  }
}

function assertInside(parent: string, child: string): void {
  const rel = relative(parent, child)
  if (rel !== "" && (rel.startsWith("..") || rel.includes(`..${sep}`))) {
    throw new Error(`Path escapes OpenPanels root: ${child}`)
  }
}
