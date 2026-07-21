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
  const accordionModules: readonly WikiModule[] = writing
    ? ["generated", "structured", "raw"]
    : ["raw", "generated"]
  const next = new Set(current)

  if (!accordionModules.includes(module)) {
    if (next.has(module)) next.delete(module)
    else next.add(module)
    return next
  }

  if (next.has(module)) {
    next.delete(module)
    return next
  }

  const openSiblings = accordionModules.filter(
    (candidate) => candidate !== module && !next.has(candidate)
  )
  if (openSiblings.length > 0) {
    next.add(module)
    return next
  }

  const collapsedSibling = accordionModules.find(
    (candidate) => candidate !== module && next.has(candidate)
  )
  if (collapsedSibling) next.delete(collapsedSibling)
  next.add(module)
  return next
}
