import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { MODEL_GATEWAY_SETTINGS_CHANGED_EVENT } from "../constants"
import { hasUsableAgentCli } from "../lib/agent-cli"
import { apiJson } from "../lib/api"
import type {
  LocalCliInfo,
  ModelGatewaySettings,
  MyOpenPanelsTransport,
  ProjectTask,
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
  const [queue, setQueue] = useState<ProjectTask[]>([])
  const [awaitingCheck, setAwaitingCheck] = useState<ProjectTask[]>([])
  const observedRef = useRef<{
    ids: Set<string>
    projectId: string
  } | null>(null)
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
        ids: new Set(
          tasks.filter(isTaskReadyForManualAgent).map((task) => task.id)
        ),
        projectId,
      }
      setQueue([])
      setAwaitingCheck([])
      return
    }

    const newTasks = tasks.filter(
      (task) => isTaskReadyForManualAgent(task) && !observed.ids.has(task.id)
    )
    for (const task of newTasks) observed.ids.add(task.id)
    if (!newTasks.length) return
    setAwaitingCheck((current) => appendUniqueTasks(current, newTasks))
  }, [projectId, tasks])

  useEffect(() => {
    if (hasUsableCli === null) return
    if (hasUsableCli) {
      setQueue(clearTasksIfNeeded)
      setAwaitingCheck(clearTasksIfNeeded)
      return
    }
    if (!awaitingCheck.length) return
    setQueue((current) => appendUniqueTasks(current, awaitingCheck))
    setAwaitingCheck([])
  }, [awaitingCheck, hasUsableCli])

  return {
    dismiss: useCallback(() => setQueue((current) => current.slice(1)), []),
    dismissAll: useCallback(() => {
      setQueue([])
      setAwaitingCheck([])
    }, []),
    hasUsableCli,
    open: useCallback((task: ProjectTask) => setQueue([task]), []),
    task: queue[0] ?? null,
  }
}

export type ManualTaskInstructionsController = ReturnType<
  typeof useManualTaskInstructions
>

export function clearTasksIfNeeded(tasks: ProjectTask[]): ProjectTask[] {
  return tasks.length ? [] : tasks
}

function appendUniqueTasks(
  current: ProjectTask[],
  incoming: ProjectTask[]
): ProjectTask[] {
  return [
    ...current,
    ...incoming.filter(
      (task) => !current.some((candidate) => candidate.id === task.id)
    ),
  ]
}

function isTaskReadyForManualAgent(task: ProjectTask): boolean {
  return (
    Boolean(task.ready) &&
    (task.status === "queued" || task.status === "failed")
  )
}
