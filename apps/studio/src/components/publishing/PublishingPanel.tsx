import { Button, Chip, Modal, Spinner, Tooltip } from "@heroui/react"
import { AlertTriangle, KeyRound, Send, X } from "lucide-react"
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { useTypesettingStateEditor } from "../../hooks/use-typesetting-state-editor"
import { apiJson } from "../../lib/api"
import { randomId } from "../../lib/id"
import {
  publishingAttemptIsActive,
  publishingPublicationSummary,
  publishingSourceHasContent,
  typesettingContentToPlainText,
} from "../../lib/publishing"
import {
  createTypesettingPublication,
  selectPublicationTitle,
} from "../../lib/typesetting"
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
import {
  PublicationDetail,
  PublicationModeHeader,
  type PublicationView,
} from "../typesetting/TypesettingPublication"
import { ConfirmDialog } from "../wiki/Dialogs"
import { PublicationPreview } from "./PublicationPreview"
import { PublishingAttemptRow } from "./PublishingAttemptRow"
import {
  loadWechatConfiguration,
  WECHAT_API_SKILL_ID,
  WechatApiConfigurationDialog,
  type WechatConfigurationStatus,
} from "./WechatApiConfigurationDialog"

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
      mode: "manual"
      release: PublishingRelease
      skillId: string
      skillName: string
    }

