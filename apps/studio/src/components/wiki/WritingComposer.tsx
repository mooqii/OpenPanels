import {
  Alert,
  Button,
  Dropdown,
  Input,
  Label,
  ListBox,
  Select,
  Separator,
  Tabs,
  TextArea,
} from "@heroui/react"
import {
  BookOpen,
  Eye,
  FileOutput,
  MoreHorizontal,
  PanelLeft,
  Pencil,
  Send,
  Sparkles,
  Trash2,
} from "lucide-react"
import { useCallback, useEffect, useMemo, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { apiJson } from "../../lib/api"
import {
  activeWritingSkillIds,
  distillationTaskGroups,
  selectSingleSkill,
  toggleWritingSkillSelection,
  writingReferenceSelectionError,
  writingSkillSelectionError,
} from "../../lib/writing"
import type {
  ManagedProjectSkill,
  ManagedSkillModule,
  MyDocument,
  MyOpenPanelsTransport,
  ProjectTask,
  WritingState,
} from "../../types"
import { ConfirmDialog, SkillFilesDialog, type SkillTextFile } from "./Dialogs"

interface WikiAgentSelection {
  isWikiSelected: boolean
  selectedMyDocumentIds: string[]
}

export function WritingComposer({
  documents,
  isSelectionBusy,
  onOpenAgentTasks,
  onOpenLibrary,
  onManageSkills,
  onReload,
  selection,
  skillsRevision,
  state,
  tasks,
  transport,
}: {
  documents: MyDocument[]
  isSelectionBusy: boolean
  onOpenAgentTasks: (
    filter: "active" | "pending" | "all",
    taskIds?: string[]
  ) => void
  onOpenLibrary: () => void
  onManageSkills: () => void
  onReload: () => Promise<void>
  selection: WikiAgentSelection
  skillsRevision: number
  state: WritingState
  tasks: ProjectTask[]
  transport: MyOpenPanelsTransport
}) {
  const { t } = useMyOpenPanelsI18n()
  const [createDraft, setCreateDraft] = useState(
    state.createDraft ?? (state.mode === "revise" ? "" : state.draft)
  )
  const [revisionDraft, setRevisionDraft] = useState(
    state.revisionDraft ?? (state.mode === "revise" ? state.draft : "")
  )
  const [mode, setMode] = useState<WritingState["mode"]>(state.mode)
  const [distillationName, setDistillationName] = useState(
    state.distillationName
  )
  const [targetId, setTargetId] = useState<string | null>(
    state.targetMyDocumentId
  )
  const [selectedCreateWritingSkillIds, setSelectedCreateWritingSkillIds] =
    useState(state.selectedCreateWritingSkillIds)
  const [selectedRevisionWritingSkillId, setSelectedRevisionWritingSkillId] =
    useState<string | null>(state.selectedRevisionWritingSkillId)
  const [selectedDistillationSkillId, setSelectedDistillationSkillId] =
    useState(
      state.selectedDistillationSkillId || "writing-distillation-default"
    )
  const selectedWritingSkillIds = activeWritingSkillIds(
    mode,
    selectedCreateWritingSkillIds,
    selectedRevisionWritingSkillId
  )
  const draft = mode === "revise" ? revisionDraft : createDraft
  const [writingSkills, setWritingSkills] = useState<ManagedProjectSkill[]>([])
  const [distillationSkills, setDistillationSkills] = useState<
    ManagedProjectSkill[]
  >([])
  const [skillFilesDialog, setSkillFilesDialog] = useState<{
    files: SkillTextFile[]
    skill: ManagedProjectSkill
  } | null>(null)
  const [pendingDeleteSkill, setPendingDeleteSkill] =
    useState<ManagedProjectSkill | null>(null)
  const [isDeletingSkill, setIsDeletingSkill] = useState(false)
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const distillationGroups = useMemo(
    () => distillationTaskGroups(tasks),
    [tasks]
  )
  const distillationTaskVersion = useMemo(
    () =>
      tasks
        .filter(
          (task) =>
            task.queue === "writing" && task.type === "distill_writing_skill"
        )
        .map((task) => `${task.id}:${task.status}:${task.updatedAt}`)
        .join("|"),
    [tasks]
  )

  useEffect(() => {
    if (targetId && !documents.some((document) => document.id === targetId)) {
      setTargetId(null)
    }
  }, [documents, targetId])

  useEffect(() => {
    let isCancelled = false
    apiJson<{ modules: ManagedSkillModule[] }>(
      transport.apiBase,
      `/api/skills?taskVersion=${encodeURIComponent(distillationTaskVersion)}&skillsRevision=${skillsRevision}`
    )
      .then((data) => {
        if (!isCancelled) {
          setWritingSkills(
            data.modules.find((module) => module.kind === "writing")?.skills ??
              []
          )
          setDistillationSkills(
            data.modules.find(
              (module) => module.kind === "writing-distillation"
            )?.skills ?? []
          )
        }
      })
      .catch((skillError) => {
        if (!isCancelled) {
          console.error("Failed to load Writing Skills", skillError)
        }
      })
    return () => {
      isCancelled = true
    }
  }, [distillationTaskVersion, skillsRevision, transport.apiBase])

  useEffect(() => {
    const timer = window.setTimeout(() => {
      apiJson(transport.apiBase, "/api/writing/draft", {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          draft,
          createDraft,
          mode,
          distillationName,
          revisionDraft,
          selectedCreateWritingSkillIds,
          selectedDistillationSkillId,
          selectedRevisionWritingSkillId,
          targetMyDocumentId: mode === "revise" ? targetId : null,
        }),
      }).catch((saveError) => {
        console.error("Failed to save Writing draft", saveError)
      })
    }, 500)
    return () => window.clearTimeout(timer)
  }, [
    draft,
    createDraft,
    mode,
    distillationName,
    revisionDraft,
    selectedCreateWritingSkillIds,
    selectedDistillationSkillId,
    selectedRevisionWritingSkillId,
    targetId,
    transport.apiBase,
  ])

  const skillSelectionError = writingSkillSelectionError(
    mode,
    selectedWritingSkillIds
  )
  const hasValidSkillSelection = skillSelectionError === null
  const selectedMyDocuments = documents.filter((document) =>
    selection.selectedMyDocumentIds.includes(document.id)
  )
  const unreadySourceCount = selectedMyDocuments.filter(
    (document) => document.writeOperation !== undefined
  ).length
  const selectedSourceCount = selectedMyDocuments.length
  const selectedReferenceCount =
    selectedSourceCount + Number(selection.isWikiSelected)
  const referenceSelectionError = writingReferenceSelectionError(
    mode,
    selectedReferenceCount,
    unreadySourceCount
  )
  const hasValidReferenceSelection = referenceSelectionError === null
  const hasValidDistillationSkillSelection = distillationSkills.some(
    (item) => item.id === selectedDistillationSkillId
  )

  const submit = useCallback(async () => {
    if (
      !(
        draft.trim() &&
        hasValidSkillSelection &&
        hasValidReferenceSelection &&
        (mode !== "revise" || targetId)
      )
    ) {
      return
    }
    setIsSubmitting(true)
    setError(null)
    try {
      await apiJson(transport.apiBase, "/api/writing/requests", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          instruction: draft,
          mode,
          targetMyDocumentId: mode === "revise" ? targetId : null,
          writingSkillIds: selectedWritingSkillIds,
        }),
      })
      if (mode === "revise") setRevisionDraft("")
      else setCreateDraft("")
      setTargetId(null)
      await onReload()
    } catch (submitError) {
      setError(
        submitError instanceof Error
          ? submitError.message
          : t`Failed to submit writing request`
      )
    } finally {
      setIsSubmitting(false)
    }
  }, [
    draft,
    hasValidReferenceSelection,
    hasValidSkillSelection,
    mode,
    onReload,
    selectedWritingSkillIds,
    t,
    targetId,
    transport.apiBase,
  ])

  const submitDistillation = useCallback(async () => {
    if (
      !distillationName.trim() ||
      distillationName.trim().length > 80 ||
      selectedSourceCount === 0 ||
      unreadySourceCount > 0 ||
      !hasValidDistillationSkillSelection
    ) {
      return
    }
    setIsSubmitting(true)
    setError(null)
    try {
      await apiJson(transport.apiBase, "/api/writing/distillation-requests", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          name: distillationName,
          distillerSkillId: selectedDistillationSkillId,
        }),
      })
      setDistillationName("")
      await onReload()
    } catch (submitError) {
      setError(
        submitError instanceof Error
          ? submitError.message
          : t`Failed to submit distillation request`
      )
    } finally {
      setIsSubmitting(false)
    }
  }, [
    onReload,
    hasValidDistillationSkillSelection,
    distillationName,
    selectedDistillationSkillId,
    selectedSourceCount,
    t,
    transport.apiBase,
    unreadySourceCount,
  ])

  const toggleWritingSkill = useCallback(
    (skillId: string, isSelected: boolean) => {
      if (mode === "revise") {
        setSelectedRevisionWritingSkillId((current) =>
          selectSingleSkill(current, skillId, isSelected)
        )
        return
      }
      setSelectedCreateWritingSkillIds((current) =>
        toggleWritingSkillSelection(current, skillId, isSelected, "create")
      )
    },
    [mode]
  )

  const openSkillFiles = useCallback(
    async (skill: ManagedProjectSkill) => {
      const payload = await apiJson<{ files?: SkillTextFile[] }>(
        transport.apiBase,
        `/api/skills/${encodeURIComponent(skill.id)}`
      )
      setSkillFilesDialog({ files: payload.files ?? [], skill })
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
    setIsDeletingSkill(true)
    try {
      await apiJson(
        transport.apiBase,
        `/api/skills/${encodeURIComponent(pendingDeleteSkill.id)}`,
        { method: "DELETE" }
      )
      const deletedId = pendingDeleteSkill.id
      setWritingSkills((current) =>
        current.filter((item) => item.id !== deletedId)
      )
      setDistillationSkills((current) =>
        current.filter((item) => item.id !== deletedId)
      )
      if (selectedDistillationSkillId === deletedId) {
        setSelectedDistillationSkillId(
          distillationSkills.find((item) => item.id !== deletedId)?.id ?? ""
        )
      }
      setSelectedCreateWritingSkillIds((current) =>
        current.filter((id) => id !== deletedId)
      )
      setSelectedRevisionWritingSkillId((current) =>
        current === deletedId ? null : current
      )
      setPendingDeleteSkill(null)
    } finally {
      setIsDeletingSkill(false)
    }
  }, [
    pendingDeleteSkill,
    distillationSkills,
    selectedDistillationSkillId,
    transport.apiBase,
  ])

  const renderSelectedSources = (
    includeWiki: boolean,
    title: string,
    emptyMessage?: string
  ) => {
    const count =
      selectedSourceCount + Number(includeWiki && selection.isWikiSelected)
    return (
      <div className="op-writing-distillation__sources">
        <strong className="op-writing-section-title">
          {title}: {count}
        </strong>
        {count === 0 && emptyMessage ? (
          <Alert
            className="op-writing-distillation__warning op-writing-distillation__warning--empty"
            status="warning"
          >
            <Alert.Indicator />
            <Alert.Content>
              <Alert.Title className="op-writing-distillation__warning-text">
                {emptyMessage}
              </Alert.Title>
            </Alert.Content>
          </Alert>
        ) : count === 0 ? (
          <div className="op-wiki-empty-inline">
            {t`No reference documents selected`}
          </div>
        ) : (
          <>
            <ul className="op-writing-distillation__source-list">
              {includeWiki && selection.isWikiSelected ? (
                <li>
                  <BookOpen aria-hidden size={14} />
                  <span title={t`Structured Wiki`}>{t`Structured Wiki`}</span>
                  <small>{t`Wiki`}</small>
                </li>
              ) : null}
              {selectedMyDocuments.map((document) => (
                <li key={document.id}>
                  <FileOutput aria-hidden size={14} />
                  <span title={document.title}>{document.title}</span>
                  <small>{t`My Documents`}</small>
                </li>
              ))}
            </ul>
            {unreadySourceCount > 0 ? (
              <Alert
                className="op-writing-distillation__warning"
                status="warning"
              >
                <Alert.Indicator />
                <Alert.Content>
                  <Alert.Title className="op-writing-distillation__warning-text">
                    {t`Some selected documents are not ready. Wait for processing or deselect them.`}
                  </Alert.Title>
                </Alert.Content>
              </Alert>
            ) : null}
          </>
        )}
      </div>
    )
  }

  const renderSkillList = (
    items: ManagedProjectSkill[],
    selectedIds: string[],
    onToggle: (skillId: string, isSelected: boolean) => void,
    inputIdPrefix: string,
    emptyLabel: string
  ) => (
    <div className="op-writing-skills__list">
      {items.map((item) => (
        <div className="op-writing-skill" key={item.id}>
          <Button
            aria-label={`${t`Select Writing Skill`}: ${item.name}`}
            aria-pressed={selectedIds.includes(item.id)}
            className="op-writing-skill__selector"
            id={`${inputIdPrefix}-${item.id}`}
            isIconOnly
            onPress={() => onToggle(item.id, !selectedIds.includes(item.id))}
            variant="ghost"
          >
            <span aria-hidden />
          </Button>
          <div className="op-writing-skill__body">
            <span className="op-writing-skill__title">
              <strong title={item.name}>{item.name}</strong>
            </span>
            <span className="op-writing-skill__description">
              {item.description}
            </span>
          </div>
          <Dropdown>
            <Button
              aria-label={`${t`Writing Skill actions`}: ${item.name}`}
              className="op-writing-skill__menu"
              isIconOnly
              size="sm"
              variant="ghost"
            >
              <MoreHorizontal size={16} />
            </Button>
            <Dropdown.Popover>
              <Dropdown.Menu
                onAction={(key) => {
                  if (key === "view" || key === "edit") {
                    openSkillFiles(item).catch((openError) => {
                      console.error("Failed to open Writing Skill", openError)
                    })
                  } else if (key === "delete") {
                    setPendingDeleteSkill(item)
                  }
                }}
              >
                {item.canEdit ? (
                  <>
                    <Dropdown.Item id="edit" textValue={t`Edit`}>
                      <Pencil size={14} />
                      <Label>{t`Edit`}</Label>
                    </Dropdown.Item>
                    <Separator />
                    {item.canDelete ? (
                      <Dropdown.Item
                        id="delete"
                        textValue={t`Delete`}
                        variant="danger"
                      >
                        <Trash2 size={14} />
                        <Label>{t`Delete`}</Label>
                      </Dropdown.Item>
                    ) : null}
                  </>
                ) : (
                  <Dropdown.Item id="view" textValue={t`View`}>
                    <Eye size={14} />
                    <Label>{t`View`}</Label>
                  </Dropdown.Item>
                )}
              </Dropdown.Menu>
            </Dropdown.Popover>
          </Dropdown>
        </div>
      ))}
      {items.length === 0 ? (
        <div className="op-wiki-empty-inline">{emptyLabel}</div>
      ) : null}
    </div>
  )

  return (
    <section className="op-wiki-column op-writing-composer">
      <div className="op-wiki-column__header">
        <div className="op-wiki-column__title">
          <Button
            aria-label={t`Open document library`}
            className="op-writing-mobile-library-button"
            isIconOnly
            onPress={onOpenLibrary}
            size="sm"
            variant="ghost"
          >
            <PanelLeft size={17} />
          </Button>
          <h2>{t`Writing`}</h2>
        </div>
        <div className="op-writing-distillation-statuses">
          {(
            [
              [
                "active",
                t`distillation in progress`,
                t`distillations in progress`,
              ],
              ["waiting", t`distillation waiting`, t`distillations waiting`],
              ["error", t`distillation error`, t`distillation errors`],
            ] as const
          ).map(([group, singularLabel, pluralLabel]) => {
            const groupTasks = distillationGroups[group]
            if (!groupTasks.length) return null
            return (
              <Button
                className={`op-writing-distillation-status op-writing-distillation-status--${group}`}
                key={group}
                onPress={() =>
                  onOpenAgentTasks(
                    "all",
                    groupTasks.map((task) => task.id)
                  )
                }
                size="sm"
                variant={group === "error" ? "danger" : "ghost"}
              >
                {groupTasks.length}{" "}
                {groupTasks.length === 1 ? singularLabel : pluralLabel}
              </Button>
            )
          })}
        </div>
      </div>
      <div className="op-writing-composer__content op-wiki-column__content">
        <Tabs
          className="op-writing-mode"
          onSelectionChange={(key) =>
            setMode(String(key) as WritingState["mode"])
          }
          selectedKey={mode}
        >
          <Tabs.ListContainer>
            <Tabs.List aria-label={t`Writing mode`}>
              <Tabs.Tab id="create">
                {t`Create`}
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="revise">
                {t`Revise`}
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="distill">
                {t`Distill`}
                <Tabs.Indicator />
              </Tabs.Tab>
            </Tabs.List>
          </Tabs.ListContainer>
        </Tabs>

        {mode === "distill" ? (
          <div className="op-writing-distillation">
            <div className="op-writing-distillation__intro">
              <Sparkles aria-hidden size={18} />
              <div>
                <strong>{t`Turn selected articles into a Writing Skill`}</strong>
                <p>
                  {t`The Agent will extract reusable voice, structure, pacing, and techniques from all selected documents in My Documents.`}
                </p>
              </div>
            </div>
            {renderSelectedSources(
              false,
              t`Selected articles`,
              t`Select at least one document from My Documents`
            )}
            <div className="op-writing-skills">
              <div className="op-writing-skills__header">
                <strong className="op-writing-section-title">
                  {t`Distillation Skill`}
                </strong>
                <div className="op-writing-skills__header-actions">
                  <span>{t`Select one`}</span>
                  <Button
                    className="op-writing-skills__manage"
                    onPress={onManageSkills}
                    size="sm"
                    variant="ghost"
                  >
                    {t`Manage Skill`}
                  </Button>
                </div>
              </div>
              {renderSkillList(
                distillationSkills,
                [selectedDistillationSkillId],
                (skillId, isSelected) => {
                  setSelectedDistillationSkillId(
                    (current) =>
                      selectSingleSkill(current, skillId, isSelected) ?? ""
                  )
                },
                "distillation-skill",
                t`No Distillation Skills available`
              )}
              {hasValidDistillationSkillSelection ? null : (
                <div className="op-writing-error">
                  {t`Select a Distillation Skill`}
                </div>
              )}
            </div>
            <div className="op-writing-target">
              <Label className="op-writing-section-title">
                {t`Writing Skill name`}
              </Label>
              <Input
                aria-label={t`Writing Skill name`}
                fullWidth
                maxLength={80}
                onChange={(event) => setDistillationName(event.target.value)}
                placeholder={t`Name this reusable writing method`}
                value={distillationName}
              />
            </div>
            {error ? <div className="op-writing-error">{error}</div> : null}
          </div>
        ) : (
          <>
            {mode === "revise" ? (
              <div className="op-writing-target">
                <Label className="op-writing-section-title">
                  {t`Document to revise`}
                </Label>
                <Select
                  aria-label={t`Document to revise`}
                  onChange={(key) => setTargetId(key ? String(key) : null)}
                  selectionMode="single"
                  value={targetId ?? ""}
                >
                  <Select.Trigger>
                    <Select.Value>
                      {documents.find((document) => document.id === targetId)
                        ?.title ?? t`Select a document from My Documents`}
                    </Select.Value>
                    <Select.Indicator />
                  </Select.Trigger>
                  <Select.Popover>
                    <ListBox>
                      {documents.map((document) => (
                        <ListBox.Item
                          id={document.id}
                          key={document.id}
                          textValue={document.title}
                        >
                          {document.title}
                        </ListBox.Item>
                      ))}
                    </ListBox>
                  </Select.Popover>
                </Select>
              </div>
            ) : null}

            {mode === "create"
              ? renderSelectedSources(
                  true,
                  t`Selected references`,
                  t`Select at least one reference document`
                )
              : mode === "revise"
                ? renderSelectedSources(true, t`Selected reference documents`)
                : null}

            <div className="op-writing-skills">
              <div className="op-writing-skills__header">
                <strong className="op-writing-section-title">
                  {t`Writing Skills`}
                </strong>
                <div className="op-writing-skills__header-actions">
                  <span>
                    {mode === "revise"
                      ? t`Select one`
                      : t`Multiple Skills generate multiple articles`}
                  </span>
                  <Button
                    className="op-writing-skills__manage"
                    onPress={onManageSkills}
                    size="sm"
                    variant="ghost"
                  >
                    {t`Manage Skill`}
                  </Button>
                </div>
              </div>
              {renderSkillList(
                writingSkills,
                selectedWritingSkillIds,
                toggleWritingSkill,
                "writing-skill",
                t`No Writing Skills available`
              )}
              {skillSelectionError === "required" ? (
                <div className="op-writing-error">
                  {t`Select at least one Writing Skill`}
                </div>
              ) : skillSelectionError === "revision_limit" ? (
                <div className="op-writing-error">
                  {t`Revision mode supports one Writing Skill`}
                </div>
              ) : null}
            </div>

            <TextArea
              aria-label={
                mode === "revise"
                  ? t`Revision instructions`
                  : t`New document instructions`
              }
              className="op-writing-instructions"
              fullWidth
              onChange={(event) => {
                if (mode === "revise") setRevisionDraft(event.target.value)
                else setCreateDraft(event.target.value)
              }}
              placeholder={
                mode === "revise"
                  ? t`Describe how the agent should revise this document`
                  : t`Describe what the agent should write in the new document`
              }
              value={draft}
            />
            {error ? <div className="op-writing-error">{error}</div> : null}
          </>
        )}
      </div>
      <div className="op-writing-submit-dock">
        {mode === "distill" ? (
          <Button
            className="op-writing-submit"
            isDisabled={
              isSubmitting ||
              isSelectionBusy ||
              !distillationName.trim() ||
              distillationName.trim().length > 80 ||
              selectedSourceCount === 0 ||
              unreadySourceCount > 0 ||
              !hasValidDistillationSkillSelection
            }
            onPress={() => submitDistillation()}
            variant="primary"
          >
            <Sparkles size={15} />
            <span>{isSubmitting ? t`Submitting` : t`Start distillation`}</span>
          </Button>
        ) : (
          <Button
            className="op-writing-submit"
            isDisabled={
              isSubmitting ||
              isSelectionBusy ||
              !draft.trim() ||
              !hasValidSkillSelection ||
              !hasValidReferenceSelection ||
              (mode === "revise" && !targetId)
            }
            onPress={() => submit()}
            variant="primary"
          >
            <Send size={15} />
            <span>
              {isSubmitting
                ? t`Submitting`
                : mode === "revise"
                  ? t`Start revision`
                  : t`Start writing`}
            </span>
          </Button>
        )}
      </div>
      {skillFilesDialog ? (
        <SkillFilesDialog
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
          cancelLabel={t`Cancel`}
          confirmLabel={t`Delete`}
          isBusy={isDeletingSkill}
          message={t`After deletion, this Writing Skill can no longer be used.`}
          onCancel={() => setPendingDeleteSkill(null)}
          onConfirm={() =>
            deleteSkill().catch((deleteError) => {
              console.error("Failed to delete Writing Skill", deleteError)
            })
          }
          title={t`Delete Writing Skill?`}
        />
      ) : null}
    </section>
  )
}
