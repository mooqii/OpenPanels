import {
  Button,
  type Color,
  ColorArea,
  ColorField,
  ColorPickerRoot,
  ColorSlider,
  ColorSwatch,
  ColorSwatchPicker,
  parseColor,
} from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { Shuffle } from "lucide-react"
import { useCallback } from "react"

export const TRANSPARENT_BG =
  "url('data:image/svg+xml,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%228%22%20height%3D%228%22%3E%3Crect%20width%3D%224%22%20height%3D%224%22%20fill%3D%22%23ccc%22%2F%3E%3Crect%20x%3D%224%22%20y%3D%224%22%20width%3D%224%22%20height%3D%224%22%20fill%3D%22%23ccc%22%2F%3E%3C%2Fsvg%3E')"

// Preset colors for quick selection
const PRESET_COLORS = [
  "#000000", // black
  "#6b7280", // gray
  "#ef4444", // red
  "#f97316", // orange
  "#eab308", // yellow
  "#22c55e", // green
  "#06b6d4", // cyan
  "#3b82f6", // blue
  "#8b5cf6", // violet
  "#ec4899", // pink
]

interface ColorPickerProps {
  onChange: (color: string) => void
  value: string
}

export function ColorPicker({ value, onChange }: ColorPickerProps) {
  const { t } = useLingui()
  const handleChange = useCallback(
    (color: Color) => {
      onChange(color.toString("rgba"))
    },
    [onChange]
  )

  const shuffleColor = useCallback(() => {
    const randomHue = Math.floor(Math.random() * 360)
    const randomSaturation = 50 + Math.floor(Math.random() * 50) // 50-100%
    const randomLightness = 40 + Math.floor(Math.random() * 30) // 40-70%
    handleChange(
      parseColor(`hsl(${randomHue}, ${randomSaturation}%, ${randomLightness}%)`)
    )
  }, [handleChange])

  return (
    <ColorPickerRoot onChange={handleChange} value={value}>
      <div className="flex flex-col gap-3">
        <ColorArea
          aria-label={t`Color area`}
          className="max-w-full"
          colorSpace="hsb"
          xChannel="saturation"
          yChannel="brightness"
        >
          <ColorArea.Thumb />
        </ColorArea>

        <ColorSlider
          aria-label={t`Hue slider`}
          channel="hue"
          className="flex-1"
          colorSpace="hsb"
        >
          <ColorSlider.Track>
            <ColorSlider.Thumb />
          </ColorSlider.Track>
        </ColorSlider>

        <ColorSlider channel="alpha" className="w-full max-w-xs">
          <ColorSlider.Track>
            <ColorSlider.Thumb />
          </ColorSlider.Track>
        </ColorSlider>

        <div className="flex items-center gap-2">
          <ColorField aria-label={t`Color field`}>
            <ColorField.Group variant="secondary">
              <ColorField.Prefix>
                <ColorSwatch size="xs" />
              </ColorField.Prefix>
              <ColorField.Input />
            </ColorField.Group>
          </ColorField>
          <Button
            aria-label={t`Shuffle color`}
            isIconOnly
            onPress={shuffleColor}
            size="sm"
            variant="tertiary"
          >
            <Shuffle size={16} strokeWidth={1.5} />
          </Button>
        </div>

        <ColorSwatchPicker className="justify-center pt-2" size="md">
          {PRESET_COLORS.map((preset) => (
            <ColorSwatchPicker.Item color={preset} key={preset}>
              <ColorSwatchPicker.Swatch />
            </ColorSwatchPicker.Item>
          ))}
        </ColorSwatchPicker>
      </div>
    </ColorPickerRoot>
  )
}
