import { Button, Input, Label, ListBox, Select, Tabs } from "@heroui/react"
import {
  FileArchive,
  FolderOpen,
  Globe2,
  Link,
  Plus,
  Search,
  Sparkles,
  Upload,
} from "lucide-react"
import { useCallback, useRef, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import type { RecommendedSkill, SkillUpdateState } from "../../types"
import { RecommendedSkillsPanel } from "./RecommendedSkillsPanel"
import { moduleLabel } from "./SkillManagerPanels"

type SkillImportSourceType = "url" | "folder" | "zip"
type AddSkillSourceTab = "recommended" | "url" | "local"
export const DEFAULT_ADD_SKILL_SOURCE_TAB: AddSkillSourceTab = "recommended"

interface SkillImportFile {
  contentBase64: string
  path: string
}

export interface SkillImportRequest {
  archiveBase64?: string
  files?: SkillImportFile[]
  moduleKind?: string
  replaceExisting: boolean
  skills?: Array<{
    moduleKind: string | null
    subpath: string
  }>
  sourceType: SkillImportSourceType
  url?: string
}

export interface ScannedUrlSkill {
  description: string
  name: string
  subpath: string
}

export interface SkillUrlScanResponse {
  skills: ScannedUrlSkill[]
  sourceUrl: string
}

const ASSOCIATION_MODULES = [
  "wiki-update",
  "writing",
  "writing-refinement",
  "typesetting-cover",
  "typesetting-title",
  "typesetting-layout",
  "publishing",
] as const

export function canInstallSkill(input: {
  folderFileCount: number
  moduleKind: string
  sourceType: SkillImportSourceType
  url: string
  zipSelected: boolean
}) {
  if (input.sourceType === "url") return input.url.trim().length > 0
  if (!input.moduleKind) return false
  if (input.sourceType === "folder") return input.folderFileCount > 0
  return input.zipSelected
}

export function scannedSkillAssignments(
  skills: ScannedUrlSkill[],
  initialModuleKind: string
) {
  return Object.fromEntries(
    skills.map((skill) => [skill.subpath, initialModuleKind])
  )
}

export function AddSkillPanel({
  initialModuleKind = "",
  isCheckingUpdates,
  isImporting,
  isLoadingRecommended,
  onInstall,
  onInstallRecommended,
  onScanUrl,
  onUpdateRecommended,
  pendingCatalogId,
  recommendedSkills,
  skillUpdateStates,
  updatingSkillId,
}: {
  initialModuleKind?: string
  isCheckingUpdates: boolean
  isImporting: boolean
  isLoadingRecommended: boolean
  onInstall: (request: SkillImportRequest) => Promise<boolean>
  onInstallRecommended: (skill: RecommendedSkill) => void
  onScanUrl: (url: string) => Promise<SkillUrlScanResponse>
  onUpdateRecommended: (skill: RecommendedSkill) => void
  pendingCatalogId: string | null
  recommendedSkills: RecommendedSkill[]
  skillUpdateStates: Record<string, SkillUpdateState>
  updatingSkillId: string | null
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const [sourceTab, setSourceTab] = useState<AddSkillSourceTab>(
    DEFAULT_ADD_SKILL_SOURCE_TAB
  )
  const [sourceType, setSourceType] = useState<SkillImportSourceType>("url")
  const [url, setUrl] = useState("")
  const [scannedUrl, setScannedUrl] = useState("")
  const [scannedSkills, setScannedSkills] = useState<ScannedUrlSkill[]>([])
  const [urlSkillModules, setUrlSkillModules] = useState<
    Record<string, string>
  >({})
  const [isScanningUrl, setIsScanningUrl] = useState(false)
  const [moduleKind, setModuleKind] = useState(initialModuleKind)
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

  const canInstallUrlSkills =
    scannedSkills.length > 0 && scannedUrl === url.trim()

  const scanUrl = useCallback(async () => {
    const nextUrl = url.trim()
    if (!nextUrl || isScanningUrl) return
    setIsScanningUrl(true)
    setLocalError(null)
    setScannedSkills([])
    setScannedUrl("")
    try {
      const response = await onScanUrl(nextUrl)
      setScannedUrl(nextUrl)
      setScannedSkills(response.skills)
      setUrlSkillModules(
        scannedSkillAssignments(response.skills, initialModuleKind)
      )
    } catch (cause) {
      setLocalError(String((cause as Error)?.message || cause))
    } finally {
      setIsScanningUrl(false)
    }
  }, [initialModuleKind, isScanningUrl, onScanUrl, url])

  const install = useCallback(async () => {
    if (sourceType === "url" ? !canInstallUrlSkills : !canInstall) return
    setLocalError(null)
    try {
      let request: SkillImportRequest
      if (sourceType === "url") {
        request = {
          replaceExisting: false,
          skills: scannedSkills.map((skill) => ({
            moduleKind: urlSkillModules[skill.subpath] || null,
            subpath: skill.subpath,
          })),
          sourceType,
          url: scannedUrl,
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
        setScannedUrl("")
        setScannedSkills([])
        setUrlSkillModules({})
        setFolderFiles([])
        setZipFile(null)
        setModuleKind("")
      }
    } catch (cause) {
      setLocalError(String((cause as Error)?.message || cause))
    }
  }, [
    canInstall,
    canInstallUrlSkills,
    folderFiles,
    moduleKind,
    onInstall,
    scannedSkills,
    scannedUrl,
    sourceType,
    urlSkillModules,
    zipFile,
  ])

  return (
    <div className="op-skill-import">
      <div className="op-skill-import__intro">
        <strong>{t`Install a Skill`}</strong>
        <span>{t`Choose a source, review the Skills found, and configure module associations.`}</span>
      </div>
      <Tabs
        className="op-skill-import__source-tabs"
        onSelectionChange={(key) => {
          setLocalError(null)
          const nextSource = String(key) as AddSkillSourceTab
          setSourceTab(nextSource)
          if (nextSource === "url") setSourceType("url")
          if (nextSource === "local" && sourceType === "url") {
            setSourceType("folder")
          }
        }}
        selectedKey={sourceTab}
        variant="secondary"
      >
        <Tabs.ListContainer>
          <Tabs.List aria-label={t`Skill source`}>
            <Tabs.Tab id="recommended">
              <Sparkles size={15} />
              {t`Recommended Skills`}
              <Tabs.Indicator />
            </Tabs.Tab>
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
        <Tabs.Panel id="recommended">
          <RecommendedSkillsPanel
            initialModuleKind={initialModuleKind}
            isCheckingUpdates={isCheckingUpdates}
            isLoading={isLoadingRecommended}
            onInstall={onInstallRecommended}
            onUpdate={onUpdateRecommended}
            pendingCatalogId={pendingCatalogId}
            skills={recommendedSkills}
            updateStates={skillUpdateStates}
            updatingSkillId={updatingSkillId}
          />
        </Tabs.Panel>
        <Tabs.Panel id="url">
          <div className="op-skill-import__field">
            <Label htmlFor="op-skill-import-url">{t`Skill URL`}</Label>
            <div className="op-skill-import__url-controls">
              <div className="op-skill-import__url">
                <Globe2 aria-hidden size={16} />
                <Input
                  id="op-skill-import-url"
                  onChange={(event) => {
                    setUrl(event.target.value)
                    setScannedSkills([])
                    setScannedUrl("")
                    setLocalError(null)
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault()
                      scanUrl().catch(() => undefined)
                    }
                  }}
                  placeholder="https://github.com/owner/skills"
                  type="url"
                  value={url}
                  variant="secondary"
                />
              </div>
              <Button
                isDisabled={!canInstall}
                isPending={isScanningUrl}
                onPress={scanUrl}
                variant="secondary"
              >
                <Search size={15} />
                {isScanningUrl ? t`Scanning` : t`Scan URL`}
              </Button>
            </div>
            <small>
              {t`Supported: GitHub repository and tree URLs, skills.sh repository and Skill detail URLs, ClawHub Skill detail URLs, and SkillHub.cn Skill detail URLs.`}
            </small>
          </div>
          {scannedSkills.length > 0 ? (
            <div className="op-skill-import__scan-results">
              <div className="op-skill-import__scan-summary">
                <div>
                  <strong>
                    {locale === "zh-CN"
                      ? `发现 ${scannedSkills.length} 个 Skill`
                      : `${scannedSkills.length} ${
                          scannedSkills.length === 1 ? "Skill" : "Skills"
                        } found`}
                  </strong>
                  <span>{t`Choose a feature module for each Skill, or leave it unassociated.`}</span>
                </div>
                <Button isPending={isImporting} onPress={install}>
                  <Plus size={15} />
                  {isImporting
                    ? t`Validating and installing`
                    : t`Install Skills`}
                </Button>
              </div>
              <div className="op-skill-import__scan-list">
                {scannedSkills.map((skill) => (
                  <div
                    className="op-skill-import__scan-row"
                    key={skill.subpath || skill.name}
                  >
                    <div className="op-skill-import__scan-skill">
                      <strong>{skill.name}</strong>
                      <span>{skill.description}</span>
                      {skill.subpath ? <small>{skill.subpath}</small> : null}
                    </div>
                    <Select
                      aria-label={`${skill.name}: ${t`Associated feature module`}`}
                      onChange={(key) =>
                        setUrlSkillModules((current) => ({
                          ...current,
                          [skill.subpath]:
                            String(key) === "unassociated" ? "" : String(key),
                        }))
                      }
                      selectionMode="single"
                      value={urlSkillModules[skill.subpath] || "unassociated"}
                      variant="secondary"
                    >
                      <Select.Trigger>
                        <Select.Value>
                          {urlSkillModules[skill.subpath]
                            ? moduleLabel(urlSkillModules[skill.subpath], t)
                            : t`Do not associate`}
                        </Select.Value>
                        <Select.Indicator />
                      </Select.Trigger>
                      <Select.Popover>
                        <ListBox>
                          <ListBox.Item
                            id="unassociated"
                            textValue={t`Do not associate`}
                          >
                            {t`Do not associate`}
                          </ListBox.Item>
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
                  </div>
                ))}
              </div>
            </div>
          ) : null}
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
      {sourceTab === "local" ? (
        <>
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
        </>
      ) : null}
      {sourceTab === "url" && localError ? (
        <div className="op-skill-import__error">{localError}</div>
      ) : null}
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
