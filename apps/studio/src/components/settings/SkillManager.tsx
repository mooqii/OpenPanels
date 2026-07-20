import {
  Button,
  Chip,
  Input,
  Label,
  ListBox,
  Modal,
  Select,
  Tabs,
} from "@heroui/react"
import {
  Blocks,
  FileArchive,
  FolderOpen,
  Globe2,
  Link,
  Plus,
  RefreshCw,
  Search,
  Upload,
  X,
} from "lucide-react"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { type MyOpenPanelsLocale, useMyOpenPanelsI18n } from "../../canvas"
import { apiJson } from "../../lib/api"
import type {
  DeviceSkillGroup,
  ManagedProjectSkill,
  ManagedSkillModule,
  MyOpenPanelsTransport,
} from "../../types"
import {
  ConfirmDialog,
  SkillFilesDialog,
  type SkillTextFile,
} from "../wiki/Dialogs"
import {
  AssociationDialog,
  DeviceSkillsPanel,
  InstalledSkillsPanel,
  MismatchDialog,
  moduleLabel,
} from "./SkillManagerPanels"

export { managedSkillActionIds } from "./SkillManagerPanels"

type SkillManagerTab = "installed" | "device" | "add"
type SkillImportSourceType = "url" | "folder" | "zip"

interface SkillImportFile {
  contentBase64: string
  path: string
}

export interface SkillImportRequest {
  archiveBase64?: string
  files?: SkillImportFile[]
  moduleKind: string
  replaceExisting: boolean
  sourceType: SkillImportSourceType
  url?: string
}

interface SkillImportResponse {
  incomingSkill?: { description: string; name: string }
  message?: string
  skill?: ManagedProjectSkill
  status: "conflict" | "installed"
}

interface ManagedSkillsResponse {
  modules: ManagedSkillModule[]
  systemSkills: ManagedProjectSkill[]
}

const ASSOCIATION_MODULES = [
  "wiki-update",
  "writing",
  "writing-refinement",
  "publishing-xiaohongshu",
] as const

export function filterDeviceSkills(
  skills: DeviceSkillGroup[],
  selectedLocations: Record<string, string>,
  query: string
) {
  const needle = query.trim().toLocaleLowerCase()
  if (!needle) return skills
  return skills.filter((skill) => {
    const selectedPath =
      selectedLocations[skill.key] ?? skill.locations[0]?.path
    const selected = skill.locations.find(
      (location) => location.path === selectedPath
    )
    return [
      skill.name,
      selected?.description ?? skill.description,
      selected?.path ?? "",
      ...(selected?.agents ?? []),
    ].some((value) => value.toLocaleLowerCase().includes(needle))
  })
}

export function installedSkillCountLabel(
  count: number,
  locale: MyOpenPanelsLocale
) {
  return locale === "zh-CN"
    ? `${count}个已安装的Skill`
    : `${count} installed ${count === 1 ? "Skill" : "Skills"}`
}

export function canInstallSkill(input: {
  folderFileCount: number
  moduleKind: string
  sourceType: SkillImportSourceType
  url: string
  zipSelected: boolean
}) {
  if (!input.moduleKind) return false
  if (input.sourceType === "url") return input.url.trim().length > 0
  if (input.sourceType === "folder") return input.folderFileCount > 0
  return input.zipSelected
}

