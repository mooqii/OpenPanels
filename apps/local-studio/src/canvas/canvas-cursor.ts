import type { PointerEvent as ReactPointerEvent } from "react"

interface CanvasPointerPosition {
  x: number
  y: number
}

export interface MarkerCursorPreviewState {
  color: string
  diameter: number
  x: number
  y: number
}

export function createPencilCursor(color: string): string {
  const svg = `
    <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="${color}" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">
      <path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"/>
      <path d="m15 5 4 4"/>
    </svg>
  `.trim()

  return `url("data:image/svg+xml,${encodeURIComponent(svg)}") 2 22, auto`
}

export function getRelativePointerPosition(
  container: HTMLDivElement,
  event: PointerEvent | ReactPointerEvent<HTMLDivElement>
): CanvasPointerPosition {
  const rect = container.getBoundingClientRect()

  return {
    x: event.clientX - rect.left,
    y: event.clientY - rect.top,
  }
}
