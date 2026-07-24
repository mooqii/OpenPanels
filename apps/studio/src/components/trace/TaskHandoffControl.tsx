import { Button, Tooltip } from "@heroui/react"
import { Copy } from "lucide-react"
import { useMyOpenPanelsI18n } from "../../canvas"
import type { TaskExecutionScope } from "../../types"

export function TaskHandoffControl({
  hasUsableAgentCli,
  instructionLabel,
  onOpenManualTask,
  scope,
}: {
  hasUsableAgentCli: boolean | null
  instructionLabel?: string
  onOpenManualTask: (scope: TaskExecutionScope) => void
  scope: TaskExecutionScope
}) {
  const { t } = useMyOpenPanelsI18n()
  const requiresManualInstruction = hasUsableAgentCli === false
  const accessibleLabel = instructionLabel ?? t`Copy task instruction`
  const openInstruction = () => onOpenManualTask(scope)

  return (
    <div className="op-agent-task__dispatch">
      {requiresManualInstruction ? (
        <Button
          aria-label={accessibleLabel}
          className="op-agent-task__copy-instruction"
          onPress={openInstruction}
          size="sm"
          variant="primary"
        >
          <Copy size={14} />
          {t`Copy instruction`}
        </Button>
      ) : (
        <Tooltip closeDelay={0} delay={300}>
          <Button
            aria-label={accessibleLabel}
            isIconOnly
            onPress={openInstruction}
            size="sm"
            variant="secondary"
          >
            <Copy size={14} />
          </Button>
          <Tooltip.Content placement="top">{accessibleLabel}</Tooltip.Content>
        </Tooltip>
      )}
      <small>
        {requiresManualInstruction
          ? t`Please send the instruction to an Agent manually`
          : t`Waiting for an active Agent CLI to claim the task`}
      </small>
    </div>
  )
}