export function SkillManagerDialog({
  isOpen,
  onOpenChange,
  transport,
}: {
  isOpen: boolean
  onOpenChange: (isOpen: boolean) => void
  transport: MyOpenPanelsTransport
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const [activeTab, setActiveTab] = useState<SkillManagerTab>("installed")
  const [installed, setInstalled] = useState<ManagedSkillsResponse>({
    modules: [],
    systemSkills: [],
  })
  const [deviceSkills, setDeviceSkills] = useState<DeviceSkillGroup[]>([])
  const [deviceSearch, setDeviceSearch] = useState("")
  const [deviceLocations, setDeviceLocations] = useState<
    Record<string, string>
  >({})
  const [isLoading, setIsLoading] = useState(false)
  const [isScanning, setIsScanning] = useState(false)
  const [hasScannedDevice, setHasScannedDevice] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [skillFilesDialog, setSkillFilesDialog] = useState<{
    files: SkillTextFile[]
    skill: ManagedProjectSkill
  } | null>(null)
  const [pendingDeleteSkill, setPendingDeleteSkill] =
    useState<ManagedProjectSkill | null>(null)
  const [isDeleting, setIsDeleting] = useState(false)
  const [associationTarget, setAssociationTarget] = useState<{
    skill: DeviceSkillGroup
    locationPath: string
  } | null>(null)
  const [removalTarget, setRemovalTarget] = useState<{
    moduleKind: string
    skill: DeviceSkillGroup
  } | null>(null)
  const [mismatchTarget, setMismatchTarget] = useState<{
    locationPath: string
    skill: DeviceSkillGroup
  } | null>(null)
  const [isMutating, setIsMutating] = useState(false)
  const [pendingReplacement, setPendingReplacement] = useState<{
    request: SkillImportRequest
    skillName: string
  } | null>(null)
  const [isImporting, setIsImporting] = useState(false)

  const loadInstalled = useCallback(async () => {
    setIsLoading(true)
    setError(null)
    try {
      setInstalled(
        await apiJson<ManagedSkillsResponse>(transport.apiBase, "/api/skills")
      )
    } catch (cause) {
      setError(String((cause as Error)?.message || cause))
    } finally {
      setIsLoading(false)
    }
  }, [transport.apiBase])

  const scanDevice = useCallback(async () => {
    setIsScanning(true)
    setError(null)
    try {
      const response = await apiJson<{ skills: DeviceSkillGroup[] }>(
        transport.apiBase,
        "/api/device/skills"
      )
      setDeviceSkills(response.skills)
      setDeviceLocations(
        Object.fromEntries(
          response.skills.flatMap((skill) =>
            skill.locations[0] ? [[skill.key, skill.locations[0].path]] : []
          )
        )
      )
      setHasScannedDevice(true)
    } catch (cause) {
      setError(String((cause as Error)?.message || cause))
    } finally {
      setIsScanning(false)
    }
  }, [transport.apiBase])

  useEffect(() => {
    if (!isOpen) return
    setActiveTab("installed")
    setHasScannedDevice(false)
    setDeviceSkills([])
    loadInstalled()
  }, [isOpen, loadInstalled])

  useEffect(() => {
    if (isOpen && activeTab === "device" && !hasScannedDevice && !isScanning) {
      scanDevice()
    }
  }, [activeTab, hasScannedDevice, isOpen, isScanning, scanDevice])

  const openSkill = useCallback(
    async (skill: ManagedProjectSkill) => {
      setError(null)
      try {
        const response = await apiJson<{ files: SkillTextFile[] }>(
          transport.apiBase,
          `/api/skills/${encodeURIComponent(skill.id)}`
        )
        setSkillFilesDialog({ files: response.files, skill })
      } catch (cause) {
        setError(String((cause as Error)?.message || cause))
      }
    },
    [transport.apiBase]
  )

  const saveSkillFile = useCallback(
    async (path: string, content: string) => {
      if (!skillFilesDialog) return
      await apiJson(
        transport.apiBase,
        `/api/skills/${encodeURIComponent(skillFilesDialog.skill.id)}/file`,
        {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ path, content }),
        }
      )
      setSkillFilesDialog((current) =>
        current
          ? {
              ...current,
              files: current.files.map((file) =>
                file.path === path ? { ...file, content } : file
              ),
            }
          : null
      )
    },
    [skillFilesDialog, transport.apiBase]
  )

  const deleteSkill = useCallback(async () => {
    if (!pendingDeleteSkill) return
    setIsDeleting(true)
    setError(null)
    try {
      await apiJson(
        transport.apiBase,
        `/api/skills/${encodeURIComponent(pendingDeleteSkill.id)}`,
        { method: "DELETE" }
      )
      setPendingDeleteSkill(null)
      await loadInstalled()
    } catch (cause) {
      setError(String((cause as Error)?.message || cause))
    } finally {
      setIsDeleting(false)
    }
  }, [loadInstalled, pendingDeleteSkill, transport.apiBase])

  const refreshSkills = useCallback(async () => {
    await Promise.all([loadInstalled(), scanDevice()])
  }, [loadInstalled, scanDevice])

  const installAssociations = useCallback(
    async (moduleKinds: string[]) => {
      if (!associationTarget) return
      setIsMutating(true)
      setError(null)
      try {
        for (const moduleKind of moduleKinds) {
          await apiJson(transport.apiBase, "/api/device/skills/install", {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({
              locationPath: associationTarget.locationPath,
              moduleKind,
            }),
          })
        }
        setAssociationTarget(null)
        await refreshSkills()
      } catch (cause) {
        setError(String((cause as Error)?.message || cause))
      } finally {
        setIsMutating(false)
      }
    },
    [associationTarget, refreshSkills, transport.apiBase]
  )

  const removeAssociation = useCallback(async () => {
    if (!removalTarget?.skill.installed) return
    setIsMutating(true)
    setError(null)
    try {
      await apiJson(
        transport.apiBase,
        `/api/skills/${encodeURIComponent(removalTarget.skill.installed.id)}/modules/${encodeURIComponent(removalTarget.moduleKind)}`,
        { method: "DELETE" }
      )
      setRemovalTarget(null)
      await refreshSkills()
    } catch (cause) {
      setError(String((cause as Error)?.message || cause))
    } finally {
      setIsMutating(false)
    }
  }, [refreshSkills, removalTarget, transport.apiBase])

  const resolveMismatch = useCallback(
    async (action: "ignore" | "replace") => {
      if (!mismatchTarget?.skill.installed) return
      const selected = mismatchTarget.skill.locations.find(
        (location) => location.path === mismatchTarget.locationPath
      )
      if (!selected) return
      setIsMutating(true)
      setError(null)
      try {
        const skillId = mismatchTarget.skill.installed.id
        const path =
          action === "ignore"
            ? `/api/skills/${encodeURIComponent(skillId)}/mismatch-ignore`
            : `/api/skills/${encodeURIComponent(skillId)}/source`
        await apiJson(transport.apiBase, path, {
          method: "PUT",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(
            action === "ignore"
              ? {
                  locationPath: selected.path,
                  installedHash: mismatchTarget.skill.installed.contentHash,
                  deviceHash: selected.contentHash,
                }
              : { locationPath: selected.path }
          ),
        })
        setMismatchTarget(null)
        await refreshSkills()
      } catch (cause) {
        setError(String((cause as Error)?.message || cause))
      } finally {
        setIsMutating(false)
      }
    },
    [mismatchTarget, refreshSkills, transport.apiBase]
  )

  const importSkill = useCallback(
    async (request: SkillImportRequest, replaceExisting = false) => {
      setIsImporting(true)
      setError(null)
      try {
        const response = await apiJson<SkillImportResponse>(
          transport.apiBase,
          "/api/skills/import",
          {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ ...request, replaceExisting }),
          }
        )
        if (response.status === "conflict") {
          setPendingReplacement({
            request,
            skillName: response.incomingSkill?.name ?? t`This Skill`,
          })
          return false
        }
        setPendingReplacement(null)
        await refreshSkills()
        setActiveTab("installed")
        return true
      } catch (cause) {
        setError(String((cause as Error)?.message || cause))
        return false
      } finally {
        setIsImporting(false)
      }
    },
    [refreshSkills, t, transport.apiBase]
  )

  const filteredDeviceSkills = useMemo(
    () => filterDeviceSkills(deviceSkills, deviceLocations, deviceSearch),
    [deviceLocations, deviceSearch, deviceSkills]
  )

  const installedCount = useMemo(
    () =>
      installed.systemSkills.length +
      installed.modules.reduce(
        (count, module) => count + module.skills.length,
        0
      ),
    [installed]
  )

  return (
    <>
      <Modal.Backdrop
        className="op-skill-manager-backdrop"
        isDismissable={
          !(
            skillFilesDialog ||
            pendingDeleteSkill ||
            associationTarget ||
            removalTarget ||
            mismatchTarget ||
            pendingReplacement
          )
        }
        isOpen={isOpen}
        onOpenChange={onOpenChange}
        variant="blur"
      >
        <Modal.Container size="cover">
          <Modal.Dialog className="op-skill-manager">
            <Modal.CloseTrigger aria-label={t`Close`} />
            <Modal.Header className="op-skill-manager__header">
              <Blocks size={20} />
              <Modal.Heading>{t`Skill management`}</Modal.Heading>
            </Modal.Header>
            <Modal.Body className="op-skill-manager__body">
              <Tabs
                className="op-skill-manager__tabs"
                onSelectionChange={(key) =>
                  setActiveTab(String(key) as SkillManagerTab)
                }
                selectedKey={activeTab}
              >
                <Tabs.ListContainer>
                  <Tabs.List aria-label={t`Skill management pages`}>
                    <Tabs.Tab id="installed">
                      <Blocks size={15} />
                      <span>{t`Installed Skills`}</span>
                      <Chip size="sm" variant="soft">
                        {installedCount}
                      </Chip>
                      <Tabs.Indicator />
                    </Tabs.Tab>
                    <Tabs.Tab id="device">
                      <FolderOpen size={15} />
                      <span>{t`Device Skills`}</span>
                      <Tabs.Indicator />
                    </Tabs.Tab>
                    <Tabs.Tab id="add">
                      <Plus size={15} />
                      <span>{t`Add Skill`}</span>
                      <Tabs.Indicator />
                    </Tabs.Tab>
                  </Tabs.List>
                </Tabs.ListContainer>
                <Tabs.Panel id="installed">
                  <InstalledSkillsPanel
                    isLoading={isLoading}
                    modules={installed.modules}
                    onDelete={setPendingDeleteSkill}
                    onOpen={openSkill}
                    systemSkills={installed.systemSkills}
                  />
                </Tabs.Panel>
                <Tabs.Panel id="device">
                  <div className="op-skill-manager__toolbar">
                    <div className="op-skill-manager__search">
                      <Search aria-hidden size={15} />
                      <Input
                        aria-label={t`Search device Skills`}
                        onChange={(event) =>
                          setDeviceSearch(event.target.value)
                        }
                        placeholder={installedSkillCountLabel(
                          deviceSkills.length,
                          locale
                        )}
                        value={deviceSearch}
                      />
                      {deviceSearch ? (
                        <Button
                          aria-label={t`Clear search`}
                          isIconOnly
                          onPress={() => setDeviceSearch("")}
                          size="sm"
                          variant="ghost"
                        >
                          <X size={14} />
                        </Button>
                      ) : null}
                    </div>
                    <Button
                      aria-label={t`Rescan device Skills`}
                      isIconOnly
                      isPending={isScanning}
                      onPress={scanDevice}
                      size="sm"
                      variant="ghost"
                    >
                      <RefreshCw size={15} />
                    </Button>
                  </div>
                  <DeviceSkillsPanel
                    isLoading={isScanning}
                    locations={deviceLocations}
                    onAdd={(skill, locationPath) =>
                      setAssociationTarget({ skill, locationPath })
                    }
                    onLocationChange={(skillKey, path) =>
                      setDeviceLocations((current) => ({
                        ...current,
                        [skillKey]: path,
                      }))
                    }
                    onMismatch={(skill, locationPath) =>
                      setMismatchTarget({ skill, locationPath })
                    }
                    onRemove={(skill, moduleKind) =>
                      setRemovalTarget({ skill, moduleKind })
                    }
                    skills={filteredDeviceSkills}
                  />
                </Tabs.Panel>
                <Tabs.Panel id="add">
                  <AddSkillPanel
                    isImporting={isImporting}
                    onInstall={importSkill}
                  />
                </Tabs.Panel>
              </Tabs>
              {error ? (
                <div className="op-skill-manager__error">{error}</div>
              ) : null}
            </Modal.Body>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
      {skillFilesDialog ? (
        <SkillFilesDialog
          backdropClassName="op-skill-manager-child-backdrop"
          closeLabel={t`Close`}
          files={skillFilesDialog.files}
          onClose={() => setSkillFilesDialog(null)}
          onSave={saveSkillFile}
          readOnly={!skillFilesDialog.skill.canEdit}
          title={skillFilesDialog.skill.name}
        />
      ) : null}
      {pendingDeleteSkill ? (
        <ConfirmDialog
          backdropClassName="op-skill-manager-child-backdrop"
          cancelLabel={t`Cancel`}
          confirmLabel={t`Delete`}
          isBusy={isDeleting}
          message={t`This Skill will be removed from every project.`}
          onCancel={() => setPendingDeleteSkill(null)}
          onConfirm={() => deleteSkill()}
          title={t`Delete Skill?`}
        />
      ) : null}
      {associationTarget ? (
        <AssociationDialog
          associated={associationTarget.skill.installed?.moduleKinds ?? []}
          isBusy={isMutating}
          onClose={() => setAssociationTarget(null)}
          onSave={installAssociations}
        />
      ) : null}
      {removalTarget ? (
        <ConfirmDialog
          backdropClassName="op-skill-manager-child-backdrop"
          cancelLabel={t`Cancel`}
          confirmLabel={t`Remove`}
          isBusy={isMutating}
          message={
            removalTarget.skill.installed?.moduleKinds.length === 1
              ? t`This is the last association. The Skill package will be deleted from MyOpenPanels.`
              : t`This association will be removed from MyOpenPanels.`
          }
          onCancel={() => setRemovalTarget(null)}
          onConfirm={() => removeAssociation()}
          title={t`Remove Skill association?`}
        />
      ) : null}
      {mismatchTarget ? (
        <MismatchDialog
          isBusy={isMutating}
          onClose={() => setMismatchTarget(null)}
          onIgnore={() => resolveMismatch("ignore")}
          onReplace={() => resolveMismatch("replace")}
          skillName={mismatchTarget.skill.name}
        />
      ) : null}
      {pendingReplacement ? (
        <ConfirmDialog
          backdropClassName="op-skill-manager-child-backdrop"
          cancelLabel={t`Cancel`}
          confirmLabel={t`Replace`}
          isBusy={isImporting}
          message={
            locale === "zh-CN"
              ? `已安装同名的自建 Skill“${pendingReplacement.skillName}”。替换后会更新 Skill 文件，并保留现有的功能模块关联。`
              : `A self-built Skill named "${pendingReplacement.skillName}" is already installed. Replacing it updates the Skill files and keeps its existing module associations.`
          }
          onCancel={() => setPendingReplacement(null)}
          onConfirm={() => importSkill(pendingReplacement.request, true)}
          title={t`Replace existing Skill?`}
        />
      ) : null}
    </>
  )
}

