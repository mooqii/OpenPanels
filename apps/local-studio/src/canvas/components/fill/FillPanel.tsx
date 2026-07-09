/**
 * FillPanel - Main tabbed container for fill configuration.
 * Supports Solid, Gradient, and Image fill types.
 */

import { Tabs } from "@heroui/react"
import { useLingui } from "@lingui/react/macro"
import { useCallback, useMemo } from "react"
import type { Asset } from "../../types/assets"
import type {
  ImageFill,
  LinearGradientFill,
  RadialGradientFill,
  ShapeFill,
} from "../../types/shapes"
import { DEFAULT_LINEAR_GRADIENT, DEFAULT_SOLID_FILL } from "../../utils/fill"
import { ColorPicker } from "../ColorPicker"
import { GradientFillPanel } from "./GradientFillPanel"
import { ImageFillPanel } from "./ImageFillPanel"

type FillTabKey = "solid" | "gradient" | "image"

interface FillPanelProps {
  /** Function to get asset by ID (for image fills) */
  getAsset?: (assetId: string) => Asset | undefined
  onChange: (fill: ShapeFill) => void
  /** Function to upload a file and create an asset (for image fills) */
  onUpload?: (file: File) => Promise<string>
  value: ShapeFill
}

/**
 * Determine which tab should be active based on the fill type
 */
function getFillTabKey(fill: ShapeFill): FillTabKey {
  switch (fill.type) {
    case "solid":
      return "solid"
    case "linear-gradient":
    case "radial-gradient":
      return "gradient"
    case "image":
      return "image"
    default:
      return "solid"
  }
}

export function FillPanel({
  value,
  onChange,
  getAsset,
  onUpload,
}: FillPanelProps) {
  const { t } = useLingui()
  const activeTab = getFillTabKey(value)

  // Track the last used gradient/image settings for tab switching
  const lastGradient = useMemo<LinearGradientFill | RadialGradientFill>(() => {
    if (value.type === "linear-gradient" || value.type === "radial-gradient") {
      return value
    }
    return DEFAULT_LINEAR_GRADIENT
  }, [value])

  const lastImage = useMemo<ImageFill | null>(() => {
    if (value.type === "image") {
      return value
    }
    return null
  }, [value])

  // Handle tab change
  const handleTabChange = useCallback(
    (key: string | number) => {
      const newTab = String(key) as FillTabKey

      if (newTab === "solid" && value.type !== "solid") {
        // Switch to solid fill
        onChange(DEFAULT_SOLID_FILL)
      } else if (
        newTab === "gradient" &&
        value.type !== "linear-gradient" &&
        value.type !== "radial-gradient"
      ) {
        // Switch to gradient fill
        onChange(lastGradient)
      } else if (newTab === "image" && value.type !== "image") {
        // Switch to image fill
        if (lastImage) {
          onChange(lastImage)
        } else {
          // Create a placeholder image fill
          // The user will need to upload an image
          onChange({
            type: "image",
            assetId: "" as any,
          })
        }
      }
    },
    [value, lastGradient, lastImage, onChange]
  )

  // Handle solid color change
  const handleSolidColorChange = useCallback(
    (color: string) => {
      onChange({ type: "solid", color })
    },
    [onChange]
  )

  // Handle gradient change
  const handleGradientChange = useCallback(
    (gradient: LinearGradientFill | RadialGradientFill) => {
      onChange(gradient)
    },
    [onChange]
  )

  // Handle image fill change
  const handleImageChange = useCallback(
    (imageFill: ImageFill | null) => {
      if (imageFill) {
        onChange(imageFill)
      } else {
        // If cleared, switch back to solid
        onChange(DEFAULT_SOLID_FILL)
      }
    },
    [onChange]
  )

  return (
    <div className="flex flex-col gap-4">
      {/* Fill type tabs */}
      <Tabs
        className="w-full"
        onSelectionChange={handleTabChange}
        selectedKey={activeTab}
      >
        <Tabs.ListContainer>
          <Tabs.List className="w-full">
            <Tabs.Tab id="solid">
              {t`Solid`}
              <Tabs.Indicator />
            </Tabs.Tab>
            <Tabs.Tab id="gradient">
              {t`Gradient`}
              <Tabs.Indicator />
            </Tabs.Tab>
            <Tabs.Tab id="image">
              {t`Image`}
              <Tabs.Indicator />
            </Tabs.Tab>
          </Tabs.List>
        </Tabs.ListContainer>

        <Tabs.Panel id="solid">
          <ColorPicker
            onChange={handleSolidColorChange}
            value={value.type === "solid" ? value.color : "#ffffff"}
          />
        </Tabs.Panel>
        <Tabs.Panel id="gradient">
          <GradientFillPanel
            onChange={handleGradientChange}
            value={
              value.type === "linear-gradient" ||
              value.type === "radial-gradient"
                ? value
                : lastGradient
            }
          />
        </Tabs.Panel>
        <Tabs.Panel id="image">
          <ImageFillPanel
            getAsset={getAsset}
            onChange={handleImageChange}
            onUpload={onUpload}
            value={value.type === "image" ? value : null}
          />
        </Tabs.Panel>
      </Tabs>
    </div>
  )
}
