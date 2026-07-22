import {
  type Dispatch,
  type DragEvent,
  type SetStateAction,
  useCallback,
  useRef,
  useState,
} from "react"
import { apiFetch, fileToDataUrl, titleFromFileName } from "../../lib/api"

export function useRawDocumentDrop({
  activeSpaceId,
  apiBase,
  onReload,
  setIsBusy,
}: {
  activeSpaceId?: string
  apiBase: string
  onReload: () => Promise<void>
  setIsBusy: Dispatch<SetStateAction<boolean>>
}) {
  const [isRawDragActive, setIsRawDragActive] = useState(false)
  const rawDragDepthRef = useRef(0)
  const fileInputRef = useRef<HTMLInputElement | null>(null)
  const addFiles = useCallback(
    async (files: FileList | null) => {
      if (!files?.length) return
      setIsBusy(true)
      try {
        for (const file of [...files]) {
          await apiFetch(apiBase, "/api/wiki/raw-documents", {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({
              dataUrl: await fileToDataUrl(file),
              fileName: file.name,
              mimeType: file.type || "application/octet-stream",
              title: titleFromFileName(file.name),
              source: "user",
              wikiSpaceId: activeSpaceId,
            }),
          })
        }
        await onReload()
      } finally {
        setIsBusy(false)
      }
    },
    [activeSpaceId, apiBase, onReload, setIsBusy]
  )
  const handleRawDragEnter = useCallback((event: DragEvent<HTMLElement>) => {
    if (!event.dataTransfer.types.includes("Files")) return
    event.preventDefault()
    rawDragDepthRef.current += 1
    setIsRawDragActive(true)
  }, [])
  const handleRawDragOver = useCallback((event: DragEvent<HTMLElement>) => {
    if (!event.dataTransfer.types.includes("Files")) return
    event.preventDefault()
    event.dataTransfer.dropEffect = "copy"
  }, [])
  const handleRawDragLeave = useCallback((event: DragEvent<HTMLElement>) => {
    if (!event.dataTransfer.types.includes("Files")) return
    event.preventDefault()
    rawDragDepthRef.current = Math.max(0, rawDragDepthRef.current - 1)
    if (rawDragDepthRef.current === 0) setIsRawDragActive(false)
  }, [])
  const handleRawDrop = useCallback(
    async (event: DragEvent<HTMLElement>) => {
      if (!event.dataTransfer.types.includes("Files")) return
      event.preventDefault()
      rawDragDepthRef.current = 0
      setIsRawDragActive(false)
      await addFiles(event.dataTransfer.files)
    },
    [addFiles]
  )
  return {
    addFiles,
    fileInputRef,
    handleRawDragEnter,
    handleRawDragLeave,
    handleRawDragOver,
    handleRawDrop,
    isRawDragActive,
  }
}
