import { Button, Modal } from "@heroui/react"
import { Copy, Settings, Terminal } from "lucide-react"
import { useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import type { ManualTaskInstructionsController } from "../../hooks/use-manual-task-instructions"
import { copyTextToClipboard } from "../../lib/clipboard"
import type { ProjectTask } from "../../types"
import { manualTaskInstruction } from "./trace-utils"

export function ManualTaskInstructionPrompt({
  controller,
  onConfigureCli,
}: {
  controller: ManualTaskInstructionsController
  onConfigureCli: () => void
}) {
  return (
    <ManualTaskInstructionDialog
      onClose={controller.dismiss}
      onConfigureCli={() => {
        controller.dismissAll()
        onConfigureCli()
      }}
      task={controller.task}
    />
  )
}

export function ManualTaskInstructionDialog({
  onClose,
  onConfigureCli,
  task,
}: {
  onClose: () => void
  onConfigureCli: () => void
  task: ProjectTask | null
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const [copyResult, setCopyResult] = useState<{
    status: "copied" | "failed"
    taskId: string
  } | null>(null)
  const instruction = task ? manualTaskInstruction(task, locale) : ""

  if (!task) return null
  const copyStatus = copyResult?.taskId === task.id ? copyResult.status : null
  const copyInstruction = async () => {
    const copied = await copyTextToClipboard(instruction)
    setCopyResult({ status: copied ? "copied" : "failed", taskId: task.id })
  }

  return (
    <Modal.Backdrop
      isOpen
      onOpenChange={(isOpen) => !isOpen && onClose()}
      variant="blur"
    >
      <Modal.Container size="md">
        <Modal.Dialog className="op-manual-task-dialog">
          <Modal.Header>
            <Modal.Icon>
              <Terminal size={20} />
            </Modal.Icon>
            <Modal.Heading>{t`Send task to an Agent`}</Modal.Heading>
          </Modal.Header>
          <Modal.Body>
            <p>
              {t`No active and usable Agent CLI is available. Copy the instruction below and send it to an Agent to process this task.`}
            </p>
            <div className="op-manual-task-dialog__instruction">
              <span>{t`Task instruction`}</span>
              <pre>{instruction}</pre>
            </div>
          </Modal.Body>
          <Modal.Footer className="op-manual-task-dialog__footer">
            <Button onPress={onConfigureCli} variant="tertiary">
              <Settings size={16} />
              {t`Configure CLI`}
            </Button>
            <div className="op-manual-task-dialog__actions">
              {copyStatus ? (
                <span
                  className={`op-manual-task-dialog__copy-status op-manual-task-dialog__copy-status--${copyStatus}`}
                  role="status"
                >
                  {copyStatus === "copied" ? t`Copied` : t`Copy failed`}
                </span>
              ) : null}
              <Button onPress={copyInstruction} variant="primary">
                <Copy size={16} />
                {t`Copy instruction`}
              </Button>
              <Button onPress={onClose} variant="secondary">
                {t`Close`}
              </Button>
            </div>
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}
