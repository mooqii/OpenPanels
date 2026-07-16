/**
 * ImageFillPanel component for configuring image pattern fills.
 */

import { Button, Label, Slider } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { ImageIcon, Trash2, Upload } from "lucide-react"
import { useCallback, useEffect, useState } from "react"
import { FileTrigger } from "react-aria-components"
import type { Asset } from "../../types/assets"
import type { ImageFill } from "../../types/shapes"
import { fileToDataUrl } from "../../utils/clipboard"
import { NumberInput } from "../number-input"

interface ImageFillPanelProps {
  /** Function to get asset by ID */
  getAsset?: (assetId: string) => Asset | undefined
  onChange: (fill: ImageFill | null) => void
  /** Function to upload a file and create an asset */
  onUpload?: (file: File) => Promise<string>
  value: ImageFill | null
}

export function ImageFillPanel({
  value,
  onChange,
  getAsset,
  onUpload,
}: ImageFillPanelProps) {
  const { t } = useLingui()
  const [previewUrl, setPreviewUrl] = useState<string | null>(null)
  const [isUploading, setIsUploading] = useState(false)

  // Load preview image from asset
  useEffect(() => {
    if (value?.assetId && getAsset) {
      const asset = getAsset(value.assetId as string)
      if (asset && "src" in asset.props) {
        setPreviewUrl(asset.props.src)
      }
    } else {
      setPreviewUrl(null)
    }
  }, [value?.assetId, getAsset])

  // Handle file selection
  const handleFileSelect = useCallback(
    async (files: FileList | null) => {
      if (!files || files.length === 0) return

      const file = files[0]
      if (!file.type.startsWith("image/")) return

      setIsUploading(true)

      try {
        let assetId: string

        if (onUpload) {
          // Use provided upload function
          assetId = await onUpload(file)
        } else {
          // Fallback: create data URL and use it as asset ID
          const dataUrl = await fileToDataUrl(file)
          assetId = `temp:${dataUrl}`
          setPreviewUrl(dataUrl)
        }

        onChange({
          type: "image",
          assetId: assetId as any,
          scale: value?.scale ?? { x: 1, y: 1 },
          offset: value?.offset ?? { x: 0, y: 0 },
        })
      } catch (error) {
        console.error("Failed to upload image:", error)
      } finally {
        setIsUploading(false)
      }
    },
    [onUpload, onChange, value]
  )

  // Handle scale change
  const handleScaleChange = useCallback(
    (axis: "x" | "y", scale: number) => {
      if (!value) return
      onChange({
        ...value,
        scale: {
          x: axis === "x" ? scale : (value.scale?.x ?? 1),
          y: axis === "y" ? scale : (value.scale?.y ?? 1),
        },
      })
    },
    [value, onChange]
  )

  // Handle uniform scale change
  const handleUniformScaleChange = useCallback(
    (scale: number) => {
      if (!value) return
      onChange({
        ...value,
        scale: { x: scale, y: scale },
      })
    },
    [value, onChange]
  )

  // Handle clear
  const handleClear = useCallback(() => {
    onChange(null)
    setPreviewUrl(null)
  }, [onChange])

  const scaleX = value?.scale?.x ?? 1
  const scaleY = value?.scale?.y ?? 1
  const uniformScale = scaleX === scaleY ? scaleX : null

  return (
    <div className="flex flex-col gap-4">
      {/* Image preview / upload area */}
      {previewUrl ? (
        <div className="relative">
          <div
            className="flex h-32 items-center justify-center overflow-hidden rounded-lg border border-border bg-field"
            style={{
              backgroundImage: `url(${previewUrl})`,
              backgroundSize: "contain",
              backgroundPosition: "center",
              backgroundRepeat: "no-repeat",
            }}
          />
          <div className="absolute top-2 right-2 flex gap-1">
            <FileTrigger
              acceptedFileTypes={["image/*"]}
              onSelect={handleFileSelect}
            >
              <Button isIconOnly size="sm" variant="ghost">
                <Upload size={14} />
              </Button>
            </FileTrigger>
            <Button isIconOnly onPress={handleClear} size="sm" variant="ghost">
              <Trash2 size={14} />
            </Button>
          </div>
        </div>
      ) : (
        <FileTrigger
          acceptedFileTypes={["image/*"]}
          onSelect={handleFileSelect}
        >
          <Button
            className="flex h-32 w-full flex-col items-center justify-center gap-2 rounded-lg border-2 border-field-border border-dashed bg-field transition-colors hover:border-field-border-hover hover:bg-field-hover"
            isDisabled={isUploading}
          >
            {isUploading ? (
              <>
                <div className="h-6 w-6 animate-spin rounded-full border-2 border-field-border border-t-focus" />
                <span className="text-muted text-sm">{t`Uploading...`}</span>
              </>
            ) : (
              <>
                <ImageIcon className="text-muted" size={24} />
                <span className="text-muted text-sm">
                  {t`Click to upload an image`}
                </span>
              </>
            )}
          </Button>
        </FileTrigger>
      )}

      {/* Scale controls (only show when image is selected) */}
      {value && (
        <div className="flex flex-col gap-3">
          {/* Uniform scale slider */}
          <div className="flex items-center gap-3">
            <Slider
              className="w-full max-w-xs"
              defaultValue={30}
              maxValue={3}
              minValue={0.1}
              onChange={(v) => handleUniformScaleChange(v as number)}
              step={0.1}
              value={uniformScale ?? 1}
            >
              <Label>{t`Scale`}</Label>
              <Slider.Output />
              <Slider.Track>
                <Slider.Fill />
                <Slider.Thumb />
              </Slider.Track>
            </Slider>

            {/* <NumberInput */}
            {/*   className="rounded-lg px-2 py-1" */}
            {/*   max={3} */}
            {/*   min={0.1} */}
            {/*   onChange={(v) => handleUniformScaleChange(v)} */}
            {/*   step={0.1} */}
            {/*   value={uniformScale ?? 1} */}
            {/* > */}
            {/*   <NumberInput.Input /> */}
            {/* </NumberInput> */}
          </div>

          {/* Individual X/Y scale */}
          <div className="grid grid-cols-2 gap-2">
            <NumberInput
              className="rounded-lg px-2 py-1"
              max={3}
              min={0.1}
              onChange={(v: number) => handleScaleChange("x", v)}
              step={0.1}
              value={scaleX}
            >
              <NumberInput.DragHandle icon="X" />
              <NumberInput.Input className="w-22" />
            </NumberInput>
            <NumberInput
              className="rounded-lg px-2 py-1"
              max={3}
              min={0.1}
              onChange={(v: number) => handleScaleChange("y", v)}
              step={0.1}
              value={scaleY}
            >
              <NumberInput.DragHandle icon="Y" />
              <NumberInput.Input className="w-22" />
            </NumberInput>
          </div>
        </div>
      )}
    </div>
  )
}
