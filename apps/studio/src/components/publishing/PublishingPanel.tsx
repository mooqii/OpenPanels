import { Button, Chip, Modal, Spinner, Tooltip } from "@heroui/react"
import {
  AlertTriangle,
  CheckCircle2,
  CircleHelp,
  CircleX,
  Clock3,
  LoaderCircle,
  Play,
  Plus,
  Send,
  X,
} from "lucide-react"
import { type ReactNode, useEffect, useMemo, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { useTypesettingStateEditor } from "../../hooks/use-typesetting-state-editor"
import { apiJson } from "../../lib/api"
import { randomId } from "../../lib/id"
import {
  type PublishingPublicationStatus,
  publishingAttemptIsActive,
  publishingAttemptStatus,
  publishingPublicationSummary,
  publishingSourceHasContent,
  typesettingContentToPlainText,
} from "../../lib/publishing"
import type {
  ManagedProjectSkill,
  ManagedSkillModule,
  MyOpenPanelsTransport,
  ProjectTask,
  PublishingAttempt,
  PublishingRelease,
  PublishingState,
  TaskExecutionScope,
  TypesettingPublication,
  TypesettingState,
} from "../../types"
import { PublicationContentModule } from "../typesetting/TypesettingLibrary"
import { PublicationDetail } from "../typesetting/TypesettingPublication"
import { ConfirmDialog } from "../wiki/Dialogs"
import { PublicationPreview } from "./PublicationPreview"

interface PublishingResponse {
  attempt?: PublishingAttempt
  release?: PublishingRelease
  revision: number
  state: PublishingState
  task?: ProjectTask
}

type PendingAction =
  | { kind: "release"; skillId: string; skillName: string }
  | {
      acknowledgedUnknown: boolean
      kind: "attempt"
      mode: "auto" | "manual"
      release: PublishingRelease
      skillId: string
      skillName: string
    }

export function PublishingPanel({
  chromeContent,
  onAddSkill,
  onOpenAgentTasks,
  onOpenManualTask,
  onStateSaved,
  onTypesettingStateSaved,
  projectId,
  state: initialState,
  skillsRevision,
  tasks,
  transport,
  typesetting,
  typesettingPanelId,
  typesettingRevision,
}: {
  chromeContent: ReactNode
  onAddSkill: () => void
  onOpenAgentTasks: (taskIds: string[]) => void
  onOpenManualTask: (scope: TaskExecutionScope) => void
  onStateSaved: (
    state: PublishingState,
    revision: number,
    task?: ProjectTask
  ) => void
  onTypesettingStateSaved: (state: TypesettingState, revision: number) => void
  projectId: string
  state: PublishingState
  skillsRevision: number
  tasks: ProjectTask[]
  transport: MyOpenPanelsTransport
  typesetting: TypesettingState
  typesettingPanelId: string
  typesettingRevision: number
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const [state, setState] = useState(initialState)
  const [skills, setSkills] = useState<ManagedProjectSkill[]>([])
  const [skillsLoading, setSkillsLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [isEditing, setIsEditing] = useState(false)
  const [isPublicationModuleCollapsed, setIsPublicationModuleCollapsed] =
    useState(false)
  const [isSourceListOpen, setIsSourceListOpen] = useState(false)
  const [pendingDelete, setPendingDelete] =
    useState<TypesettingPublication | null>(null)
  const [pendingAction, setPendingAction] = useState<PendingAction | null>(null)

  const {
    flushSave: flushTypesettingSave,
    importAsset,
    replaceState: replaceTypesettingState,
    saveError: typesettingSaveError,
    saveStatus: typesettingSaveStatus,
    state: editableTypesetting,
    updatePublication,
  } = useTypesettingStateEditor({
    initialState: typesetting,
    onStateSaved: onTypesettingStateSaved,
    panelId: typesettingPanelId,
    projectId,
    revision: typesettingRevision,
    transport,
  })

  useEffect(() => setState(initialState), [initialState])

  useEffect(() => {
    let cancelled = false
    setSkillsLoading(true)
    apiJson<{ modules: ManagedSkillModule[] }>(
      transport.apiBase,
      `/api/skills?refresh=${skillsRevision}`
    )
      .then((response) => {
        if (cancelled) return
        setSkills(
          response.modules.find((module) => module.kind === "publishing")
            ?.skills ?? []
        )
      })
      .catch((cause) => {
        if (!cancelled) setError(String((cause as Error)?.message || cause))
      })
      .finally(() => {
        if (!cancelled) setSkillsLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [skillsRevision, transport.apiBase])

  const selectedPublication = useMemo(() => {
    const selected = editableTypesetting.publications.find(
      (publication) => publication.id === state.selectedPublicationId
    )
    return selected ?? editableTypesetting.publications[0] ?? null
  }, [editableTypesetting.publications, state.selectedPublicationId])
  const fallbackSkillId = skills.some(
    (skill) => skill.id === state.selectedSkillIds.xiaohongshu
  )
    ? state.selectedSkillIds.xiaohongshu
    : (skills[0]?.id ?? "publishing-xiaohongshu")
  const bodyText = selectedPublication
    ? typesettingContentToPlainText(selectedPublication.content)
    : ""
  const relatedReleases = selectedPublication
    ? state.releases.filter(
        (release) => release.sourcePublicationId === selectedPublication.id
      )
    : []
  const taskById = new Map(tasks.map((task) => [task.id, task]))
  const attemptsBySkill = useMemo(() => {
    const visibleTaskIds = new Set(tasks.map((task) => task.id))
    const grouped = new Map<
      string,
      Array<{ attempt: PublishingAttempt; release: PublishingRelease }>
    >()
    for (const release of relatedReleases) {
      for (const attempt of [...release.attempts].reverse()) {
        if (!visibleTaskIds.has(attempt.taskId)) continue
        const current = grouped.get(attempt.skillId) ?? []
        current.push({ attempt, release })
        grouped.set(attempt.skillId, current)
      }
    }
    return grouped
  }, [relatedReleases, tasks])
  const publicationSummaryById = useMemo(() => {
    const releasesByPublicationId = new Map<string, PublishingRelease[]>()
    for (const release of state.releases) {
      const current =
        releasesByPublicationId.get(release.sourcePublicationId) ?? []
      current.push(release)
      releasesByPublicationId.set(release.sourcePublicationId, current)
    }
    return new Map(
      editableTypesetting.publications.map((publication) => [
        publication.id,
        publishingPublicationSummary(
          releasesByPublicationId.get(publication.id) ?? [],
          tasks
        ),
      ])
    )
  }, [editableTypesetting.publications, state.releases, tasks])
  const sourceComplete = Boolean(
    selectedPublication &&
      publishingSourceHasContent(bodyText, selectedPublication.covers.length)
  )
  const skillRows = [
    ...skills.map((skill) => ({ ...skill, isInstalled: true as const })),
    ...Array.from(attemptsBySkill.entries())
      .filter(([skillId]) => !skills.some((skill) => skill.id === skillId))
      .map(([skillId, attempts]) => ({
        description: t`This Skill is no longer installed`,
        id: skillId,
        isInstalled: false as const,
        name: attempts[0]?.attempt.skillName ?? skillId,
      })),
  ]

  async function savePreference(
    publicationId: string | null,
    skillId = fallbackSkillId
  ) {
    setError(null)
    try {
      const response = await apiJson<PublishingResponse>(
        transport.apiBase,
        "/api/publishing/preferences",
        {
          body: JSON.stringify({
            selectedPublicationId: publicationId,
            skillId,
          }),
          headers: { "content-type": "application/json" },
          method: "PUT",
        }
      )
      setState(response.state)
      onStateSaved(response.state, response.revision)
    } catch (cause) {
      setError(String((cause as Error)?.message || cause))
    }
  }

  async function executePendingAction() {
    if (!(pendingAction && selectedPublication)) return
    setIsSubmitting(true)
    setError(null)
    try {
      const response =
        pendingAction.kind === "release"
          ? await apiJson<PublishingResponse>(
              transport.apiBase,
              "/api/publishing/releases",
              {
                body: JSON.stringify({
                  publicationId: selectedPublication.id,
                  requestId: randomId("publishing-request"),
                  skillId: pendingAction.skillId,
                }),
                headers: { "content-type": "application/json" },
                method: "POST",
              }
            )
          : await apiJson<PublishingResponse>(
              transport.apiBase,
              `/api/publishing/releases/${encodeURIComponent(pendingAction.release.id)}/attempts`,
              {
                body: JSON.stringify({
                  acknowledgedUnknown: pendingAction.acknowledgedUnknown,
                  mode: pendingAction.mode,
                  requestId: randomId("publishing-request"),
                  skillId: pendingAction.skillId,
                }),
                headers: { "content-type": "application/json" },
                method: "POST",
              }
            )
      setState(response.state)
      onStateSaved(response.state, response.revision, response.task)
      if (pendingAction.kind === "attempt" && pendingAction.mode === "manual") {
        const taskId = response.task?.id ?? response.attempt?.taskId
        if (taskId) onOpenManualTask({ kind: "exact-task", taskId })
      }
      setPendingAction(null)
    } catch (cause) {
      setError(String((cause as Error)?.message || cause))
    } finally {
      setIsSubmitting(false)
    }
  }

  return (
    <section className="op-publishing-panel">
      <header className="op-canvas-title">{chromeContent}</header>
      <div className="op-publishing-workspace">
        {isSourceListOpen ? (
          <button
            aria-label={t`Close publication content`}
            className="op-publishing-source-backdrop"
            onClick={() => setIsSourceListOpen(false)}
            type="button"
          />
        ) : null}
        <aside
          aria-label={t`Publication content`}
          className={`op-publishing-sources ${isSourceListOpen ? "is-open" : ""}`}
        >
          <div className="op-publishing-sources__mobile-header">
            <strong>{t`Publication content`}</strong>
            <Button
              aria-label={t`Close publication content`}
              isIconOnly
              onPress={() => setIsSourceListOpen(false)}
              size="sm"
              variant="ghost"
            >
              <X size={16} />
            </Button>
          </div>
          <PublicationContentModule
            activePublicationId={selectedPublication?.id ?? null}
            className="op-publishing-publications-module"
            isCollapsed={isPublicationModuleCollapsed}
            onOpenPublication={(publication) => {
              savePreference(publication.id)
              setIsSourceListOpen(false)
            }}
            onToggle={() =>
              setIsPublicationModuleCollapsed((current) => !current)
            }
            publications={editableTypesetting.publications}
            renderPublicationMeta={(publication) => {
              const publishedCount =
                publicationSummaryById.get(publication.id)?.publishedCount ?? 0
              return publishedCount ? (
                <span>
                  {locale === "zh-CN"
                    ? `${publishedCount.toLocaleString(locale)}${t`published`}`
                    : `${publishedCount.toLocaleString(locale)} ${t`published`}`}
                </span>
              ) : null
            }}
            renderPublicationStatus={(publication) =>
              publicationSummaryById
                .get(publication.id)
                ?.statuses.slice(0, 1)
                .map((status) => (
                  <PublicationStatusChip key={status} status={status} t={t} />
                )) ?? null
            }
            transport={transport}
          />
        </aside>

        <section
          className={
            isEditing
              ? "is-editing op-publishing-detail"
              : "op-publishing-detail"
          }
        >
          {isEditing && selectedPublication ? (
            <main className="op-publishing-editor op-publishing-module">
              <PublicationDetail
                importAsset={importAsset}
                key={selectedPublication.id}
                onDelete={() => setPendingDelete(selectedPublication)}
                onFlushSave={flushTypesettingSave}
                onInsertHandlerChange={() => undefined}
                onOpenAgentTasks={onOpenAgentTasks}
                onOpenLibrary={() => setIsSourceListOpen(true)}
                onPreview={() => setIsEditing(false)}
                onRetrySave={() =>
                  flushTypesettingSave().catch(() => undefined)
                }
                onUpdate={(updater) =>
                  updatePublication(selectedPublication.id, updater)
                }
                publication={selectedPublication}
                saveError={typesettingSaveError}
                saveStatus={typesettingSaveStatus}
                tasks={tasks}
                transport={transport}
              />
            </main>
          ) : (
            <>
              {selectedPublication ? (
                <PublicationPreview
                  onEdit={() => setIsEditing(true)}
                  onOpenSources={() => setIsSourceListOpen(true)}
                  publication={selectedPublication}
                  transport={transport}
                />
              ) : (
                <main className="op-publishing-module op-publishing-preview">
                  <EmptyMessage
                    icon={<Send size={21} />}
                    message={t`No content selected`}
                  />
                </main>
              )}

              <div className="op-publishing-side-stack">
                <section className="op-publishing-module op-publishing-status-module">
                  <div className="op-publishing-section-heading">
                    <h2>{t`Publishing status`}</h2>
                    <Button
                      aria-label={t`Add content publishing Skill`}
                      isIconOnly
                      onPress={onAddSkill}
                      size="sm"
                      variant="ghost"
                    >
                      <Plus size={16} />
                    </Button>
                  </div>
                  {skillsLoading ? (
                    <div className="op-publishing-skill-loading">
                      <Spinner size="sm" /> {t`Loading...`}
                    </div>
                  ) : skillRows.length ? (
                    <div className="op-publishing-status-list">
                      {skillRows.map((skill) => {
                        const attempts = attemptsBySkill.get(skill.id) ?? []
                        const hasActiveAttempt = attempts.some(({ attempt }) =>
                          publishingAttemptIsActive(
                            attempt,
                            taskById.get(attempt.taskId)
                          )
                        )
                        return (
                          <section
                            className="op-publishing-skill-status"
                            key={skill.id}
                          >
                            <div className="op-publishing-skill-status__header">
                              <strong className="op-publishing-skill-status__name">
                                {skill.name}
                              </strong>
                              {skill.isInstalled ? (
                                <Button
                                  isDisabled={
                                    !sourceComplete || hasActiveAttempt
                                  }
                                  isPending={
                                    isSubmitting &&
                                    pendingAction?.skillId === skill.id
                                  }
                                  onPress={() =>
                                    setPendingAction({
                                      kind: "release",
                                      skillId: skill.id,
                                      skillName: skill.name,
                                    })
                                  }
                                  size="sm"
                                  variant="secondary"
                                >
                                  <Play size={14} />
                                  {hasActiveAttempt ? t`In progress` : t`Start`}
                                </Button>
                              ) : (
                                <Chip size="sm" variant="soft">
                                  {t`Unavailable`}
                                </Chip>
                              )}
                            </div>
                            {attempts.length ? (
                              <div className="op-publishing-skill-attempts">
                                {attempts.map(({ attempt }) => (
                                  <AttemptRow
                                    attempt={attempt}
                                    key={attempt.id}
                                    onOpenTask={() =>
                                      onOpenAgentTasks([attempt.taskId])
                                    }
                                    t={t}
                                    task={taskById.get(attempt.taskId)}
                                  />
                                ))}
                              </div>
                            ) : (
                              <p className="op-publishing-skill-status__empty">
                                {t`No publishing tasks yet`}
                              </p>
                            )}
                          </section>
                        )
                      })}
                    </div>
                  ) : (
                    <EmptyMessage
                      icon={<Send size={21} />}
                      message={t`No content publishing Skills installed`}
                    />
                  )}
                  {error ? (
                    <p className="op-publishing-error">{error}</p>
                  ) : null}
                </section>
              </div>
            </>
          )}
        </section>
      </div>
      {pendingAction ? (
        <PublishingConfirmDialog
          action={pendingAction}
          isBusy={isSubmitting}
          onCancel={() => setPendingAction(null)}
          onConfirm={executePendingAction}
          publication={selectedPublication}
          t={t}
        />
      ) : null}
      {pendingDelete ? (
        <ConfirmDialog
          cancelLabel={t`Cancel`}
          confirmLabel={t`Delete`}
          isBusy={false}
          message={t`This publication project and its layout content will be removed.`}
          onCancel={() => setPendingDelete(null)}
          onConfirm={() => {
            const nextPublications = editableTypesetting.publications.filter(
              (publication) => publication.id !== pendingDelete.id
            )
            replaceTypesettingState(
              { ...editableTypesetting, publications: nextPublications },
              pendingDelete.id,
              { deleted: true }
            )
            setPendingDelete(null)
            setIsEditing(false)
            savePreference(nextPublications[0]?.id ?? null).catch(
              () => undefined
            )
          }}
          title={t`Delete publication project?`}
        />
      ) : null}
    </section>
  )
}

function AttemptRow({
  attempt,
  onOpenTask,
  task,
  t,
}: {
  attempt: PublishingAttempt
  onOpenTask: () => void
  task?: ProjectTask
  t: (value: TemplateStringsArray) => string
}) {
  const status = publishingAttemptStatus(attempt, task)
  const label = publishingStatusLabel(status, t)
  return (
    <div className="op-publishing-attempt">
      <time dateTime={attempt.createdAt}>
        {new Date(attempt.createdAt).toLocaleString()}
      </time>
      <Tooltip closeDelay={0} delay={300}>
        <Button
          aria-label={`${label}: ${t`Open task`}`}
          className="op-publishing-attempt__status"
          data-status={status}
          isIconOnly
          onPress={onOpenTask}
          size="sm"
          variant="ghost"
        >
          {publishingStatusIcon(status)}
        </Button>
        <Tooltip.Content placement="top">{label}</Tooltip.Content>
      </Tooltip>
    </div>
  )
}

function publishingStatusIcon(
  status: ReturnType<typeof publishingAttemptStatus>
) {
  if (status === "queued") return <Clock3 size={16} />
  if (status === "running") return <LoaderCircle size={16} />
  if (status === "committing") return <Send size={16} />
  if (status === "published") return <CheckCircle2 size={16} />
  if (status === "needs_user_action") return <AlertTriangle size={16} />
  if (status === "not_published") return <CircleX size={16} />
  return <CircleHelp size={16} />
}

function PublicationStatusChip({
  status,
  t,
}: {
  status: PublishingPublicationStatus
  t: (value: TemplateStringsArray) => string
}) {
  const color =
    status === "error"
      ? "danger"
      : status === "pending" || status === "needs_user_action"
        ? "warning"
        : status === "publishing"
          ? "accent"
          : "default"
  const label =
    status === "pending"
      ? t`Pending publish`
      : status === "publishing"
        ? t`Publishing now`
        : status === "needs_user_action"
          ? t`Needs user action`
          : status === "error"
            ? t`Publishing error`
            : t`Publishing status unknown`

  return (
    <Chip
      className="op-typesetting-publication-row__status"
      color={color}
      size="sm"
      variant="soft"
    >
      {label}
    </Chip>
  )
}

function PublishingConfirmDialog({
  action,
  isBusy,
  onCancel,
  onConfirm,
  publication,
  t,
}: {
  action: PendingAction
  isBusy: boolean
  onCancel: () => void
  onConfirm: () => void
  publication: TypesettingPublication | null
  t: (value: TemplateStringsArray) => string
}) {
  const unknown = action.kind === "attempt" && action.acknowledgedUnknown
  return (
    <Modal.Backdrop isOpen onOpenChange={(open) => !open && onCancel()}>
      <Modal.Container placement="center" size="sm">
        <Modal.Dialog>
          <Modal.CloseTrigger />
          <Modal.Header>
            <Modal.Icon className={unknown ? "text-warning" : "text-accent"}>
              {unknown ? <AlertTriangle size={20} /> : <Send size={20} />}
            </Modal.Icon>
            <Modal.Heading>
              {unknown
                ? t`Confirm another publishing attempt`
                : t`Start publishing task?`}
            </Modal.Heading>
          </Modal.Header>
          <Modal.Body>
            <p>
              {unknown
                ? t`The previous attempt may already have published. Check the target platform before continuing.`
                : publication?.covers.length
                  ? `${action.skillName}: ${publication.covers.length} ${t`The images will be used in order and the Agent will perform the final publishing action once.`}`
                  : `${action.skillName}: ${t`The text content will be used and the Agent will perform the final publishing action once.`}`}
            </p>
          </Modal.Body>
          <Modal.Footer>
            <Button
              isDisabled={isBusy}
              onPress={onCancel}
              variant="tertiary"
            >{t`Cancel`}</Button>
            <Button isPending={isBusy} onPress={onConfirm}>
              {action.kind === "attempt" && action.mode === "manual"
                ? t`Create handoff`
                : t`Confirm start`}
            </Button>
          </Modal.Footer>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}

function EmptyMessage({ icon, message }: { icon: ReactNode; message: string }) {
  return (
    <div className="op-publishing-empty">
      {icon}
      <span>{message}</span>
    </div>
  )
}

function publishingStatusLabel(
  status: ReturnType<typeof publishingAttemptStatus>,
  t: (value: TemplateStringsArray) => string
) {
  if (status === "queued") return t`Queued`
  if (status === "running") return t`Running`
  if (status === "committing") return t`Submitting`
  if (status === "published") return t`Published`
  if (status === "needs_user_action") return t`Needs user action`
  if (status === "not_published") return t`Not published`
  return t`Result unknown`
}
