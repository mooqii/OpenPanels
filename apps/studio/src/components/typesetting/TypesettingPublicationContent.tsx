import { Button, Tooltip } from "@heroui/react"
import { EditorContent, type useEditor } from "@tiptap/react"
import { AlertCircle, LoaderCircle, Sparkles, X } from "lucide-react"
import { useMyOpenPanelsI18n } from "../../canvas"
import {
  isTypesettingDocumentEmpty,
  publicationLayoutTaskStatus,
} from "../../lib/typesetting"
import type { ProjectTask } from "../../types"
import { TypesettingToolbar } from "./TypesettingToolbar"

export function PublicationContentSection({
  activeTask,
  assetError,
  editor,
  latestTask,
  onDismissError,
  onInsertImages,
  onOpenLayout,
  onOpenTask,
}: {
  activeTask: ProjectTask | null
  assetError: string | null
  editor: ReturnType<typeof useEditor>
  latestTask: ProjectTask | null
  onDismissError: () => void
  onInsertImages: () => void
  onOpenLayout: () => void
  onOpenTask: (taskId: string) => void
}) {
  const { t } = useMyOpenPanelsI18n()
  return (
    <>
      <section className="op-typesetting-section op-publication-content-section">
        <div className="op-typesetting-section__heading">
          <div>
            <span>{t`Content details`}</span>
          </div>
          <div className="op-typesetting-section__actions">
            {latestTask ? (
              <LayoutTaskStatus
                onOpen={() => onOpenTask(latestTask.id)}
                task={latestTask}
              />
            ) : null}
            <Button
              isDisabled={Boolean(activeTask)}
              onPress={onOpenLayout}
              size="sm"
              variant="secondary"
            >
              <Sparkles size={14} />
              {t`Automatic layout`}
            </Button>
          </div>
        </div>
        <div
          aria-disabled={Boolean(activeTask)}
          className={
            activeTask
              ? "is-layout-locked op-typesetting-editor"
              : "op-typesetting-editor"
          }
        >
          <TypesettingToolbar
            disabled={Boolean(activeTask)}
            editor={editor}
            onInsertImages={onInsertImages}
          />
          <div className="op-typesetting-editor__body">
            {editor && isTypesettingDocumentEmpty(editor.getJSON()) ? (
              <div className="op-typesetting-editor__empty">
                <span>{t`Open a document from the library and insert it here.`}</span>
              </div>
            ) : null}
            <EditorContent editor={editor} />
          </div>
          {activeTask ? (
            <Tooltip closeDelay={0} delay={0}>
              <button
                aria-label={t`A layout task is in progress. Cancel it or wait for it to finish before editing.`}
                className="op-typesetting-editor__lock"
                onClick={() => onOpenTask(activeTask.id)}
                type="button"
              />
              <Tooltip.Content placement="top">
                {t`A layout task is in progress. Cancel it or wait for it to finish before editing.`}
              </Tooltip.Content>
            </Tooltip>
          ) : null}
        </div>
      </section>
      {assetError ? (
        <div className="op-typesetting-inline-error" role="alert">
          <AlertCircle size={15} />
          <span className="op-typesetting-inline-error__message">
            {assetError}
          </span>
          <Button
            aria-label={t`Dismiss`}
            isIconOnly
            onPress={onDismissError}
            size="sm"
            variant="ghost"
          >
            <X size={14} />
          </Button>
        </div>
      ) : null}
    </>
  )
}

function LayoutTaskStatus({
  onOpen,
  task,
}: {
  onOpen: () => void
  task: ProjectTask
}) {
  const { t } = useMyOpenPanelsI18n()
  const status = publicationLayoutTaskStatus(task)
  const label =
    status === "waiting"
      ? t`Waiting for layout`
      : status === "running"
        ? t`Formatting content`
        : status === "completed"
          ? t`Layout completed`
          : status === "failed"
            ? t`Layout failed`
            : t`Layout cancelled`
  return (
    <button
      className={`is-${status} op-publication-layout-status`}
      onClick={onOpen}
      type="button"
    >
      {status === "waiting" || status === "running" ? (
        <LoaderCircle className="op-spin" size={13} />
      ) : status === "failed" ? (
        <AlertCircle size={13} />
      ) : null}
      <span>{label}</span>
    </button>
  )
}
