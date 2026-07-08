import { existsSync } from "node:fs"
import { readdir, readFile } from "node:fs/promises"
import { join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import {
  getProjectBootstrap,
  getSelection,
  type ProjectBootstrap,
  type SelectionResult,
} from "@openpanels/local-control"

export interface AgentGuideMetadata {
  appliesTo: string[]
  id: string
  loadWhen: string[]
  requiresCapabilities: string[]
  source: "builtin" | "project" | "user"
  taskTypes: string[]
  title: string
  tokens: "short" | "medium" | "long"
}

export interface AgentGuide extends AgentGuideMetadata {
  body: string
}

export interface AgentGuideOptions {
  contextId?: string
  projectDir?: string
  storageDir?: string
  taskId?: string
}

interface WikiTaskLike {
  documentId?: string | null
  id: string
  markdownVersion?: number | null
  status: string
  type: string
  wikiSpaceId?: string | null
}

export async function listAgentGuides(): Promise<AgentGuideMetadata[]> {
  const guides = await loadAgentGuides()
  return guides.map(({ body: _body, ...metadata }) => metadata)
}

export async function readAgentGuide(
  guideId: string,
  options: AgentGuideOptions
) {
  const guide = (await loadAgentGuides()).find((item) => item.id === guideId)
  if (!guide) {
    throw new Error(`OpenPanels agent guide not found: ${guideId}`)
  }
  const bootstrap = await getProjectBootstrap({
    contextId: options.contextId,
    projectDir: options.projectDir,
    storageDir: options.storageDir,
  })
  const selection = await safeGetSelection(options)
  return {
    guide,
    markdown: renderAgentGuide(guide, {
      bootstrap,
      selection,
      taskId: options.taskId,
    }),
  }
}

export function renderAgentGuidesMarkdown(guides: AgentGuideMetadata[]) {
  return `# OpenPanels Agent Guides

${renderGuideTable(guides)}
`
}

export function renderAgentGuide(
  guide: AgentGuide,
  context: {
    bootstrap: ProjectBootstrap
    selection: SelectionResult | null
    taskId?: string
  }
) {
  const task = context.taskId
    ? findWikiTask(context.bootstrap, context.taskId)
    : null
  return `# Guide: ${guide.id}

Title: ${guide.title}
Source: ${guide.source}
Applies to: ${guide.appliesTo.join(", ") || "any"}

## Current Context

${renderCurrentContext(context.bootstrap, context.selection, task)}

## Commands For This Guide

${renderGuideCommands(guide, task)}

## Instructions

${guide.body.trim()}
`
}

async function loadAgentGuides(): Promise<AgentGuide[]> {
  const guidesDir = resolveAgentGuidesDir()
  const entries = await readdir(guidesDir)
  const guides = await Promise.all(
    entries
      .filter((entry) => entry.endsWith(".md"))
      .sort()
      .map(async (entry) =>
        parseGuide(await readFile(join(guidesDir, entry), "utf8"), entry)
      )
  )
  return guides
}

function resolveAgentGuidesDir() {
  const candidates = [
    process.env.OPENPANELS_AGENT_GUIDES_DIR,
    fileURLToPath(new URL("./agent-guides", import.meta.url)),
    fileURLToPath(new URL("../../../agent-guides", import.meta.url)),
  ].filter(Boolean) as string[]
  const found = candidates.find((candidate) => existsSync(candidate))
  if (!found) {
    throw new Error(
      `OpenPanels agent guides directory not found. Checked: ${candidates.join(", ")}`
    )
  }
  return resolve(found)
}

function parseGuide(source: string, fileName: string): AgentGuide {
  const match = source.match(/^---\n([\s\S]*?)\n---\n?([\s\S]*)$/)
  if (!match) {
    throw new Error(`Agent guide is missing frontmatter: ${fileName}`)
  }
  const frontmatter = parseFrontmatter(match[1])
  const id = scalar(frontmatter, "id")
  const title = scalar(frontmatter, "title")
  if (!(id && title)) {
    throw new Error(`Agent guide requires id and title: ${fileName}`)
  }
  const sourceValue = scalar(frontmatter, "source") ?? "builtin"
  if (
    sourceValue !== "builtin" &&
    sourceValue !== "project" &&
    sourceValue !== "user"
  ) {
    throw new Error(`Agent guide has invalid source: ${fileName}`)
  }
  const tokens = scalar(frontmatter, "tokens") ?? "medium"
  if (tokens !== "short" && tokens !== "medium" && tokens !== "long") {
    throw new Error(`Agent guide has invalid tokens value: ${fileName}`)
  }
  return {
    appliesTo: list(frontmatter, "appliesTo"),
    body: match[2],
    id,
    loadWhen: list(frontmatter, "loadWhen"),
    requiresCapabilities: list(frontmatter, "requiresCapabilities"),
    source: sourceValue,
    taskTypes: list(frontmatter, "taskTypes"),
    title,
    tokens,
  }
}

function parseFrontmatter(source: string): Map<string, string | string[]> {
  const result = new Map<string, string | string[]>()
  let currentListKey: string | null = null
  for (const line of source.split("\n")) {
    const listItem = line.match(/^\s*-\s+(.*)$/)
    if (listItem && currentListKey) {
      const current = result.get(currentListKey)
      const listValue = Array.isArray(current) ? current : []
      listValue.push(listItem[1].trim())
      result.set(currentListKey, listValue)
      continue
    }
    const keyValue = line.match(/^([A-Za-z0-9_-]+):\s*(.*)$/)
    if (!keyValue) continue
    const [, key, rawValue] = keyValue
    const value = rawValue.trim()
    if (value) {
      result.set(key, value)
      currentListKey = null
    } else {
      result.set(key, [])
      currentListKey = key
    }
  }
  return result
}

function scalar(frontmatter: Map<string, string | string[]>, key: string) {
  const value = frontmatter.get(key)
  return typeof value === "string" ? value : null
}

function list(frontmatter: Map<string, string | string[]>, key: string) {
  const value = frontmatter.get(key)
  if (Array.isArray(value)) return value
  if (typeof value === "string" && value.length > 0) return [value]
  return []
}

async function safeGetSelection(options: AgentGuideOptions) {
  try {
    return await getSelection({
      contextId: options.contextId,
      projectDir: options.projectDir,
      storageDir: options.storageDir,
    })
  } catch (_error) {
    return null
  }
}

function renderGuideTable(guides: AgentGuideMetadata[]) {
  if (!guides.length) return "- none"
  return `| ID | Source | Applies To | Task Types | Load When |
| --- | --- | --- | --- | --- |
${guides
  .map(
    (guide) =>
      `| \`${guide.id}\` | ${guide.source} | ${guide.appliesTo.join(", ")} | ${guide.taskTypes.join(", ")} | ${guide.loadWhen.join("; ")} |`
  )
  .join("\n")}`
}

function renderCurrentContext(
  bootstrap: ProjectBootstrap,
  selection: SelectionResult | null,
  task: WikiTaskLike | null
) {
  const wikiState = wikiStateFromBootstrap(bootstrap)
  const selectedShapeCount = Array.isArray(selection?.selection?.selectedShapes)
    ? selection.selection.selectedShapes.length
    : 0
  const lines = [
    `- project: ${bootstrap.session.title} (${bootstrap.session.id})`,
    `- active panel: ${bootstrap.activePanelKind} (${bootstrap.panel.title})`,
    `- wiki language: ${wikiState.language ?? "not set"}`,
    `- canvas selected shape count: ${selectedShapeCount}`,
  ]
  if (task) {
    lines.push(
      `- task id: ${task.id}`,
      `- task type: ${task.type}`,
      `- task status: ${task.status}`,
      `- document id: ${task.documentId ?? "none"}`,
      `- wiki space id: ${task.wikiSpaceId ?? wikiState.activeWikiSpaceId ?? "none"}`
    )
  }
  return lines.join("\n")
}

function renderGuideCommands(guide: AgentGuide, task: WikiTaskLike | null) {
  if (task) {
    const wikiSpaceId = task.wikiSpaceId ?? "<wiki-space-id>"
    const documentId = task.documentId ?? "<document-id>"
    return `\`\`\`bash
openpanels-local wiki tasks claim --project "$PWD" --task-id ${task.id} --format json
openpanels-local wiki markdown read --project "$PWD" --document-id ${documentId} --format json
openpanels-local wiki pages write --project "$PWD" --wiki-space-id ${wikiSpaceId} --path <page-path> --file <md-file> --task-id ${task.id} --format json
openpanels-local wiki tasks complete --project "$PWD" --task-id ${task.id} --format json
\`\`\``
  }
  if (guide.appliesTo.includes("canvas")) {
    return `\`\`\`bash
openpanels-local selection --project "$PWD" --format json
openpanels-local insert-placeholder --project "$PWD" --display-width <w> --display-height <h> --format json
openpanels-local insert-image --project "$PWD" --image <generated-path> --replace-shape-id <placeholder-shape-id> --format json
\`\`\``
  }
  return "- No task-specific commands."
}

