import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
  type ManualAgentScopeCandidate,
  manualAgentScopeCandidates,
  taskExecutionScopeKey,
} from "../components/trace/trace-utils"
import { MODEL_GATEWAY_SETTINGS_CHANGED_EVENT } from "../constants"
import { hasUsableAgentCli } from "../lib/agent-cli"
import { apiJson } from "../lib/api"
import type {
  LocalCliInfo,
  ModelGatewaySettings,
  MyOpenPanelsTransport,
  ProjectTask,
  TaskExecutionScope,
} from "../types"

const EMPTY_TASKS: ProjectTask[] = []

interface ManualTaskInstructionRequest {
  requiresAgentMessage: boolean
  scope: TaskExecutionScope
}

export function useManualTaskInstructions({
  projectId,
  tasks = EMPTY_TASKS,
  transport,
}: {
  projectId: string | null
  tasks?: ProjectTask[]
  transport: MyOpenPanelsTransport
}) {
  const [refreshVersion, setRefreshVersion] = useState(0)
  const [availability, setAvailability] = useState<{
    checkKey: string
    hasUsableCli: boolean
  } | null>(null)
  const [queue, setQueue] = useState<ManualTaskInstructionRequest[]>([])
  const [awaitingCheck, setAwaitingCheck] = useState<TaskExecutionScope[]>([])
  const observedRef = useRef<{
    projectId: string
    readyKeys: Set<string>
  } | null>(null)
  const candidates = useMemo(() => manualAgentScopeCandidates(tasks), [tasks])
  const taskIdsKey = useMemo(
    () =>
      `${projectId ?? ""}:${tasks
        .map((task) => task.id)
        .sort()
        .join(",")}`,
    [projectId, tasks]
  )
  const checkKey = `${taskIdsKey}:${refreshVersion}`
  const hasUsableCli =
    availability?.checkKey === checkKey ? availability.hasUsableCli : null
  const openWhenAgentCliUnavailable = useCallback(
    (scope: TaskExecutionScope) => {
      switch (manualTaskInstructionAction(hasUsableCli)) {
        case "queue":
          setQueue((current) =>
            appendUniqueRequests(current, [
              { requiresAgentMessage: false, scope },
            ])
          )
          break
        case "await":
          setAwaitingCheck((current) => appendUniqueScopes(current, [scope]))
          break
        default:
          break
      }
    },
    [hasUsableCli]
  )
  const openRequiredAgentMessage = useCallback(
    (scope: TaskExecutionScope) => {
      const observed = observedRef.current
      if (observed?.projectId === projectId) {
        markManualAgentScopeObserved(observed.readyKeys, scope)
      }
      const scopeKey = taskExecutionScopeKey(scope)
      setAwaitingCheck((current) =>
        current.filter(
          (candidate) => taskExecutionScopeKey(candidate) !== scopeKey
        )
      )
      setQueue([{ requiresAgentMessage: true, scope }])
    },
    [projectId]
  )

  useEffect(() => {
    const onSettingsChanged = () => setRefreshVersion((version) => version + 1)
    window.addEventListener(
      MODEL_GATEWAY_SETTINGS_CHANGED_EVENT,
      onSettingsChanged
    )
    return () =>
      window.removeEventListener(
        MODEL_GATEWAY_SETTINGS_CHANGED_EVENT,
        onSettingsChanged
      )
  }, [])

  useEffect(() => {
    if (!projectId) return
    let cancelled = false
    Promise.all([
      apiJson<{ settings: ModelGatewaySettings }>(
        transport.apiBase,
        "/api/model-gateway/settings"
      ),
      apiJson<{ localClis: LocalCliInfo[] }>(
        transport.apiBase,
        "/api/model-gateway/local-clis"
      ),
    ])
      .then(([settingsResponse, scanResponse]) => {
        if (cancelled) return
        setAvailability({
          checkKey,
          hasUsableCli: hasUsableAgentCli(
            settingsResponse.settings,
            scanResponse.localClis
          ),
        })
      })
      .catch(() => {
        if (!cancelled) setAvailability({ checkKey, hasUsableCli: false })
      })
    return () => {
      cancelled = true
    }
  }, [checkKey, projectId, transport.apiBase])

  useEffect(() => {
    if (!projectId) {
      observedRef.current = null
      return
    }
    const observed = observedRef.current
    if (!observed || observed.projectId !== projectId) {
      observedRef.current = {
        projectId,
        readyKeys: new Set(
          candidates
            .filter((candidate) => candidate.isReady)
            .map((candidate) => candidate.key)
        ),
      }
      setQueue([])
      setAwaitingCheck([])
      return
    }

    const newScopes = unobservedReadyManualAgentScopes(
      candidates,
      observed.readyKeys
    )
    for (const candidate of candidates) {
      if (candidate.isReady) observed.readyKeys.add(candidate.key)
    }
    if (!newScopes.length) return
    setAwaitingCheck((current) => appendUniqueScopes(current, newScopes))
  }, [candidates, projectId])

  useEffect(() => {
    if (hasUsableCli === null) return
    if (hasUsableCli) {
      setQueue(keepRequiredAgentMessageRequests)
      setAwaitingCheck(clearTasksIfNeeded)
      return
    }
    if (!awaitingCheck.length) return
    setQueue((current) =>
      appendUniqueRequests(
        current,
        awaitingCheck.map((scope) => ({
          requiresAgentMessage: false,
          scope,
        }))
      )
    )
    setAwaitingCheck([])
  }, [awaitingCheck, hasUsableCli])

  return {
    dismiss: useCallback(() => setQueue((current) => current.slice(1)), []),
    dismissAll: useCallback(() => {
      setQueue([])
      setAwaitingCheck([])
    }, []),
    hasUsableCli,
    open: useCallback(
      (scope: TaskExecutionScope) =>
        setQueue([{ requiresAgentMessage: false, scope }]),
      []
    ),
    openRequiredAgentMessage,
    openWhenAgentCliUnavailable,
    requiresAgentMessage: queue[0]?.requiresAgentMessage ?? false,
    scope: queue[0]?.scope ?? null,
  }
}

