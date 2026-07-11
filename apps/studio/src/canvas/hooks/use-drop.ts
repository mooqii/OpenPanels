import { useCallback, useEffect, useState } from "react"
import type { Editor } from "../editor"
import {
  createImageFromFile,
  createImageFromUrl,
  getImageDimensions,
} from "../utils/clipboard"
import { getCanvasPosition } from "../utils/coordinates"

interface UseDropOptions {
  containerRef: React.RefObject<HTMLDivElement | null>
  editor: Editor
}

/**
 * Hook to handle drag & drop of files and image URLs onto the canvas.
 * Supports:
 * - Dropping image files (png, jpg, gif, webp, etc.)
 * - Dropping image URLs from browser
 */
export function useDrop({ editor, containerRef }: UseDropOptions) {
  const [isDragging, setIsDragging] = useState(false)

  const getDropPosition = useCallback(
    (e: DragEvent): { x: number; y: number } => {
      const container = containerRef.current
      if (!container) {
        return { x: 100, y: 100 }
      }

      const rect = container.getBoundingClientRect()
      return {
        x: e.clientX - rect.left,
        y: e.clientY - rect.top,
      }
    },
    [containerRef]
  )

  const handleDragOver = useCallback((e: DragEvent) => {
    e.preventDefault()
    e.stopPropagation()

    // Check if dragging files or URLs
    const hasFiles = e.dataTransfer?.types.includes("Files")
    const hasUri =
      e.dataTransfer?.types.includes("text/uri-list") ||
      e.dataTransfer?.types.includes("text/plain")

    if (hasFiles || hasUri) {
      if (e.dataTransfer) {
        e.dataTransfer.dropEffect = "copy"
      }
      setIsDragging(true)
    }
  }, [])

  const handleDragEnter = useCallback((e: DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    setIsDragging(true)
  }, [])

  const handleDragLeave = useCallback(
    (e: DragEvent) => {
      e.preventDefault()
      e.stopPropagation()

      // Only set isDragging to false if we're leaving the container
      const container = containerRef.current
      if (container && e.relatedTarget instanceof Node) {
        if (!container.contains(e.relatedTarget)) {
          setIsDragging(false)
        }
      } else {
        setIsDragging(false)
      }
    },
    [containerRef]
  )

  const handleDrop = useCallback(
    async (e: DragEvent) => {
      e.preventDefault()
      e.stopPropagation()
      setIsDragging(false)

      const dataTransfer = e.dataTransfer
      if (!dataTransfer) return

      const position = getDropPosition(e)

      // Convert to canvas coordinates accounting for pan and zoom
      let canvasPosition = position
      const stage = editor.stage
      if (stage) {
        canvasPosition = getCanvasPosition(stage, position.x, position.y)
      }

      const assetStore = editor.getAssetStore()

      // Handle dropped files
      const files = Array.from(dataTransfer.files)
      const imageFiles = files.filter((file) => file.type.startsWith("image/"))

      if (imageFiles.length > 0) {
        // Process each image file
        for (let i = 0; i < imageFiles.length; i++) {
          const file = imageFiles[i]
          // Offset each image slightly so they don't stack
          const offsetPosition = {
            x: canvasPosition.x + i * 20,
            y: canvasPosition.y + i * 20,
          }
          try {
            await createImageFromFile(
              editor,
              file,
              offsetPosition,
              assetStore,
              true
            )
          } catch (error) {
            console.error("Failed to create image from dropped file:", error)
          }
        }
        return
      }

      // Handle dropped URLs
      const uriList = dataTransfer.getData("text/uri-list")
      const plainText = dataTransfer.getData("text/plain")
      const url = uriList || plainText

      if (url && (await isImageUrl(url))) {
        try {
          await createImageFromUrl(
            editor,
            url,
            canvasPosition,
            assetStore,
            true
          )
        } catch (error) {
          console.error("Failed to create image from dropped URL:", error)
        }
      }
    },
    [editor, getDropPosition]
  )

  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    container.addEventListener("dragover", handleDragOver)
    container.addEventListener("dragenter", handleDragEnter)
    container.addEventListener("dragleave", handleDragLeave)
    container.addEventListener("drop", handleDrop)

    return () => {
      container.removeEventListener("dragover", handleDragOver)
      container.removeEventListener("dragenter", handleDragEnter)
      container.removeEventListener("dragleave", handleDragLeave)
      container.removeEventListener("drop", handleDrop)
    }
  }, [
    containerRef,
    handleDragOver,
    handleDragEnter,
    handleDragLeave,
    handleDrop,
  ])

  return {
    isDragging,
  }
}

/**
 * Check if a URL appears to be an image URL
 */
async function isImageUrl(url: string) {
  try {
    const parsed = new URL(url)
    const pathname = parsed.pathname.toLowerCase()

    // Check common image extensions
    const imageExtensions = [
      ".png",
      ".jpg",
      ".jpeg",
      ".gif",
      ".webp",
      ".svg",
      ".bmp",
      ".ico",
    ]
    if (imageExtensions.some((ext) => pathname.endsWith(ext))) {
      return true
    }

    await getImageDimensions(url)

    return true
  } catch {
    return false
  }
}
