import { Button, Chip } from "@heroui/react"
import { FileText } from "lucide-react"
import { type OpenPanelsLocale, useOpenPanelsI18n } from "../../canvas"
import type { WikiRawDocument } from "../../types"

export function WikiStatus({
  document,
  isDisabled,
  onOpenMarkdown,
}: {
  document: WikiRawDocument
  isDisabled?: boolean
  onOpenMarkdown?: () => void
}) {
  const { t } = useOpenPanelsI18n()
  if (document.conversion.status === "failed") {
    return (
      <Chip color="danger" size="sm" variant="soft">
        {t`Conversion failed`}
      </Chip>
    )
  }
  if (
    document.conversion.status === "queued" ||
    document.conversion.status === "converting"
  ) {
    return (
      <Chip color="warning" size="sm" variant="soft">
        {t`Converting`}
      </Chip>
    )
  }
  return (
    <Button
      aria-label={t`Open Markdown`}
      isDisabled={isDisabled}
      isIconOnly
      onPress={onOpenMarkdown}
      size="sm"
      variant="ghost"
    >
      <FileText size={15} />
    </Button>
  )
}

export function WikiIndexStatus({
  status,
}: {
  status: ReturnType<typeof documentIndexStatus>
}) {
  const { t } = useOpenPanelsI18n()
  const color =
    status.kind === "failed"
      ? "danger"
      : status.kind === "running"
        ? "accent"
        : "warning"
  return (
    <Chip color={color} size="sm" variant="soft">
      {t(status.label)}
    </Chip>
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
    case "rebuild_wiki_index":
      return t`Rebuild wiki index`
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
): { kind: "done" | "failed" | "pending" | "running"; label: string } {
  const ingestion = wikiSpaceId
    ? document.ingestionByWikiSpace[wikiSpaceId]
    : undefined
  if (ingestion?.status === "ingested") {
    return { kind: "done", label: "Indexed" }
  }
  if (ingestion?.status === "failed") {
    return { kind: "failed", label: "Index failed" }
  }
  if (ingestion?.status === "ingesting") {
    return { kind: "running", label: "Indexing" }
  }
  return { kind: "pending", label: "Pending index" }
}

export function isWikiLanguage(
  language: unknown
): language is OpenPanelsLocale {
  return language === "en" || language === "zh-CN"
}
