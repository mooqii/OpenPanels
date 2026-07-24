import { Button, Dropdown, Label, Tag, TagGroup } from "@heroui/react"
import { AlertTriangle, ChevronDown, PanelLeft, Pencil } from "lucide-react"
import { type ReactNode, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { apiUrl } from "../../lib/api"
import {
  publishingSourceHasContent,
  typesettingContentToPlainText,
} from "../../lib/publishing"
import {
  isTypesettingCoverVideo,
  selectedPublicationTitleId,
  typesettingPublicationTitles,
} from "../../lib/typesetting"
import type {
  MyOpenPanelsTransport,
  TypesettingPublication,
  TypesettingPublicationImage,
} from "../../types"
import { ImagePreviewDialog } from "../ImagePreviewDialog"

export function PublicationPreview({
  className = "",
  onEdit,
  onOpenSources,
  onSelectTitle,
  publication,
  modeHeader,
  showHeader = true,
  transport,
}: {
  className?: string
  onEdit: () => void
  onOpenSources: () => void
  onSelectTitle?: (titleId: string) => void
  publication: TypesettingPublication
  modeHeader?: ReactNode
  showHeader?: boolean
  transport: MyOpenPanelsTransport
}) {
  const { t } = useMyOpenPanelsI18n()
  const bodyText = typesettingContentToPlainText(publication.content)
  const sourceComplete = publishingSourceHasContent(
    bodyText,
    publication.covers.length
  )
  const titleOptions = typesettingPublicationTitles(publication)
  const selectedTitleId = selectedPublicationTitleId(publication)
  const [previewedCover, setPreviewedCover] = useState<{
    alt: string
    src: string
  } | null>(null)

  return (
    <main
      className={`op-publishing-module op-publishing-preview ${className}`.trim()}
    >
      {modeHeader}
      {showHeader ? (
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
      ) : null}
      <div className="op-publishing-preview__scroll">
        <div className="op-publishing-media-strip">
          {publication.covers.map((cover, index) => (
            <figure key={`${cover.assetRef}:${index}`}>
              <PublishingMediaPreview
                cover={cover}
                label={`${publication.title || t`Untitled publication`} ${index + 1}`}
                onPreview={(src, alt) => setPreviewedCover({ alt, src })}
                previewLabel={t`View cover`}
                src={apiUrl(transport.apiBase, cover.src).toString()}
              />
              <figcaption>
                {index === 0 ? t`Primary cover` : `${index + 1}`}
              </figcaption>
            </figure>
          ))}
        </div>
        <article className="op-publishing-note-preview">
          <div className="op-publishing-note-preview__title-row">
            <h1>{publication.title || t`Untitled`}</h1>
            {onSelectTitle && titleOptions.length > 1 ? (
              <Dropdown>
                <Button
                  aria-label={t`Expand titles`}
                  className="op-publishing-note-preview__title-button"
                  size="sm"
                  variant="tertiary"
                >
                  {titleOptions.length}
                  <ChevronDown size={14} />
                </Button>
                <Dropdown.Popover
                  className="op-publishing-note-preview__title-popover"
                  placement="bottom end"
                >
                  <Dropdown.Menu
                    aria-label={t`Titles`}
                    onAction={(key) => onSelectTitle(String(key))}
                    selectedKeys={[selectedTitleId]}
                    selectionMode="single"
                  >
                    {titleOptions.map((title) => {
                      const label =
                        title.value.trim() || t`Untitled publication`
                      return (
                        <Dropdown.Item
                          id={title.id}
                          key={title.id}
                          textValue={label}
                        >
                          <Dropdown.ItemIndicator />
                          <Label>{label}</Label>
                        </Dropdown.Item>
                      )
                    })}
                  </Dropdown.Menu>
                </Dropdown.Popover>
              </Dropdown>
            ) : null}
          </div>
          {(publication.tags ?? []).length > 0 ? (
            <TagGroup
              aria-label={t`Tags`}
              className="op-publishing-note-preview__tags"
              size="sm"
              variant="surface"
            >
              <TagGroup.List
                items={(publication.tags ?? []).map((tag) => ({
                  id: tag,
                  name: tag,
                }))}
              >
                {(tag) => (
                  <Tag id={tag.id} textValue={tag.name}>
                    {tag.name}
                  </Tag>
                )}
              </TagGroup.List>
            </TagGroup>
          ) : null}
          <pre>{bodyText || t`Empty body`}</pre>
        </article>
        {sourceComplete ? null : (
          <div className="op-publishing-warning">
            <AlertTriangle size={16} />
            {t`Add text content or at least one media file to publish`}
          </div>
        )}
      </div>
      {previewedCover ? (
        <ImagePreviewDialog
          alt={previewedCover.alt}
          closeLabel={t`Close`}
          onClose={() => setPreviewedCover(null)}
          src={previewedCover.src}
        />
      ) : null}
    </main>
  )
}

function PublishingMediaPreview({
  cover,
  label,
  onPreview,
  previewLabel,
  src,
}: {
  cover: TypesettingPublicationImage
  label: string
  onPreview: (src: string, alt: string) => void
  previewLabel: string
  src: string
}) {
  if (isTypesettingCoverVideo(cover)) {
    return (
      // biome-ignore lint/a11y/useMediaCaption: Cover videos are user-supplied publishing media without caption tracks.
      <video
        aria-label={label}
        controls
        playsInline
        preload="metadata"
        src={src}
      />
    )
  }
  return (
    <Button
      aria-label={previewLabel}
      className="op-publishing-media-preview__button"
      onPress={() => onPreview(src, label)}
      variant="ghost"
    >
      <img alt={label} src={src} />
    </Button>
  )
}
