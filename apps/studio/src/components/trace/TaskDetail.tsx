import { Button } from "@heroui/react"
import { Copy } from "lucide-react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { taskDisplayPhase } from "../../lib/task-status"
import type { MyOpenPanelsBuildInfo, ProjectTask } from "../../types"
import { TaskRetryControl } from "./TaskRetryControl"
import {
  formatDispatchState,
  formatTaskError,
  formatTaskTime,
} from "./trace-utils"

export function TaskDetail({
  apiBase,
  buildInfo,
  task,
}: {
  apiBase: string
  buildInfo?: MyOpenPanelsBuildInfo
  task: ProjectTask
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const detail = JSON.stringify(task, null, 2)
  return (
    <div className="op-agent-task__detail">
      <div className="op-agent-task__meta">
        <span>{t(task.queue)}</span>
        <span>{t(task.status)}</span>
        {task.attempt ? (
          <span>
            {t`attempt`} {task.attempt}
            {task.attemptLimit ? `/${task.attemptLimit}` : ""}
          </span>
        ) : null}
        {task.dispatchState ? (
          <span>{t(formatDispatchState(task.dispatchState))}</span>
        ) : null}
        <span>{t(task.panelKind)}</span>
        <span>{task.targetId || task.id}</span>
      </div>
      {task.error &&
      (taskDisplayPhase(task) === "failed" ||
        taskDisplayPhase(task) === "cancelled") ? (
        <span className="op-agent-task__note">
          {t(formatTaskError(task.error))}
        </span>
      ) : null}
      {task.nextRunAt ? (
        <span className="op-agent-task__note">
          {t`Next run`} {formatTaskTime(task.nextRunAt, locale)}
        </span>
      ) : task.lease?.expiresAt && task.blockedReason === "leased" ? (
        <span className="op-agent-task__note">
          {t`Lease until`} {formatTaskTime(task.lease.expiresAt, locale)}
        </span>
      ) : null}
      <TaskRetryControl apiBase={apiBase} buildInfo={buildInfo} task={task} />
      <code>{task.id}</code>
      {task.dependencies?.length ? (
        <div className="op-agent-task__command">
          <span>{t`Prerequisites`}</span>
          <code>
            {task.dependencies
              .map(
                (dependency) =>
                  `${dependency.prerequisiteTaskId} · ${t(dependency.status)} · ${t(dependency.failurePolicy)}`
              )
              .join("\n")}
          </code>
        </div>
      ) : null}
      {task.executionGeneration !== undefined ||
      task.compatibleTargetCount !== undefined ? (
        <div className="op-agent-task__command">
          <span>{t`Execution`}</span>
          <code>
            {t`generation`} {task.executionGeneration ?? 0} ·{" "}
            {t`compatible targets`} {task.compatibleTargetCount ?? 0}
          </code>
        </div>
      ) : null}
      <div className="op-agent-task__json">
        <Button
          aria-label={t`Copy task detail`}
          isIconOnly
          onPress={() => navigator.clipboard?.writeText(detail)}
          size="sm"
          variant="ghost"
        >
          <Copy size={14} />
        </Button>
        <pre>{detail}</pre>
      </div>
    </div>
  )
}
