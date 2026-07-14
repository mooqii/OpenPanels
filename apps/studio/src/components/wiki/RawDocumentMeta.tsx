import { useMyOpenPanelsI18n } from "../../canvas"
import type { WikiRawDocument } from "../../types"
import { rawDocumentFormats } from "./raw-document-display"

export function RawDocumentMeta({
  document,
  onOpenOriginal,
}: {
  document: WikiRawDocument
  onOpenOriginal: () => void
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const formats = rawDocumentFormats(document)
  const hasWordCount =
    document.wordCount !== null && document.wordCount !== undefined

  return (
    <span className="op-raw-document-meta">
      {formats.original ? (
        <button
          aria-label={`${t`Open original file`}: ${document.originalFileName}`}
          className="op-raw-document-meta__original"
          onClick={onOpenOriginal}
          type="button"
        >
          {formats.original}
        </button>
      ) : null}
      {formats.converted ? (
        <>
          <span aria-hidden="true">→</span>
          <span>{formats.converted}</span>
        </>
      ) : null}
      {hasWordCount ? (
        <>
          <span aria-hidden="true">·</span>
          <span>
            {document.wordCount?.toLocaleString(locale)} {t`characters`}
          </span>
        </>
      ) : null}
    </span>
  )
}
