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
    myDocumentDialog,
    setMyDocumentDialog,
    renameMyDocumentFile,
    saveMyDocumentMarkdown,
    pendingRenameMyDocument,
    setPendingRenameMyDocument,
    isBusy,
    renameMyDocument,
    pendingDeleteMyDocument,
    setPendingDeleteMyDocument,
    deleteMyDocument,
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

      {myDocumentDialog ? (
        <MarkdownDialog
          closeLabel={t`Close`}
          content={myDocumentDialog.content}
          fileName={myDocumentDialog.document.originalFileName}
          onChange={(content) =>
            setMyDocumentDialog((current) =>
              current ? { ...current, content } : current
            )
          }
          onClose={() => setMyDocumentDialog(null)}
          onRenameFileName={renameMyDocumentFile}
          onSave={saveMyDocumentMarkdown}
        />
      ) : null}

      {pendingRenameMyDocument ? (
        <RenameDocumentDialog
          cancelLabel={t`Cancel`}
          confirmLabel={t`Rename`}
          isBusy={isBusy}
          onCancel={() => setPendingRenameMyDocument(null)}
          onConfirm={(title) =>
            renameMyDocument(pendingRenameMyDocument, title).catch((error) => {
              console.error("Failed to rename My Document", error)
            })
          }
          title={t`Rename document`}
          value={pendingRenameMyDocument.title}
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

      {pendingDeleteMyDocument ? (
        <ConfirmDialog
          cancelLabel={t`Cancel`}
          confirmLabel={t`Delete`}
          isBusy={isBusy}
          message={
            mentionRawDocuments
              ? t`This document will be removed from My Documents. Published raw documents will be kept.`
              : t`This document will be removed from My Documents.`
          }
          onCancel={() => setPendingDeleteMyDocument(null)}
          onConfirm={() =>
            deleteMyDocument(pendingDeleteMyDocument).catch((error) => {
              console.error("Failed to delete My Document", error)
            })
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