function AddSkillPanel({
  isImporting,
  onInstall,
}: {
  isImporting: boolean
  onInstall: (request: SkillImportRequest) => Promise<boolean>
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const [sourceType, setSourceType] = useState<SkillImportSourceType>("url")
  const [url, setUrl] = useState("")
  const [moduleKind, setModuleKind] = useState("")
  const [folderFiles, setFolderFiles] = useState<File[]>([])
  const [zipFile, setZipFile] = useState<File | null>(null)
  const [localError, setLocalError] = useState<string | null>(null)
  const folderInput = useRef<HTMLInputElement>(null)
  const zipInput = useRef<HTMLInputElement>(null)

  const canInstall = canInstallSkill({
    folderFileCount: folderFiles.length,
    moduleKind,
    sourceType,
    url,
    zipSelected: zipFile !== null,
  })

  const install = useCallback(async () => {
    if (!canInstall) return
    setLocalError(null)
    try {
      let request: SkillImportRequest
      if (sourceType === "url") {
        request = {
          moduleKind,
          replaceExisting: false,
          sourceType,
          url: url.trim(),
        }
      } else if (sourceType === "folder") {
        validateLocalSkillSelection(folderFiles)
        request = {
          files: await Promise.all(
            folderFiles.map(async (file) => ({
              contentBase64: await fileToBase64(file),
              path: file.webkitRelativePath || file.name,
            }))
          ),
          moduleKind,
          replaceExisting: false,
          sourceType,
        }
      } else {
        if (!zipFile) return
        validateLocalSkillSelection([zipFile])
        request = {
          archiveBase64: await fileToBase64(zipFile),
          moduleKind,
          replaceExisting: false,
          sourceType,
        }
      }
      if (await onInstall(request)) {
        setUrl("")
        setFolderFiles([])
        setZipFile(null)
        setModuleKind("")
      }
    } catch (cause) {
      setLocalError(String((cause as Error)?.message || cause))
    }
  }, [canInstall, folderFiles, moduleKind, onInstall, sourceType, url, zipFile])

  return (
    <div className="op-skill-import">
      <div className="op-skill-import__intro">
        <strong>{t`Install a Skill`}</strong>
        <span>{t`Choose one source and associate the Skill with a feature module.`}</span>
      </div>
      <Tabs
        className="op-skill-import__source-tabs"
        onSelectionChange={(key) => {
          setLocalError(null)
          setSourceType((current) =>
            key === "url" ? "url" : current === "url" ? "folder" : current
          )
        }}
        selectedKey={sourceType === "url" ? "url" : "local"}
        variant="secondary"
      >
        <Tabs.ListContainer>
          <Tabs.List aria-label={t`Skill source`}>
            <Tabs.Tab id="url">
              <Link size={15} />
              {t`From URL`}
              <Tabs.Indicator />
            </Tabs.Tab>
            <Tabs.Tab id="local">
              <Upload size={15} />
              {t`Local upload`}
              <Tabs.Indicator />
            </Tabs.Tab>
          </Tabs.List>
        </Tabs.ListContainer>
        <Tabs.Panel id="url">
          <div className="op-skill-import__field">
            <Label htmlFor="op-skill-import-url">{t`Skill URL`}</Label>
            <div className="op-skill-import__url">
              <Globe2 aria-hidden size={16} />
              <Input
                id="op-skill-import-url"
                onChange={(event) => setUrl(event.target.value)}
                placeholder="https://clawhub.ai/owner/skills/example"
                type="url"
                value={url}
                variant="secondary"
              />
            </div>
            <small>
              {t`Supported: GitHub repository and tree URLs, ClawHub Skill detail URLs, and SkillHub.cn Skill detail URLs.`}
            </small>
          </div>
        </Tabs.Panel>
        <Tabs.Panel id="local">
          <div className="op-skill-import__upload-options">
            <Button
              className={sourceType === "folder" ? "is-selected" : ""}
              onPress={() => folderInput.current?.click()}
              variant="secondary"
            >
              <FolderOpen size={19} />
              <span>
                <strong>{t`Skill folder`}</strong>
                <small>
                  {folderFiles.length > 0
                    ? locale === "zh-CN"
                      ? `已选择 ${folderFiles.length} 个文件`
                      : `${folderFiles.length} files selected`
                    : t`Choose a folder containing SKILL.md`}
                </small>
              </span>
            </Button>
            <Button
              className={sourceType === "zip" ? "is-selected" : ""}
              onPress={() => zipInput.current?.click()}
              variant="secondary"
            >
              <FileArchive size={19} />
              <span>
                <strong>{t`ZIP package`}</strong>
                <small>{zipFile?.name ?? t`Choose a .zip file`}</small>
              </span>
            </Button>
            <input
              {...({ directory: "", webkitdirectory: "" } as Record<
                string,
                string
              >)}
              hidden
              multiple
              onChange={(event) => {
                const files = Array.from(event.target.files ?? [])
                setFolderFiles(files)
                setSourceType("folder")
                setLocalError(null)
                event.target.value = ""
              }}
              ref={folderInput}
              type="file"
            />
            <input
              accept=".zip,application/zip"
              hidden
              onChange={(event) => {
                setZipFile(event.target.files?.[0] ?? null)
                setSourceType("zip")
                setLocalError(null)
                event.target.value = ""
              }}
              ref={zipInput}
              type="file"
            />
          </div>
        </Tabs.Panel>
      </Tabs>
      <div className="op-skill-import__module">
        <Label>{t`Associated feature module`}</Label>
        <Select
          aria-label={t`Associated feature module`}
          onChange={(key) => setModuleKind(String(key))}
          selectionMode="single"
          value={moduleKind}
          variant="secondary"
        >
          <Select.Trigger>
            <Select.Value>
              {moduleKind
                ? moduleLabel(moduleKind, t)
                : t`Select a feature module`}
            </Select.Value>
            <Select.Indicator />
          </Select.Trigger>
          <Select.Popover>
            <ListBox>
              {ASSOCIATION_MODULES.map((kind) => (
                <ListBox.Item
                  id={kind}
                  key={kind}
                  textValue={moduleLabel(kind, t)}
                >
                  {moduleLabel(kind, t)}
                </ListBox.Item>
              ))}
            </ListBox>
          </Select.Popover>
        </Select>
        <small>{t`Required. The Skill will be available from this module after installation.`}</small>
      </div>
      {localError ? (
        <div className="op-skill-import__error">{localError}</div>
      ) : null}
      <div className="op-skill-import__actions">
        <Button
          isDisabled={!canInstall}
          isPending={isImporting}
          onPress={install}
        >
          <Plus size={15} />
          {isImporting ? t`Validating and installing` : t`Install Skill`}
        </Button>
      </div>
    </div>
  )
}

function validateLocalSkillSelection(files: File[]) {
  if (files.length > 512) {
    throw new Error("The Skill package contains more than 512 files.")
  }
  const bytes = files.reduce((total, file) => total + file.size, 0)
  if (bytes > 20 * 1024 * 1024) {
    throw new Error("The Skill upload exceeds the 20 MB limit.")
  }
}

async function fileToBase64(file: File) {
  const bytes = new Uint8Array(await file.arrayBuffer())
  let binary = ""
  for (let offset = 0; offset < bytes.length; offset += 0x80_00) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x80_00))
  }
  return btoa(binary)
}
