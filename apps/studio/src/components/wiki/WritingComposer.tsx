import {
  Alert,
  Button,
  Checkbox,
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
  Eye,
  FileOutput,
  FileText,
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
  refinementTaskGroups,
  toggleWritingSkillSelection,
  writingSkillSelectionError,
} from "../../lib/writing"
import type {
  AgentSkillListing,
  MyOpenPanelsTransport,
  ProjectTask,
  WikiGeneratedDocument,
  WikiRawDocument,
  WritingState,
} from "../../types"
import { ConfirmDialog, SkillFilesDialog, type SkillTextFile } from "./Dialogs"

interface WikiAgentSelection {
  isWikiSelected: boolean
  selectedGeneratedDocumentIds: string[]
  selectedRawDocumentIds: string[]
}

export function WritingComposer({
  documents,
  isSelectionBusy,
  onOpenAgentTasks,
  onOpenLibrary,
  onReload,
  rawDocuments,
  selection,
  state,
  tasks,
  transport,
}: {
  documents: WikiGeneratedDocument[]
  isSelectionBusy: boolean
  onOpenAgentTasks: (
    filter: "active" | "pending" | "all",
    taskIds?: string[]
  ) => void
  onOpenLibrary: () => void
  onReload: () => Promise<void>
  rawDocuments: WikiRawDocument[]
  selection: WikiAgentSelection
  state: WritingState
  tasks: ProjectTask[]
  transport: MyOpenPanelsTransport
}) {
  const { t } = useMyOpenPanelsI18n()
  const [draft, setDraft] = useState(state.draft)
  const [mode, setMode] = useState<WritingState["mode"]>(state.mode)
  const [refinementName, setRefinementName] = useState(state.refinementName)
  const [targetId, setTargetId] = useState<string | null>(
    state.targetGeneratedDocumentId
  )
  const [selectedCreateWritingSkillIds, setSelectedCreateWritingSkillIds] =
    useState(state.selectedCreateWritingSkillIds)
  const [selectedRevisionWritingSkillId, setSelectedRevisionWritingSkillId] =
    useState<string | null>(state.selectedRevisionWritingSkillId)
  const selectedWritingSkillIds = activeWritingSkillIds(
    mode,
    selectedCreateWritingSkillIds,
    selectedRevisionWritingSkillId
  )
  const [writingSkills, setWritingSkills] = useState<AgentSkillListing[]>([])
  const [skillFilesDialog, setSkillFilesDialog] = useState<{
    files: SkillTextFile[]
    skill: AgentSkillListing
  } | null>(null)
  const [pendingDeleteSkill, setPendingDeleteSkill] =
    useState<AgentSkillListing | null>(null)
  const [isDeletingSkill, setIsDeletingSkill] = useState(false)
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const refinementGroups = useMemo(() => refinementTaskGroups(tasks), [tasks])
  const refinementTaskVersion = useMemo(
    () =>
      tasks
        .filter(
          (task) =>
            task.queue === "writing" && task.type === "refine_writing_skill"
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
    apiJson<{
      skills?: AgentSkillListing[]
    }>(
      transport.apiBase,
      `/api/writing/skills?taskVersion=${encodeURIComponent(refinementTaskVersion)}`
    )
      .then((data) => {
        if (!isCancelled) setWritingSkills(data.skills ?? [])
      })
      .catch((skillError) => {
        if (!isCancelled) {
          console.error("Failed to load Writing Skills", skillError)
        }
      })
    return () => {
      isCancelled = true
    }
  }, [refinementTaskVersion, transport.apiBase])

  useEffect(() => {
    const timer = window.setTimeout(() => {
      apiJson(transport.apiBase, "/api/writing/draft", {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          draft,
          mode,
          refinementName,
          selectedCreateWritingSkillIds,
          selectedRevisionWritingSkillId,
          targetGeneratedDocumentId: mode === "revise" ? targetId : null,
        }),
      }).catch((saveError) => {
        console.error("Failed to save Writing draft", saveError)
      })
    }, 500)
    return () => window.clearTimeout(timer)
  }, [
    draft,
    mode,
    refinementName,
    selectedCreateWritingSkillIds,
    selectedRevisionWritingSkillId,
    targetId,
    transport.apiBase,
  ])

  const skillSelectionError = writingSkillSelectionError(
    mode,
    selectedWritingSkillIds
  )
  const hasValidSkillSelection = skillSelectionError === null
  const selectedRawDocuments = rawDocuments.filter((document) =>
    selection.selectedRawDocumentIds.includes(document.id)
  )
  const selectedGeneratedDocuments = documents.filter((document) =>
    selection.selectedGeneratedDocumentIds.includes(document.id)
  )
  const unreadySourceCount =
    selectedRawDocuments.filter((document) => !document.markdownRef).length +
    selectedGeneratedDocuments.filter(
      (document) =>
        document.generation !== undefined &&
        document.generation.status !== "completed"
    ).length
  const selectedSourceCount =
    selectedRawDocuments.length + selectedGeneratedDocuments.length

  const submit = useCallback(async () => {
    if (
      !(
        draft.trim() &&
        hasValidSkillSelection &&
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
          targetGeneratedDocumentId: mode === "revise" ? targetId : null,
          writingSkillIds: selectedWritingSkillIds,
        }),
      })
      setDraft("")
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
    hasValidSkillSelection,
    mode,
    onReload,
    selectedWritingSkillIds,
    t,
    targetId,
    transport.apiBase,
  ])

  const submitRefinement = useCallback(async () => {
    if (
      !refinementName.trim() ||
      refinementName.trim().length > 80 ||
      selectedSourceCount === 0 ||
      unreadySourceCount > 0
    ) {
      return
    }
    setIsSubmitting(true)
    setError(null)
    try {
      await apiJson(transport.apiBase, "/api/writing/refinement-requests", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name: refinementName }),
      })
      setRefinementName("")
      await onReload()
    } catch (submitError) {
      setError(
        submitError instanceof Error
          ? submitError.message
          : t`Failed to submit refinement request`
      )
    } finally {
      setIsSubmitting(false)
    }
  }, [
    onReload,
    refinementName,
    selectedSourceCount,
    t,
    transport.apiBase,
    unreadySourceCount,
  ])

  const toggleWritingSkill = useCallback(
    (skillId: string, isSelected: boolean) => {
      if (mode === "revise") {
        setSelectedRevisionWritingSkillId(isSelected ? skillId : null)
        return
      }
      setSelectedCreateWritingSkillIds((current) =>
        toggleWritingSkillSelection(current, skillId, isSelected, "create")
      )
    },
    [mode]
  )

  const openSkillFiles = useCallback(
    async (skill: AgentSkillListing) => {
      const payload = await apiJson<{ files?: SkillTextFile[] }>(
        transport.apiBase,
        `/api/writing/skills/${encodeURIComponent(skill.skill.id)}`
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
        `/api/writing/skills/${encodeURIComponent(skillFilesDialog.skill.skill.id)}/file`,
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
        `/api/writing/skills/${encodeURIComponent(pendingDeleteSkill.skill.id)}`,
        { method: "DELETE" }
      )
      const deletedId = pendingDeleteSkill.skill.id
      setWritingSkills((current) =>
        current.filter((item) => item.skill.id !== deletedId)
      )
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
  }, [pendingDeleteSkill, transport.apiBase])

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
        <div className="op-writing-refinement-statuses">
          {(
            [
              ["active", t`refinement in progress`, t`refinements in progress`],
              ["waiting", t`refinement waiting`, t`refinements waiting`],
              ["error", t`refinement error`, t`refinement errors`],
            ] as const
          ).map(([group, singularLabel, pluralLabel]) => {
            const groupTasks = refinementGroups[group]
            if (!groupTasks.length) return null
            return (
              <Button
                className={`op-writing-refinement-status op-writing-refinement-status--${group}`}
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
                {t`New document`}
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="revise">
                {t`Revise`}
                <Tabs.Indicator />
              </Tabs.Tab>
              <Tabs.Tab id="refine">
                {t`Refine`}
                <Tabs.Indicator />
              </Tabs.Tab>
            </Tabs.List>
          </Tabs.ListContainer>
        </Tabs>

        {mode === "refine" ? (
          <div className="op-writing-refinement">
            <div className="op-writing-refinement__intro">
              <Sparkles aria-hidden size={18} />
              <div>
                <strong>{t`Turn selected articles into a Writing Skill`}</strong>
                <p>
                  {t`The Agent will extract reusable voice, structure, pacing, and techniques from all selected raw and generated documents.`}
                </p>
              </div>
            </div>
            <div className="op-writing-target">
              <Label className="op-writing-section-title">
                {t`Writing Skill name`}
              </Label>
              <Input
                aria-label={t`Writing Skill name`}
                fullWidth
                maxLength={80}
                onChange={(event) => setRefinementName(event.target.value)}
                placeholder={t`Name this reusable writing method`}
                value={refinementName}
              />
            </div>
            <div className="op-writing-refinement__sources">
              <strong className="op-writing-section-title">
                {t("Selected articles")}: {selectedSourceCount}
              </strong>
              {selectedSourceCount === 0 ? (
                <Alert
                  className="op-writing-refinement__warning"
                  status="warning"
                >
                  <Alert.Indicator />
                  <Alert.Content>
                    <Alert.Title className="op-writing-refinement__warning-text">
                      {t`Select at least one raw or generated document`}
                    </Alert.Title>
                  </Alert.Content>
                </Alert>
              ) : (
                <>
                  <ul className="op-writing-refinement__source-list">
                    {selectedRawDocuments.map((document) => (
                      <li key={document.id}>
                        <FileText aria-hidden size={14} />
                        <span title={document.title}>{document.title}</span>
                        <small>{t`Raw Documents`}</small>
                      </li>
                    ))}
                    {selectedGeneratedDocuments.map((document) => (
                      <li key={document.id}>
                        <FileOutput aria-hidden size={14} />
                        <span title={document.title}>{document.title}</span>
                        <small>{t`Generated Documents`}</small>
                      </li>
                    ))}
                  </ul>
                  {unreadySourceCount > 0 ? (
                    <Alert
                      className="op-writing-refinement__warning"
                      status="warning"
                    >
                      <Alert.Indicator />
                      <Alert.Content>
                        <Alert.Title className="op-writing-refinement__warning-text">
                          {t`Some selected documents are not ready. Wait for processing or deselect them.`}
                        </Alert.Title>
                      </Alert.Content>
                    </Alert>
                  ) : null}
                </>
              )}
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
                        ?.title ?? t`Select a generated document`}
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

            <div className="op-writing-skills">
              <div className="op-writing-skills__header">
                <strong className="op-writing-section-title">
                  {t`Writing Skills`}
                </strong>
                <span>
                  {mode === "revise" ? t`Select one` : t`Select one or more`}
                </span>
              </div>
              <div className="op-writing-skills__list">
                {writingSkills.map((item) => (
                  <div className="op-writing-skill" key={item.skill.id}>
                    <Checkbox
                      aria-label={`${t`Select Writing Skill`}: ${item.skill.title}`}
                      id={`writing-skill-${item.skill.id}`}
                      isSelected={selectedWritingSkillIds.includes(
                        item.skill.id
                      )}
                      onChange={(isSelected) =>
                        toggleWritingSkill(item.skill.id, isSelected)
                      }
                    />
                    <label
                      className="op-writing-skill__body"
                      htmlFor={`writing-skill-${item.skill.id}`}
                    >
                      <span className="op-writing-skill__title">
                        <strong>{item.skill.title}</strong>
                        <span>
                          {item.source === "builtin"
                            ? t`Built-in`
                            : t`Self-built`}
                        </span>
                      </span>
                      <span className="op-writing-skill__description">
                        {item.skill.description}
                      </span>
                    </label>
                    <Dropdown>
                      <Button
                        aria-label={`${t`Writing Skill actions`}: ${item.skill.title}`}
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
                                console.error(
                                  "Failed to open Writing Skill",
                                  openError
                                )
                              })
                            } else if (key === "delete") {
                              setPendingDeleteSkill(item)
                            }
                          }}
                        >
                          {item.source === "builtin" ? (
                            <Dropdown.Item id="view" textValue={t`View`}>
                              <Eye size={14} />
                              <Label>{t`View`}</Label>
                            </Dropdown.Item>
                          ) : (
                            <>
                              <Dropdown.Item id="edit" textValue={t`Edit`}>
                                <Pencil size={14} />
                                <Label>{t`Edit`}</Label>
                              </Dropdown.Item>
                              <Separator />
                              <Dropdown.Item
                                id="delete"
                                textValue={t`Delete`}
                                variant="danger"
                              >
                                <Trash2 size={14} />
                                <Label>{t`Delete`}</Label>
                              </Dropdown.Item>
                            </>
                          )}
                        </Dropdown.Menu>
                      </Dropdown.Popover>
                    </Dropdown>
                  </div>
                ))}
                {writingSkills.length === 0 ? (
                  <div className="op-wiki-empty-inline">
                    {t`No Writing Skills available`}
                  </div>
                ) : null}
              </div>
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
              aria-label={t`Writing instructions`}
              className="op-writing-instructions"
              fullWidth
              onChange={(event) => setDraft(event.target.value)}
              placeholder={t`Describe what the agent should write`}
              value={draft}
            />
            {error ? <div className="op-writing-error">{error}</div> : null}
          </>
        )}
      </div>
      <div className="op-writing-submit-dock">
        {mode === "refine" ? (
          <Button
            className="op-writing-submit"
            isDisabled={
              isSubmitting ||
              isSelectionBusy ||
              !refinementName.trim() ||
              refinementName.trim().length > 80 ||
              selectedSourceCount === 0 ||
              unreadySourceCount > 0
            }
            onPress={() => submitRefinement()}
            variant="primary"
          >
            <Sparkles size={15} />
            <span>{isSubmitting ? t`Submitting` : t`Start refinement`}</span>
          </Button>
        ) : (
          <Button
            className="op-writing-submit"
            isDisabled={
              isSubmitting ||
              !draft.trim() ||
              !hasValidSkillSelection ||
              (mode === "revise" && !targetId)
            }
            onPress={() => submit()}
            variant="primary"
          >
            <Send size={15} />
            <span>{isSubmitting ? t`Submitting` : t`Start writing`}</span>
          </Button>
        )}
      </div>
      {skillFilesDialog ? (
        <SkillFilesDialog
          closeLabel={t`Close`}
          files={skillFilesDialog.files}
          onClose={() => setSkillFilesDialog(null)}
          onSave={saveSkillFile}
          readOnly={skillFilesDialog.skill.source === "builtin"}
          title={skillFilesDialog.skill.skill.title}
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
