import { Button, Tooltip } from "@heroui/react"
import { Trash2 } from "lucide-react"
import { useMyOpenPanelsI18n } from "../../canvas"
import type { ProjectTask } from "../../types"
import { isPendingTask } from "./trace-utils"

export function TaskDeleteButton({
  isDeleting,
  onDelete,
  task,
}: {
  isDeleting: boolean
  onDelete: (task: ProjectTask) => void
  task: ProjectTask
}) {
  const { t } = useMyOpenPanelsI18n()
  if (!isPendingTask(task)) return null
  return (
    <Tooltip>
      <Button
        aria-label={t`Delete task`}
        className="op-agent-task__delete"
        isDisabled={isDeleting}
        isIconOnly
        onPress={() => onDelete(task)}
        size="sm"
        variant="ghost"
      >
        <Trash2 size={14} />
      </Button>
      <Tooltip.Content placement="top">{t`Delete task`}</Tooltip.Content>
    </Tooltip>
  )
}
