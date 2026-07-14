export type TypesettingLibraryModule = "raw" | "generated" | "assets"

const ALL_LIBRARY_MODULES: readonly TypesettingLibraryModule[] = [
  "raw",
  "generated",
  "assets",
]

export function nextCollapsedLibraryModules(
  current: ReadonlySet<TypesettingLibraryModule>,
  module: TypesettingLibraryModule
): Set<TypesettingLibraryModule> {
  const next = new Set(current)

  if (next.has(module)) {
    next.delete(module)
    return next
  }

  const isOnlyExpanded = ALL_LIBRARY_MODULES.every(
    (candidate) => candidate === module || next.has(candidate)
  )
  if (!isOnlyExpanded) next.add(module)

  return next
}
