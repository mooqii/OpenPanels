import { Button, Chip, Tooltip } from "@heroui/react"
import {
  Ban,
  CheckCircle2,
  CircleAlert,
  Clock3,
  RefreshCw,
  Sparkles,
} from "lucide-react"
import { useMyOpenPanelsI18n } from "../../canvas"
import type { ProjectTask, WikiRawDocument } from "../../types"

export type WikiTaskListFilter = "active" | "done" | "pending"

export function conversionStatusTaskFilter(
  status: WikiRawDocument["conversion"]["status"]
): WikiTaskListFilter {
  if (status === "converting") return "active"
  return status === "cancelled" || status === "ready" ? "done" : "pending"
}

export function indexStatusTaskFilter(status: {
  kind: ReturnType<typeof documentIndexStatus>["kind"]
  label?: string
  taskId?: string | null
}): WikiTaskListFilter {
  if (status.kind === "running") return "active"
  return ["cancelled", "covered", "done", "filtered", "unrecorded"].includes(
    status.kind
  )
    ? "done"
    : "pending"
}

export type WikiTaskStatusKind =
  | "cancelled"
  | "covered"
  | "done"
  | "failed"
  | "filtered"
  | "pending"
  | "running"
  | "unrecorded"
  | "unscheduled"

export function WikiTaskStatusIcon({
  doneIcon = "check",
  kind,
  filter,
  label,
  onOpenTasks,
  taskId,
}: {
  doneIcon?: "check" | "sparkles"
  kind: WikiTaskStatusKind
  filter: WikiTaskListFilter
  label: string
  onOpenTasks: (filter: WikiTaskListFilter, taskIds?: string[]) => void
  taskId: string | null | undefined
}) {
  const { t } = useMyOpenPanelsI18n()
  if (!taskId) return null
  const icon =
    kind === "done" ? (
      doneIcon === "sparkles" ? (
        <Sparkles size={12} />
      ) : (
        <CheckCircle2 size={12} />
      )
    ) : kind === "failed" ? (
      <CircleAlert size={12} />
    ) : kind === "cancelled" ? (
      <Ban size={12} />
    ) : kind === "running" ? (
      <RefreshCw className="op-wiki-spin" size={12} />
    ) : (
      <Clock3 size={12} />
    )
  return (
    <Tooltip closeDelay={0} delay={0}>
      <Button
        aria-label={`${label}. ${t`View related tasks`}`}
        className="op-wiki-task-status"
        data-status={kind}
        isIconOnly
        onPress={() => onOpenTasks(filter, [taskId])}
        size="sm"
        variant="ghost"
      >
        {icon}
      </Button>
      <Tooltip.Content placement="top" shouldFlip={false}>
        {label}
      </Tooltip.Content>
    </Tooltip>
  )
}

function WikiTaskStatusLabel({
  color,
  filter,
  label,
  onOpenTasks,
  taskId,
}: {
  color?: "accent" | "danger" | "success" | "warning"
  filter: WikiTaskListFilter
  label: string
  onOpenTasks: (filter: WikiTaskListFilter, taskIds?: string[]) => void
  taskId: string | null | undefined
}) {
  const { t } = useMyOpenPanelsI18n()
  if (!taskId) {
    return (
      <Chip
        className="op-wiki-task-status-label__chip"
        color={color}
        size="sm"
        variant="soft"
      >
        {label}
      </Chip>
    )
  }
  return (
    <button
      aria-label={`${label}. ${t`View related tasks`}`}
      className="op-wiki-task-status-label"
      onClick={() => onOpenTasks(filter, taskId ? [taskId] : undefined)}
      title={t`View related tasks`}
      type="button"
    >
      <Chip
        className="op-wiki-task-status-label__chip"
        color={color}
        size="sm"
        variant="soft"
      >
        {label}
      </Chip>
    </button>
  )
}

export function WikiStatus({
  document,
  onOpenTasks,
}: {
  document: WikiRawDocument
  onOpenTasks: (filter: WikiTaskListFilter, taskIds?: string[]) => void
}) {
  const { t } = useMyOpenPanelsI18n()
  if (document.conversion.status === "cancelled") {
    return (
      <WikiTaskStatusIcon
        filter={conversionStatusTaskFilter(document.conversion.status)}
        kind="cancelled"
        label={t`Conversion cancelled`}
        onOpenTasks={onOpenTasks}
        taskId={document.conversion.taskId}
      />
    )
  }
  if (document.conversion.status === "failed") {
    return (
      <WikiTaskStatusIcon
        filter={conversionStatusTaskFilter(document.conversion.status)}
        kind="failed"
        label={t`Conversion failed`}
        onOpenTasks={onOpenTasks}
        taskId={document.conversion.taskId}
      />
    )
  }
  if (
    document.conversion.status === "queued" ||
    document.conversion.status === "converting"
  ) {
    return (
      <WikiTaskStatusIcon
        filter={conversionStatusTaskFilter(document.conversion.status)}
        kind={
          document.conversion.status === "converting" ? "running" : "pending"
        }
        label={
          document.conversion.status === "queued"
            ? t`Pending conversion`
            : t`Converting`
        }
        onOpenTasks={onOpenTasks}
        taskId={document.conversion.taskId}
      />
    )
  }
  return null
}

