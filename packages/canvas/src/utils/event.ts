export function isTextInput(event: KeyboardEvent | ClipboardEvent): boolean {
  return (
    event.target instanceof HTMLInputElement ||
    event.target instanceof HTMLTextAreaElement ||
    (event.target instanceof HTMLElement && event.target.isContentEditable)
  )
}

export function hasNativeTextSelection(): boolean {
  if (
    typeof window === "undefined" ||
    typeof window.getSelection !== "function"
  ) {
    return false
  }

  const selection = window.getSelection()
  if (!selection || selection.rangeCount === 0 || selection.isCollapsed) {
    return false
  }

  return selection.toString().length > 0
}
