import { useEffect, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { apiJson } from "../../lib/api"
import { formatRelativeOrDate } from "../../lib/date-time"
import type { WikiGeneratedDocument } from "../../types"
import { countDocumentCharacters } from "./generated-document-display"

export function GeneratedDocumentMeta({
  apiBase,
  document,
}: {
  apiBase: string
  document: WikiGeneratedDocument
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const [now, setNow] = useState(() => Date.now())
  const [wordCount, setWordCount] = useState(document.wordCount)

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
