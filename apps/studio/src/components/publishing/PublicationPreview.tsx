import { Button } from "@heroui/react"
import { AlertTriangle, PanelLeft, Pencil } from "lucide-react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { apiUrl } from "../../lib/api"
import {
  publishingSourceHasContent,
  typesettingContentToPlainText,
} from "../../lib/publishing"
import type { MyOpenPanelsTransport, TypesettingPublication } from "../../types"

export function PublicationPreview({
  className = "",
  onEdit,
  onOpenSources,
  publication,
  transport,
}: {
  className?: string
  onEdit: () => void
  onOpenSources: () => void
  publication: TypesettingPublication
  transport: MyOpenPanelsTransport
}) {
  const { t } = useMyOpenPanelsI18n()
  const bodyText = typesettingContentToPlainText(publication.content)
  const sourceComplete = publishingSourceHasContent(
    bodyText,
    publication.covers.length
  )

  return (
    <main
      className={`op-publishing-module op-publishing-preview ${className}`.trim()}
    >
      <div className="op-publishing-preview__heading">
        <div className="op-publishing-preview__title">
          <Button
            aria-label={t`Open publication content`}
            className="op-publication-preview__sources-button"
            isIconOnly
            onPress={onOpenSources}
            size="sm"
            variant="ghost"
          >
            <PanelLeft size={17} />
          </Button>
          <h2>{t`Publish preview`}</h2>
        </div>
        <Button onPress={onEdit} size="sm" variant="secondary">
          <Pencil size={14} />
          {t`Edit`}
        </Button>
      </div>
      <div className="op-publishing-media-strip">
        {publication.covers.map((cover, index) => (
          <figure key={`${cover.assetRef}:${index}`}>
            <img
              alt={`${publication.title || t`Untitled publication`} ${index + 1}`}
              src={apiUrl(transport.apiBase, cover.src).toString()}
            />
            <figcaption>
              {index === 0 ? t`Primary cover` : `${index + 1}`}
            </figcaption>
          </figure>
        ))}
      </div>
      <article className="op-publishing-note-preview">
        <h1>{publication.title || t`Untitled`}</h1>
        <pre>{bodyText || t`Empty body`}</pre>
      </article>
      {sourceComplete ? null : (
        <div className="op-publishing-warning">
          <AlertTriangle size={16} />
          {t`Add text content or at least one image to publish`}
        </div>
      )}
    </main>
  )
}
