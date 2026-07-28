import { Button, Tooltip } from "@heroui/react"
import {
  AlertTriangle,
  CheckCircle2,
  CircleHelp,
  CircleX,
  Clock3,
  ExternalLink,
  LoaderCircle,
  Send,
} from "lucide-react"
import { publishingAttemptStatus } from "../../lib/publishing"
import type { ProjectTask, PublishingAttempt } from "../../types"

export function PublishingAttemptRow({
  attempt,
  onOpenTask,
  task,
  t,
}: {
  attempt: PublishingAttempt
  onOpenTask?: () => void
  task?: ProjectTask
  t: (value: TemplateStringsArray) => string
}) {
  const status = publishingAttemptStatus(attempt, task)
  const label = publishingStatusLabel(status, t)
  const tooltip = attempt.summary ? `${label}: ${attempt.summary}` : label
  return (
    <div className="op-publishing-attempt">
      <time dateTime={attempt.createdAt}>
        {new Date(attempt.createdAt).toLocaleString()}
      </time>
      <div className="op-publishing-attempt__actions">
        {status === "published" && attempt.remoteUrl ? (
          <a
            className="op-publishing-attempt__view"
            href={attempt.remoteUrl}
            rel="noreferrer"
            target="_blank"
          >
            {t`View`}
            <ExternalLink aria-hidden size={12} />
          </a>
        ) : null}
        <Tooltip closeDelay={0} delay={300}>
          {onOpenTask ? (
            <Button
              aria-label={`${label}: ${t`Open task`}`}
              className="op-publishing-attempt__status"
              data-status={status}
              isIconOnly
              onPress={onOpenTask}
              size="sm"
              variant="ghost"
            >
              {publishingStatusIcon(status)}
            </Button>
          ) : (
            <span
              aria-label={label}
              className="op-publishing-attempt__status"
              data-status={status}
              role="img"
            >
              {publishingStatusIcon(status)}
            </span>
          )}
          <Tooltip.Content placement="top">{tooltip}</Tooltip.Content>
        </Tooltip>
      </div>
    </div>
  )
}

function publishingStatusIcon(
  status: ReturnType<typeof publishingAttemptStatus>
) {
  if (status === "queued") return <Clock3 size={16} />
  if (status === "running") return <LoaderCircle size={16} />
  if (status === "committing") return <Send size={16} />
  if (status === "published") return <CheckCircle2 size={16} />
  if (status === "needs_user_action") return <AlertTriangle size={16} />
  if (status === "not_published") return <CircleX size={16} />
  return <CircleHelp size={16} />
}

function publishingStatusLabel(
  status: ReturnType<typeof publishingAttemptStatus>,
  t: (value: TemplateStringsArray) => string
) {
  if (status === "queued") return t`Queued`
  if (status === "running") return t`Running`
  if (status === "committing") return t`Submitting`
  if (status === "published") return t`Published`
  if (status === "needs_user_action") return t`Needs user action`
  if (status === "not_published") return t`Not published`
  return t`Result unknown`
}
