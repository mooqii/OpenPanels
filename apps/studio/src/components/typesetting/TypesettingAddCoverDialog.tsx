import { Button, Chip, Modal, Spinner, Tabs } from "@heroui/react"
import { Check, Images, Upload } from "lucide-react"
import {
  type ClipboardEvent,
  type DragEvent,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { apiJson, apiUrl } from "../../lib/api"
import {
  groupTypesettingAssets,
  isSupportedTypesettingCoverImage,
  TYPESETTING_COVER_IMAGE_ACCEPT,
} from "../../lib/typesetting"
import type {
  MyOpenPanelsTransport,
  TypesettingCanvasAsset,
  TypesettingPublicationImage,
} from "../../types"

type AddCoverTab = "canvas" | "upload"
type ImagePickerPurpose = "content" | "cover"

export function TypesettingAddCoverDialog({
  importAsset,
  isOpen,
  onAdd,
  onOpenChange,
  projectId,
  purpose = "cover",
  transport,
  uploadAsset,
}: {
  importAsset: (
    asset: TypesettingCanvasAsset
  ) => Promise<TypesettingPublicationImage>
  isOpen: boolean
  onAdd: (images: TypesettingPublicationImage[]) => void
  onOpenChange: (open: boolean) => void
  projectId: string
  purpose?: ImagePickerPurpose
  transport: MyOpenPanelsTransport
  uploadAsset: (file: File) => Promise<TypesettingPublicationImage>
}) {
  const { t } = useMyOpenPanelsI18n()
  const inputRef = useRef<HTMLInputElement>(null)
  const [tab, setTab] = useState<AddCoverTab>("canvas")
  const [assets, setAssets] = useState<TypesettingCanvasAsset[]>([])
  const [selectedAssetIds, setSelectedAssetIds] = useState<Set<string>>(
    () => new Set()
  )
  const [isLoading, setIsLoading] = useState(false)
  const [isAdding, setIsAdding] = useState(false)
  const [isDropActive, setIsDropActive] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!isOpen) return
    let cancelled = false
    setTab("canvas")
    setAssets([])
    setSelectedAssetIds(new Set())
    setError(null)
    setIsLoading(true)
    const url = new URL(apiUrl(transport.apiBase, "/api/assets/canvas"))
    url.searchParams.set("projectId", projectId)
    url.searchParams.set("scope", "all")
    apiJson<{ assets?: TypesettingCanvasAsset[] }>(transport.apiBase, url)
      .then((response) => {
        if (!cancelled) setAssets(response.assets ?? [])
      })
      .catch((loadError) => {
        if (!cancelled) {
          setError(
            String(loadError instanceof Error ? loadError.message : loadError)
          )
        }
      })
      .finally(() => {
        if (!cancelled) setIsLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [isOpen, projectId, transport.apiBase])

  const groups = useMemo(
    () =>
      groupTypesettingAssets(assets).sort(
        (left, right) =>
          Number(right.projectId === projectId) -
          Number(left.projectId === projectId)
      ),
    [assets, projectId]
  )
  const selectedAssets = useMemo(
    () => assets.filter((asset) => selectedAssetIds.has(asset.id)),
    [assets, selectedAssetIds]
  )
  const isBusy = isLoading || isAdding
  const heading = purpose === "content" ? t`Insert images` : t`Add cover images`
  const sourceLabel =
    purpose === "content" ? t`Image source` : t`Add cover image source`
  const confirmLabel =
    purpose === "content" ? t`Insert selected images` : t`Add selected images`

  const addCanvasAssets = async () => {
    if (isAdding || selectedAssets.length === 0) return
    setIsAdding(true)
    setError(null)
    const added: TypesettingPublicationImage[] = []
    let failed = false
    for (const asset of selectedAssets) {
      try {
        added.push(await importAsset(asset))
      } catch {
        failed = true
      }
    }
    if (added.length > 0) onAdd(added)
    setIsAdding(false)
    if (failed) setError(t`Failed to add some images`)
    else onOpenChange(false)
  }

  const uploadFiles = async (files: Iterable<File>) => {
    if (isAdding) return
    const supported = Array.from(files).filter(isSupportedTypesettingCoverImage)
    if (supported.length === 0) return
    setIsAdding(true)
    setIsDropActive(false)
    setError(null)
    const added: TypesettingPublicationImage[] = []
    let failed = false
    for (const file of supported) {
      try {
        added.push(await uploadAsset(file))
      } catch {
        failed = true
      }
    }
    if (added.length > 0) onAdd(added)
    setIsAdding(false)
    if (failed) setError(t`Failed to upload some images`)
    else onOpenChange(false)
  }

  const pasteImages = (event: ClipboardEvent<HTMLElement>) => {
    if (tab !== "upload" || isAdding) return
    const files = Array.from(event.clipboardData.items).flatMap((item) => {
      const file = item.kind === "file" ? item.getAsFile() : null
      return file ? [file] : []
    })
    if (!files.some(isSupportedTypesettingCoverImage)) return
    event.preventDefault()
    uploadFiles(files).catch(() => undefined)
  }

  if (!isOpen) return null

  return (
    <Modal.Backdrop
      isOpen
      onOpenChange={(open) => {
        if (!(open || isBusy)) onOpenChange(false)
      }}
    >
      <Modal.Container placement="center" size="lg">
        <Modal.Dialog className="op-typesetting-add-cover-dialog">
          <Modal.CloseTrigger aria-label={t`Close`} />
          <Modal.Header>
            <Modal.Icon>
              <Images size={19} />
            </Modal.Icon>
            <Modal.Heading>{heading}</Modal.Heading>
          </Modal.Header>
          <Modal.Body>
            <div
              className="op-typesetting-add-cover-dialog__content"
              onPaste={pasteImages}
            >
              <Tabs
                className="op-typesetting-add-cover-tabs"
                onSelectionChange={(key) => {
                  setTab(key === "upload" ? "upload" : "canvas")
                  setError(null)
                }}
                selectedKey={tab}
                variant="secondary"
              >
                <Tabs.ListContainer>
                  <Tabs.List aria-label={sourceLabel}>
                    <Tabs.Tab id="canvas">
                      {t`From Canvas`}
                      <Tabs.Indicator />
                    </Tabs.Tab>
                    <Tabs.Tab id="upload">
                      {t`Upload`}
                      <Tabs.Indicator />
                    </Tabs.Tab>
                  </Tabs.List>
                </Tabs.ListContainer>
                <Tabs.Panel id="canvas">
                  <div className="op-publication-cover-picker">
                    {isLoading ? (
                      <div className="op-publication-cover-picker__empty">
                        <Spinner size="sm" />
                        <span>{t`Loading assets`}</span>
                      </div>
                    ) : groups.length ? (
                      groups.map((group) => (
                        <section
                          className="op-publication-cover-picker__group"
                          key={group.projectId}
                        >
                          <div className="op-publication-cover-picker__group-title">
                            <strong>{group.projectTitle}</strong>
                            {group.projectId === projectId ? (
                              <Chip color="accent" size="sm" variant="soft">
                                {t`Current project`}
                              </Chip>
                            ) : null}
                          </div>
                          <div className="op-publication-cover-picker__grid">
                            {group.assets.map((asset) => {
                              const selected = selectedAssetIds.has(asset.id)
                              return (
                                <button
                                  aria-label={asset.name}
                                  aria-pressed={selected}
                                  className={
                                    selected
                                      ? "is-selected op-publication-cover-picker__asset"
                                      : "op-publication-cover-picker__asset"
                                  }
                                  key={asset.id}
                                  onClick={() => {
                                    setSelectedAssetIds((current) => {
                                      const next = new Set(current)
                                      if (next.has(asset.id))
                                        next.delete(asset.id)
                                      else next.add(asset.id)
                                      return next
                                    })
                                  }}
                                  title={asset.name}
                                  type="button"
                                >
                                  <img
                                    alt=""
                                    src={apiUrl(
                                      transport.apiBase,
                                      asset.src
                                    ).toString()}
                                  />
                                  <span className="op-publication-cover-picker__check">
                                    <Check size={14} />
                                  </span>
                                </button>
                              )
                            })}
                          </div>
                        </section>
                      ))
                    ) : (
                      <div className="op-publication-cover-picker__empty">
                        {error ?? t`No Canvas images yet`}
                      </div>
                    )}
                  </div>
                </Tabs.Panel>
                <Tabs.Panel id="upload">
                  <input
                    accept={TYPESETTING_COVER_IMAGE_ACCEPT}
                    hidden
                    multiple
                    onChange={(event) => {
                      uploadFiles(event.currentTarget.files ?? []).catch(
                        () => undefined
                      )
                      event.currentTarget.value = ""
                    }}
                    ref={inputRef}
                    type="file"
                  />
                  <div
                    className={
                      isDropActive
                        ? "is-active op-publication-cover-upload"
                        : "op-publication-cover-upload"
                    }
                    onDragLeave={() => setIsDropActive(false)}
                    onDragOver={(event: DragEvent<HTMLDivElement>) => {
                      if (!event.dataTransfer.types.includes("Files")) return
                      event.preventDefault()
                      event.dataTransfer.dropEffect = "copy"
                      setIsDropActive(true)
                    }}
                    onDrop={(event: DragEvent<HTMLDivElement>) => {
                      event.preventDefault()
                      uploadFiles(event.dataTransfer.files).catch(
                        () => undefined
                      )
                    }}
                  >
                    {isAdding ? (
                      <>
                        <Spinner size="md" />
                        <strong>{t`Uploading images`}</strong>
                      </>
                    ) : (
                      <>
                        <span className="op-publication-cover-upload__icon">
                          <Upload size={22} />
                        </span>
                        <strong>{t`Drag or paste images here`}</strong>
                        <span>{t`PNG, JPEG, WebP, GIF`}</span>
                        <Button
                          onPress={() => inputRef.current?.click()}
                          size="sm"
                          variant="secondary"
                        >
                          <Upload size={15} />
                          {t`Upload images`}
                        </Button>
                      </>
                    )}
                  </div>
                </Tabs.Panel>
              </Tabs>
            </div>
            {error && (tab === "upload" || groups.length) ? (
              <div className="op-publication-cover-picker__error" role="alert">
                {error}
              </div>
            ) : null}
          </Modal.Body>
          <Modal.Footer>
            <Button
              isDisabled={isAdding}
              onPress={() => onOpenChange(false)}
              variant="secondary"
            >
              {t`Cancel`}
            </Button>
            {tab === "canvas" ? (
              <Button
                isDisabled={isBusy || selectedAssets.length === 0}
                onPress={() => addCanvasAssets().catch(() => undefined)}
                variant="primary"
              >
                {isAdding ? <Spinner size="sm" /> : null}
                {confirmLabel} ({selectedAssets.length})
              </Button>
            ) : null}
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}
