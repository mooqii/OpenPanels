import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
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
  const [queue, setQueue] = useState<TaskExecutionScope[]>([])
  const [awaitingCheck, setAwaitingCheck] = useState<TaskExecutionScope[]>([])
  const observedRef = useRef<{
    ids: Set<string>
    projectId: string
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
        ids: new Set(candidates.map((candidate) => candidate.key)),
        projectId,
      }
      setQueue([])
      setAwaitingCheck([])
      return
    }

    const newScopes = candidates
      .filter(
        (candidate) => candidate.isReady && !observed.ids.has(candidate.key)
      )
      .map((candidate) => candidate.scope)
    observed.ids = new Set(candidates.map((candidate) => candidate.key))
    if (!newScopes.length) return
    setAwaitingCheck((current) => appendUniqueScopes(current, newScopes))
  }, [candidates, projectId])

  useEffect(() => {
    if (hasUsableCli === null) return
    if (hasUsableCli) {
      setQueue(clearTasksIfNeeded)
      setAwaitingCheck(clearTasksIfNeeded)
      return
    }
    if (!awaitingCheck.length) return
    setQueue((current) => appendUniqueScopes(current, awaitingCheck))
    setAwaitingCheck([])
  }, [awaitingCheck, hasUsableCli])

  return {
    dismiss: useCallback(() => setQueue((current) => current.slice(1)), []),
    dismissAll: useCallback(() => {
      setQueue([])
      setAwaitingCheck([])
    }, []),
    hasUsableCli,
    open: useCallback((scope: TaskExecutionScope) => setQueue([scope]), []),
    scope: queue[0] ?? null,
  }
}

export type ManualTaskInstructionsController = ReturnType<
  typeof useManualTaskInstructions
>

export function clearTasksIfNeeded<T>(tasks: T[]): T[] {
  return tasks.length ? [] : tasks
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
