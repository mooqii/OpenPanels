import type {
  ProjectBootstrap,
  SelectionResult,
} from "@openpanels/local-control"
import { AGENT_CAPABILITIES } from "./agent-capabilities"
import type { AgentGuideMetadata } from "./agent-guides"

export interface AgentContextBuildOptions {
  cliVersion: string
  guides: AgentGuideMetadata[]
  selectionResult?: SelectionResult | null
}

interface WikiTaskSummary {
  documentId?: string | null
  id: string
  status: string
  type: string
  wikiSpaceId?: string | null
}

interface WikiStateSummary {
  language: string | null
  nextTask: WikiTaskSummary | null
  pendingTaskCount: number
}

interface CanvasStateSummary {
  fallback: string | null
  hasSelectedImageAsset: boolean
  hasSelection: boolean
  selectedShapeCount: number
}

export function agentContextPayload(
  result: ProjectBootstrap,
  options: AgentContextBuildOptions
) {
  return {
    protocolVersion: 1,
    cliVersion: options.cliVersion,
    project: {
      id: result.session.id,
      title: result.session.title,
    },
    activePanel: {
      id: result.activePanelId,
      kind: result.activePanelKind,
      title: result.panel.title,
    },
    panels: result.panels.map(({ panel }) => ({
      id: panel.id,
      kind: panel.kind,
      title: panel.title,
    })),
    state: {
      wiki: wikiStateFromBootstrap(result),
      canvas: canvasStateFromSelection(options.selectionResult),
    },
    capabilities: AGENT_CAPABILITIES,
    suggestedCommands: suggestedCommands(result, options.guides),
    availableGuides: options.guides,
  }
}

export function agentContextMarkdown(
  result: ProjectBootstrap,
  options: AgentContextBuildOptions
) {
  const wikiState = wikiStateFromBootstrap(result)
  const canvasState = canvasStateFromSelection(options.selectionResult)
  const relatedGuideIds = new Set(options.guides.map((guide) => guide.id))

  return `# OpenPanels Agent Context

Protocol version: 1
CLI version: ${options.cliVersion}
Project: ${result.session.title} (${result.session.id})
Active panel: ${result.activePanelKind} (${result.panel.title})

## Panels

${result.panels
  .map(({ panel }) => {
    const marker = panel.id === result.activePanelId ? "*" : "-"
    return `${marker} ${panel.kind}: ${panel.title} (${panel.id})`
  })
  .join("\n")}

## State

### Wiki

- language: ${wikiState.language ?? "not set"}
- pending task count: ${wikiState.pendingTaskCount}
- next task: ${formatNextTask(wikiState.nextTask)}

### Canvas

- has selection: ${canvasState.hasSelection}
- selected shape count: ${canvasState.selectedShapeCount}
- selected image asset: ${canvasState.hasSelectedImageAsset}
- fallback: ${canvasState.fallback ?? "none"}

## Capabilities

${AGENT_CAPABILITIES.map((capability) =>
  renderCapability(capability, relatedGuideIds)
).join("\n\n")}

## Suggested Next Commands

${suggestedCommands(result, options.guides)
  .map(
    (item) => `### \`${item.intent}\`

\`\`\`bash
${item.command}
\`\`\``
  )
  .join("\n\n")}

## Available Guides

${renderGuideTable(options.guides)}
`
}

function renderCapability(
  capability: (typeof AGENT_CAPABILITIES)[number],
  availableGuideIds: Set<string>
) {
  const relatedGuides = (capability.relatedGuides ?? []).filter((guideId) =>
    availableGuideIds.has(guideId)
  )
  return `### \`${capability.intent}\`

${capability.description}

Command:

\`\`\`bash
${capability.command}
\`\`\`

Arguments:

${renderArgsTable(capability.args)}

Output:

- ${capability.output}${
    relatedGuides.length
      ? `

Related guides:

${relatedGuides.map((guideId) => `- \`${guideId}\``).join("\n")}`
      : ""
  }`
}

