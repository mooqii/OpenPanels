export type WikiModule = "raw" | "structured" | "generated"

export const WIKI_COLLAPSED_MODULES_STORAGE_KEY =
  "myopenpanels.wiki.collapsed-modules.v1"

export function wikiCollapsedModulesFromStorage(
  value: string | null
): Set<WikiModule> {
  if (value === null) return new Set(["generated"])
  try {
    const stored = JSON.parse(value) as unknown
    if (!Array.isArray(stored)) return new Set(["generated"])
    if (
      !stored.every((module) => module === "raw" || module === "generated") ||
      stored.length > 1
    ) {
      return new Set(["generated"])
    }
    return new Set(stored)
  } catch {
    return new Set(["generated"])
  }
}

export function serializeWikiCollapsedModules(
  modules: ReadonlySet<WikiModule>
): string {
  return JSON.stringify(
    (["raw", "generated"] as const).filter((module) => modules.has(module))
  )
}

export function nextCollapsedModules(
  current: ReadonlySet<WikiModule>,
  module: WikiModule,
  writing: boolean
): Set<WikiModule> {
  const pair: readonly WikiModule[] = writing
    ? ["structured", "raw"]
    : ["raw", "generated"]
  const next = new Set(current)

  if (!pair.includes(module)) {
    if (next.has(module)) next.delete(module)
    else next.add(module)
    return next
  }

  const sibling = pair.find((candidate) => candidate !== module)
  if (!sibling) return next

  if (next.has(module)) {
    next.delete(module)
  } else if (next.has(sibling)) {
    next.delete(sibling)
    next.add(module)
  } else {
    next.add(module)
  }

  return next
}
