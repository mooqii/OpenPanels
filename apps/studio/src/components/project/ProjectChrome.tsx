import {
  AlertDialog,
  Button,
  Dropdown,
  Header,
  Input,
  Label,
  Separator,
  Tabs,
} from "@heroui/react"
import {
  FileText,
  LayoutTemplate,
  Palette,
  Pencil,
  PenLine,
  Plus,
  Send,
  Trash2,
} from "lucide-react"
import { useCallback, useEffect, useState } from "react"
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
  onOpenModelSettings,
  onOpenSkillManager,
  onRenameProject,
  onSwitchProject,
}: {
  currentProject: MyOpenPanelsProject
  onCreateProject: () => void
  onDeleteProject: (projectId: string) => void
  onOpenModelSettings: () => void
  onOpenSkillManager: () => void
  onRenameProject: (title: string) => void
  onSwitchProject: (projectId: string) => void
  projects: MyOpenPanelsProject[]
}) {
  return (
    <>
      <CanvasMenu
        onOpenModelSettings={onOpenModelSettings}
        onOpenSkillManager={onOpenSkillManager}
      />
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
  const { locale, t } = useMyOpenPanelsI18n()
  const visiblePanels = panels.filter(
    (panel) =>
      panel.kind === "wiki" ||
      panel.kind === "writing" ||
      panel.kind === "canvas" ||
      panel.kind === "typesetting" ||
      panel.kind === "publishing"
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
          <Tabs.List aria-label={t`Project panels`}>
            {visiblePanels.map((panel) => (
              <Tabs.Tab
                className="op-panel-tabs__tab"
                id={panel.kind}
                key={panel.id}
              >
                {panel.kind === "wiki" ? (
                  <FileText size={15} strokeWidth={1.8} />
                ) : panel.kind === "writing" ? (
                  <PenLine size={15} strokeWidth={1.8} />
                ) : panel.kind === "typesetting" ? (
                  <LayoutTemplate size={15} strokeWidth={1.8} />
                ) : panel.kind === "publishing" ? (
                  <Send size={15} strokeWidth={1.8} />
                ) : (
                  <Palette size={15} strokeWidth={1.8} />
                )}
                <span>
                  {panel.kind === "wiki"
                    ? locale === "zh-CN"
                      ? "文档"
                      : "Wiki"
                    : panel.kind === "writing"
                      ? t`Writing`
                      : panel.kind === "typesetting"
                        ? t`Typeset`
                        : panel.kind === "publishing"
                          ? t`Publish`
                          : t`Canvas`}
                </span>
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
  const [isEditing, setIsEditing] = useState(false)
  const [pendingDeleteProject, setPendingDeleteProject] =
    useState<MyOpenPanelsProject | null>(null)
  const [draftTitle, setDraftTitle] = useState(currentProject.title)

  useEffect(() => {
    if (!isEditing) {
      setDraftTitle(currentProject.title)
    }
  }, [currentProject.title, isEditing])

  const commitTitle = useCallback(() => {
    const nextTitle = draftTitle.trim()
    setIsEditing(false)
    if (nextTitle && nextTitle !== currentProject.title) {
      onRenameProject(nextTitle)
    } else {
      setDraftTitle(currentProject.title)
    }
  }, [currentProject.title, draftTitle, onRenameProject])

  const confirmDeleteProject = useCallback(() => {
    if (!pendingDeleteProject) return
    onDeleteProject(pendingDeleteProject.id)
    setPendingDeleteProject(null)
  }, [onDeleteProject, pendingDeleteProject])

  if (isEditing) {
    return (
      <div className="op-project-title op-project-title--editing">
        <Input
          aria-label={t`Project name`}
          autoFocus
          className="min-w-0 max-w-90"
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
            }
          }}
          value={draftTitle}
        />
      </div>
    )
  }

  return (
    <div className="op-project-title">
      <div className="op-project-title__activation">
        <Dropdown>
          <Button className="op-project-title__trigger" variant="ghost">
            <span>{currentProject.title}</span>
          </Button>
          <Dropdown.Popover className="min-w-64" placement="top start">
            <Dropdown.Menu
              onAction={(key) => {
                const action = String(key)
                if (action === "create") {
                  onCreateProject()
                  return
                }
                if (action === "delete-current") {
                  setPendingDeleteProject(currentProject)
                  return
                }
                if (action.startsWith("switch:")) {
                  const projectId = action.slice("switch:".length)
                  if (projectId !== currentProject.id) {
                    onSwitchProject(projectId)
                  }
                }
              }}
            >
              <Dropdown.Section
                selectedKeys={[`switch:${currentProject.id}`]}
                selectionMode="single"
              >
                <Header>{t`Projects`}</Header>
                {projects.map((project) => (
                  <Dropdown.Item
                    id={`switch:${project.id}`}
                    key={project.id}
                    textValue={project.title}
                  >
                    <Dropdown.ItemIndicator />
                    <Label>{project.title}</Label>
                  </Dropdown.Item>
                ))}
              </Dropdown.Section>
              <Separator />
              <Dropdown.Item id="create" textValue={t`New project`}>
                <Plus size={14} />
                <Label>{t`New project`}</Label>
              </Dropdown.Item>
              <Dropdown.Item
                id="delete-current"
                isDisabled={projects.length <= 1}
                textValue={t`Delete current project`}
                variant="danger"
              >
                <Trash2 size={14} />
                <Label>{t`Delete current project`}</Label>
              </Dropdown.Item>
            </Dropdown.Menu>
          </Dropdown.Popover>
        </Dropdown>
        <Button
          aria-label={t`Rename project`}
          className="op-project-title__edit-button"
          isIconOnly
          onPress={() => {
            setIsEditing(true)
          }}
          size="sm"
          variant="ghost"
        >
          <Pencil size={14} strokeWidth={1.8} />
        </Button>
      </div>

      {pendingDeleteProject ? (
        <AlertDialog.Backdrop
          isOpen
          onOpenChange={(isOpen) => {
            if (!isOpen) setPendingDeleteProject(null)
          }}
        >
          <AlertDialog.Container placement="center">
            <AlertDialog.Dialog className="sm:max-w-105">
              <AlertDialog.Header>
                <AlertDialog.Icon status="danger" />
                <AlertDialog.Heading>{t`Delete project?`}</AlertDialog.Heading>
              </AlertDialog.Header>
              <AlertDialog.Body>
                <p>
                  {t`Deleting this project removes its Wiki, writing requests, My Documents, canvas content, and publication projects. This cannot be undone.`}
                </p>
                <div className="op-project-title__confirm-name">
                  {pendingDeleteProject.title}
                </div>
              </AlertDialog.Body>
              <AlertDialog.Footer>
                <Button
                  onPress={() => setPendingDeleteProject(null)}
                  slot="close"
                  variant="tertiary"
                >
                  {t`Cancel`}
                </Button>
                <Button
                  onPress={confirmDeleteProject}
                  slot="close"
                  variant="danger"
                >
                  {t`Delete`}
                </Button>
              </AlertDialog.Footer>
            </AlertDialog.Dialog>
          </AlertDialog.Container>
        </AlertDialog.Backdrop>
      ) : null}
    </div>
  )
}

export class MyOpenPanelsBrowserAssetStore implements AssetStore {
  private readonly apiBase: string
  private readonly panelId: string

  constructor(apiBase: string, _projectId: string, panelId: string) {
    this.apiBase = apiBase
    this.panelId = panelId
  }

  async upload(_asset: Partial<Asset>, file: File) {
    const dataUrl = await fileToDataUrl(file)
    const response = await apiFetch(this.apiBase, "/api/assets", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        dataUrl,
        fileName: file.name || "image.png",
        mimeType: file.type || "image/png",
        originPanelId: this.panelId,
      }),
    })
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
