import { Button, Tooltip } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { Image } from "lucide-react"
import { useCallback } from "react"
import { FileTrigger } from "react-aria-components"
import { useEditor } from "../EditorContext"
import type { ShapeId } from "../types/ids"
import {
  createImageFromFile,
  fileToDataUrl,
  getImageDimensions,
} from "../utils/clipboard"
import { getViewportCenter } from "../utils/coordinates"

export function ImageTool() {
  const { t } = useLingui()
  const editor = useEditor()

  const handleFileSelect = useCallback(
    async (files: FileList | null) => {
      if (!files || files.length === 0) return

      // Filter image files
      const imageFiles = Array.from(files).filter((file) =>
        file.type.startsWith("image/")
      )
      if (imageFiles.length === 0) return

      // Calculate viewport center in canvas coordinates
      const viewportCenter = getViewportCenter(editor.stage) ?? {
        x: 400,
        y: 300,
      }

      const assetStore = editor.getAssetStore()
      const SPACING = 20 // Space between images

      // Get dimensions for all images
      interface ImageInfo {
        dataUrl: string
        file: File
        finalHeight: number
        finalWidth: number
        originalHeight: number
        originalWidth: number
      }

      const imageInfos: ImageInfo[] = []
      for (const file of imageFiles) {
        try {
          const dataUrl = await fileToDataUrl(file)
          const { width, height } = await getImageDimensions(dataUrl)

          imageInfos.push({
            file,
            dataUrl,
            originalWidth: width,
            originalHeight: height,
            finalWidth: width,
            finalHeight: height,
          })
        } catch (error) {
          console.error("Failed to process image:", error)
        }
      }

      if (imageInfos.length === 0) return

      // Calculate horizontal layout positions (top-aligned)
      let currentX = 0
      const positions: Array<{ x: number; y: number }> = []
      let maxHeight = 0

      for (const info of imageInfos) {
        positions.push({ x: currentX, y: 0 })
        currentX += info.finalWidth + SPACING
        maxHeight = Math.max(maxHeight, info.finalHeight)
      }

      // Calculate overall bounds
      const totalWidth = currentX - SPACING // Remove last spacing
      const overallBounds = {
        x: 0,
        y: 0,
        width: totalWidth,
        height: maxHeight,
      }

      // Center the group on viewport
      const groupCenterX = overallBounds.x + overallBounds.width / 2
      const groupCenterY = overallBounds.y + overallBounds.height / 2
      const offsetX = viewportCenter.x - groupCenterX
      const offsetY = viewportCenter.y - groupCenterY

      // Create all images with adjusted positions
      const shapeIds: ShapeId[] = []
      for (let i = 0; i < imageInfos.length; i++) {
        const info = imageInfos[i]
        const position = {
          x: positions[i].x + offsetX,
          y: positions[i].y + offsetY,
        }

        try {
          const shapeId = await createImageFromFile(
            editor,
            info.file,
            position,
            assetStore,
            false // Don't center individual images, we've already positioned them
          )
          shapeIds.push(shapeId)
        } catch (error) {
          console.error("Failed to add image:", error)
        }
      }

      // Select all created shapes
      if (shapeIds.length > 0) {
        editor.setSelectedShapes(shapeIds)
      }
    },
    [editor]
  )

  return (
    <Tooltip>
      <FileTrigger
        acceptedFileTypes={["image/*"]}
        allowsMultiple
        onSelect={handleFileSelect}
      >
        <Button isIconOnly variant="ghost">
          <Image size={16} strokeWidth={1.5} />
        </Button>
      </FileTrigger>
      <Tooltip.Content placement="right">
        <span>{t`Image`}</span>
      </Tooltip.Content>
    </Tooltip>
  )
}
