import { Chip } from "@heroui/react"
import { useMyOpenPanelsI18n } from "../../canvas"
import type { WikiRawDocument } from "../../types"

export type WikiTaskListFilter = "active" | "done" | "pending"

export function conversionStatusTaskFilter(
  status: WikiRawDocument["conversion"]["status"]
): WikiTaskListFilter {
  if (status === "converting") return "active"
  return status === "cancelled" ? "done" : "pending"
}

export function indexStatusTaskFilter(
  status: ReturnType<typeof documentIndexStatus>
): WikiTaskListFilter {
  if (status.kind === "running") return "active"
  return status.kind === "cancelled" ? "done" : "pending"
}

function TaskStatusChip({
  color,
  filter,
  label,
  onOpenTasks,
}: {
  color: "accent" | "danger" | "warning"
  filter: WikiTaskListFilter
  label: string
  onOpenTasks: (filter: WikiTaskListFilter) => void
}) {
  const { t } = useMyOpenPanelsI18n()
  return (
    <button
      aria-label={`${label}. ${t`View related tasks`}`}
      className="op-wiki-task-status"
      onClick={() => onOpenTasks(filter)}
      title={t`View related tasks`}
      type="button"
    >
      <Chip
        className="op-wiki-task-status__chip"
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
  onOpenTasks: (filter: WikiTaskListFilter) => void
}) {
  const { t } = useMyOpenPanelsI18n()
  if (document.conversion.status === "cancelled") {
    return (
      <TaskStatusChip
        color="warning"
        filter={conversionStatusTaskFilter(document.conversion.status)}
        label={t`Conversion cancelled`}
        onOpenTasks={onOpenTasks}
      />
    )
  }
  if (document.conversion.status === "failed") {
    return (
      <TaskStatusChip
        color="danger"
        filter={conversionStatusTaskFilter(document.conversion.status)}
        label={t`Conversion failed`}
        onOpenTasks={onOpenTasks}
      />
    )
  }
  if (
    document.conversion.status === "queued" ||
    document.conversion.status === "converting"
  ) {
    return (
      <TaskStatusChip
        color="warning"
        filter={conversionStatusTaskFilter(document.conversion.status)}
        label={t`Converting`}
        onOpenTasks={onOpenTasks}
      />
    )
  }
  return null
}

export function WikiIndexStatus({
  onOpenTasks,
  status,
}: {
  onOpenTasks: (filter: WikiTaskListFilter) => void
  status: ReturnType<typeof documentIndexStatus>
}) {
  const { t } = useMyOpenPanelsI18n()
  const color =
    status.kind === "failed"
      ? "danger"
      : status.kind === "running"
        ? "accent"
        : "warning"
  return (
    <TaskStatusChip
      color={color}
      filter={indexStatusTaskFilter(status)}
      label={t(status.label)}
      onOpenTasks={onOpenTasks}
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
  status: string,
  t: (input: TemplateStringsArray | string, ...values: unknown[]) => string
) {
  switch (status) {
    case "queued":
      return t`Queued`
    case "claimed":
      return t`Claimed`
    case "running":
      return t`Running`
    case "failed":
      return t`Failed`
    case "succeeded":
      return t`Succeeded`
    case "stale":
      return t`Stale`
    default:
      return status
  }
}

export function documentIndexStatus(
  document: WikiRawDocument,
  wikiSpaceId: string | null | undefined
): {
  kind: "cancelled" | "done" | "failed" | "pending" | "running"
  label: string
} {
  const ingestion = wikiSpaceId
    ? document.ingestionByWikiSpace[wikiSpaceId]
    : undefined
  if (ingestion?.status === "ingested") {
    return { kind: "done", label: "Indexed" }
  }
  if (ingestion?.status === "failed") {
    return { kind: "failed", label: "Index failed" }
  }
  if (ingestion?.status === "cancelled") {
    return { kind: "cancelled", label: "Index cancelled" }
  }
  if (ingestion?.status === "ingesting") {
    return { kind: "running", label: "Indexing" }
  }
  return { kind: "pending", label: "Pending index" }
}