function findWikiTask(
  bootstrap: ProjectBootstrap,
  taskId: string
): WikiTaskLike {
  const wikiSnapshot =
    bootstrap.panels.find(({ panel }) => panel.kind === "wiki") ??
    (bootstrap.activePanelKind === "wiki"
      ? { panel: bootstrap.panel, state: bootstrap.state }
      : null)
  const state =
    wikiSnapshot && typeof wikiSnapshot.state === "object"
      ? (wikiSnapshot.state as { tasks?: unknown })
      : null
  const tasks = Array.isArray(state?.tasks) ? state.tasks : []
  const task = tasks.find(
    (item): item is WikiTaskLike =>
      typeof item === "object" &&
      item !== null &&
      typeof (item as { id?: unknown }).id === "string" &&
      (item as { id: string }).id === taskId &&
      typeof (item as { type?: unknown }).type === "string" &&
      typeof (item as { status?: unknown }).status === "string"
  )
  if (!task) throw new Error(`Wiki task not found: ${taskId}`)
  return task
}

function wikiStateFromBootstrap(bootstrap: ProjectBootstrap): {
  activeWikiSpaceId: string | null
  language: string | null
} {
  const wikiSnapshot =
    bootstrap.panels.find(({ panel }) => panel.kind === "wiki") ??
    (bootstrap.activePanelKind === "wiki"
      ? { panel: bootstrap.panel, state: bootstrap.state }
      : null)
  const state =
    wikiSnapshot && typeof wikiSnapshot.state === "object"
      ? (wikiSnapshot.state as {
          activeWikiSpaceId?: unknown
          wikiLanguage?: unknown
        })
      : null
  return {
    activeWikiSpaceId:
      typeof state?.activeWikiSpaceId === "string"
        ? state.activeWikiSpaceId
        : null,
    language:
      state?.wikiLanguage === "en" || state?.wikiLanguage === "zh-CN"
        ? state.wikiLanguage
        : null,
  }
}
