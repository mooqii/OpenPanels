import {
  type Dispatch,
  type DragEvent,
  type SetStateAction,
  useCallback,
  useRef,
  useState,
} from "react"
import { apiFetch, fileToDataUrl, titleFromFileName } from "../../lib/api"

export function useGeneratedDocumentDrop({
  apiBase,
  onReload,
  setIsBusy,
}: {
  apiBase: string
  onReload: () => Promise<void>
  setIsBusy: Dispatch<SetStateAction<boolean>>
}) {
  const [isGeneratedDragActive, setIsGeneratedDragActive] = useState(false)
  const generatedDragDepthRef = useRef(0)
  const generatedFileInputRef = useRef<HTMLInputElement | null>(null)
  const addGeneratedFiles = useCallback(
    async (files: FileList | null) => {
      if (!files?.length) return
      setIsBusy(true)
      try {
        for (const file of [...files]) {
          await apiFetch(apiBase, "/api/wiki/generated-documents", {
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
  const handleGeneratedDragEnter = useCallback(
    (event: DragEvent<HTMLElement>) => {
      if (!event.dataTransfer.types.includes("Files")) return
      event.preventDefault()
      generatedDragDepthRef.current += 1
      setIsGeneratedDragActive(true)
    },
    []
  )
  const handleGeneratedDragOver = useCallback(
    (event: DragEvent<HTMLElement>) => {
      if (!event.dataTransfer.types.includes("Files")) return
      event.preventDefault()
      event.dataTransfer.dropEffect = "copy"
    },
    []
  )
  const handleGeneratedDragLeave = useCallback(
    (event: DragEvent<HTMLElement>) => {
      if (!event.dataTransfer.types.includes("Files")) return
      event.preventDefault()
      generatedDragDepthRef.current = Math.max(
        0,
        generatedDragDepthRef.current - 1
      )
      if (generatedDragDepthRef.current === 0) setIsGeneratedDragActive(false)
    },
    []
  )
  const handleGeneratedDrop = useCallback(
    async (event: DragEvent<HTMLElement>) => {
      if (!event.dataTransfer.types.includes("Files")) return
      event.preventDefault()
      generatedDragDepthRef.current = 0
      setIsGeneratedDragActive(false)
      await addGeneratedFiles(event.dataTransfer.files)
    },
    [addGeneratedFiles]
  )
  return {
    addGeneratedFiles,
    generatedFileInputRef,
    handleGeneratedDragEnter,
    handleGeneratedDragLeave,
    handleGeneratedDragOver,
    handleGeneratedDrop,
    isGeneratedDragActive,
  }
}
