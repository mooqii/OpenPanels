import { Button } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { Images } from "lucide-react"
import { useCallback } from "react"
import type { Editor } from "~/canvas/editor"
import { useSelectedShapes } from "~/canvas/hooks/use-editor-state"
import type { Transformer } from "~/canvas/shapes/Transformer"
import { captureTransformer } from "~/canvas/utils/capture"
import { createImageFromFile } from "~/canvas/utils/clipboard"
import { getShapesBounds } from "~/canvas/utils/coordinates"
import { CanvasToolbarTooltip as Tooltip } from "./CanvasToolbarTooltip"

interface RasterizeItemProps {
  editor: Editor
  transformerRef: React.RefObject<Transformer | null>
}

function dataUrlToFile(dataUrl: string, name: string): File {
  const [header, base64 = ""] = dataUrl.split(",")
  const mime = header.match(/data:(.*?);base64/)?.[1] ?? "image/png"
  const binary = atob(base64)
  const bytes = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i)
  }
  return new File([bytes], name, { type: mime })
}

const RASTERIZED_SELECTION_OFFSET_RATIO = 0.2

export function RasterizeItem({ editor, transformerRef }: RasterizeItemProps) {
  const { t } = useLingui()
  const shapes = useSelectedShapes(editor)

  const handleClick = useCallback(async () => {
    if (shapes.length < 2) return

    const dataUrl = captureTransformer(transformerRef.current)
    if (!dataUrl) return

    const bounds = getShapesBounds(shapes)
    const file = dataUrlToFile(dataUrl, "selection.png")
    const gap = {
      x: bounds.width * RASTERIZED_SELECTION_OFFSET_RATIO,
      y: bounds.height * RASTERIZED_SELECTION_OFFSET_RATIO,
    }
    const position = {
      x: bounds.x - gap.x,
      y: bounds.y + bounds.height + gap.y,
    }
    await createImageFromFile(
      editor,
      file,
      position,
      editor.getAssetStore(),
      false,
      { height: bounds.height, width: bounds.width }
    )
  }, [editor, shapes, transformerRef])

  if (shapes.length < 2) return null

  return (
    <Tooltip>
      <Button
        aria-label={t`Create image from selection`}
        isIconOnly
        onClick={handleClick}
        variant="ghost"
      >
        <Images size={16} strokeWidth={1.5} />
      </Button>
      <Tooltip.Content>{t`Create image from selection`}</Tooltip.Content>
    </Tooltip>
  )
}
