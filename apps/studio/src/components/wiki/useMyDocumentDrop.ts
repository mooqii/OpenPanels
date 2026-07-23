import {
  type Dispatch,
  type DragEvent,
  type SetStateAction,
  useCallback,
  useRef,
  useState,
} from "react"
import { apiFetch, fileToDataUrl, titleFromFileName } from "../../lib/api"

export function useMyDocumentDrop({
  apiBase,
  onReload,
  setIsBusy,
}: {
  apiBase: string
  onReload: () => Promise<void>
  setIsBusy: Dispatch<SetStateAction<boolean>>
}) {
  const [isMyDocumentDragActive, setIsMyDocumentDragActive] = useState(false)
  const myDocumentDragDepthRef = useRef(0)
  const myDocumentFileInputRef = useRef<HTMLInputElement | null>(null)
  const addMyDocumentFiles = useCallback(
    async (files: FileList | null) => {
      if (!files?.length) return
      setIsBusy(true)
      try {
        for (const file of [...files]) {
          await apiFetch(apiBase, "/api/my-documents", {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({
              dataUrl: await fileToDataUrl(file),
              fileName: file.name,
              mimeType: file.type || "application/octet-stream",
              title: titleFromFileName(file.name),
            }),
          })
        }
        await onReload()
      } finally {
        setIsBusy(false)
      }
    },
    [apiBase, onReload, setIsBusy]
  )
  const handleMyDocumentDragEnter = useCallback(
    (event: DragEvent<HTMLElement>) => {
      if (!event.dataTransfer.types.includes("Files")) return
      event.preventDefault()
      myDocumentDragDepthRef.current += 1
      setIsMyDocumentDragActive(true)
    },
    []
  )
  const handleMyDocumentDragOver = useCallback(
    (event: DragEvent<HTMLElement>) => {
      if (!event.dataTransfer.types.includes("Files")) return
      event.preventDefault()
      event.dataTransfer.dropEffect = "copy"
    },
    []
  )
  const handleMyDocumentDragLeave = useCallback(
    (event: DragEvent<HTMLElement>) => {
      if (!event.dataTransfer.types.includes("Files")) return
      event.preventDefault()
      myDocumentDragDepthRef.current = Math.max(
        0,
        myDocumentDragDepthRef.current - 1
      )
      if (myDocumentDragDepthRef.current === 0) setIsMyDocumentDragActive(false)
    },
    []
  )
  const handleMyDocumentDrop = useCallback(
    async (event: DragEvent<HTMLElement>) => {
      if (!event.dataTransfer.types.includes("Files")) return
      event.preventDefault()
      myDocumentDragDepthRef.current = 0
      setIsMyDocumentDragActive(false)
      await addMyDocumentFiles(event.dataTransfer.files)
    },
    [addMyDocumentFiles]
  )
  return {
    addMyDocumentFiles,
    myDocumentFileInputRef,
    handleMyDocumentDragEnter,
    handleMyDocumentDragLeave,
    handleMyDocumentDragOver,
    handleMyDocumentDrop,
    isMyDocumentDragActive,
  }
}
