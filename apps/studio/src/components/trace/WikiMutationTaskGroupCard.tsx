import { Chip } from "@heroui/react"
import { LoaderCircle } from "lucide-react"
import type { RefObject } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import type {
  MyOpenPanelsBuildInfo,
  ProjectTask,
  TaskExecutionScope,
} from "../../types"
import { TaskDeleteButton } from "./TaskDeleteButton"
import { TaskDetail } from "./TaskDetail"
import { TaskHandoffControl } from "./TaskHandoffControl"
import {
  formatBlockedReason,
  formatTaskName,
  formatTaskTime,
  isActiveTask,
  isPendingTask,
  taskStatusColor,
  type WikiMutationTaskGroup,
} from "./trace-utils"

export function WikiMutationTaskGroupCard({
  apiBase,
  buildInfo,
  deletingTaskId,
  expandedTaskId,
  focusedTaskRef,
  group,
  hasUsableAgentCli,
  isFocused,
  onDeleteTask,
  onExpandTask,
  onOpenManualTask,
}: {
  apiBase: string
  buildInfo?: MyOpenPanelsBuildInfo
  deletingTaskId: string | null
  expandedTaskId: string | null
  focusedTaskRef: RefObject<HTMLElement | null>
  group: WikiMutationTaskGroup
  hasUsableAgentCli: boolean | null
  isFocused: boolean
  onDeleteTask: (task: ProjectTask) => void
  onExpandTask: (taskId: string | null) => void
  onOpenManualTask: (scope: TaskExecutionScope) => void
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const isActive = group.tasks.some(isActiveTask)
  return (
    <article
      className="op-agent-wiki-task-group"
      ref={isFocused ? focusedTaskRef : undefined}
    >
      <header className="op-agent-wiki-task-group__header">
        <div>
          <strong>{t`Wiki updates`}</strong>
          <span>
            {group.tasks.length}{" "}
            {group.tasks.length === 1 ? t`subtask` : t`subtasks`}
          </span>
        </div>
        <Chip color={isActive ? "accent" : "warning"} size="sm" variant="soft">
          <Chip.Label>{t(isActive ? "running" : "queued")}</Chip.Label>
        </Chip>
      </header>
      <div className="op-agent-wiki-task-group__tasks">
        {group.tasks.map((task) => {
          const isExpanded = expandedTaskId === task.id
          return (
            <div className="op-agent-wiki-task-group__task" key={task.id}>
              <button
                aria-expanded={isExpanded}
                className={
                  isPendingTask(task)
                    ? "op-agent-wiki-task-group__task-summary op-agent-wiki-task-group__task-summary--deletable"
                    : "op-agent-wiki-task-group__task-summary"
                }
                onClick={() => onExpandTask(isExpanded ? null : task.id)}
                type="button"
              >
                <span className="op-agent-wiki-task-group__task-status">
                  <span>
                    <Chip
                      className="op-agent-task__status"
                      color={taskStatusColor(task.status)}
                      size="sm"
                      variant="soft"
                    >
                      <Chip.Label>{t(task.status)}</Chip.Label>
                      {task.status === "running" ? (
                        <LoaderCircle
                          aria-hidden="true"
                          className="op-agent-task__status-spinner"
                          size={13}
                        />
                      ) : null}
                    </Chip>
                    {isPendingTask(task) &&
                    task.ready === false &&
                    task.blockedReason !== "mutationPredecessor" ? (
                      <Chip color="warning" size="sm" variant="soft">
                        {task.blockedReason
                          ? t(formatBlockedReason(task.blockedReason))
                          : t`not ready`}
                      </Chip>
                    ) : null}
                  </span>
                  <time>{formatTaskTime(task.updatedAt, locale)}</time>
                </span>
                <strong>{t(formatTaskName(task))}</strong>
              </button>
              <TaskDeleteButton
                isDeleting={deletingTaskId === task.id}
                onDelete={onDeleteTask}
                task={task}
              />
              {isExpanded ? (
                <TaskDetail
                  apiBase={apiBase}
                  buildInfo={buildInfo}
                  task={task}
                />
              ) : null}
            </div>
          )
        })}
      </div>
      <footer className="op-agent-wiki-task-group__footer">
        {isActive ? (
          <span className="op-agent-wiki-task-group__active">
            {t`Agent is processing Wiki update tasks`}
          </span>
        ) : (
          <TaskHandoffControl
            hasUsableAgentCli={hasUsableAgentCli}
            instructionLabel={t`Copy Wiki update instruction`}
            onOpenManualTask={onOpenManualTask}
            scope={{
              kind: "wiki-mutation-drain",
              mutationKey: group.mutationKey,
              projectId: group.projectId,
            }}
          />
        )}
      </footer>
    </article>
  )
}
