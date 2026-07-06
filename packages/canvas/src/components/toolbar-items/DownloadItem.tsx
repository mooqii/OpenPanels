import { Button } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { Download } from "lucide-react"
import { useCallback } from "react"
import type { Editor } from "~/canvas/editor"
import type { Transformer } from "../../shapes/Transformer"
import type { ImageShape, Shape } from "../../types/shapes"
import { captureTransformer } from "../../utils/capture"
import { exportCroppedImageDataUrl, hasCrop } from "../../utils/crop-image"
import { CanvasToolbarTooltip as Tooltip } from "./CanvasToolbarTooltip"

interface DownloadItemProps {
  editor: Editor
  shape: Shape | null
  transformerRef: React.RefObject<Transformer | null>
}

/**
 * Add suffix to filename before extension (e.g., "image.png" -> "image-cropped.png")
 */
function addFilenameSuffix(name: string, suffix: string): string {
  const lastDotIndex = name.lastIndexOf(".")
  if (lastDotIndex === -1) {
    return `${name}${suffix}`
  }
  return `${name.slice(0, lastDotIndex)}${suffix}${name.slice(lastDotIndex)}`
}

export function DownloadItem({
  editor,
  shape,
  transformerRef,
}: DownloadItemProps) {
  const { t } = useLingui()
  const handleDownload = useCallback(async () => {
    if (!(shape || transformerRef.current)) return

    const asset =
      shape?.type === "image" && shape.props.assetId
        ? editor.getAsset(shape.props.assetId)
        : null

    if (asset && !(asset.type === "image" || asset.type === "video")) return

    try {
      let url: string | null = null
      let isCropped = false

      if (asset && shape?.type === "image") {
        const imageShape = shape as ImageShape
        const crop = imageShape.props.crop

        // Check if shape has a valid crop applied
        if (hasCrop(crop)) {
          // Get the asset URL to load the full image
          const assetStore = editor.getAssetStore()
          const assetUrl = assetStore
            ? assetStore.resolve(asset)
            : (asset.props as any).src

          if (assetUrl) {
            // Export the cropped region as a data URL
            url = await exportCroppedImageDataUrl({
              src: assetUrl,
              crop,
              mimeType: "image/png",
            })
            isCropped = url !== null
          }
        }

        // Fall back to full asset download if crop export failed or no crop
        if (!url) {
          const assetStore = editor.getAssetStore()
          if (assetStore) {
            url = await assetStore.download(asset)
          }
        }
      } else if (asset) {
        // Non-image asset (e.g., video) - use standard download
        const assetStore = editor.getAssetStore()
        if (assetStore) {
          url = await assetStore.download(asset)
        }
      } else if (transformerRef.current) {
        // For transformer selection (multi-shape or non-asset shapes)
        url = captureTransformer(transformerRef.current)
      }

      if (!url) return

      let name =
        asset?.meta?.name ||
        (asset?.meta &&
        typeof asset.meta === "object" &&
        "fileName" in asset.meta
          ? (asset.meta as any).fileName
          : null) ||
        asset?.props.name ||
        `shape-${shape?.id || new Date()}.png`

      // Add "-cropped" suffix if we exported a cropped image
      if (isCropped) {
        name = addFilenameSuffix(name as string, "-cropped")
      }

      // Create download link
      const link = document.createElement("a")
      link.target = "_blank"
      link.download = name as string
      link.href = url
      document.body.appendChild(link)
      link.click()
      document.body.removeChild(link)
    } catch (error) {
      console.error("Failed to export shape:", error)
    }
  }, [editor, shape, transformerRef])

  return (
    <Tooltip>
      <Button
        aria-label={t`Download`}
        isIconOnly
        onClick={handleDownload}
        variant="ghost"
      >
        <Download size={16} />
      </Button>
      <Tooltip.Content>{t`Download`}</Tooltip.Content>
    </Tooltip>
  )
}
