const EMBEDDED_BROWSER_MARKERS = /\b(?:Codex|Electron)\b/i
const STANDALONE_BROWSER_MARKERS =
  /\b(?:Chrome|CriOS|Edg|EdgiOS|EdgA|Firefox|FxiOS|OPR|Opera|Safari)\//i

export function shouldShowOpenInBrowserPrompt(userAgent: string): boolean {
  if (EMBEDDED_BROWSER_MARKERS.test(userAgent)) return true
  return !STANDALONE_BROWSER_MARKERS.test(userAgent)
}

export function externalBrowserPath(location: {
  hash: string
  pathname: string
  search: string
}): string {
  return `${location.pathname}${location.search}${location.hash}`
}
