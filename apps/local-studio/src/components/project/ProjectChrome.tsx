import { Button, Modal, Tabs } from "@heroui/react"
import { FileText, Palette, Pencil, Plus, Trash2 } from "lucide-react"
import { useCallback, useEffect, useRef, useState } from "react"
import {
  type Asset,
  type AssetStore,
  CanvasMenu,
  useOpenPanelsI18n,
} from "../../canvas"
import { apiFetch, apiUrl, fileToDataUrl } from "../../lib/api"
import type {
  OpenPanelsPanel,
  OpenPanelsPanelKind,
  OpenPanelsSession,
} from "../../protocol"

export function ProjectChrome({
  currentSession,
  sessions,
  onCreateProject,
  onDeleteProject,
  onRenameProject,
  onSwitchProject,
}: {
  currentSession: OpenPanelsSession
  onCreateProject: () => void
  onDeleteProject: (sessionId: string) => void
  onRenameProject: (title: string) => void
  onSwitchProject: (sessionId: string) => void
  sessions: OpenPanelsSession[]
}) {
  return (
    <>
      <CanvasMenu />
      <ProjectTitleControl
        currentSession={currentSession}
        onCreateProject={onCreateProject}
        onDeleteProject={onDeleteProject}
        onRenameProject={onRenameProject}
        onSwitchProject={onSwitchProject}
        sessions={sessions}
      />
    </>
  )
}

export function BottomPanelTabs({
  activePanelKind,
  panels,
  onSwitchPanel,
}: {
  activePanelKind: OpenPanelsPanelKind
  onSwitchPanel: (kind: OpenPanelsPanelKind) => void
  panels: OpenPanelsPanel[]
}) {
  const { t } = useOpenPanelsI18n()
  const visiblePanels = panels.filter(
    (panel) => panel.kind === "wiki" || panel.kind === "canvas"
  )
  return (
    <div className="op-panel-tabs">
      <Tabs
        className="op-panel-tabs__tabs"
        onSelectionChange={(key) =>
          onSwitchPanel(String(key) as OpenPanelsPanelKind)
        }
        selectedKey={activePanelKind}
      >
        <Tabs.ListContainer>
          <Tabs.List
            aria-label={t`Project panels`}
            className="op-panel-tabs__list"
          >
            {visiblePanels.map((panel, index) => (
              <Tabs.Tab
                className="op-panel-tabs__tab"
                id={panel.kind}
                key={panel.id}
              >
                {index > 0 ? <Tabs.Separator /> : null}
                {panel.kind === "wiki" ? (
                  <FileText size={15} strokeWidth={1.8} />
                ) : (
                  <Palette size={15} strokeWidth={1.8} />
                )}
                <span>{panel.kind === "wiki" ? t`Wiki` : t`Canvas`}</span>
                <Tabs.Indicator />
              </Tabs.Tab>
            ))}
          </Tabs.List>
        </Tabs.ListContainer>
      </Tabs>
    </div>
  )
}

