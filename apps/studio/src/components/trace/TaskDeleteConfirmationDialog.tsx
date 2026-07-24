import { AlertDialog, Button } from "@heroui/react"
import { useMyOpenPanelsI18n } from "../../canvas"
import type { ProjectTask } from "../../types"
import { formatTaskName } from "./trace-utils"

export function TaskDeleteConfirmationDialog({
  isDeleting,
  onCancel,
  onConfirm,
  task,
}: {
  isDeleting: boolean
  onCancel: () => void
  onConfirm: () => void
  task: ProjectTask | null
}) {
  const { t } = useMyOpenPanelsI18n()
  return (
    <AlertDialog.Backdrop
      isOpen={task !== null}
      onOpenChange={(isOpen) => {
        if (!(isOpen || isDeleting)) onCancel()
      }}
    >
      <AlertDialog.Container placement="center" size="sm">
        <AlertDialog.Dialog>
          <AlertDialog.Header>
            <AlertDialog.Icon status="danger" />
            <AlertDialog.Heading>{t`Delete task?`}</AlertDialog.Heading>
          </AlertDialog.Header>
          <AlertDialog.Body>
            <p>
              {task ? <strong>{t(formatTaskName(task))}</strong> : null}
              {task ? " " : null}
              {t`This task and any dependent tasks will be removed. The related source status will be updated.`}
            </p>
          </AlertDialog.Body>
          <AlertDialog.Footer>
            <Button
              isDisabled={isDeleting}
              onPress={onCancel}
              variant="tertiary"
            >
              {t`Cancel`}
            </Button>
            <Button isPending={isDeleting} onPress={onConfirm} variant="danger">
              {t`Delete`}
            </Button>
          </AlertDialog.Footer>
        </AlertDialog.Dialog>
      </AlertDialog.Container>
    </AlertDialog.Backdrop>
  )
}
