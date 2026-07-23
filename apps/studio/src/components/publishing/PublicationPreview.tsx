import { Button, Dropdown, Label, Tag, TagGroup } from "@heroui/react"
import { AlertTriangle, ChevronDown, PanelLeft, Pencil } from "lucide-react"
import type { ReactNode } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { apiUrl } from "../../lib/api"
import {
  publishingSourceHasContent,
  typesettingContentToPlainText,
} from "../../lib/publishing"
import {
  selectedPublicationTitleId,
  typesettingPublicationTitles,
} from "../../lib/typesetting"
import type { MyOpenPanelsTransport, TypesettingPublication } from "../../types"

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
                    const label = title.value.trim() || t`Untitled publication`
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
          {t`Add text content or at least one image to publish`}
        </div>
      )}
    </main>
  )
}
