import { Button, Chip, Spinner } from "@heroui/react"
import {
  AlertCircle,
  ChevronLeft,
  ChevronRight,
  Eye,
  GripVertical,
  LoaderCircle,
  Plus,
  Sparkles,
  Trash2,
} from "lucide-react"
import { type DragEvent, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { apiUrl } from "../../lib/api"
import {
  isTypesettingCoverVideo,
  moveTypesettingCover,
  publicationCoverTaskStatus,
  TYPESETTING_ASSET_DRAG_TYPE,
  type TypesettingCoverTaskDisplayStatus,
} from "../../lib/typesetting"
import type {
  MyOpenPanelsTransport,
  ProjectTask,
  TypesettingPublication,
  TypesettingPublicationImage,
} from "../../types"
import { ImagePreviewDialog } from "../ImagePreviewDialog"

const TYPESETTING_COVER_DRAG_TYPE = "application/x-myopenpanels-cover-index"

export function PublicationCoversSection({
  isUploading,
  onAdd,
  onCreate,
  onDropCover,
  onOpenTask,
  onUpdate,
  publication,
  tasks,
  transport,
}: {
  isUploading: boolean
  onAdd: () => void
  onCreate: () => void
  onDropCover: (event: DragEvent<HTMLElement>) => Promise<void>
  onOpenTask: (taskId: string) => void
  onUpdate: (
    updater: (publication: TypesettingPublication) => TypesettingPublication
  ) => void
  publication: TypesettingPublication
  tasks: ProjectTask[]
  transport: MyOpenPanelsTransport
}) {
  const { t } = useMyOpenPanelsI18n()
  const [dropActive, setDropActive] = useState(false)
  const [draggedIndex, setDraggedIndex] = useState<number | null>(null)
  const [previewedCover, setPreviewedCover] = useState<{
    alt: string
    src: string
  } | null>(null)

  return (
    <>
      <section className="op-typesetting-section">
        <div className="op-typesetting-section__heading">
          <div>
            <span>{t`Covers`}</span>
          </div>
          <div className="op-typesetting-section__actions">
            <Button onPress={onAdd} size="sm" variant="secondary">
              <Plus size={14} />
              {t`Add`}
            </Button>
            <Button onPress={onCreate} size="sm" variant="secondary">
              <Sparkles size={14} />
              {t`Create cover`}
            </Button>
          </div>
        </div>
        <div
          className={
            dropActive
              ? "is-active op-publication-cover-zone"
              : "op-publication-cover-zone"
          }
          onDragLeave={() => setDropActive(false)}
          onDragOver={(event) => {
            const isAssetDrag = event.dataTransfer.types.includes(
              TYPESETTING_ASSET_DRAG_TYPE
            )
            const isFileDrag = event.dataTransfer.types.includes("Files")
            if (!(isAssetDrag || isFileDrag)) return
            event.preventDefault()
            event.dataTransfer.dropEffect = "copy"
            setDropActive(true)
          }}
          onDrop={(event) => {
            setDropActive(false)
            onDropCover(event).catch(() => undefined)
          }}
        >
          {publication.covers.length || tasks.length || isUploading ? (
            <div className="op-publication-covers">
              {publication.covers.map((cover, index) => (
                <div
                  className="op-publication-cover"
                  draggable
                  key={cover.assetRef}
                  onDragEnd={() => setDraggedIndex(null)}
                  onDragOver={(event) => {
                    if (
                      draggedIndex === null ||
                      !event.dataTransfer.types.includes(
                        TYPESETTING_COVER_DRAG_TYPE
                      )
                    ) {
                      return
                    }
                    event.preventDefault()
                    event.dataTransfer.dropEffect = "move"
                  }}
                  onDragStart={(event) => {
                    if (
                      (event.target as HTMLElement).closest(
                        "button, [role='button']"
                      )
                    ) {
                      event.preventDefault()
                      return
                    }
                    setDraggedIndex(index)
                    event.dataTransfer.effectAllowed = "move"
                    event.dataTransfer.setData(
                      TYPESETTING_COVER_DRAG_TYPE,
                      String(index)
                    )
                  }}
                  onDrop={(event) => {
                    const rawIndex = event.dataTransfer.getData(
                      TYPESETTING_COVER_DRAG_TYPE
                    )
                    if (!rawIndex) return
                    const from = Number(rawIndex)
                    if (!Number.isInteger(from)) return
                    event.preventDefault()
                    event.stopPropagation()
                    onUpdate((current) => ({
                      ...current,
                      covers: moveTypesettingCover(current.covers, from, index),
                      updatedAt: new Date().toISOString(),
                    }))
                    setDraggedIndex(null)
                  }}
                >
                  <PublicationCoverMedia
                    cover={cover}
                    src={apiUrl(transport.apiBase, cover.src).toString()}
                  />
                  <span className="op-publication-cover__grip">
                    <GripVertical size={14} />
                  </span>
                  {isTypesettingCoverVideo(cover) ? null : (
                    <Button
                      aria-label={t`View cover`}
                      className="op-publication-cover__view"
                      isIconOnly
                      onPress={() =>
                        setPreviewedCover({
                          alt: cover.fileName,
                          src: apiUrl(transport.apiBase, cover.src).toString(),
                        })
                      }
                      size="sm"
                      variant="ghost"
                    >
                      <Eye size={14} />
                    </Button>
                  )}
                  <div className="op-publication-cover__actions">
                    <Button
                      aria-label={t`Move cover left`}
                      isDisabled={index === 0}
                      isIconOnly
                      onPress={() =>
                        onUpdate((current) => ({
                          ...current,
                          covers: moveTypesettingCover(
                            current.covers,
                            index,
                            index - 1
                          ),
                          updatedAt: new Date().toISOString(),
                        }))
                      }
                      size="sm"
                      variant="ghost"
                    >
                      <ChevronLeft size={14} />
                    </Button>
                    <Button
                      aria-label={t`Move cover right`}
                      isDisabled={index === publication.covers.length - 1}
                      isIconOnly
                      onPress={() =>
                        onUpdate((current) => ({
                          ...current,
                          covers: moveTypesettingCover(
                            current.covers,
                            index,
                            index + 1
                          ),
                          updatedAt: new Date().toISOString(),
                        }))
                      }
                      size="sm"
                      variant="ghost"
                    >
                      <ChevronRight size={14} />
                    </Button>
                    <Button
                      aria-label={t`Remove cover`}
                      isIconOnly
                      onPress={() =>
                        onUpdate((current) => ({
                          ...current,
                          covers: current.covers.filter(
                            (candidate) => candidate.assetRef !== cover.assetRef
                          ),
                          updatedAt: new Date().toISOString(),
                        }))
                      }
                      size="sm"
                      variant="ghost"
                    >
                      <Trash2 size={14} />
                    </Button>
                  </div>
                </div>
              ))}
              {tasks.map((task) => (
                <CoverTaskPlaceholder
                  key={task.id}
                  onOpen={() => onOpenTask(task.id)}
                  status={publicationCoverTaskStatus(task)}
                />
              ))}
              {isUploading ? (
                <div className="op-publication-cover-task">
                  <span className="op-publication-cover-task__icon">
                    <Spinner size="sm" />
                  </span>
                  <strong>{t`Uploading media`}</strong>
                </div>
              ) : null}
            </div>
          ) : (
            <div className="op-typesetting-drop-empty">
              <span>{t`Drag Canvas assets or image/video files here to add covers.`}</span>
            </div>
          )}
        </div>
      </section>
      {previewedCover ? (
        <ImagePreviewDialog
          alt={previewedCover.alt}
          closeLabel={t`Close`}
          onClose={() => setPreviewedCover(null)}
          src={previewedCover.src}
        />
      ) : null}
    </>
  )
}

function PublicationCoverMedia({
  cover,
  src,
}: {
  cover: TypesettingPublicationImage
  src: string
}) {
  if (isTypesettingCoverVideo(cover)) {
    return (
      <video
        aria-label={cover.fileName}
        draggable={false}
        muted
        playsInline
        preload="metadata"
        src={src}
      />
    )
  }
  return <img alt={cover.fileName} draggable={false} src={src} />
}

function CoverTaskPlaceholder({
  onOpen,
  status,
}: {
  onOpen: () => void
  status: TypesettingCoverTaskDisplayStatus
}) {
  const { t } = useMyOpenPanelsI18n()
  const active =
    status === "waiting" || status === "running" || status === "saving"
  const label =
    status === "waiting"
      ? t`Waiting to create`
      : status === "running"
        ? t`Creating cover`
        : status === "saving"
          ? t`Saving cover`
          : status === "failed"
            ? t`Cover creation failed`
            : t`Cover creation cancelled`
  return (
    <button
      className={`is-${status} op-publication-cover-task`}
      onClick={onOpen}
      type="button"
    >
      <span className="op-publication-cover-task__icon">
        {active ? (
          <LoaderCircle className="op-spin" size={18} />
        ) : (
          <AlertCircle size={18} />
        )}
      </span>
      <strong>{label}</strong>
      <Chip
        color={status === "failed" ? "danger" : "default"}
        size="sm"
        variant="soft"
      >
        {t`Open task`}
      </Chip>
    </button>
  )
}
