import { Button, Modal } from "@heroui/react"
import { Minus, Plus, X } from "lucide-react"
import { useState } from "react"
import { useMyOpenPanelsI18n } from "../canvas"
import { clampImageScale } from "../lib/api"

export function ImagePreviewDialog({
  alt,
  closeLabel,
  onClose,
  src,
}: {
  alt: string
  closeLabel: string
  onClose: () => void
  src: string
}) {
  const { t } = useMyOpenPanelsI18n()
  const [imageScale, setImageScale] = useState(1)
  const zoomPercentage = Math.round(imageScale * 100)

  return (
    <Modal.Backdrop
      className="op-image-preview"
      isOpen
      onOpenChange={(isOpen) => {
        if (!isOpen) onClose()
      }}
      variant="opaque"
    >
      <Modal.Container size="full">
        <Modal.Dialog aria-label={alt} className="op-image-preview__dialog">
          <div
            className="op-image-preview__stage"
            onClick={(event) => {
              if (event.target === event.currentTarget) onClose()
            }}
            onWheel={(event) => {
              event.preventDefault()
              setImageScale((current) =>
                clampImageScale(current + (event.deltaY < 0 ? 0.12 : -0.12))
              )
            }}
          >
            <img
              alt={alt}
              src={src}
              style={{ transform: `scale(${imageScale})` }}
            />
          </div>
          <Button
            aria-label={closeLabel}
            className="op-image-preview__close"
            isIconOnly
            onPress={onClose}
            size="md"
            variant="ghost"
          >
            <X size={21} />
          </Button>
          <div className="op-image-preview__controls">
            <Button
              aria-label={t`Zoom out`}
              isIconOnly
              onPress={() =>
                setImageScale((current) => clampImageScale(current - 0.2))
              }
              size="sm"
              variant="ghost"
            >
              <Minus size={15} />
            </Button>
            <span className="op-image-preview__zoom-value">
              {zoomPercentage}%
            </span>
            <Button
              aria-label={t`Zoom in`}
              isIconOnly
              onPress={() =>
                setImageScale((current) => clampImageScale(current + 0.2))
              }
              size="sm"
              variant="ghost"
            >
              <Plus size={15} />
            </Button>
          </div>
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  )
}
