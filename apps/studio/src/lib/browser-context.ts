export const EMBEDDED_VIEW_PARAM = "myopenpanels-view"
export const EMBEDDED_VIEW_VALUE = "embedded"

export function isEmbeddedPanelView(search: string): boolean {
  return (
    new URLSearchParams(search).get(EMBEDDED_VIEW_PARAM) === EMBEDDED_VIEW_VALUE
  )
}

export function externalBrowserPath(location: {
  hash: string
  pathname: string
  search: string
}): string {
  const search = new URLSearchParams(location.search)
  search.delete(EMBEDDED_VIEW_PARAM)
  const query = search.toString()
  return `${location.pathname}${query ? `?${query}` : ""}${location.hash}`
}
