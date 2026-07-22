import { type ReactNode, useEffect, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { apiJson } from "../../lib/api"
import { formatRelativeOrDate } from "../../lib/date-time"
import type { WikiGeneratedDocument } from "../../types"
import {
  countDocumentCharacters,
  generatedDocumentFormats,
} from "./generated-document-display"

export function GeneratedDocumentMeta({
  apiBase,
  document,
  onOpenOriginal,
  status,
}: {
  apiBase: string
  document: WikiGeneratedDocument
  onOpenOriginal?: () => void
  status?: ReactNode
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const [now, setNow] = useState(() => Date.now())
  const [wordCount, setWordCount] = useState(document.wordCount)
  const formats = generatedDocumentFormats(document)
  const hasOriginalFormat = Boolean(formats.original)

  useEffect(() => {
    setWordCount(document.wordCount)
    if (document.wordCount !== null && document.wordCount !== undefined) return
    if (document.contentVersion < 1) return

    let cancelled = false
    apiJson<{ content?: string }>(
      apiBase,
      `/api/wiki/generated-documents/${encodeURIComponent(document.id)}`
    )
      .then((payload) => {
        if (!cancelled && typeof payload.content === "string") {
          setWordCount(countDocumentCharacters(payload.content))
        }
      })
      .catch(() => {
        // Older documents can still show their generation date if content is unavailable.
      })

    return () => {
      cancelled = true
    }
  }, [apiBase, document.contentVersion, document.id, document.wordCount])

  useEffect(() => {
    const timestamp = Date.parse(document.updatedAt)
    if (Number.isNaN(timestamp) || Math.abs(timestamp - now) >= 86_400_000) {
      return
    }
    const timer = window.setInterval(() => setNow(Date.now()), 60_000)
    return () => window.clearInterval(timer)
  }, [document.updatedAt, now])

  return (
    <span className="op-generated-document-meta">
      {formats.original && onOpenOriginal ? (
        <button
          aria-label={`${t`Open original file`}: ${document.importSource?.fileName ?? ""}`}
          className="op-generated-document-meta__original"
          onClick={onOpenOriginal}
          type="button"
        >
          {formats.original}
        </button>
      ) : formats.original ? (
        <span>{formats.original}</span>
      ) : null}
      {formats.converted ? (
        <>
          <span aria-hidden="true">→</span>
          <span>{formats.converted}</span>
        </>
      ) : null}
      {status ? (
        <span className="op-generated-document-meta__status">{status}</span>
      ) : null}
      {hasOriginalFormat || status ? <span aria-hidden="true">·</span> : null}
      {wordCount !== null && wordCount !== undefined ? (
        <>
          <span>
            {wordCount.toLocaleString(locale)} {t`characters`}
          </span>
          <span aria-hidden="true">·</span>
        </>
      ) : null}
      <span>{formatRelativeOrDate(document.updatedAt, locale, now)}</span>
    </span>
  )
}