export function PublishingPanel({
  chromeContent,
  onAddSkill,
  onManageSkillModule,
  onOpenAgentTasks,
  onOpenManualTask,
  onStateSaved,
  panelId,
  projectId,
  state: initialState,
  skillsRevision,
  tasks,
  transport,
}: {
  chromeContent: ReactNode
  onAddSkill: () => void
  onManageSkillModule: (moduleKind: string) => void
  onOpenAgentTasks: (taskIds: string[]) => void
  onOpenManualTask: (scope: TaskExecutionScope) => void
  onStateSaved: (
    state: PublishingState,
    revision: number,
    task?: ProjectTask
  ) => void
  panelId: string
  projectId: string
  state: PublishingState
  skillsRevision: number
  tasks: ProjectTask[]
  transport: MyOpenPanelsTransport
}) {
  const { locale, t } = useMyOpenPanelsI18n()
  const [state, setState] = useState(initialState)
  const [skills, setSkills] = useState<ManagedProjectSkill[]>([])
  const [skillsLoading, setSkillsLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [submittingSkillId, setSubmittingSkillId] = useState<string | null>(
    null
  )
  const [view, setView] = useState<PublicationView>("preview")
  const [isSourceListOpen, setIsSourceListOpen] = useState(false)
  const [pendingDelete, setPendingDelete] =
    useState<TypesettingPublication | null>(null)
  const [pendingAction, setPendingAction] = useState<PendingAction | null>(null)
  const [wechatConfiguration, setWechatConfiguration] =
    useState<WechatConfigurationStatus | null>(null)
  const [wechatConfigurationOpen, setWechatConfigurationOpen] = useState(false)
  const [wechatIssueReasonCode, setWechatIssueReasonCode] = useState<
    string | null
  >(null)
  const [wechatPendingAction, setWechatPendingAction] =
    useState<PendingAction | null>(null)
  const [wechatPendingDirect, setWechatPendingDirect] = useState(false)
  const [checkingWechatConfiguration, setCheckingWechatConfiguration] =
    useState(false)
  const promptedWechatAttemptRef = useRef<string | null>(null)
  const [publicationState, setPublicationState] = useState<TypesettingState>({
    publications: [],
  })
  const [publicationRevision, setPublicationRevision] = useState(0)

  const {
    flushSave: flushTypesettingSave,
    importAsset,
    replaceState: replaceTypesettingState,
    saveError: typesettingSaveError,
    saveStatus: typesettingSaveStatus,
    state: editableTypesetting,
    updatePublication,
    uploadAsset,
  } = useTypesettingStateEditor({
    initialState: publicationState,
    onStateSaved: (next, revision) => {
      setPublicationState(next)
      setPublicationRevision(revision)
    },
    panelId,
    revision: publicationRevision,
    transport,
  })

  useEffect(() => setState(initialState), [initialState])

  useEffect(() => {
    if (!projectId) return
    let cancelled = false
    apiJson<{ releases: PublishingRelease[] }>(
      transport.apiBase,
      "/api/releases"
    )
      .then((response) => {
        if (!cancelled) {
          setState((current) => ({ ...current, releases: response.releases }))
        }
      })
      .catch((cause) => {
        if (!cancelled) setError(String((cause as Error)?.message || cause))
      })
    return () => {
      cancelled = true
    }
  }, [projectId, transport.apiBase])

  useEffect(() => {
    if (!projectId) return
    let cancelled = false
    apiJson<{
      publications: TypesettingPublication[]
      revision: number
    }>(transport.apiBase, "/api/publications")
      .then((response) => {
        if (cancelled) return
        setPublicationState({ publications: response.publications })
        setPublicationRevision(response.revision)
      })
      .catch((cause) => {
        if (!cancelled) setError(String((cause as Error)?.message || cause))
      })
    return () => {
      cancelled = true
    }
  }, [projectId, transport.apiBase])

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
          response.modules.find((module) => module.kind === "release")
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
    : (skills[0]?.id ?? "release-xiaohongshu")
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
        if (
          attempt.mode !== "direct" &&
          !visibleTaskIds.has(attempt.taskId ?? "")
        ) {
          continue
        }
        const current = grouped.get(attempt.skillId) ?? []
        current.push({ attempt, release })
        grouped.set(attempt.skillId, current)
      }
    }
    return grouped
  }, [relatedReleases, tasks])
  const wechatConfigurationProblem = useMemo(
    () =>
      state.releases
        .flatMap((release) =>
          release.attempts.map((attempt) => ({ attempt, release }))
        )
        .filter(
          ({ attempt }) =>
            attempt.skillId === WECHAT_API_SKILL_ID &&
            Boolean(attempt.taskId) &&
            attempt.outcome === "needs_user_action" &&
            isWechatConfigurationReason(attempt.reasonCode)
        )
        .sort((left, right) =>
          right.attempt.createdAt.localeCompare(left.attempt.createdAt)
        )[0],
    [state.releases]
  )
  useEffect(() => {
    const problem = wechatConfigurationProblem
    if (!problem || promptedWechatAttemptRef.current === problem.attempt.id) {
      return
    }
    promptedWechatAttemptRef.current = problem.attempt.id
    setWechatIssueReasonCode(problem.attempt.reasonCode)
    setWechatPendingAction({
      acknowledgedUnknown: false,
      kind: "attempt",
      mode: "manual",
      release: problem.release,
      skillId: problem.attempt.skillId,
      skillName: problem.attempt.skillName,
    })
    setWechatConfigurationOpen(true)
  }, [wechatConfigurationProblem])
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
    {
      description: t`Built-in direct API submission`,
      id: WECHAT_API_SKILL_ID,
      isDirect: true as const,
      isInstalled: true as const,
      name: t`WeChat Official Account (API)`,
    },
    ...skills
      .filter((skill) => skill.id !== WECHAT_API_SKILL_ID)
      .map((skill) => ({
        ...skill,
        isDirect: false as const,
        isInstalled: true as const,
      })),
    ...Array.from(attemptsBySkill.entries())
      .filter(
        ([skillId]) =>
          skillId !== WECHAT_API_SKILL_ID &&
          !skills.some((skill) => skill.id === skillId)
      )
      .map(([skillId, attempts]) => ({
        description: t`This Skill is no longer installed`,
        id: skillId,
        isDirect: false as const,
        isInstalled: false as const,
        name: attempts[0]?.attempt.skillName ?? skillId,
      })),
  ]
  const publishingStatusModule = (
    <div className="op-publishing-side-stack">
      <section className="op-publishing-module op-publishing-status-module">
        <div className="op-publishing-section-heading">
          <h2>{t`Publish`}</h2>
          <Button
            aria-label={t`Manage Skill`}
            className="op-publishing-skills__manage"
            onPress={onAddSkill}
            size="sm"
            variant="ghost"
          >
            {t`Manage Skill`}
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
                  taskById.get(attempt.taskId ?? "")
                )
              )
              return (
                <section className="op-publishing-skill-status" key={skill.id}>
                  <div className="op-publishing-skill-status__header">
                    <strong className="op-publishing-skill-status__name">
                      {skill.name}
                    </strong>
                    {skill.isInstalled ? (
                      <div className="op-publishing-skill-status__actions">
                        {skill.isDirect ? (
                          <Tooltip closeDelay={0} delay={300}>
                            <Button
                              aria-label={t`Configure WeChat API`}
                              isDisabled={isSubmitting}
                              isIconOnly
                              onPress={() => {
                                setWechatIssueReasonCode(null)
                                setWechatPendingAction(null)
                                setWechatPendingDirect(false)
                                setWechatConfigurationOpen(true)
                              }}
                              size="sm"
                              variant="ghost"
                            >
                              <KeyRound size={14} />
                            </Button>
                            <Tooltip.Content placement="top">
                              {t`Configure WeChat API`}
                            </Tooltip.Content>
                          </Tooltip>
                        ) : null}
                        <Button
                          isDisabled={
                            !sourceComplete || hasActiveAttempt || isSubmitting
                          }
                          isPending={
                            submittingSkillId === skill.id ||
                            (skill.id === WECHAT_API_SKILL_ID &&
                              checkingWechatConfiguration)
                          }
                          onPress={() =>
                            skill.isDirect
                              ? beginDirectWechatDraft()
                              : beginRelease({
                                  kind: "release",
                                  skillId: skill.id,
                                  skillName: skill.name,
                                })
                          }
                          size="sm"
                          variant="secondary"
                        >
                          <Send size={14} />
                          {hasActiveAttempt ? t`In progress` : t`Publish`}
                        </Button>
                      </div>
                    ) : (
                      <Chip size="sm" variant="soft">
                        {t`Unavailable`}
                      </Chip>
                    )}
                  </div>
                  {attempts.length ? (
                    <div className="op-publishing-skill-attempts">
                      {attempts.map(({ attempt }) => (
                        <PublishingAttemptRow
                          attempt={attempt}
                          key={attempt.id}
                          onOpenTask={
                            attempt.taskId
                              ? () =>
                                  onOpenAgentTasks([attempt.taskId as string])
                              : undefined
                          }
                          t={t}
                          task={taskById.get(attempt.taskId ?? "")}
                        />
                      ))}
                    </div>
                  ) : (
                    <p className="op-publishing-skill-status__empty">
                      {skill.isDirect
                        ? t`No submissions yet`
                        : t`No publishing tasks yet`}
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
        {error ? <p className="op-publishing-error">{error}</p> : null}
      </section>
    </div>
  )

  function createPublication() {
    const timestamp = new Date().toISOString()
    const publication = createTypesettingPublication(
      randomId("publication"),
      timestamp
    )
    replaceTypesettingState(
      {
        ...editableTypesetting,
        publications: [publication, ...editableTypesetting.publications],
      },
      publication.id
    )
    setView("edit")
    setIsSourceListOpen(false)
    savePreference(publication.id).catch(() => undefined)
  }

  async function savePreference(
    publicationId: string | null,
    skillId = fallbackSkillId
  ) {
    setError(null)
    try {
      const response = await apiJson<PublishingResponse>(
        transport.apiBase,
        "/api/panels/publishing/preferences",
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

  async function executeAction(action: PendingAction) {
    if (!selectedPublication) return
    setIsSubmitting(true)
    setSubmittingSkillId(action.skillId)
    setError(null)
    try {
      if (action.kind === "release") await flushTypesettingSave()
      const response =
        action.kind === "release"
          ? await apiJson<PublishingResponse>(
              transport.apiBase,
              "/api/releases",
              {
                body: JSON.stringify({
                  publicationId: selectedPublication.id,
                  requestId: randomId("publishing-request"),
                  skillId: action.skillId,
                }),
                headers: { "content-type": "application/json" },
                method: "POST",
              }
            )
          : await apiJson<PublishingResponse>(
              transport.apiBase,
              `/api/releases/${encodeURIComponent(action.release.id)}/attempts`,
              {
                body: JSON.stringify({
                  acknowledgedUnknown: action.acknowledgedUnknown,
                  mode: action.mode,
                  requestId: randomId("publishing-request"),
                  skillId: action.skillId,
                }),
                headers: { "content-type": "application/json" },
                method: "POST",
              }
            )
      setState(response.state)
      onStateSaved(response.state, response.revision, response.task)
      const taskId = response.task?.id ?? response.attempt?.taskId
      if (taskId) onOpenManualTask({ kind: "exact-task", taskId })
      setPendingAction(null)
    } catch (cause) {
      setError(String((cause as Error)?.message || cause))
    } finally {
      setIsSubmitting(false)
      setSubmittingSkillId(null)
    }
  }

  async function beginRelease(action: PendingAction) {
    if (action.skillId !== WECHAT_API_SKILL_ID) {
      await executeAction(action)
      return
    }
    setCheckingWechatConfiguration(true)
    setError(null)
    try {
      const configuration = await loadWechatConfiguration(transport.apiBase)
      setWechatConfiguration(configuration)
      if (!configuration.ready) {
        setWechatIssueReasonCode(configuration.reasonCode)
        setWechatPendingAction(action)
        setWechatConfigurationOpen(true)
        return
      }
      await executeAction(action)
    } catch (cause) {
      setError(String((cause as Error)?.message || cause))
    } finally {
      setCheckingWechatConfiguration(false)
    }
  }

  async function submitDirectWechatDraft() {
    if (!selectedPublication) return
    setIsSubmitting(true)
    setSubmittingSkillId(WECHAT_API_SKILL_ID)
    setError(null)
    try {
      await flushTypesettingSave()
      const response = await apiJson<PublishingResponse>(
        transport.apiBase,
        "/api/publishing/wechat/drafts",
        {
          body: JSON.stringify({
            publicationId: selectedPublication.id,
            requestId: randomId("wechat-draft-request"),
          }),
          headers: { "content-type": "application/json" },
          method: "POST",
        }
      )
      setState(response.state)
      onStateSaved(response.state, response.revision)
      const attempt = response.attempt
      if (
        attempt?.outcome === "needs_user_action" &&
        isWechatConfigurationReason(attempt.reasonCode)
      ) {
        setWechatIssueReasonCode(attempt.reasonCode)
        setWechatPendingDirect(true)
        setWechatConfigurationOpen(true)
      } else if (attempt?.outcome && attempt.outcome !== "published") {
        setError(attempt.summary ?? t`WeChat draft submission failed`)
      }
    } catch (cause) {
      setError(String((cause as Error)?.message || cause))
    } finally {
      setIsSubmitting(false)
      setSubmittingSkillId(null)
    }
  }

  async function beginDirectWechatDraft() {
    setCheckingWechatConfiguration(true)
    setError(null)
    try {
      const configuration = await loadWechatConfiguration(transport.apiBase)
      setWechatConfiguration(configuration)
      if (!configuration.ready) {
        setWechatIssueReasonCode(configuration.reasonCode)
        setWechatPendingDirect(true)
        setWechatConfigurationOpen(true)
        return
      }
      await submitDirectWechatDraft()
    } catch (cause) {
      setError(String((cause as Error)?.message || cause))
    } finally {
      setCheckingWechatConfiguration(false)
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
            createButtonIconOnly
            onCreatePublication={createPublication}
            onOpenPublication={(publication) => {
              savePreference(publication.id)
              setIsSourceListOpen(false)
            }}
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
            transport={transport}
          />
        </aside>

        <section
          className={
            view === "edit"
              ? "is-editing op-publishing-detail"
              : "op-publishing-detail"
          }
        >
          {view === "edit" && selectedPublication ? (
            <>
              <main className="op-publishing-editor op-publishing-module">
                <PublicationModeHeader
                  onDelete={() => setPendingDelete(selectedPublication)}
                  onOpenLibrary={() => setIsSourceListOpen(true)}
                  onRetrySave={() =>
                    flushTypesettingSave().catch(() => undefined)
                  }
                  onViewChange={setView}
                  publication={selectedPublication}
                  saveError={typesettingSaveError}
                  saveStatus={typesettingSaveStatus}
                  view={view}
                />
                <PublicationDetail
                  importAsset={importAsset}
                  key={selectedPublication.id}
                  onDelete={() => setPendingDelete(selectedPublication)}
                  onFlushSave={flushTypesettingSave}
                  onInsertHandlerChange={() => undefined}
                  onManageSkillModule={onManageSkillModule}
                  onOpenAgentTasks={onOpenAgentTasks}
                  onOpenLibrary={() => setIsSourceListOpen(true)}
                  onPreview={() => setView("preview")}
                  onRetrySave={() =>
                    flushTypesettingSave().catch(() => undefined)
                  }
                  onUpdate={(updater) =>
                    updatePublication(selectedPublication.id, updater)
                  }
                  projectId={projectId}
                  publication={selectedPublication}
                  saveError={typesettingSaveError}
                  saveStatus={typesettingSaveStatus}
                  showHeader={false}
                  tasks={tasks}
                  transport={transport}
                  uploadAsset={uploadAsset}
                />
              </main>
              {publishingStatusModule}
            </>
          ) : (
            <>
              {selectedPublication ? (
                <PublicationPreview
                  className="op-publishing-preview--with-mode-header"
                  modeHeader={
                    <PublicationModeHeader
                      onDelete={() => setPendingDelete(selectedPublication)}
                      onOpenLibrary={() => setIsSourceListOpen(true)}
                      onRetrySave={() =>
                        flushTypesettingSave().catch(() => undefined)
                      }
                      onViewChange={setView}
                      publication={selectedPublication}
                      saveError={typesettingSaveError}
                      saveStatus={typesettingSaveStatus}
                      view={view}
                    />
                  }
                  onEdit={() => setView("edit")}
                  onOpenSources={() => setIsSourceListOpen(true)}
                  onSelectTitle={(titleId) =>
                    updatePublication(selectedPublication.id, (current) => ({
                      ...selectPublicationTitle(current, titleId),
                      updatedAt: new Date().toISOString(),
                    }))
                  }
                  publication={selectedPublication}
                  showHeader={false}
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

              {publishingStatusModule}
            </>
          )}
        </section>
      </div>
      {pendingAction?.kind === "attempt" ? (
        <PublishingConfirmDialog
          action={pendingAction}
          isBusy={isSubmitting}
          onCancel={() => setPendingAction(null)}
          onConfirm={() => executeAction(pendingAction)}
          publication={selectedPublication}
          t={t}
        />
      ) : null}
      <WechatApiConfigurationDialog
        initialStatus={wechatConfiguration}
        isOpen={wechatConfigurationOpen}
        issueReasonCode={wechatIssueReasonCode}
        onCancel={() => {
          setWechatConfigurationOpen(false)
          setWechatPendingAction(null)
          setWechatPendingDirect(false)
        }}
        onReady={() => {
          const action = wechatPendingAction
          const submitDirect = wechatPendingDirect
          setWechatConfigurationOpen(false)
          setWechatIssueReasonCode(null)
          setWechatPendingAction(null)
          setWechatPendingDirect(false)
          if (submitDirect) {
            submitDirectWechatDraft()
          } else if (action) {
            executeAction(action)
          }
        }}
        transport={transport}
      />
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
            setView("preview")
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
              {t`Create handoff`}
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

function isWechatConfigurationReason(reasonCode: string | null) {
  return Boolean(
    reasonCode &&
      [
        "wechat_api_unauthorized",
        "wechat_app_id_missing",
        "wechat_app_secret_missing",
        "wechat_configuration_validation_required",
        "wechat_credentials_missing",
        "wechat_credentials_rejected",
        "wechat_ip_not_allowed",
        "wechat_public_ip_unavailable",
      ].includes(reasonCode)
  )
}