export function WikiIndexStatus({
  onOpenTasks,
  status,
}: {
  onOpenTasks: (filter: WikiTaskListFilter, taskIds?: string[]) => void
  status: ReturnType<typeof documentIndexStatus>
}) {
  const { t } = useMyOpenPanelsI18n()
  const color =
    status.kind === "done"
      ? "success"
      : status.kind === "failed"
        ? "danger"
        : status.kind === "running"
          ? "accent"
          : status.kind === "pending" ||
              status.kind === "unrecorded" ||
              status.kind === "unscheduled"
            ? "warning"
            : undefined
  return (
    <WikiTaskStatusLabel
      color={color}
      filter={indexStatusTaskFilter(status)}
      label={t(status.label)}
      onOpenTasks={onOpenTasks}
      taskId={status.taskId}
    />
  )
}

export function formatWikiPageType(
  type: string,
  t: (input: TemplateStringsArray | string, ...values: unknown[]) => string
) {
  switch (type) {
    case "overview":
      return t`Overview`
    case "log":
      return t`Log`
    case "source":
      return t`Source`
    case "topic":
      return t`Topic`
    case "entity":
      return t`Entity`
    case "category":
      return t`Category`
    default:
      return type.replaceAll("_", " ") || t`Page`
  }
}

export function formatWikiTaskType(
  type: string,
  t: (input: TemplateStringsArray | string, ...values: unknown[]) => string
) {
  switch (type) {
    case "convert_document_to_markdown":
      return t`Convert to Markdown`
    case "ingest_markdown_into_wiki":
      return t`Update wiki`
    case "maintain_wiki":
      return t`Maintain wiki`
    case "lint_wiki":
      return t`Check wiki`
    default:
      return type.replaceAll("_", " ")
  }
}

export function formatWikiTaskStatus(
  status: ProjectTask["status"],
  t: (input: TemplateStringsArray | string, ...values: unknown[]) => string
) {
  switch (status) {
    case "queued":
      return t`Queued`
    case "running":
      return t`Running`
    case "failed":
      return t`Failed`
    case "succeeded":
      return t`Succeeded`
    case "cancelled":
      return t`Cancelled`
    case "superseded":
      return t`Superseded`
    default:
      return status
  }
}

export function documentIndexStatus(
  document: WikiRawDocument,
  wikiSpaceId: string | null | undefined
): {
  kind:
    | "cancelled"
    | "covered"
    | "done"
    | "failed"
    | "filtered"
    | "pending"
    | "running"
    | "unrecorded"
    | "unscheduled"
  label: string
  taskId: string | null
} {
  const ingestion = wikiSpaceId
    ? document.ingestionByWikiSpace[wikiSpaceId]
    : undefined
  if (ingestion?.status === "ingested") {
    return { kind: "done", label: "Indexed", taskId: ingestion.taskId }
  }
  if (ingestion?.status === "covered") {
    return { kind: "covered", label: "Covered", taskId: ingestion.taskId }
  }
  if (ingestion?.status === "filtered") {
    return { kind: "filtered", label: "Filtered", taskId: ingestion.taskId }
  }
  if (ingestion?.status === "failed") {
    return { kind: "failed", label: "Failed", taskId: ingestion.taskId }
  }
  if (ingestion?.status === "cancelled") {
    return {
      kind: "cancelled",
      label: "Cancelled",
      taskId: ingestion.taskId,
    }
  }
  if (ingestion?.status === "ingesting") {
    return { kind: "running", label: "Indexing", taskId: ingestion.taskId }
  }
  if (ingestion?.status === "unrecorded") {
    return {
      kind: "unrecorded",
      label: "Unrecorded",
      taskId: ingestion.taskId,
    }
  }
  if (!ingestion || ingestion.status === "unscheduled") {
    return {
      kind: "unscheduled",
      label: "Unscheduled",
      taskId: ingestion?.taskId ?? null,
    }
  }
  return {
    kind: "pending",
    label: "Pending",
    taskId: ingestion?.taskId ?? null,
  }
}
