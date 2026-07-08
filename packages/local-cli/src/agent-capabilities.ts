export type AgentCapabilityArgType =
  | "boolean"
  | "enum"
  | "integer"
  | "path"
  | "string"

export interface AgentCapabilityArg {
  default?: string
  description: string
  name: string
  required: boolean
  type: AgentCapabilityArgType
  values?: string[]
}

export interface AgentCapability {
  args: AgentCapabilityArg[]
  command: string
  description: string
  intent: string
  output: string
  relatedGuides?: string[]
}

const PROJECT_ARG: AgentCapabilityArg = {
  default: "$PWD",
  description: "Project directory.",
  name: "project",
  required: false,
  type: "path",
}

const FORMAT_JSON_ARG: AgentCapabilityArg = {
  default: "json",
  description: "Emit stable JSON output for command results.",
  name: "format",
  required: false,
  type: "enum",
  values: ["json"],
}

const FORMAT_MARKDOWN_ARG: AgentCapabilityArg = {
  default: "markdown",
  description: "Emit agent-readable markdown.",
  name: "format",
  required: false,
  type: "enum",
  values: ["markdown"],
}

export const AGENT_CAPABILITIES: AgentCapability[] = [
  {
    intent: "agent.context.read",
    description:
      "Read the compact OpenPanels agent context, current state, capabilities, and available guides.",
    command: 'openpanels-local agent context --project "$PWD"',
    args: [PROJECT_ARG, FORMAT_MARKDOWN_ARG],
    output: "Markdown agent context.",
  },
  {
    intent: "agent.guides.list",
    description: "List built-in agent guides that can be loaded on demand.",
    command: 'openpanels-local agent guides --project "$PWD"',
    args: [PROJECT_ARG, FORMAT_MARKDOWN_ARG],
    output: "Markdown guide table.",
  },
  {
    intent: "agent.guide.read",
    description: "Read one full guide, optionally enriched with task context.",
    command:
      'openpanels-local agent guide <guide-id> --project "$PWD" --task-id <task-id>',
    args: [
      {
        description: "Guide id, for example wiki.index-document.",
        name: "guide-id",
        required: true,
        type: "string",
      },
      PROJECT_ARG,
      {
        description: "Wiki task id. Supported for task-aware guides.",
        name: "task-id",
        required: false,
        type: "string",
      },
      FORMAT_MARKDOWN_ARG,
    ],
    output: "Markdown guide body with dynamic context.",
  },
  {
    intent: "studio.start",
    description: "Start or reuse the local OpenPanels studio.",
    command: 'openpanels-local studio start --project "$PWD" --format json',
    args: [PROJECT_ARG, FORMAT_JSON_ARG],
    output: "Studio process and browser URL metadata.",
  },
  {
    intent: "studio.status",
    description: "Read local studio process status.",
    command: 'openpanels-local studio status --project "$PWD" --format json',
    args: [PROJECT_ARG, FORMAT_JSON_ARG],
    output: "Studio status metadata.",
  },
  {
    intent: "studio.stop",
    description: "Stop the conversation-local studio process.",
    command: 'openpanels-local studio stop --project "$PWD" --format json',
    args: [PROJECT_ARG, FORMAT_JSON_ARG],
    output: "Stop confirmation.",
  },
  {
    intent: "panel.list",
    description: "List panels in the current OpenPanels project.",
    command: 'openpanels-local panels --project "$PWD" --format json',
    args: [PROJECT_ARG, FORMAT_JSON_ARG],
    output: "Panel list and active panel metadata.",
  },
  {
    intent: "panel.switch",
    description: "Switch the active panel by kind.",
    command:
      'openpanels-local active-panel --project "$PWD" --kind <kind> --format json',
    args: [
      PROJECT_ARG,
      {
        description: "Panel kind.",
        name: "kind",
        required: true,
        type: "enum",
        values: ["wiki", "canvas", "image", "diff", "preview", "files"],
      },
      FORMAT_JSON_ARG,
    ],
    output: "Active panel metadata.",
  },
  {
    intent: "panel.state.read",
    description: "Read panel state by kind.",
    command:
      'openpanels-local panel-state --project "$PWD" --kind <kind> --format json',
    args: [
      PROJECT_ARG,
      {
        description: "Panel kind.",
        name: "kind",
        required: true,
        type: "enum",
        values: ["wiki", "canvas", "image", "diff", "preview", "files"],
      },
      FORMAT_JSON_ARG,
    ],
    output: "Panel state payload.",
  },
  {
    intent: "canvas.state.read",
    description: "Read the current canvas project, panel, and state.",
    command: 'openpanels-local canvas-state --project "$PWD" --format json',
    args: [PROJECT_ARG, FORMAT_JSON_ARG],
    output: "Canvas bootstrap payload.",
    relatedGuides: ["canvas.selection-reference"],
  },
  {
    intent: "canvas.selection.read",
    description: "Read current canvas selection summary.",
    command: 'openpanels-local selection --project "$PWD" --format json',
    args: [PROJECT_ARG, FORMAT_JSON_ARG],
    output: "Canvas selection payload.",
    relatedGuides: ["canvas.selection-reference"],
  },
  {
    intent: "canvas.selection.asset.read",
    description:
      "Write selected canvas pixels or fallback image asset to a file.",
    command:
      'openpanels-local read-selection-asset --project "$PWD" --output <path> --format json',
    args: [
      PROJECT_ARG,
      {
        description: "Output image path.",
        name: "output",
        required: true,
        type: "path",
      },
      FORMAT_JSON_ARG,
    ],
    output: "Written asset path and metadata.",
    relatedGuides: ["canvas.selection-reference", "canvas.image-generation"],
  },
  {
    intent: "canvas.placeholder.create",
    description: "Create a generation placeholder on the canvas.",
    command:
      'openpanels-local insert-placeholder --project "$PWD" --display-width <w> --display-height <h> --format json',
    args: [
      PROJECT_ARG,
      {
        description: "Displayed placeholder width in canvas units.",
        name: "display-width",
        required: false,
        type: "integer",
      },
      {
        description: "Displayed placeholder height in canvas units.",
        name: "display-height",
        required: false,
        type: "integer",
      },
      {
        description: "Selected or reference shape id to place near.",
        name: "anchor-shape-id",
        required: false,
        type: "string",
      },
      FORMAT_JSON_ARG,
    ],
    output: "Placeholder shape id and placement metadata.",
    relatedGuides: ["canvas.image-generation"],
  },
  {
    intent: "canvas.image.insert",
    description: "Insert or replace a local image in the canvas.",
    command:
      'openpanels-local insert-image --project "$PWD" --image <path> --placement right --format json',
    args: [
      PROJECT_ARG,
      {
        description: "Local image path.",
        name: "image",
        required: true,
        type: "path",
      },
      {
        description: "Placement relative to selection or canvas content.",
        name: "placement",
        required: false,
        type: "enum",
        values: ["right", "left", "below"],
      },
      {
        description: "Shape id to replace, commonly a placeholder id.",
        name: "replace-shape-id",
        required: false,
        type: "string",
      },
      FORMAT_JSON_ARG,
    ],
    output: "Inserted image shape id and metadata.",
    relatedGuides: ["canvas.image-generation"],
  },
  {
    intent: "wiki.context.read",
    description: "Read compact agent context with wiki prioritized.",
    command: 'openpanels-local wiki context --project "$PWD"',
    args: [PROJECT_ARG, FORMAT_MARKDOWN_ARG],
    output: "Markdown agent context.",
    relatedGuides: ["wiki.task-intake"],
  },
  {
    intent: "wiki.task.list",
    description: "List wiki tasks.",
    command: 'openpanels-local wiki tasks list --project "$PWD" --format json',
    args: [
      PROJECT_ARG,
      {
        description: "Filter by task status.",
        name: "status",
        required: false,
        type: "enum",
        values: [
          "queued",
          "claimed",
          "running",
          "failed",
          "succeeded",
          "stale",
        ],
      },
      FORMAT_JSON_ARG,
    ],
    output: "Wiki task list.",
    relatedGuides: ["wiki.task-intake"],
  },
  {
    intent: "wiki.task.next",
    description: "Read the next queued or failed wiki task.",
    command: 'openpanels-local wiki tasks next --project "$PWD" --format json',
    args: [PROJECT_ARG, FORMAT_JSON_ARG],
    output: "Next wiki task or null.",
    relatedGuides: ["wiki.task-intake", "wiki.index-document"],
  },
  {
    intent: "wiki.task.claim",
    description: "Claim a wiki task before working on it.",
    command:
      'openpanels-local wiki tasks claim --project "$PWD" --task-id <task-id> --format json',
    args: [
      PROJECT_ARG,
      {
        description: "Wiki task id.",
        name: "task-id",
        required: true,
        type: "string",
      },
      FORMAT_JSON_ARG,
    ],
    output: "Claimed task and process context.",
    relatedGuides: ["wiki.task-intake"],
  },
  {
    intent: "wiki.task.complete",
    description: "Mark a wiki task complete.",
    command:
      'openpanels-local wiki tasks complete --project "$PWD" --task-id <task-id> --format json',
    args: [
      PROJECT_ARG,
      {
        description: "Wiki task id.",
        name: "task-id",
        required: true,
        type: "string",
      },
      FORMAT_JSON_ARG,
    ],
    output: "Completed task payload.",
  },
  {
    intent: "wiki.task.fail",
    description: "Mark a wiki task failed with a message.",
    command:
      'openpanels-local wiki tasks fail --project "$PWD" --task-id <task-id> --message <message> --format json',
    args: [
      PROJECT_ARG,
      {
        description: "Wiki task id.",
        name: "task-id",
        required: true,
        type: "string",
      },
      {
        description: "Failure message.",
        name: "message",
        required: true,
        type: "string",
      },
      FORMAT_JSON_ARG,
    ],
    output: "Failed task payload.",
  },
  {
    intent: "wiki.raw.add",
    description: "Add a raw source document to the wiki panel.",
    command:
      'openpanels-local wiki raw add --project "$PWD" --file <path> --format json',
    args: [
      PROJECT_ARG,
      {
        description: "Source file path.",
        name: "file",
        required: true,
        type: "path",
      },
      FORMAT_JSON_ARG,
    ],
    output: "Raw document metadata and queued tasks.",
    relatedGuides: ["wiki.convert-document", "wiki.index-document"],
  },
  {
    intent: "wiki.source.read",
    description: "Read markdown for a raw wiki document.",
    command:
      'openpanels-local wiki markdown read --project "$PWD" --document-id <document-id> --format json',
    args: [
      PROJECT_ARG,
      {
        description: "Raw document id.",
        name: "document-id",
        required: true,
        type: "string",
      },
      FORMAT_JSON_ARG,
    ],
    output: "Raw document markdown.",
    relatedGuides: ["wiki.index-document"],
  },
  {
    intent: "wiki.source.write",
    description: "Write markdown for a raw wiki document.",
    command:
      'openpanels-local wiki markdown write --project "$PWD" --document-id <document-id> --file <path> --task-id <task-id> --format json',
    args: [
      PROJECT_ARG,
      {
        description: "Raw document id.",
        name: "document-id",
        required: true,
        type: "string",
      },
      {
        description: "Markdown file path.",
        name: "file",
        required: true,
        type: "path",
      },
      {
        description: "Related wiki task id.",
        name: "task-id",
        required: false,
        type: "string",
      },
      FORMAT_JSON_ARG,
    ],
    output: "Updated raw document metadata and queued tasks.",
    relatedGuides: ["wiki.convert-document"],
  },
  {
    intent: "wiki.page.list",
    description: "List pages in a wiki space.",
    command:
      'openpanels-local wiki pages list --project "$PWD" --wiki-space-id <wiki-space-id> --format json',
    args: [
      PROJECT_ARG,
      {
        description: "Wiki space id.",
        name: "wiki-space-id",
        required: true,
        type: "string",
      },
      FORMAT_JSON_ARG,
    ],
    output: "Wiki page index items.",
    relatedGuides: ["wiki.index-document", "wiki.rebuild-index"],
  },
  {
    intent: "wiki.page.read",
    description: "Read one wiki page.",
    command:
      'openpanels-local wiki pages read --project "$PWD" --wiki-space-id <wiki-space-id> --path <page-path> --format json',
    args: [
      PROJECT_ARG,
      {
        description: "Wiki space id.",
        name: "wiki-space-id",
        required: true,
        type: "string",
      },
      {
        description: "Page path.",
        name: "path",
        required: true,
        type: "string",
      },
      FORMAT_JSON_ARG,
    ],
    output: "Wiki page markdown.",
    relatedGuides: ["wiki.index-document"],
  },
  {
    intent: "wiki.page.write",
    description: "Create or update one wiki page from a markdown file.",
    command:
      'openpanels-local wiki pages write --project "$PWD" --wiki-space-id <wiki-space-id> --path <page-path> --file <md-file> --task-id <task-id> --format json',
    args: [
      PROJECT_ARG,
      {
        description: "Wiki space id.",
        name: "wiki-space-id",
        required: true,
        type: "string",
      },
      {
        description: "Page path.",
        name: "path",
        required: true,
        type: "string",
      },
      {
        description: "Markdown file path.",
        name: "file",
        required: true,
        type: "path",
      },
      {
        description: "Related wiki task id.",
        name: "task-id",
        required: false,
        type: "string",
      },
      FORMAT_JSON_ARG,
    ],
    output: "Written page metadata and queued tasks.",
    relatedGuides: ["wiki.index-document", "wiki.rebuild-index"],
  },
  {
    intent: "wiki.space.list",
    description: "List wiki spaces.",
    command: 'openpanels-local wiki spaces list --project "$PWD" --format json',
    args: [PROJECT_ARG, FORMAT_JSON_ARG],
    output: "Wiki spaces.",
  },
  {
    intent: "wiki.space.switch",
    description: "Switch the active wiki space.",
    command:
      'openpanels-local wiki spaces active --project "$PWD" --wiki-space-id <wiki-space-id> --format json',
    args: [
      PROJECT_ARG,
      {
        description: "Wiki space id.",
        name: "wiki-space-id",
        required: true,
        type: "string",
      },
      FORMAT_JSON_ARG,
    ],
    output: "Updated wiki state.",
  },
]

export function getAgentCapability(intent: string) {
  return AGENT_CAPABILITIES.find((capability) => capability.intent === intent)
}
