import { Button, Tabs } from "@heroui/react"
import {
  FileText,
  Image as ImageIcon,
  LoaderCircle,
  Plus,
  X,
} from "lucide-react"
import { type ReactNode, useEffect, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { apiJson, apiUrl } from "../../lib/api"
import { formatRelativeOrDate } from "../../lib/date-time"
import {
  countTypesettingCharacters,
  groupTypesettingAssets,
  TYPESETTING_ASSET_DRAG_TYPE,
} from "../../lib/typesetting"
import type {
  MyOpenPanelsTransport,
  TypesettingCanvasAsset,
  TypesettingPublication,
  WikiGeneratedDocument,
  WikiRawDocument,
  WikiState,
} from "../../types"
import {
  GeneratedDocumentsEmpty,
  RawDocumentsEmpty,
} from "../wiki/DocumentModuleEmpty"
import { GeneratedDocumentMeta } from "../wiki/GeneratedDocumentMeta"
import { RawDocumentMeta } from "../wiki/RawDocumentMeta"
import {
  nextCollapsedLibraryModules,
  type TypesettingLibraryModule,
} from "./library-accordion"

type AssetScope = "current" | "all"
export function TypesettingLibrary({
  activePublicationId,
  className,
  onClose,
  onCreatePublication,
  onOpenGenerated,
  onOpenPublication,
  onOpenRaw,
  onOpenRawOriginal,
  projectId,
  publications,
  transport,
  wiki,
}: {
  activePublicationId: string | null
  className: string
  onClose: () => void
  onCreatePublication: () => void
  onOpenGenerated: (document: WikiGeneratedDocument) => void
  onOpenPublication: (publication: TypesettingPublication) => void
  onOpenRaw: (document: WikiRawDocument) => void
  onOpenRawOriginal: (document: WikiRawDocument) => void
  projectId: string
  publications: TypesettingPublication[]
  transport: MyOpenPanelsTransport
  wiki: WikiState
}) {
  const { t } = useMyOpenPanelsI18n()
  const [scope, setScope] = useState<AssetScope>("current")
  const [assets, setAssets] = useState<TypesettingCanvasAsset[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [collapsedLibraryModules, setCollapsedLibraryModules] = useState<
    Set<TypesettingLibraryModule>
  >(() => new Set())

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError(null)
    const url = new URL(
      apiUrl(transport.apiBase, "/api/typesetting/canvas-assets")
    )
    url.searchParams.set("projectId", projectId)
    url.searchParams.set("scope", scope)
    apiJson<{ assets?: TypesettingCanvasAsset[] }>(transport.apiBase, url)
      .then((data) => {
        if (!cancelled) setAssets(data.assets ?? [])
      })
      .catch((loadError) => {
        if (!cancelled) {
          setAssets([])
          setError(
            String(loadError instanceof Error ? loadError.message : loadError)
          )
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [projectId, scope, transport.apiBase])

  const groups = groupTypesettingAssets(assets)
  const toggleLibraryModule = (module: TypesettingLibraryModule) => {
    setCollapsedLibraryModules((current) =>
      nextCollapsedLibraryModules(current, module)
    )
  }

  return (
    <aside className={`op-typesetting-library ${className}`}>
      <div className="op-typesetting-library__mobile-header">
        <strong>{t`Documents and assets`}</strong>
        <Button
          aria-label={t`Close library`}
          isIconOnly
          onPress={onClose}
          size="sm"
          variant="ghost"
        >
          <X size={16} />
        </Button>
      </div>
      <div className="op-typesetting-document-library">
        <PublicationContentModule
          activePublicationId={activePublicationId}
          isCollapsed={collapsedLibraryModules.has("publications")}
          onCreatePublication={() => {
            setCollapsedLibraryModules((current) => {
              const next = new Set(current)
              next.delete("publications")
              return next
            })
            onCreatePublication()
          }}
          onOpenPublication={onOpenPublication}
          onToggle={() => toggleLibraryModule("publications")}
          publications={publications}
          transport={transport}
        />

        <LibraryModule
          isCollapsed={collapsedLibraryModules.has("raw")}
          isEmpty={wiki.rawDocuments.length === 0}
          onToggle={() => toggleLibraryModule("raw")}
          title={t`Raw Documents`}
        >
          {wiki.rawDocuments.length ? (
            wiki.rawDocuments.map((document) => (
              <div className="op-typesetting-document" key={document.id}>
                <button
                  aria-label={document.title}
                  className="op-raw-document-open"
                  onClick={() => onOpenRaw(document)}
                  type="button"
                />
                <FileText size={15} />
                <span className="op-raw-document-copy">
                  <strong>{document.title}</strong>
                  <RawDocumentMeta
                    document={document}
                    onOpenOriginal={() => onOpenRawOriginal(document)}
                  />
                </span>
              </div>
            ))
          ) : (
            <RawDocumentsEmpty />
          )}
        </LibraryModule>

        <LibraryModule
          isCollapsed={collapsedLibraryModules.has("generated")}
          isEmpty={wiki.generatedDocuments.length === 0}
          onToggle={() => toggleLibraryModule("generated")}
          title={t`Generated Documents`}
        >
          {wiki.generatedDocuments.length ? (
            wiki.generatedDocuments.map((document) => (
              <button
                className="op-typesetting-document"
                key={document.id}
                onClick={() => onOpenGenerated(document)}
                type="button"
              >
                <FileText size={15} />
                <span>
                  <strong>{document.title}</strong>
                  <GeneratedDocumentMeta
                    apiBase={transport.apiBase}
                    document={document}
                  />
                </span>
              </button>
            ))
          ) : (
            <GeneratedDocumentsEmpty />
          )}
        </LibraryModule>

        <LibraryModule
          className="op-typesetting-assets-module"
          isCollapsed={collapsedLibraryModules.has("assets")}
          onToggle={() => toggleLibraryModule("assets")}
          title={t`Assets`}
        >
          <Tabs
            className="op-typesetting-assets-tabs"
            onSelectionChange={(key) => setScope(String(key) as AssetScope)}
            selectedKey={scope}
          >
            <Tabs.ListContainer>
              <Tabs.List aria-label={t`Asset scope`}>
                <Tabs.Tab id="current">
                  {t`Current project`}
                  <Tabs.Indicator />
                </Tabs.Tab>
                <Tabs.Tab id="all">
                  {t`All projects`}
                  <Tabs.Indicator />
                </Tabs.Tab>
              </Tabs.List>
            </Tabs.ListContainer>
          </Tabs>
          <div className="op-typesetting-assets">
            {loading ? (
              <LibraryEmpty>
                <LoaderCircle className="op-spin" size={16} />
                {t`Loading assets`}
              </LibraryEmpty>
            ) : error ? (
              <LibraryEmpty>{t`Failed to load assets`}</LibraryEmpty>
            ) : groups.length ? (
              groups.map((group) => (
                <section
                  className="op-typesetting-asset-group"
                  key={group.projectId}
                >
                  {scope === "all" ? <h4>{group.projectTitle}</h4> : null}
                  <div className="op-typesetting-asset-grid">
                    {group.assets.map((asset) => (
                      <button
                        className="op-typesetting-asset"
                        draggable
                        key={asset.id}
                        onDragStart={(event) => {
                          event.dataTransfer.effectAllowed = "copy"
                          event.dataTransfer.setData(
                            TYPESETTING_ASSET_DRAG_TYPE,
                            JSON.stringify(asset)
                          )
                          event.dataTransfer.setData(
                            "text/uri-list",
                            apiUrl(transport.apiBase, asset.src).toString()
                          )
                        }}
                        title={asset.name}
                        type="button"
                      >
                        <img
                          alt={asset.name}
                          draggable={false}
                          src={apiUrl(transport.apiBase, asset.src).toString()}
                        />
                      </button>
                    ))}
                  </div>
                </section>
              ))
            ) : (
              <LibraryEmpty>{t`No Canvas images yet`}</LibraryEmpty>
            )}
          </div>
        </LibraryModule>
      </div>
    </aside>
  )
}

function LibraryModule({
  action,
  children,
  className = "",
  isCollapsed,
  isEmpty = false,
  onToggle,
  title,
}: {
  action?: ReactNode
  children: ReactNode
  className?: string
  isCollapsed: boolean
  isEmpty?: boolean
  onToggle: () => void
  title: string
}) {
  return (
    <section
      className={
        isCollapsed
          ? `is-collapsed op-typesetting-library-module ${className}`.trim()
          : `op-typesetting-library-module ${className}`.trim()
      }
    >
      <div className="op-typesetting-library-module__header">
        <button aria-expanded={!isCollapsed} onClick={onToggle} type="button">
          <h3 className="op-typesetting-library-module__title">{title}</h3>
        </button>
        {action}
      </div>
      <div
        className={
          isEmpty
            ? "is-empty op-typesetting-library-module__content"
            : "op-typesetting-library-module__content"
        }
      >
        {children}
      </div>
    </section>
  )
}

function LibraryEmpty({ children }: { children: ReactNode }) {
  return <div className="op-typesetting-library-empty">{children}</div>
}

export function PublicationContentModule({
  activePublicationId,
  className = "",
  isCollapsed,
  onCreatePublication,
  onOpenPublication,
  onToggle,
  publications,
  renderPublicationMeta,
  renderPublicationStatus,
  transport,
}: {
  activePublicationId: string | null
  className?: string
  isCollapsed: boolean
  onCreatePublication?: () => void
  onOpenPublication: (publication: TypesettingPublication) => void
  onToggle: () => void
  publications: TypesettingPublication[]
  renderPublicationMeta?: (publication: TypesettingPublication) => ReactNode
  renderPublicationStatus?: (publication: TypesettingPublication) => ReactNode
  transport: MyOpenPanelsTransport
}) {
  const { t } = useMyOpenPanelsI18n()

  return (
    <LibraryModule
      action={
        onCreatePublication ? (
          <Button onPress={onCreatePublication} size="sm" variant="primary">
            <Plus size={14} />
            {t`New`}
          </Button>
        ) : undefined
      }
      className={className}
      isCollapsed={isCollapsed}
      isEmpty={publications.length === 0}
      onToggle={onToggle}
      title={t`Publication content`}
    >
      <PublicationList
        activePublicationId={activePublicationId}
        onOpen={onOpenPublication}
        publications={publications}
        renderMeta={renderPublicationMeta}
        renderStatus={renderPublicationStatus}
        transport={transport}
      />
    </LibraryModule>
  )
}

export function PublicationList({
  activePublicationId,
  onOpen,
  publications,
  renderMeta,
  renderStatus,
  transport,
}: {
  activePublicationId: string | null
  onOpen: (publication: TypesettingPublication) => void
  publications: TypesettingPublication[]
  renderMeta?: (publication: TypesettingPublication) => ReactNode
  renderStatus?: (publication: TypesettingPublication) => ReactNode
  transport: MyOpenPanelsTransport
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 60_000)
    return () => window.clearInterval(timer)
  }, [])

  return (
    <>
      {publications.length ? (
        <div className="op-typesetting-publication-list">
          {publications.map((publication) => (
            <button
              className="op-typesetting-publication-row"
              data-selected={
                publication.id === activePublicationId || undefined
              }
              key={publication.id}
              onClick={() => onOpen(publication)}
              type="button"
            >
              <span className="op-typesetting-publication-row__cover">
                {publication.covers[0] ? (
                  <img
                    alt=""
                    src={apiUrl(
                      transport.apiBase,
                      publication.covers[0].src
                    ).toString()}
                  />
                ) : (
                  <ImageIcon size={16} />
                )}
              </span>
              <span className="op-typesetting-publication-row__text">
                <span className="op-typesetting-publication-row__title">
                  <strong>
                    {publication.title.trim() || t`Untitled publication`}
                  </strong>
                  {renderStatus ? (
                    <span className="op-typesetting-publication-row__statuses">
                      {renderStatus(publication)}
                    </span>
                  ) : null}
                </span>
                <small className="op-typesetting-publication-row__meta">
                  <span>
                    {publication.covers.length.toLocaleString(locale)}{" "}
                    {publication.covers.length === 1
                      ? t`cover image`
                      : t`cover images`}
                  </span>
                  <span>
                    {countTypesettingCharacters(
                      publication.content
                    ).toLocaleString(locale)}{" "}
                    {t`characters`}
                  </span>
                  <span>
                    {formatRelativeOrDate(publication.updatedAt, locale, now)}
                  </span>
                  {renderMeta?.(publication)}
                </small>
              </span>
            </button>
          ))}
        </div>
      ) : (
        <div className="op-typesetting-list-empty">
          {t`No publication projects yet`}
        </div>
      )}
    </>
  )
}
