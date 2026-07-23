import { useCallback, useState } from "react"
import { apiJson } from "../../lib/api"
import type {
  ManagedProjectSkill,
  RecommendedSkillsResponse,
} from "../../types"

interface RecommendedSkillInstallResponse {
  operation: "installed" | "associated" | "unchanged"
  skill: ManagedProjectSkill
}

export function useRecommendedSkills({
  apiBase,
  onApplied,
  onError,
}: {
  apiBase: string
  onApplied: (response: RecommendedSkillInstallResponse) => Promise<void>
  onError: (message: string) => void
}) {
  const [catalog, setCatalog] = useState<RecommendedSkillsResponse>({
    skills: [],
  })
  const [isLoading, setIsLoading] = useState(false)
  const [pendingCatalogId, setPendingCatalogId] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    setCatalog(
      await apiJson<RecommendedSkillsResponse>(
        apiBase,
        "/api/skills/recommended"
      )
    )
  }, [apiBase])

  const load = useCallback(async () => {
    setIsLoading(true)
    onError("")
    try {
      await refresh()
    } catch (cause) {
      onError(String((cause as Error)?.message || cause))
    } finally {
      setIsLoading(false)
    }
  }, [onError, refresh])

  const install = useCallback(
    async (catalogId: string) => {
      setPendingCatalogId(catalogId)
      onError("")
      try {
        const response = await apiJson<RecommendedSkillInstallResponse>(
          apiBase,
          `/api/skills/recommended/${encodeURIComponent(catalogId)}/install`,
          { method: "POST" }
        )
        await onApplied(response)
        await refresh()
        return true
      } catch (cause) {
        onError(String((cause as Error)?.message || cause))
        return false
      } finally {
        setPendingCatalogId(null)
      }
    },
    [apiBase, onApplied, onError, refresh]
  )

  return {
    catalog,
    install,
    isLoading,
    load,
    pendingCatalogId,
    refresh,
  }
}
