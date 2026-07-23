import { useEffect, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { apiJson } from "../../lib/api"
import { formatRelativeOrDate } from "../../lib/date-time"
import type { WikiPageIndexItem } from "../../types"
import { countDocumentCharacters } from "./my-document-display"

export function WikiPageMeta({
  apiBase,
  page,
  wikiSpaceId,
}: {
  apiBase: string
  page: WikiPageIndexItem
  wikiSpaceId: string
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const [now, setNow] = useState(() => Date.now())
  const [wordCount, setWordCount] = useState(page.wordCount)

  useEffect(() => {
    setWordCount(page.wordCount)
    if (page.wordCount !== null && page.wordCount !== undefined) return

    let cancelled = false
    const pagePath = page.path.split("/").map(encodeURIComponent).join("/")
    apiJson<{ markdown?: string }>(
      apiBase,
      `/api/wiki/spaces/${encodeURIComponent(wikiSpaceId)}/pages/${pagePath}`
    )
      .then((payload) => {
        if (!cancelled && typeof payload.markdown === "string") {
          setWordCount(countDocumentCharacters(payload.markdown))
        }
      })
      .catch(() => {
        // The page timestamp remains useful if legacy content is unavailable.
      })

    return () => {
      cancelled = true
    }
  }, [apiBase, page.path, page.wordCount, wikiSpaceId])

  useEffect(() => {
    const timestamp = Date.parse(page.updatedAt)
    if (Number.isNaN(timestamp) || Math.abs(timestamp - now) >= 86_400_000) {
      return
    }
    const timer = window.setInterval(() => setNow(Date.now()), 60_000)
    return () => window.clearInterval(timer)
  }, [now, page.updatedAt])

  return (
    <span className="op-wiki-page-row__summary">
      {wordCount !== null && wordCount !== undefined ? (
        <>
          <span>
            {wordCount.toLocaleString(locale)} {t`characters`}
          </span>
          <span aria-hidden="true">·</span>
        </>
      ) : null}
      <span>{formatRelativeOrDate(page.updatedAt, locale, now)}</span>
    </span>
  )
}
