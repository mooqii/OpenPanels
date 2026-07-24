import { Button, Tooltip } from "@heroui/react"
import { Copy } from "lucide-react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { taskDisplayPhase } from "../../lib/task-status"
import type { ProjectTask, TaskExecutionScope } from "../../types"
import { taskExecutionScope } from "./trace-utils"

export function TaskHandoffControl({
  onOpenManualTask,
  task,
}: {
  onOpenManualTask: (scope: TaskExecutionScope) => void
  task: ProjectTask
}) {
  const { t } = useMyOpenPanelsI18n()
  if (taskDisplayPhase(task) !== "waiting") return null

  return (
    <div className="op-agent-task__dispatch">
      <Tooltip closeDelay={0} delay={300}>
        <Button
          aria-label={t`Copy task instruction`}
          isIconOnly
          onPress={() => onOpenManualTask(taskExecutionScope(task))}
          size="sm"
          variant="secondary"
        >
          <Copy size={14} />
        </Button>
        <Tooltip.Content placement="top">
          {t`Copy task instruction`}
        </Tooltip.Content>
      </Tooltip>
    </div>
  )
}
