import { useMyOpenPanelsI18n } from "../../canvas"
import {
  ConfirmDialog,
  MarkdownDialog,
  OriginalPreviewDialog,
  RenameDocumentDialog,
} from "./Dialogs"
import type { ReturnTypeOfWikiPanelController } from "./useWikiPanelController"

export function WikiDialogsLayer({
  controller,
  mentionRawDocuments,
}: {
  controller: ReturnTypeOfWikiPanelController
  mentionRawDocuments: boolean
}) {
  const { t } = useMyOpenPanelsI18n()
  const {
    markdownDialog,
    setMarkdownDialog,
    renameRawDocumentFile,
    pendingRenameRawDocument,
    setPendingRenameRawDocument,
    renameRawDocument,
    saveMarkdown,
    pageDialog,
    setPageDialog,
    renameWikiPageFile,
    saveWikiPage,
    originalPreview,
    setOriginalPreview,
    generatedDocumentDialog,
    setGeneratedDocumentDialog,
    renameGeneratedDocumentFile,
    saveGeneratedMarkdown,
    pendingRenameGeneratedDocument,
    setPendingRenameGeneratedDocument,
    isBusy,
    renameGeneratedDocument,
    pendingDeleteGeneratedDocument,
    setPendingDeleteGeneratedDocument,
    deleteGeneratedDocument,
    pendingDeleteDocument,
    setPendingDeleteDocument,
    deleteRawDocument,
    pendingWikiAgentSkillId,
    setPendingWikiAgentSkillId,
    updateWikiAgentSkill,
  } = controller
  return (
    <>
      {markdownDialog ? (
        <MarkdownDialog
          closeLabel={t`Close`}
          content={markdownDialog.content}
          fileName={markdownDialog.document.originalFileName}
          onChange={(content) =>
            setMarkdownDialog((current) =>
              current ? { ...current, content } : current
            )
          }
          onClose={() => setMarkdownDialog(null)}
          onRenameFileName={renameRawDocumentFile}
          onSave={saveMarkdown}
        />
      ) : null}

      {pageDialog ? (
        <MarkdownDialog
          closeLabel={t`Close`}
          content={pageDialog.content}
          fileName={pageDialog.pagePath}
          onChange={(content) =>
            setPageDialog((current) =>
              current ? { ...current, content } : current
            )
          }
          onClose={() => setPageDialog(null)}
          onRenameFileName={renameWikiPageFile}
          onSave={saveWikiPage}
        />
      ) : null}

      {originalPreview ? (
        <OriginalPreviewDialog
          closeLabel={t`Close`}
          document={originalPreview.document}
          key={originalPreview.document.id}
          onClose={() => setOriginalPreview(null)}
          previewUrl={originalPreview.previewUrl}
          titleLabel={t`Original file`}
        />
      ) : null}

      {generatedDocumentDialog ? (
        <MarkdownDialog
          closeLabel={t`Close`}
          content={generatedDocumentDialog.content}
          fileName={generatedDocumentDialog.document.originalFileName}
          onChange={(content) =>
            setGeneratedDocumentDialog((current) =>
              current ? { ...current, content } : current
            )
          }
          onClose={() => setGeneratedDocumentDialog(null)}
          onRenameFileName={renameGeneratedDocumentFile}
          onSave={saveGeneratedMarkdown}
        />
      ) : null}

      {pendingRenameGeneratedDocument ? (
        <RenameDocumentDialog
          cancelLabel={t`Cancel`}
          confirmLabel={t`Rename`}
          isBusy={isBusy}
          onCancel={() => setPendingRenameGeneratedDocument(null)}
          onConfirm={(title) =>
            renameGeneratedDocument(
              pendingRenameGeneratedDocument,
              title
            ).catch((error) => {
              console.error("Failed to rename generated document", error)
            })
          }
          title={t`Rename document`}
          value={pendingRenameGeneratedDocument.title}
        />
      ) : null}

      {pendingRenameRawDocument ? (
        <RenameDocumentDialog
          cancelLabel={t`Cancel`}
          confirmLabel={t`Rename`}
          isBusy={isBusy}
          onCancel={() => setPendingRenameRawDocument(null)}
          onConfirm={(title) =>
            renameRawDocument(pendingRenameRawDocument, title).catch(
              (error) => {
                console.error("Failed to rename raw document", error)
              }
            )
          }
          title={t`Rename raw document`}
          value={pendingRenameRawDocument.title}
        />
      ) : null}

      {pendingDeleteGeneratedDocument ? (
        <ConfirmDialog
          cancelLabel={t`Cancel`}
          confirmLabel={t`Delete`}
          isBusy={isBusy}
          message={
            mentionRawDocuments
              ? t`This document will be removed from My Documents. Published raw documents will be kept.`
              : t`This document will be removed from My Documents.`
          }
          onCancel={() => setPendingDeleteGeneratedDocument(null)}
          onConfirm={() =>
            deleteGeneratedDocument(pendingDeleteGeneratedDocument).catch(
              (error) => {
                console.error("Failed to delete generated document", error)
              }
            )
          }
          title={t`Delete document?`}
        />
      ) : null}

      {pendingDeleteDocument ? (
        <ConfirmDialog
          cancelLabel={t`Cancel`}
          confirmLabel={t`Delete`}
          isBusy={isBusy}
          message={t`This raw document will be removed from the source library.`}
          onCancel={() => setPendingDeleteDocument(null)}
          onConfirm={() =>
            deleteRawDocument(pendingDeleteDocument).catch((error) => {
              console.error("Failed to delete wiki raw document", error)
            })
          }
          title={t`Delete document?`}
        />
      ) : null}

      {pendingWikiAgentSkillId ? (
        <ConfirmDialog
          cancelLabel={t`Cancel`}
          confirmLabel={t`Switch and rebuild`}
          isBusy={isBusy}
          message={
            mentionRawDocuments
              ? t`All generated Wiki pages in this project will be deleted and rebuilt with the selected Skill. Raw documents and My Documents will be kept.`
              : t`All generated Wiki pages in this project will be deleted and rebuilt with the selected Skill. My Documents will be kept.`
          }
          onCancel={() => setPendingWikiAgentSkillId(null)}
          onConfirm={() =>
            updateWikiAgentSkill(pendingWikiAgentSkillId, true).catch(
              (error) => {
                console.error("Failed to switch Wiki generation Skill", error)
              }
            )
          }
          title={t`Switch Wiki generation Skill?`}
        />
      ) : null}
    </>
  )
}
