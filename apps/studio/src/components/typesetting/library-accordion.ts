export type TypesettingLibraryModule = "publications" | "myDocuments" | "assets"

const ALL_LIBRARY_MODULES: readonly TypesettingLibraryModule[] = [
  "publications",
  "myDocuments",
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
