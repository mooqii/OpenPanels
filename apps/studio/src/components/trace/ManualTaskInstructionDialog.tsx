import { Button, Modal } from "@heroui/react"
import { Copy, Settings, Terminal } from "lucide-react"
import { useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import type { ManualTaskInstructionsController } from "../../hooks/use-manual-task-instructions"
import { copyTextToClipboard } from "../../lib/clipboard"
import type { MyOpenPanelsBuildInfo, TaskExecutionScope } from "../../types"
import { manualTaskInstruction, taskExecutionScopeKey } from "./trace-utils"

export function ManualTaskInstructionPrompt({
  buildInfo,
  controller,
  onConfigureCli,
}: {
  buildInfo: MyOpenPanelsBuildInfo
  controller: ManualTaskInstructionsController
  onConfigureCli: () => void
}) {
  return (
    <ManualTaskInstructionDialog
      buildInfo={buildInfo}
      onClose={controller.dismiss}
      onConfigureCli={() => {
        controller.dismissAll()
        onConfigureCli()
      }}
      scope={controller.scope}
    />
  )
}

export function ManualTaskInstructionDialog({
  buildInfo,
  onClose,
  onConfigureCli,
  scope,
}: {
  buildInfo: MyOpenPanelsBuildInfo
  onClose: () => void
  onConfigureCli: () => void
  scope: TaskExecutionScope | null
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const [copyResult, setCopyResult] = useState<{
    status: "copied" | "failed"
    scopeKey: string
  } | null>(null)
  const instruction = scope
    ? manualTaskInstruction(scope, locale, buildInfo)
    : ""

  if (!scope) return null
  const scopeKey = taskExecutionScopeKey(scope)
  const copyStatus =
    copyResult?.scopeKey === scopeKey ? copyResult.status : null
  const copyInstruction = async () => {
    const copied = await copyTextToClipboard(instruction)
    setCopyResult({ scopeKey, status: copied ? "copied" : "failed" })
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
            <Modal.Heading>{t`Send Task Handoff to an Agent`}</Modal.Heading>
          </Modal.Header>
          <Modal.Body>
            <p>
              {t`No active and usable Agent CLI is available. Copy the instruction below and send it to an Agent to run this Task Handoff.`}
            </p>
            <div className="op-manual-task-dialog__instruction">
              <span>{t`Task Handoff instruction`}</span>
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