export type ManualTaskInstructionsController = ReturnType<
  typeof useManualTaskInstructions
>

export function clearTasksIfNeeded<T>(tasks: T[]): T[] {
  return tasks.length ? [] : tasks
}

export function keepRequiredAgentMessageRequests(
  requests: ManualTaskInstructionRequest[]
): ManualTaskInstructionRequest[] {
  return requests.filter((request) => request.requiresAgentMessage)
}

export function manualTaskInstructionAction(
  hasUsableCli: boolean | null
): "await" | "ignore" | "queue" {
  if (hasUsableCli === null) return "await"
  return hasUsableCli ? "ignore" : "queue"
}

export function markManualAgentScopeObserved(
  observedReadyKeys: Set<string>,
  scope: TaskExecutionScope
) {
  observedReadyKeys.add(taskExecutionScopeKey(scope))
}

export function unobservedReadyManualAgentScopes(
  candidates: ManualAgentScopeCandidate[],
  observedReadyKeys: ReadonlySet<string>
): TaskExecutionScope[] {
  return candidates
    .filter(
      (candidate) => candidate.isReady && !observedReadyKeys.has(candidate.key)
    )
    .map((candidate) => candidate.scope)
}

function appendUniqueRequests(
  current: ManualTaskInstructionRequest[],
  incoming: ManualTaskInstructionRequest[]
): ManualTaskInstructionRequest[] {
  return [
    ...current,
    ...incoming.filter((request) => {
      const key = taskExecutionScopeKey(request.scope)
      return !current.some(
        (candidate) => taskExecutionScopeKey(candidate.scope) === key
      )
    }),
  ]
}

function appendUniqueScopes(
  current: TaskExecutionScope[],
  incoming: TaskExecutionScope[]
): TaskExecutionScope[] {
  return [
    ...current,
    ...incoming.filter((scope) => {
      const key = taskExecutionScopeKey(scope)
      return !current.some(
        (candidate) => taskExecutionScopeKey(candidate) === key
      )
    }),
  ]
}
