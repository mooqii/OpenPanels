import { Button, cn, ListBox, Select, Separator } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { Check, Crop, Link, Unlink, X } from "lucide-react"
import { useCallback, useEffect, useRef, useState } from "react"
import type {
  AspectRatioPreset,
  CropRect,
  UseCropReturn,
} from "../hooks/use-crop"
import { NumberInput } from "./number-input"

interface CropToolbarProps {
  /** Crop state from useCrop hook */
  crop: UseCropReturn
  /** Position of the toolbar (from transformer bounds) */
  position?: { x: number; y: number; width: number; height: number }
}

export function CropToolbar({ crop, position }: CropToolbarProps) {
  const { t } = useLingui()

  const aspectRatioPresets: { value: AspectRatioPreset; label: string }[] = [
    { value: "free", label: t`Custom` },
    { value: "1:1", label: "1:1" },
    { value: "4:3", label: "4:3" },
    { value: "3:4", label: "3:4" },
    { value: "16:9", label: "16:9" },
    { value: "9:16", label: "9:16" },
  ]

  const {
    cropRect,
    aspectRatioLock,
    aspectRatioPreset,
    setCropRect,
    setAspectRatioLock,
    setAspectRatioPreset,
    applyCrop,
    exitCropMode,
  } = crop

  // Local state for input fields
  const [localWidth, setLocalWidth] = useState(0)
  const [localHeight, setLocalHeight] = useState(0)
  const aspectRatioRef = useRef(1)

  // Sync local state with crop rect
  useEffect(() => {
    if (cropRect) {
      setLocalWidth(Math.round(cropRect.width))
      setLocalHeight(Math.round(cropRect.height))
      if (cropRect.height > 0) {
        aspectRatioRef.current = cropRect.width / cropRect.height
      }
    }
  }, [cropRect])

  // Handle width change
  const handleWidthChange = useCallback(
    (newWidth: number) => {
      setLocalWidth(newWidth)
      if (!cropRect) return

      const newRect: CropRect = { ...cropRect, width: newWidth }

      // Apply aspect ratio if locked
      if (aspectRatioLock && aspectRatioRef.current > 0) {
        newRect.height = Math.round(newWidth / aspectRatioRef.current)
        setLocalHeight(newRect.height)
      }

      setCropRect(newRect)
    },
    [cropRect, aspectRatioLock, setCropRect]
  )

  // Handle height change
  const handleHeightChange = useCallback(
    (newHeight: number) => {
      setLocalHeight(newHeight)
      if (!cropRect) return

      const newRect: CropRect = { ...cropRect, height: newHeight }

      // Apply aspect ratio if locked
      if (aspectRatioLock && aspectRatioRef.current > 0) {
        newRect.width = Math.round(newHeight * aspectRatioRef.current)
        setLocalWidth(newRect.width)
      }

      setCropRect(newRect)
    },
    [cropRect, aspectRatioLock, setCropRect]
  )

  // Handle aspect ratio preset change
  const handlePresetChange = useCallback(
    (key: string | null) => {
      if (!key) return
      const preset = key as AspectRatioPreset
      setAspectRatioPreset(preset)
    },
    [setAspectRatioPreset]
  )

  // Toggle aspect ratio lock
  const toggleAspectLock = useCallback(() => {
    if (!aspectRatioLock && localWidth > 0 && localHeight > 0) {
      // When linking, store current aspect ratio
      aspectRatioRef.current = localWidth / localHeight
    }
    setAspectRatioLock(!aspectRatioLock)
  }, [aspectRatioLock, localWidth, localHeight, setAspectRatioLock])

  // Handle keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault()
        exitCropMode()
      } else if (e.key === "Enter") {
        e.preventDefault()
        applyCrop()
      }
    }

    window.addEventListener("keydown", handleKeyDown)
    return () => window.removeEventListener("keydown", handleKeyDown)
  }, [exitCropMode, applyCrop])

  if (!cropRect) return null

  // Position toolbar above the crop area
  const toolbarStyle: React.CSSProperties = position
    ? {
        position: "absolute",
        left: `${position.x}px`,
        top: `${position.y}px`,
        width: `${position.width}px`,
        transform: "translateY(-56px)",
        display: "flex",
        justifyContent: "center",
        pointerEvents: "none",
      }
    : {
        position: "absolute",
        top: "20px",
        left: "50%",
        transform: "translateX(-50%)",
        pointerEvents: "none",
      }

  return (
    <div style={toolbarStyle}>
      <div className="pointer-events-auto flex h-11 items-center gap-2 rounded-full bg-canvas-toolbar px-3 shadow-lg">
        {/* Crop icon and label */}
        <div className="flex items-center gap-1.5 font-medium text-gray-700 text-sm">
          <Crop size={16} strokeWidth={1.5} />
          <span>{t`Crop`}</span>
        </div>

        <Separator orientation="vertical" />

        {/* Aspect ratio preset */}
        <Select
          aria-label={t`Aspect ratio`}
          onChange={handlePresetChange as any}
          selectionMode="single"
          value={aspectRatioPreset}
          variant="secondary"
        >
          <Select.Trigger className="min-w-20">
            <Select.Value className="text-sm">
              {aspectRatioPresets.find((p) => p.value === aspectRatioPreset)
                ?.label ?? t`Custom`}
            </Select.Value>
            <Select.Indicator />
          </Select.Trigger>
          <Select.Popover>
            <ListBox>
              {aspectRatioPresets.map((preset) => (
                <ListBox.Item
                  id={preset.value}
                  key={preset.value}
                  textValue={preset.label}
                >
                  {preset.label}
                </ListBox.Item>
              ))}
            </ListBox>
          </Select.Popover>
        </Select>

        <Separator orientation="vertical" />

        {/* Width input */}
        <NumberInput
          className="group flex w-20 items-center rounded-lg bg-surface-secondary px-1 py-0.5 focus-within:ring-2 focus-within:ring-blue-500"
          dragVelocity={1}
          min={1}
          onChange={handleWidthChange}
          precision={1}
          step={1}
          value={localWidth}
        >
          <NumberInput.DragHandle icon="W" />
          <NumberInput.Input className="w-14 bg-transparent text-right" />
        </NumberInput>

        {/* Aspect ratio lock button */}
        <button
          aria-label={
            aspectRatioLock ? t`Unlock aspect ratio` : t`Lock aspect ratio`
          }
          className={cn(
            "flex h-6 w-6 items-center justify-center rounded-md transition-colors",
            aspectRatioLock
              ? "bg-blue-100 text-blue-600"
              : "text-gray-400 hover:bg-gray-100"
          )}
          onClick={toggleAspectLock}
          type="button"
        >
          {aspectRatioLock ? <Link size={14} /> : <Unlink size={14} />}
        </button>

        {/* Height input */}
        <NumberInput
          className="group flex w-20 items-center rounded-lg bg-surface-secondary px-1 py-0.5 focus-within:ring-2 focus-within:ring-blue-500"
          dragVelocity={1}
          min={1}
          onChange={handleHeightChange}
          precision={1}
          step={1}
          value={localHeight}
        >
          <NumberInput.DragHandle icon="H" />
          <NumberInput.Input className="w-14 bg-transparent text-right" />
        </NumberInput>

        <Separator orientation="vertical" />

        {/* Cancel button */}
        <Button
          aria-label={t`Cancel crop`}
          isIconOnly
          onClick={exitCropMode}
          size="sm"
          variant="ghost"
        >
          <X size={16} strokeWidth={1.5} />
        </Button>

        {/* Apply button */}
        <Button
          aria-label={t`Apply crop`}
          className="bg-primary text-white"
          isIconOnly
          onClick={applyCrop}
          size="sm"
        >
          <Check size={16} strokeWidth={1.5} />
        </Button>
      </div>
    </div>
  )
}