function ProjectTitleControl({
  currentSession,
  sessions,
  onCreateProject,
  onDeleteProject,
  onRenameProject,
  onSwitchProject,
}: {
  currentSession: OpenPanelsSession
  onCreateProject: () => void
  onDeleteProject: (sessionId: string) => void
  onRenameProject: (title: string) => void
  onSwitchProject: (sessionId: string) => void
  sessions: OpenPanelsSession[]
}) {
  const { t } = useOpenPanelsI18n()
  const [isMenuOpen, setIsMenuOpen] = useState(false)
  const [isEditing, setIsEditing] = useState(false)
  const [pendingDeleteSession, setPendingDeleteSession] =
    useState<OpenPanelsSession | null>(null)
  const [draftTitle, setDraftTitle] = useState(currentSession.title)
  const closeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

  const clearCloseTimer = useCallback(() => {
    if (closeTimerRef.current) {
      clearTimeout(closeTimerRef.current)
      closeTimerRef.current = null
    }
  }, [])

  const openMenu = useCallback(() => {
    clearCloseTimer()
    setIsMenuOpen(true)
  }, [clearCloseTimer])

  const scheduleCloseMenu = useCallback(() => {
    clearCloseTimer()
    closeTimerRef.current = setTimeout(() => {
      setIsMenuOpen(false)
      closeTimerRef.current = null
    }, 180)
  }, [clearCloseTimer])

  useEffect(() => {
    if (!isEditing) {
      setDraftTitle(currentSession.title)
    }
  }, [currentSession.title, isEditing])

  useEffect(() => clearCloseTimer, [clearCloseTimer])

  const commitTitle = useCallback(() => {
    const nextTitle = draftTitle.trim()
    setIsEditing(false)
    setIsMenuOpen(false)
    if (nextTitle && nextTitle !== currentSession.title) {
      onRenameProject(nextTitle)
    } else {
      setDraftTitle(currentSession.title)
    }
  }, [currentSession.title, draftTitle, onRenameProject])

  const confirmDeleteProject = useCallback(() => {
    if (!pendingDeleteSession) return
    setIsMenuOpen(false)
    onDeleteProject(pendingDeleteSession.id)
    setPendingDeleteSession(null)
  }, [onDeleteProject, pendingDeleteSession])

  if (isEditing) {
    return (
      <div className="op-project-title op-project-title--editing">
        <input
          aria-label={t`Project name`}
          autoFocus
          className="op-project-title__input"
          onBlur={commitTitle}
          onChange={(event) => setDraftTitle(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault()
              commitTitle()
            }
            if (event.key === "Escape") {
              event.preventDefault()
              setDraftTitle(currentSession.title)
              setIsEditing(false)
              setIsMenuOpen(false)
            }
          }}
          value={draftTitle}
        />
      </div>
    )
  }

  return (
    <div
      className="op-project-title"
      onMouseEnter={openMenu}
      onMouseLeave={scheduleCloseMenu}
    >
      <Button
        className="op-project-title__trigger"
        onPress={() => setIsMenuOpen((open) => !open)}
        variant="ghost"
      >
        <span>{currentSession.title}</span>
      </Button>
      <Button
        aria-label={t`Rename project`}
        className="op-project-title__edit-button"
        isIconOnly
        onPress={() => {
          setIsMenuOpen(false)
          setIsEditing(true)
        }}
        size="sm"
        variant="ghost"
      >
        <Pencil size={14} strokeWidth={1.8} />
      </Button>

      {isMenuOpen ? (
        <div className="op-project-title__menu">
          <div className="op-project-title__menu-header">{t`Projects`}</div>
          <div className="op-project-title__menu-list">
            {sessions.map((session) => {
              const isActive = session.id === currentSession.id
              const canDelete = sessions.length > 1
              return (
                <div
                  className={
                    isActive
                      ? "op-project-title__menu-item op-project-title__menu-item--active"
                      : "op-project-title__menu-item"
                  }
                  key={session.id}
                >
                  <Button
                    className="op-project-title__switch-button"
                    onPress={() => {
                      setIsMenuOpen(false)
                      if (!isActive) {
                        onSwitchProject(session.id)
                      }
                    }}
                    variant="ghost"
                  >
                    <span>{session.title}</span>
                  </Button>
                  <span
                    className="op-project-title__delete-wrap"
                    title={
                      canDelete
                        ? t`Delete project`
                        : t`Keep at least one project`
                    }
                  >
                    <Button
                      aria-disabled={!canDelete}
                      aria-label={t`Delete project`}
                      className="op-project-title__delete-button"
                      isIconOnly
                      onPress={() => {
                        if (!canDelete) return
                        setIsMenuOpen(false)
                        setPendingDeleteSession(session)
                      }}
                      size="sm"
                      variant="ghost"
                    >
                      <Trash2 size={14} strokeWidth={1.8} />
                    </Button>
                  </span>
                </div>
              )
            })}
          </div>
          <Button
            className="op-project-title__menu-item op-project-title__menu-item--create"
            onPress={() => {
              setIsMenuOpen(false)
              onCreateProject()
            }}
            variant="ghost"
          >
            <Plus size={14} />
            <span>{t`New project`}</span>
          </Button>
        </div>
      ) : null}

      {pendingDeleteSession ? (
        <Modal.Backdrop
          isOpen
          onOpenChange={(isOpen) => {
            if (!isOpen) setPendingDeleteSession(null)
          }}
        >
          <Modal.Container placement="center">
            <Modal.Dialog className="op-project-delete-dialog">
              <Modal.Header>
                <Modal.Heading>{t`Delete project?`}</Modal.Heading>
              </Modal.Header>
              <Modal.Body>
                <p>
                  {t`Deleting this project removes all content in the current project, including every Wiki page and everything on the canvas. This cannot be undone.`}
                </p>
                <div className="op-project-title__confirm-name">
                  {pendingDeleteSession.title}
                </div>
              </Modal.Body>
              <Modal.Footer>
                <Button
                  onPress={() => setPendingDeleteSession(null)}
                  variant="secondary"
                >
                  {t`Cancel`}
                </Button>
                <Button onPress={confirmDeleteProject} variant="danger">
                  {t`Delete`}
                </Button>
              </Modal.Footer>
            </Modal.Dialog>
          </Modal.Container>
        </Modal.Backdrop>
      ) : null}
    </div>
  )
}

export class OpenPanelsBrowserAssetStore implements AssetStore {
  private readonly apiBase: string
  private readonly panelId: string
  private readonly sessionId: string

  constructor(apiBase: string, sessionId: string, panelId: string) {
    this.apiBase = apiBase
    this.sessionId = sessionId
    this.panelId = panelId
  }

  async upload(_asset: Partial<Asset>, file: File) {
    const dataUrl = await fileToDataUrl(file)
    const response = await apiFetch(
      this.apiBase,
      `/api/panels/${encodeURIComponent(this.sessionId)}/${encodeURIComponent(this.panelId)}/assets`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          dataUrl,
          fileName: file.name || "image.png",
          mimeType: file.type || "image/png",
        }),
      }
    )
    return (await response.json()) as {
      meta?: Record<string, unknown>
      mimeType?: string
      src: string
    }
  }

  resolve(asset: Asset): string {
    if (!("src" in asset.props)) return ""
    const src = asset.props.src
    if (typeof src !== "string" || !src.startsWith("/")) return src
    return apiUrl(this.apiBase, src).toString()
  }

  download(asset: Asset): Promise<string> {
    return Promise.resolve(this.resolve(asset))
  }
}
