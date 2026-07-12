import { Button, Modal, Tabs } from "@heroui/react"
import { FileText, Palette, Pencil, Plus, Trash2 } from "lucide-react"
import { useCallback, useEffect, useRef, useState } from "react"
import {
  type Asset,
  type AssetStore,
  CanvasMenu,
  useMyOpenPanelsI18n,
} from "../../canvas"
import { apiFetch, apiUrl, fileToDataUrl } from "../../lib/api"
import type {
  MyOpenPanelsPanel,
  MyOpenPanelsPanelKind,
  MyOpenPanelsProject,
} from "../../protocol"

export function ProjectChrome({
  currentProject,
  projects,
  onCreateProject,
  onDeleteProject,
  onRenameProject,
  onSwitchProject,
}: {
  currentProject: MyOpenPanelsProject
  onCreateProject: () => void
  onDeleteProject: (projectId: string) => void
  onRenameProject: (title: string) => void
  onSwitchProject: (projectId: string) => void
  projects: MyOpenPanelsProject[]
}) {
  return (
    <>
      <CanvasMenu />
      <ProjectTitleControl
        currentProject={currentProject}
        onCreateProject={onCreateProject}
        onDeleteProject={onDeleteProject}
        onRenameProject={onRenameProject}
        onSwitchProject={onSwitchProject}
        projects={projects}
      />
    </>
  )
}

export function BottomPanelTabs({
  activePanelKind,
  panels,
  onSwitchPanel,
}: {
  activePanelKind: MyOpenPanelsPanelKind
  onSwitchPanel: (kind: MyOpenPanelsPanelKind) => void
  panels: MyOpenPanelsPanel[]
}) {
  const { t } = useMyOpenPanelsI18n()
  const visiblePanels = panels.filter(
    (panel) => panel.kind === "wiki" || panel.kind === "canvas"
  )
  return (
    <div className="op-panel-tabs">
      <Tabs
        className="op-panel-tabs__tabs"
        onSelectionChange={(key) =>
          onSwitchPanel(String(key) as MyOpenPanelsPanelKind)
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
  currentProject,
  projects,
  onCreateProject,
  onDeleteProject,
  onRenameProject,
  onSwitchProject,
}: {
  currentProject: MyOpenPanelsProject
  onCreateProject: () => void
  onDeleteProject: (projectId: string) => void
  onRenameProject: (title: string) => void
  onSwitchProject: (projectId: string) => void
  projects: MyOpenPanelsProject[]
}) {
  const { t } = useMyOpenPanelsI18n()
  const [isMenuOpen, setIsMenuOpen] = useState(false)
  const [isEditing, setIsEditing] = useState(false)
  const [pendingDeleteProject, setPendingDeleteProject] =
    useState<MyOpenPanelsProject | null>(null)
  const [draftTitle, setDraftTitle] = useState(currentProject.title)
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
      setDraftTitle(currentProject.title)
    }
  }, [currentProject.title, isEditing])

  useEffect(() => clearCloseTimer, [clearCloseTimer])

  const commitTitle = useCallback(() => {
    const nextTitle = draftTitle.trim()
    setIsEditing(false)
    setIsMenuOpen(false)
    if (nextTitle && nextTitle !== currentProject.title) {
      onRenameProject(nextTitle)
    } else {
      setDraftTitle(currentProject.title)
    }
  }, [currentProject.title, draftTitle, onRenameProject])

  const confirmDeleteProject = useCallback(() => {
    if (!pendingDeleteProject) return
    setIsMenuOpen(false)
    onDeleteProject(pendingDeleteProject.id)
    setPendingDeleteProject(null)
  }, [onDeleteProject, pendingDeleteProject])

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
              setDraftTitle(currentProject.title)
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
        <span>{currentProject.title}</span>
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
            {projects.map((project) => {
              const isActive = project.id === currentProject.id
              const canDelete = projects.length > 1
              return (
                <div
                  className={
                    isActive
                      ? "op-project-title__menu-item op-project-title__menu-item--active"
                      : "op-project-title__menu-item"
                  }
                  key={project.id}
                >
                  <Button
                    className="op-project-title__switch-button"
                    onPress={() => {
                      setIsMenuOpen(false)
                      if (!isActive) {
                        onSwitchProject(project.id)
                      }
                    }}
                    variant="ghost"
                  >
                    <span>{project.title}</span>
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
                        setPendingDeleteProject(project)
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

      {pendingDeleteProject ? (
        <Modal.Backdrop
          isOpen
          onOpenChange={(isOpen) => {
            if (!isOpen) setPendingDeleteProject(null)
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
                  {pendingDeleteProject.title}
                </div>
              </Modal.Body>
              <Modal.Footer>
                <Button
                  onPress={() => setPendingDeleteProject(null)}
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

export class MyOpenPanelsBrowserAssetStore implements AssetStore {
  private readonly apiBase: string
  private readonly panelId: string
  private readonly projectId: string

  constructor(apiBase: string, projectId: string, panelId: string) {
    this.apiBase = apiBase
    this.projectId = projectId
    this.panelId = panelId
  }

  async upload(_asset: Partial<Asset>, file: File) {
    const dataUrl = await fileToDataUrl(file)
    const response = await apiFetch(
      this.apiBase,
      `/api/projects/${encodeURIComponent(this.projectId)}/panels/${encodeURIComponent(this.panelId)}/assets`,
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
