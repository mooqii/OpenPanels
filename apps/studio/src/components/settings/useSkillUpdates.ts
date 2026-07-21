import { useCallback, useEffect, useRef, useState } from "react"
import { apiFetch, apiJsonWithTimeout } from "../../lib/api"
import type { ManagedProjectSkill, SkillUpdateState } from "../../types"

interface SkillUpdateResponse {
  skill: ManagedProjectSkill
  updateState: SkillUpdateState
}

export function shouldAutoCheckSkillUpdates(
  activeTab: string,
  checkedThisOpen: boolean
) {
  return !checkedThisOpen && (activeTab === "installed" || activeTab === "add")
}

export function useSkillUpdates({
  activeTab,
  apiBase,
  isOpen,
  onError,
  onUpdated,
}: {
  activeTab: string
  apiBase: string
  isOpen: boolean
  onError: (message: string) => void
  onUpdated: () => Promise<void>
}) {
  const [isChecking, setIsChecking] = useState(false)
  const [pendingForceSkill, setPendingForceSkill] =
    useState<ManagedProjectSkill | null>(null)
  const [states, setStates] = useState<Record<string, SkillUpdateState>>({})
  const [updatingSkillId, setUpdatingSkillId] = useState<string | null>(null)
  const checkedThisOpen = useRef(false)
  const pendingAfterUpdate = useRef<(() => Promise<void>) | undefined>(
    undefined
  )

  const checkUpdates = useCallback(async () => {
    setIsChecking(true)
    onError("")
    try {
      const response = await apiJsonWithTimeout<{ skills: SkillUpdateState[] }>(
        apiBase,
        "/api/skill-updates/check",
        { method: "POST" },
        120_000
      )
      setStates(
        Object.fromEntries(
          response.skills.map((state) => [state.skillId, state])
        )
      )
    } catch (cause) {
      onError(String((cause as Error)?.message || cause))
    } finally {
      setIsChecking(false)
    }
  }, [apiBase, onError])

  const invalidateUpdates = useCallback(() => {
    checkedThisOpen.current = false
    setStates({})
  }, [])

  useEffect(() => {
    if (!isOpen) {
      checkedThisOpen.current = false
      pendingAfterUpdate.current = undefined
      setPendingForceSkill(null)
      setStates({})
      return
    }
    if (!shouldAutoCheckSkillUpdates(activeTab, checkedThisOpen.current)) return
    checkedThisOpen.current = true
    checkUpdates().catch(() => undefined)
  }, [activeTab, checkUpdates, isOpen])

  const performUpdate = useCallback(
    async (
      skill: ManagedProjectSkill,
      force: boolean,
      afterUpdate?: () => Promise<void>
    ) => {
      setUpdatingSkillId(skill.id)
      onError("")
      try {
        const response = await apiFetch(
          apiBase,
          `/api/skills/${encodeURIComponent(skill.id)}/update`,
          {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({ force }),
          }
        )
        const payload = (await response.json()) as
          | SkillUpdateResponse
          | { code?: string; error?: string }
        if (!response.ok) {
          if (
            !force &&
            response.status === 409 &&
            "code" in payload &&
            payload.code === "skill_local_modifications"
          ) {
            pendingAfterUpdate.current = afterUpdate
            setPendingForceSkill(skill)
            return
          }
          throw new Error(
            ("error" in payload && payload.error) || `HTTP ${response.status}`
          )
        }
        const updated = payload as SkillUpdateResponse
        setStates((current) => ({
          ...current,
          [skill.id]: updated.updateState,
        }))
        setPendingForceSkill(null)
        pendingAfterUpdate.current = undefined
        await afterUpdate?.()
        await onUpdated()
      } catch (cause) {
        onError(String((cause as Error)?.message || cause))
      } finally {
        setUpdatingSkillId(null)
      }
    },
    [apiBase, onError, onUpdated]
  )

  const requestUpdate = useCallback(
    (skill: ManagedProjectSkill, afterUpdate?: () => Promise<void>) => {
      if (states[skill.id]?.localModified) {
        pendingAfterUpdate.current = afterUpdate
        setPendingForceSkill(skill)
        return
      }
      performUpdate(skill, false, afterUpdate).catch(() => undefined)
    },
    [performUpdate, states]
  )

  const confirmForceUpdate = useCallback(() => {
    if (pendingForceSkill) {
      performUpdate(pendingForceSkill, true, pendingAfterUpdate.current).catch(
        () => undefined
      )
    }
  }, [pendingForceSkill, performUpdate])

  const markUpToDate = useCallback((skill: ManagedProjectSkill) => {
    setStates((current) => ({
      ...current,
      [skill.id]: {
        checkedAt: new Date().toISOString(),
        localModified: false,
        skillId: skill.id,
        sourceLocator: skill.provenance?.sourceLocator,
        sourceType: skill.provenance?.sourceType,
        status: "upToDate",
      },
    }))
  }, [])

  return {
    checkUpdates,
    confirmForceUpdate,
    invalidateUpdates,
    isChecking,
    markUpToDate,
    pendingForceSkill,
    requestUpdate,
    setPendingForceSkill,
    states,
    updatingSkillId,
  }
}