function renderArgsTable(args: (typeof AGENT_CAPABILITIES)[number]["args"]) {
  if (!args.length) return "- none"
  return `| Name | Required | Type | Values | Description |
| --- | --- | --- | --- | --- |
${args
  .map(
    (arg) =>
      `| ${arg.name} | ${arg.required ? "yes" : "no"} | ${arg.type} | ${
        arg.values?.join(", ") ?? ""
      } | ${arg.description}${arg.default ? ` Default: ${arg.default}.` : ""} |`
  )
  .join("\n")}`
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

function suggestedCommands(
  result: ProjectBootstrap,
  guides: AgentGuideMetadata[]
): Array<{ command: string; intent: string }> {
  const commands: Array<{ command: string; intent: string }> = [
    {
      intent: "agent.context.read",
      command: 'openpanels-local agent context --project "$PWD"',
    },
  ]
  const wikiState = wikiStateFromBootstrap(result)
  if (wikiState.nextTask) {
    commands.push({
      intent: "wiki.task.next",
      command:
        'openpanels-local wiki tasks next --project "$PWD" --format json',
    })
    const guide = guideForTask(wikiState.nextTask, guides)
    if (guide) {
      commands.push({
        intent: "agent.guide.read",
        command: `openpanels-local agent guide ${guide.id} --project "$PWD" --task-id ${wikiState.nextTask.id}`,
      })
    }
    return commands
  }
  if (result.activePanelKind === "canvas") {
    commands.push({
      intent: "canvas.selection.read",
      command: 'openpanels-local selection --project "$PWD" --format json',
    })
  }
  return commands
}

function guideForTask(
  task: WikiTaskSummary,
  guides: AgentGuideMetadata[]
): AgentGuideMetadata | null {
  return guides.find((guide) => guide.taskTypes.includes(task.type)) ?? null
}

function wikiStateFromBootstrap(result: ProjectBootstrap): WikiStateSummary {
  const wikiSnapshot =
    result.panels.find(({ panel }) => panel.kind === "wiki") ??
    (result.activePanelKind === "wiki"
      ? { panel: result.panel, state: result.state }
      : null)
  const state =
    wikiSnapshot && typeof wikiSnapshot.state === "object"
      ? (wikiSnapshot.state as {
          tasks?: unknown
          wikiLanguage?: unknown
        })
      : null
  const tasks = Array.isArray(state?.tasks)
    ? state.tasks
        .filter(isWikiTaskSummary)
        .filter((task) =>
          ["queued", "claimed", "running", "failed"].includes(task.status)
        )
    : []
  return {
    language:
      state?.wikiLanguage === "en" || state?.wikiLanguage === "zh-CN"
        ? state.wikiLanguage
        : null,
    nextTask:
      tasks.find((task) => task.status === "queued") ??
      tasks.find((task) => task.status === "failed") ??
      tasks[0] ??
      null,
    pendingTaskCount: tasks.length,
  }
}

function canvasStateFromSelection(
  result: SelectionResult | null | undefined
): CanvasStateSummary {
  const selection = result?.selection
  const selectedShapes = Array.isArray(selection?.selectedShapes)
    ? selection.selectedShapes
    : []
  const selectedShapeIds = Array.isArray(selection?.selectedShapeIds)
    ? selection.selectedShapeIds
    : []
  return {
    fallback:
      typeof selection?.fallback === "string" ? selection.fallback : null,
    hasSelectedImageAsset: selectedShapes.some(
      (shape) =>
        typeof shape === "object" &&
        shape !== null &&
        "assetRef" in shape &&
        typeof (shape as { assetRef?: unknown }).assetRef === "string"
    ),
    hasSelection: selectedShapeIds.length > 0 || selectedShapes.length > 0,
    selectedShapeCount: selectedShapes.length || selectedShapeIds.length,
  }
}

function formatNextTask(task: WikiTaskSummary | null) {
  if (!task) return "none"
  return `${task.type} / ${task.status} / ${task.id}`
}

function isWikiTaskSummary(value: unknown): value is WikiTaskSummary {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { id?: unknown }).id === "string" &&
    typeof (value as { status?: unknown }).status === "string" &&
    typeof (value as { type?: unknown }).type === "string"
  )
}
