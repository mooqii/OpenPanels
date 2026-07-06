/**
 * GradientFillPanel component for configuring linear and radial gradient fills.
 */

import { Button, cn, Label, Tabs } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { Minus, Plus } from "lucide-react"
import { useCallback } from "react"
import {
  GRADIENT_PRESETS,
  type GradientPreset,
} from "../../constants/gradient-presets"
import type {
  GradientColorStop,
  LinearGradientFill,
  RadialGradientFill,
} from "../../types/shapes"
import { toCssLinearGradient, toCssRadialGradient } from "../../utils/fill"
import { NumberInput } from "../number-input"
import { GradientBar } from "./GradientBar"

type GradientType = "linear" | "radial"

interface GradientFillPanelProps {
  onChange: (fill: LinearGradientFill | RadialGradientFill) => void
  value: LinearGradientFill | RadialGradientFill
}

export function GradientFillPanel({ value, onChange }: GradientFillPanelProps) {
  const { t } = useLingui()
  const gradientType: GradientType =
    value.type === "linear-gradient" ? "linear" : "radial"

  // Handle gradient type change
  const handleTypeChange = useCallback(
    (key: string | number) => {
      const newType = String(key) as GradientType
      if (newType === "linear" && value.type !== "linear-gradient") {
        onChange({
          type: "linear-gradient",
          colorStops: value.colorStops,
          rotation: 90,
        })
      } else if (newType === "radial" && value.type !== "radial-gradient") {
        onChange({
          type: "radial-gradient",
          colorStops: value.colorStops,
        })
      }
    },
    [value, onChange]
  )

  // Handle color stops change
  const handleColorStopsChange = useCallback(
    (colorStops: GradientColorStop[]) => {
      if (value.type === "linear-gradient") {
        onChange({ ...value, colorStops })
      } else {
        onChange({ ...value, colorStops })
      }
    },
    [value, onChange]
  )

  // Handle rotation change (linear only)
  const handleRotationChange = useCallback(
    (rotation: number) => {
      if (value.type === "linear-gradient") {
        onChange({ ...value, rotation })
      }
    },
    [value, onChange]
  )

  // Handle preset selection
  const handlePresetSelect = useCallback(
    (preset: GradientPreset) => {
      if (value.type === "linear-gradient") {
        onChange({
          ...preset.fill,
          rotation:
            value.type === "linear-gradient"
              ? value.rotation
              : preset.fill.rotation,
        })
      } else {
        onChange({
          type: "radial-gradient",
          colorStops: preset.fill.colorStops,
        })
      }
    },
    [value, onChange]
  )

  // Add a new color stop
  const handleAddStop = useCallback(() => {
    const stops = value.colorStops
    // Add a stop in the middle
    const lastStop = stops.at(-1)
    const secondLastStop = stops.at(-2)
    const middleOffset =
      stops.length >= 2 && lastStop && secondLastStop
        ? (lastStop.offset + secondLastStop.offset) / 2
        : 0.5
    const newStop: GradientColorStop = {
      offset: middleOffset,
      color: "#888888",
    }
    const newStops = [...stops, newStop].sort((a, b) => a.offset - b.offset)
    handleColorStopsChange(newStops)
  }, [value.colorStops, handleColorStopsChange])

  // Remove the last color stop
  const handleRemoveStop = useCallback(() => {
    if (value.colorStops.length <= 2) return
    const newStops = value.colorStops.slice(0, -1)
    handleColorStopsChange(newStops)
  }, [value.colorStops, handleColorStopsChange])

  const displayedPresets = GRADIENT_PRESETS

  return (
    <div className="flex w-full flex-col gap-4">
      {/* Linear / Radial tabs */}
      <Tabs onSelectionChange={handleTypeChange} selectedKey={gradientType}>
        <Tabs.ListContainer>
          <Tabs.List>
            <Tabs.Tab id="linear">
              {t`Linear`}
              <Tabs.Indicator />
            </Tabs.Tab>
            <Tabs.Tab id="radial">
              {t`Radial`}
              <Tabs.Indicator />
            </Tabs.Tab>
          </Tabs.List>
        </Tabs.ListContainer>
      </Tabs>

      {/* Colors section */}
      <div className="flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <span className="text-sm text-zinc-700">{t`Colors`}</span>
          <div className="flex items-center gap-1">
            <Button
              isIconOnly
              onPress={handleAddStop}
              size="sm"
              variant="ghost"
            >
              <Plus size={14} />
            </Button>
            <Button
              isDisabled={value.colorStops.length <= 2}
              isIconOnly
              onPress={handleRemoveStop}
              size="sm"
              variant="ghost"
            >
              <Minus size={14} />
            </Button>
          </div>
        </div>

        {/* Gradient bar with draggable stops */}
        <GradientBar
          colorStops={value.colorStops}
          onChange={handleColorStopsChange}
        />
      </div>

      {/* Rotation control (linear gradient only) */}
      {value.type === "linear-gradient" && (
        <div className="flex items-center justify-between">
          <Label className="text-sm text-zinc-700">{t`Rotation`}</Label>
          <NumberInput
            className="w-28 rounded-lg bg-surface-secondary px-2 py-1"
            dragVelocity={1}
            max={360}
            min={0}
            onChange={handleRotationChange}
            precision={1}
            step={1}
            value={value.rotation}
          >
            <NumberInput.DragHandle />
            <NumberInput.Input className="w-12 text-right" />
            <NumberInput.Unit>deg</NumberInput.Unit>
          </NumberInput>
        </div>
      )}

      {/* Presets section */}
      <div className="flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <span className="text-sm text-zinc-700">{t`Preset`}</span>
        </div>

        {/* Preset grid */}
        <div className="grid grid-cols-6 gap-2">
          {displayedPresets.map((preset) => {
            const gradientCss =
              value.type === "linear-gradient"
                ? toCssLinearGradient({
                    ...preset.fill,
                    rotation: 135,
                  })
                : toCssRadialGradient({
                    type: "radial-gradient",
                    colorStops: preset.fill.colorStops,
                  })

            return (
              <button
                className={cn(
                  "h-9 w-9 rounded-full transition-all",
                  "hover:scale-110 focus:outline-none focus:ring-2 focus:ring-blue-500"
                )}
                key={preset.id}
                onClick={() => handlePresetSelect(preset)}
                style={{ background: gradientCss }}
                title={preset.name}
                type="button"
              />
            )
          })}
        </div>
      </div>
    </div>
  )
}
