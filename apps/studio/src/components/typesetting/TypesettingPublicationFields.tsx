import {
  Button,
  InputGroup,
  type Key,
  Label,
  Tag,
  TagGroup,
  Tooltip,
} from "@heroui/react"
import {
  AlertCircle,
  ChevronDown,
  LoaderCircle,
  Plus,
  Sparkles,
  Trash2,
} from "lucide-react"
import { useCallback, useEffect, useRef, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { randomId } from "../../lib/id"
import {
  addPublicationTitle,
  appendTypesettingTags,
  publicationTitleTaskStatus,
  removePublicationTitle,
  selectedPublicationTitleId,
  selectPublicationTitle,
  typesettingPublicationTitles,
  updatePublicationTitle,
} from "../../lib/typesetting"
import type { ProjectTask, TypesettingPublication } from "../../types"

type UpdatePublication = (
  updater: (publication: TypesettingPublication) => TypesettingPublication
) => void

export function PublicationTitleField({
  onGenerate,
  onOpenTask,
  onUpdate,
  publication,
  task,
}: {
  onGenerate: () => void
  onOpenTask: (taskId: string) => void
  onUpdate: UpdatePublication
  publication: TypesettingPublication
  task: ProjectTask | null
}) {
  const { t } = useMyOpenPanelsI18n()
  const [isExpanded, setIsExpanded] = useState(false)
  const fieldRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const publicationIdRef = useRef(publication.id)

  useEffect(() => {
    if (publicationIdRef.current === publication.id) return
    publicationIdRef.current = publication.id
    setIsExpanded(false)
  }, [publication.id])

  useEffect(() => {
    if (!isExpanded) return
    const collapse = (event: PointerEvent) => {
      if (
        event.target instanceof Node &&
        !fieldRef.current?.contains(event.target)
      ) {
        setIsExpanded(false)
      }
    }
    document.addEventListener("pointerdown", collapse)
    return () => document.removeEventListener("pointerdown", collapse)
  }, [isExpanded])

  const titleOptions = typesettingPublicationTitles(publication)
  const activeTitleId = selectedPublicationTitleId(publication)
  const activeTitle =
    titleOptions.find(({ id }) => id === activeTitleId) ?? titleOptions[0]

  return (
    <div
      className="op-typesetting-field op-publication-title-field"
      ref={fieldRef}
    >
      <div className="op-publication-title-field__heading">
        <Label
          htmlFor={`publication-title-${publication.id}`}
        >{t`Title`}</Label>
        <div className="op-publication-title-field__actions">
          {task ? (
            <TitleTaskStatus onOpen={() => onOpenTask(task.id)} task={task} />
          ) : null}
          <Button onPress={onGenerate} size="sm" variant="secondary">
            <Sparkles size={14} />
            {t`Generate titles`}
          </Button>
        </div>
      </div>
      <InputGroup
        aria-label={t`Title`}
        className="op-publication-title-field__control"
        fullWidth
        variant="secondary"
      >
        <InputGroup.Input
          aria-label={t`Title`}
          id={`publication-title-${publication.id}`}
          onChange={(event) => {
            const value = event.currentTarget.value
            onUpdate((current) => ({
              ...updatePublicationTitle(current, activeTitleId, value),
              updatedAt: new Date().toISOString(),
            }))
          }}
          placeholder={t`Untitled publication`}
          ref={inputRef}
          value={activeTitle.value}
        />
        <InputGroup.Suffix className="op-publication-title-field__suffix">
          <Tooltip closeDelay={0} delay={300}>
            <Button
              aria-controls={`publication-title-options-${publication.id}`}
              aria-expanded={isExpanded}
              aria-label={isExpanded ? t`Collapse titles` : t`Expand titles`}
              className="op-publication-title-field__expand-button"
              isIconOnly
              onPress={() => setIsExpanded((expanded) => !expanded)}
              size="sm"
              variant="ghost"
            >
              <ChevronDown
                className="op-publication-title-field__chevron"
                data-expanded={isExpanded}
                size={16}
              />
            </Button>
            <Tooltip.Content placement="top">
              {isExpanded ? t`Collapse titles` : t`Expand titles`}
            </Tooltip.Content>
          </Tooltip>
        </InputGroup.Suffix>
      </InputGroup>

      {isExpanded ? (
        <div
          aria-label={t`Titles`}
          className="op-publication-title-field__list"
          id={`publication-title-options-${publication.id}`}
          role="list"
        >
          {titleOptions.map((title) => (
            <div
              className="op-publication-title-field__row"
              data-selected={title.id === activeTitleId}
              key={title.id}
              role="listitem"
            >
              <Button
                aria-pressed={title.id === activeTitleId}
                className="op-publication-title-field__option"
                onPress={() => {
                  onUpdate((current) => ({
                    ...selectPublicationTitle(current, title.id),
                    updatedAt: new Date().toISOString(),
                  }))
                  setIsExpanded(false)
                  inputRef.current?.focus()
                }}
                variant="ghost"
              >
                <span className="op-publication-title-field__option-label">
                  {title.value.trim() || t`Untitled publication`}
                </span>
              </Button>
              <Tooltip closeDelay={0} delay={300}>
                <Button
                  aria-label={t`Delete title`}
                  isIconOnly
                  onPress={() => {
                    const replacementTitleId = randomId("publication-title")
                    onUpdate((current) => ({
                      ...removePublicationTitle(
                        current,
                        title.id,
                        replacementTitleId
                      ),
                      updatedAt: new Date().toISOString(),
                    }))
                  }}
                  size="sm"
                  variant="ghost"
                >
                  <Trash2 size={15} />
                </Button>
                <Tooltip.Content placement="top">
                  {t`Delete title`}
                </Tooltip.Content>
              </Tooltip>
            </div>
          ))}
          <Button
            className="op-publication-title-field__add-option"
            onPress={() => {
              onUpdate((current) => ({
                ...addPublicationTitle(current, {
                  id: randomId("publication-title"),
                  value: "",
                }),
                updatedAt: new Date().toISOString(),
              }))
              setIsExpanded(false)
              inputRef.current?.focus()
            }}
            variant="ghost"
          >
            <Plus size={16} />
            {t`New title`}
          </Button>
        </div>
      ) : null}
    </div>
  )
}

export function PublicationTagsField({
  onUpdate,
  publication,
}: {
  onUpdate: UpdatePublication
  publication: TypesettingPublication
}) {
  const { t } = useMyOpenPanelsI18n()
  const [draft, setDraft] = useState("")
  const publicationIdRef = useRef(publication.id)

  useEffect(() => {
    if (publicationIdRef.current === publication.id) return
    publicationIdRef.current = publication.id
    setDraft("")
  }, [publication.id])

  const commitDraft = useCallback(() => {
    const nextTags = appendTypesettingTags(publication.tags ?? [], draft)
    setDraft("")
    if (nextTags.length === (publication.tags ?? []).length) return
    onUpdate((current) => ({
      ...current,
      tags: appendTypesettingTags(current.tags ?? [], draft),
      updatedAt: new Date().toISOString(),
    }))
  }, [draft, onUpdate, publication.tags])

  const removeTags = useCallback(
    (keys: Set<Key>) => {
      onUpdate((current) => ({
        ...current,
        tags: (current.tags ?? []).filter((tag) => !keys.has(tag)),
        updatedAt: new Date().toISOString(),
      }))
    },
    [onUpdate]
  )

  return (
    <div className="op-typesetting-field op-typesetting-tags-field">
      <Label htmlFor="op-typesetting-tags-input">{t`Tags`}</Label>
      <InputGroup
        aria-label={t`Tags`}
        className="op-typesetting-tags-field__control"
        fullWidth
      >
        {(publication.tags ?? []).length > 0 ? (
          <InputGroup.Prefix className="op-typesetting-tags-field__prefix">
            <TagGroup
              aria-label={t`Tags`}
              className="op-typesetting-tags-field__tags"
              onRemove={removeTags}
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
          </InputGroup.Prefix>
        ) : null}
        <InputGroup.Input
          aria-label={t`Add tag`}
          id="op-typesetting-tags-input"
          onChange={(event) => setDraft(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.nativeEvent.isComposing || event.key !== "Enter") return
            event.preventDefault()
            commitDraft()
          }}
          placeholder={t`Add tag`}
          value={draft}
        />
      </InputGroup>
    </div>
  )
}

function TitleTaskStatus({
  onOpen,
  task,
}: {
  onOpen: () => void
  task: ProjectTask
}) {
  const { t } = useMyOpenPanelsI18n()
  const status = publicationTitleTaskStatus(task)
  const label =
    status === "waiting"
      ? t`Waiting for titles`
      : status === "running"
        ? t`Generating titles`
        : status === "saving"
          ? t`Saving titles`
          : status === "failed"
            ? t`Title generation failed`
            : t`Title generation cancelled`
  return (
    <button
      className={`is-${status} op-publication-title-status`}
      onClick={onOpen}
      type="button"
    >
      {status === "waiting" || status === "running" || status === "saving" ? (
        <LoaderCircle className="op-spin" size={13} />
      ) : status === "failed" ? (
        <AlertCircle size={13} />
      ) : null}
      <span>{label}</span>
    </button>
  )
}
