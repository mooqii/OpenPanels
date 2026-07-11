import { Button } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { Crop } from "lucide-react"
import { useCallback } from "react"
import { useCropContext } from "../../contexts/CropContext"
import type { Shape } from "../../types/shapes"
import { CanvasToolbarTooltip as Tooltip } from "./CanvasToolbarTooltip"

interface CropItemProps {
  shape: Shape | null
}

export function CropItem({ shape }: CropItemProps) {
  const { t } = useLingui()
  const { enterCropMode } = useCropContext()

  const handleCrop = useCallback(() => {
    if (!shape || shape.type !== "image") return
    enterCropMode(shape.id)
  }, [shape, enterCropMode])

  // Only enable for image shapes
  const isDisabled = !shape || shape.type !== "image"

  return (
    <Tooltip>
      <Button
        aria-label={t`Crop`}
        isDisabled={isDisabled}
        isIconOnly
        onClick={handleCrop}
        variant="ghost"
      >
        <Crop size={16} strokeWidth={1.5} />
      </Button>
      <Tooltip.Content>{t`Crop`}</Tooltip.Content>
    </Tooltip>
  )
}
