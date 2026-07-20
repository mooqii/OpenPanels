import { Button, Chip, ListBox, Modal, Select, Spinner } from "@heroui/react"
import {
  AlertTriangle,
  CheckCircle2,
  Clock3,
  ExternalLink,
  Image as ImageIcon,
  PanelLeft,
  Send,
  UserRoundCog,
  X,
} from "lucide-react"
import { type ReactNode, useEffect, useMemo, useState } from "react"
import { useMyOpenPanelsI18n } from "../../canvas"
import { apiJson, apiUrl } from "../../lib/api"
import { randomId } from "../../lib/id"
import {
  publishingAttemptIsActive,
  publishingAttemptStatus,
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

interface PublishingResponse {
  attempt?: PublishingAttempt
  release?: PublishingRelease
  revision: number
  state: PublishingState
  task?: ProjectTask
}

type PendingAction =
  | { kind: "release" }
  | {
      acknowledgedUnknown: boolean
      kind: "attempt"
      mode: "auto" | "manual"
      release: PublishingRelease
    }

export function PublishingPanel({
  chromeContent,
  onOpenAgentTasks,
  onOpenManualTask,
  onStateSaved,
  state: initialState,
  tasks,
  transport,
  typesetting,
}: {
  chromeContent: ReactNode
  onOpenAgentTasks: (taskIds: string[]) => void
  onOpenManualTask: (scope: TaskExecutionScope) => void
  onStateSaved: (
    state: PublishingState,
    revision: number,
    task?: ProjectTask
  ) => void
  state: PublishingState
  tasks: ProjectTask[]
  transport: MyOpenPanelsTransport
  typesetting: TypesettingState
}) {
  const { t } = useMyOpenPanelsI18n()
  const [state, setState] = useState(initialState)
  const [skills, setSkills] = useState<ManagedProjectSkill[]>([])
  const [skillsLoading, setSkillsLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [isSourceListOpen, setIsSourceListOpen] = useState(false)
  const [pendingAction, setPendingAction] = useState<PendingAction | null>(null)

  useEffect(() => setState(initialState), [initialState])

  useEffect(() => {
    let cancelled = false
    setSkillsLoading(true)
    apiJson<{ modules: ManagedSkillModule[] }>(transport.apiBase, "/api/skills")
      .then((response) => {
        if (cancelled) return
        setSkills(
          response.modules.find(
            (module) => module.kind === "publishing-xiaohongshu"
          )?.skills ?? []
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
  }, [transport.apiBase])

  const selectedPublication = useMemo(() => {
    const selected = typesetting.publications.find(
      (publication) => publication.id === state.selectedPublicationId
    )
    return selected ?? typesetting.publications[0] ?? null
  }, [state.selectedPublicationId, typesetting.publications])
  const selectedSkillId = skills.some(
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
  const latestRelease = relatedReleases[0]
  const taskById = new Map(tasks.map((task) => [task.id, task]))
  const activeAttempt = relatedReleases
    .flatMap((release) => release.attempts)
    .find((attempt) =>
      publishingAttemptIsActive(attempt, taskById.get(attempt.taskId))
    )
  const sourceComplete = Boolean(
    selectedPublication?.title.trim() &&
      bodyText.trim() &&
      selectedPublication.covers.length
  )

  async function savePreference(
    publicationId: string | null,
    skillId = selectedSkillId
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
                  platform: "xiaohongshu",
                  publicationId: selectedPublication.id,
                  requestId: randomId("publishing-request"),
                  skillId: selectedSkillId,
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
                  skillId: selectedSkillId,
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
            aria-label={t`Close Typesetting content`}
            className="op-publishing-source-backdrop"
            onClick={() => setIsSourceListOpen(false)}
            type="button"
          />
        ) : null}
        <aside
          aria-label={t`Typesetting content`}
          className={`op-publishing-module op-publishing-sources ${isSourceListOpen ? "is-open" : ""}`}
        >
          <div className="op-publishing-sources__mobile-header">
            <strong>{t`Typesetting content`}</strong>
            <Button
              aria-label={t`Close Typesetting content`}
              isIconOnly
              onPress={() => setIsSourceListOpen(false)}
              size="sm"
              variant="ghost"
            >
              <X size={16} />
            </Button>
          </div>
          <div className="op-publishing-section-heading">
            <span>{t`Typesetting content`}</span>
            <small>{typesetting.publications.length}</small>
          </div>
          {typesetting.publications.length ? (
            <div className="op-publishing-source-list">
              {typesetting.publications.map((publication) => (
                <PublicationSource
                  isSelected={publication.id === selectedPublication?.id}
                  key={publication.id}
                  onSelect={() => {
                    savePreference(publication.id)
                    setIsSourceListOpen(false)
                  }}
                  publication={publication}
                  transport={transport}
                />
              ))}
            </div>
          ) : (
            <EmptyMessage
              icon={<ImageIcon size={21} />}
              message={t`Create content in Typesetting before publishing`}
            />
          )}
        </aside>

        <section className="op-publishing-detail">
          <main className="op-publishing-module op-publishing-preview">
            <div className="op-publishing-preview__heading">
              <div className="op-publishing-preview__title">
                <Button
                  aria-label={t`Open Typesetting content`}
                  className="op-publishing-mobile-sources-button"
                  isIconOnly
                  onPress={() => setIsSourceListOpen(true)}
                  size="sm"
                  variant="ghost"
                >
                  <PanelLeft size={17} />
                </Button>
                <span>{t`Publish preview`}</span>
              </div>
              <Chip size="sm" variant="soft">{t`Read only`}</Chip>
            </div>
            {selectedPublication ? (
              <>
                <div className="op-publishing-media-strip">
                  {selectedPublication.covers.map((cover, index) => (
                    <figure key={`${cover.assetRef}:${index}`}>
                      <img
                        alt={`${selectedPublication.title} ${index + 1}`}
                        src={apiUrl(transport.apiBase, cover.src).toString()}
                      />
                      <figcaption>
                        {index === 0 ? t`Primary cover` : `${index + 1}`}
                      </figcaption>
                    </figure>
                  ))}
                </div>
                <article className="op-publishing-note-preview">
                  <h1>{selectedPublication.title || t`Untitled`}</h1>
                  <pre>{bodyText || t`Empty body`}</pre>
                </article>
                {sourceComplete ? null : (
                  <div className="op-publishing-warning">
                    <AlertTriangle size={16} />
                    {t`A title, body, and at least one cover are required`}
                  </div>
                )}
              </>
            ) : (
              <EmptyMessage
                icon={<Send size={21} />}
                message={t`No content selected`}
              />
            )}
          </main>

          <div className="op-publishing-side-stack">
            <aside className="op-publishing-actions op-publishing-module">
              <div className="op-publishing-section-heading">
                <span>{t`Publish settings`}</span>
              </div>
              <div className="op-publishing-platform">
                <span className="op-publishing-platform__mark">小红书</span>
                <div>
                  <strong>{t`Xiaohongshu image note`}</strong>
                  <small>{t`Current browser account`}</small>
                </div>
              </div>
              <div className="op-publishing-field">
                <span>{t`Publishing Skill`}</span>
                {skillsLoading ? (
                  <div className="op-publishing-skill-loading">
                    <Spinner size="sm" /> {t`Loading...`}
                  </div>
                ) : (
                  <Select
                    aria-label={t`Publishing Skill`}
                    onChange={(key) => {
                      if (key) {
                        savePreference(
                          selectedPublication?.id ?? null,
                          String(key)
                        )
                      }
                    }}
                    selectionMode="single"
                    value={selectedSkillId}
                    variant="secondary"
                  >
                    <Select.Trigger>
                      <Select.Value />
                      <Select.Indicator />
                    </Select.Trigger>
                    <Select.Popover>
                      <ListBox>
                        {skills.map((skill) => (
                          <ListBox.Item
                            id={skill.id}
                            key={skill.id}
                            textValue={skill.name}
                          >
                            {skill.name}
                          </ListBox.Item>
                        ))}
                      </ListBox>
                    </Select.Popover>
                  </Select>
                )}
              </div>
              <Button
                fullWidth
                isDisabled={
                  !sourceComplete || Boolean(activeAttempt) || skillsLoading
                }
                isPending={isSubmitting}
                onPress={() => setPendingAction({ kind: "release" })}
              >
                <Send size={16} />
                {activeAttempt ? t`Publishing in progress` : t`Publish now`}
              </Button>
              {error ? <p className="op-publishing-error">{error}</p> : null}
            </aside>

            <section className="op-publishing-history-module op-publishing-module">
              <div className="op-publishing-history-heading">
                <span>{t`Publishing history`}</span>
                {latestRelease ? (
                  <Button
                    aria-label={t`Open task`}
                    isIconOnly
                    onPress={() =>
                      onOpenAgentTasks(
                        latestRelease.attempts.map((attempt) => attempt.taskId)
                      )
                    }
                    size="sm"
                    variant="ghost"
                  >
                    <ExternalLink size={15} />
                  </Button>
                ) : null}
              </div>
              <div className="op-publishing-history">
                {relatedReleases.length ? (
                  relatedReleases.flatMap((release) =>
                    release.attempts.map((attempt) => (
                      <AttemptRow
                        attempt={attempt}
                        key={attempt.id}
                        onManual={() =>
                          setPendingAction({
                            acknowledgedUnknown:
                              publishingAttemptStatus(
                                attempt,
                                taskById.get(attempt.taskId)
                              ) === "unknown",
                            kind: "attempt",
                            mode: "manual",
                            release,
                          })
                        }
                        onOpenManual={() =>
                          onOpenManualTask({
                            kind: "exact-task",
                            taskId: attempt.taskId,
                          })
                        }
                        onRetry={() =>
                          setPendingAction({
                            acknowledgedUnknown:
                              publishingAttemptStatus(
                                attempt,
                                taskById.get(attempt.taskId)
                              ) === "unknown",
                            kind: "attempt",
                            mode: "auto",
                            release,
                          })
                        }
                        t={t}
                        task={taskById.get(attempt.taskId)}
                      />
                    ))
                  )
                ) : (
                  <p className="op-publishing-history__empty">{t`No publishing attempts yet`}</p>
                )}
              </div>
            </section>
          </div>
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
    </section>
  )
}

function PublicationSource({
  isSelected,
  onSelect,
  publication,
  transport,
}: {
  isSelected: boolean
  onSelect: () => void
  publication: TypesettingPublication
  transport: MyOpenPanelsTransport
}) {
  return (
    <button
      className="op-publishing-source"
      data-selected={isSelected || undefined}
      onClick={onSelect}
      type="button"
    >
      {publication.covers[0] ? (
        <img
          alt=""
          src={apiUrl(transport.apiBase, publication.covers[0].src).toString()}
        />
      ) : (
        <span className="op-publishing-source__placeholder">
          <ImageIcon size={18} />
        </span>
      )}
      <span>
        <strong>{publication.title || "Untitled"}</strong>
        <small>{publication.covers.length} images</small>
      </span>
    </button>
  )
}

function AttemptRow({
  attempt,
  onManual,
  onOpenManual,
  onRetry,
  task,
  t,
}: {
  attempt: PublishingAttempt
  onManual: () => void
  onOpenManual: () => void
  onRetry: () => void
  task?: ProjectTask
  t: (value: TemplateStringsArray) => string
}) {
  const status = publishingAttemptStatus(attempt, task)
  const active = ["queued", "running", "committing"].includes(status)
  const published = status === "published"
  const label = publishingStatusLabel(status, t)
  return (
    <div className="op-publishing-attempt">
      <div className="op-publishing-attempt__top">
        <Chip
          color={
            published
              ? "success"
              : status === "unknown"
                ? "warning"
                : status === "not_published"
                  ? "danger"
                  : "default"
          }
          size="sm"
          variant="soft"
        >
          {active ? (
            <Clock3 size={12} />
          ) : published ? (
            <CheckCircle2 size={12} />
          ) : null}
          {label}
        </Chip>
        <time>{new Date(attempt.createdAt).toLocaleString()}</time>
      </div>
      <p>{attempt.summary ?? attempt.skillName}</p>
      {active || published ? null : (
        <div className="op-publishing-attempt__actions">
          <Button
            onPress={onRetry}
            size="sm"
            variant="tertiary"
          >{t`Retry automatically`}</Button>
          <Button onPress={onManual} size="sm" variant="secondary">
            <UserRoundCog size={14} /> {t`Hand off to current Agent`}
          </Button>
        </div>
      )}
      {active && attempt.mode === "manual" ? (
        <Button onPress={onOpenManual} size="sm" variant="secondary">
          <UserRoundCog size={14} /> {t`Continue with current Agent`}
        </Button>
      ) : null}
      {attempt.remoteUrl ? (
        <a
          href={attempt.remoteUrl}
          rel="noreferrer"
          target="_blank"
        >{t`Open published note`}</a>
      ) : null}
    </div>
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
                : t`Publish to Xiaohongshu now?`}
            </Modal.Heading>
          </Modal.Header>
          <Modal.Body>
            <p>
              {unknown
                ? t`The previous attempt may already have published. Check Xiaohongshu Creator before continuing.`
                : `${publication?.covers.length ?? 0} ${t`images will be uploaded in order and the Agent will click Publish once.`}`}
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
                : t`Confirm publish`}
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
