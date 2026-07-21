import { Button, Chip, Input, Modal, Tabs } from "@heroui/react"
import { Blocks, FolderOpen, Plus, RefreshCw, Search, X } from "lucide-react"
import { useCallback, useEffect, useMemo, useState } from "react"
import { type MyOpenPanelsLocale, useMyOpenPanelsI18n } from "../../canvas"
import { apiJson } from "../../lib/api"
import type {
  DeviceSkillGroup,
  ManagedProjectSkill,
  ManagedSkillModule,
  MyOpenPanelsTransport,
  RecommendedSkill,
} from "../../types"
import {
  ConfirmDialog,
  SkillFilesDialog,
  type SkillTextFile,
} from "../wiki/Dialogs"
import {
  AddSkillPanel,
  type SkillImportRequest,
  type SkillUrlScanResponse,
} from "./AddSkillPanel"
import {
  AssociationDialog,
  DeviceSkillsPanel,
  InstalledSkillsPanel,
  MismatchDialog,
} from "./SkillManagerPanels"
import { useRecommendedSkills } from "./useRecommendedSkills"
import { useSkillUpdates } from "./useSkillUpdates"

export {
  canInstallSkill,
  DEFAULT_ADD_SKILL_SOURCE_TAB,
  scannedSkillAssignments,
} from "./AddSkillPanel"
export {
  managedSkillActionIds,
  moduleLabel,
  skillUpdatePresentation,
} from "./SkillManagerPanels"

export type SkillManagerTab = "installed" | "device" | "add"

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

export function SkillManagerDialog({
  initialModuleKind,
  initialTab = "installed",
  isOpen,
  openRequestId,
  onOpenChange,
  transport,
}: {
  initialModuleKind?: string
  initialTab?: SkillManagerTab
  isOpen: boolean
  openRequestId: number
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
  const [isOpenReady, setIsOpenReady] = useState(false)
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

  const {
    checkUpdates,
    confirmForceUpdate,
    invalidateUpdates,
    isChecking: isCheckingUpdates,
    markUpToDate,
    pendingForceSkill,
    requestUpdate,
    setPendingForceSkill,
    states: skillUpdateStates,
    updatingSkillId,
  } = useSkillUpdates({
    activeTab,
    apiBase: transport.apiBase,
    isOpen: isOpen && isOpenReady,
    onError: setError,
    onUpdated: loadInstalled,
  })

  const {
    catalog: recommendedCatalog,
    install: installRecommended,
    isLoading: isLoadingRecommended,
    load: loadRecommended,
    pendingCatalogId,
    refresh: refreshRecommended,
  } = useRecommendedSkills({
    apiBase: transport.apiBase,
    onApplied: async (response) => {
      if (response.operation === "installed") markUpToDate(response.skill)
      await loadInstalled()
    },
    onError: setError,
  })

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
    if (!isOpen) {
      setIsOpenReady(false)
      return
    }
    let cancelled = false
    setActiveTab(initialTab)
    setIsOpenReady(false)
    setHasScannedDevice(false)
    setDeviceSkills([])
    const prepare = async () => {
      await loadInstalled()
      await loadRecommended()
      if (!cancelled) setIsOpenReady(true)
    }
    prepare().catch(() => undefined)
    return () => {
      cancelled = true
    }
  }, [initialTab, isOpen, loadInstalled, loadRecommended])

  useEffect(() => {
    if (
      isOpen &&
      isOpenReady &&
      activeTab === "device" &&
      !hasScannedDevice &&
      !isScanning
    ) {
      scanDevice()
    }
  }, [activeTab, hasScannedDevice, isOpen, isOpenReady, isScanning, scanDevice])

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
    invalidateUpdates()
    await loadInstalled()
    await refreshRecommended()
    await scanDevice()
  }, [invalidateUpdates, loadInstalled, refreshRecommended, scanDevice])

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

  const scanSkillUrl = useCallback(
    (url: string) =>
      apiJson<SkillUrlScanResponse>(
        transport.apiBase,
        "/api/skills/import/scan",
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ url }),
        }
      ),
    [transport.apiBase]
  )

  const filteredDeviceSkills = useMemo(
    () => filterDeviceSkills(deviceSkills, deviceLocations, deviceSearch),
    [deviceLocations, deviceSearch, deviceSkills]
  )

  const installedCount = useMemo(
    () =>
      new Set([
        ...installed.systemSkills.map((skill) => skill.id),
        ...installed.modules.flatMap((module) =>
          module.skills.map((skill) => skill.id)
        ),
      ]).size,
    [installed]
  )

  const installedSkillsById = useMemo(
    () =>
      new Map(
        [
          ...installed.systemSkills,
          ...installed.modules.flatMap((module) => module.skills),
        ].map((skill) => [skill.id, skill])
      ),
    [installed]
  )

  const updateRecommendedSkill = useCallback(
    (recommended: RecommendedSkill) => {
      const skill = recommended.installedSkillId
        ? installedSkillsById.get(recommended.installedSkillId)
        : undefined
      if (!skill) {
        setError(t`Installed Skill could not be found.`)
        return
      }
      requestUpdate(skill, async () => {
        if (!(await installRecommended(recommended.id))) {
          throw new Error(
            t`The Skill was updated, but its recommended module associations could not be applied.`
          )
        }
      })
    },
    [installRecommended, installedSkillsById, requestUpdate, t]
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
            pendingReplacement ||
            pendingForceSkill
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
                    isCheckingUpdates={isCheckingUpdates}
                    isLoading={isLoading}
                    modules={installed.modules}
                    onCheckUpdates={checkUpdates}
                    onDelete={setPendingDeleteSkill}
                    onOpen={openSkill}
                    onUpdate={requestUpdate}
                    systemSkills={installed.systemSkills}
                    updateStates={skillUpdateStates}
                    updatingSkillId={updatingSkillId}
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
                    initialModuleKind={initialModuleKind}
                    isCheckingUpdates={isCheckingUpdates}
                    isImporting={isImporting}
                    isLoadingRecommended={isLoadingRecommended}
                    key={`add-skill-${openRequestId}`}
                    onInstall={importSkill}
                    onInstallRecommended={(skill) => {
                      installRecommended(skill.id).catch(() => undefined)
                    }}
                    onScanUrl={scanSkillUrl}
                    onUpdateRecommended={updateRecommendedSkill}
                    pendingCatalogId={pendingCatalogId}
                    recommendedSkills={recommendedCatalog.skills}
                    skillUpdateStates={skillUpdateStates}
                    updatingSkillId={updatingSkillId}
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
      {pendingForceSkill ? (
        <ConfirmDialog
          backdropClassName="op-skill-manager-child-backdrop"
          cancelLabel={t`Cancel`}
          confirmLabel={t`Discard changes and update`}
          isBusy={updatingSkillId === pendingForceSkill.id}
          message={t`Local edits to this Skill will be permanently discarded and replaced with the latest source version.`}
          onCancel={() => setPendingForceSkill(null)}
          onConfirm={confirmForceUpdate}
          title={t`Update Skill with local changes?`}
        />
      ) : null}
    </>
  )
}
