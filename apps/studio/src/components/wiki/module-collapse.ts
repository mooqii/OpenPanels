export type WikiModule = "structured" | "generated"

export function nextCollapsedModules(
  current: ReadonlySet<WikiModule>,
  module: WikiModule
): Set<WikiModule> {
  const accordionModules: readonly WikiModule[] = ["generated", "structured"]
  const next = new Set(current)

  if (next.has(module)) {
    next.delete(module)
    return next
  }

  const openSibling = accordionModules.find(
    (candidate) => candidate !== module && !next.has(candidate)
  )
  if (openSibling) {
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
